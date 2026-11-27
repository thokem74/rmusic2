use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, anyhow, bail};
use futures_util::{StreamExt, future::join_all};
use hickory_resolver::Resolver;
use rand::seq::SliceRandom;
use reqwest::{Client, Url};
use serde::{Deserialize, de::DeserializeOwned};
use tokio::{
    sync::RwLock,
    time::{Instant, timeout},
};

use crate::model::{Query, Station};

#[derive(Clone)]
pub(crate) struct Catalog {
    client: Client,
    mirrors: Arc<RwLock<Option<MirrorCache>>>,
}

struct MirrorCache {
    created: Instant,
    servers: Vec<Url>,
}

impl Catalog {
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .user_agent(concat!("rmusic/", env!("CARGO_PKG_VERSION")))
                .connect_timeout(Duration::from_secs(3))
                .timeout(Duration::from_secs(5))
                .build()?,
            mirrors: Arc::new(RwLock::new(None)),
        })
    }

    #[cfg(test)]
    pub fn with_servers(servers: Vec<String>) -> Result<Self> {
        let catalog = Self::new()?;
        let urls = servers
            .into_iter()
            .map(|s| Url::parse(&s))
            .collect::<Result<Vec<_>, _>>()?;
        *catalog
            .mirrors
            .try_write()
            .context("Fixture mirror cache unavailable")? = Some(MirrorCache {
            created: Instant::now(),
            servers: urls,
        });
        Ok(catalog)
    }

    pub async fn search(&self, query: &Query, page: usize) -> Result<Vec<Station>> {
        let mut params = vec![
            ("order".into(), "clickcount".into()),
            ("reverse".into(), "true".into()),
            ("limit".into(), "50".into()),
            ("offset".into(), page.saturating_mul(50).to_string()),
            ("hidebroken".into(), query.hide_broken.to_string()),
            ("nameExact".into(), "false".into()),
            ("languageExact".into(), "true".into()),
            ("tagExact".into(), "true".into()),
        ];
        for (key, value) in [
            ("name", query.name.trim()),
            ("countrycode", query.country.trim()),
            ("language", query.language.trim()),
            ("tag", query.tag.trim()),
        ] {
            if !value.is_empty() {
                params.push((
                    key.into(),
                    if key == "countrycode" {
                        value.to_uppercase()
                    } else {
                        value.into()
                    },
                ));
            }
        }
        let stations: Vec<Station> = self.get(&["json", "stations", "search"], &params).await?;
        Ok(stations
            .into_iter()
            .filter(|s| !s.stationuuid.is_empty())
            .take(50)
            .collect())
    }

    pub async fn refresh(&self, ids: &[String]) -> Result<Vec<Station>> {
        let mut stations = Vec::new();
        for chunk in ids.chunks(100) {
            let mut batch: Vec<Station> = self
                .get(
                    &["json", "stations", "byuuid"],
                    &[("uuids".into(), chunk.join(","))],
                )
                .await?;
            stations.append(&mut batch);
        }
        Ok(stations)
    }

    pub async fn resolve(&self, station: &Station) -> String {
        if station.stationuuid.is_empty() {
            // A saved stream without a directory identity has nothing to report.
            return station.stream_url().into();
        }
        #[derive(Deserialize)]
        struct Click {
            ok: bool,
            url: String,
        }
        // Reporting is best effort. Even DNS discovery is inside this deadline,
        // so a directory outage cannot consume the player's connection budget.
        if let Ok(Ok(click)) = timeout(
            Duration::from_secs(2),
            self.get::<Click>(&["json", "url", &station.stationuuid], &[]),
        )
        .await
            && click.ok
            && !click.url.is_empty()
        {
            return click.url;
        }
        station.stream_url().into()
    }

    async fn servers(&self) -> Result<Vec<Url>> {
        if let Some(cache) = &*self.mirrors.read().await
            && cache.created.elapsed() < Duration::from_secs(300)
        {
            return Ok(cache.servers.clone());
        }
        let resolver = Resolver::builder_tokio()?.build();
        let mut servers = Vec::new();
        if let Ok(Ok(records)) = timeout(
            Duration::from_secs(3),
            resolver.srv_lookup("_api._tcp.radio-browser.info."),
        )
        .await
        {
            for record in records.iter() {
                let host = record.target().to_utf8();
                if let Ok(url) = Url::parse(&format!(
                    "https://{}:{}",
                    host.trim_end_matches('.'),
                    record.port()
                )) {
                    servers.push(url);
                }
            }
        }
        if servers.is_empty() {
            let addresses = timeout(
                Duration::from_secs(3),
                resolver.lookup_ip("all.api.radio-browser.info."),
            )
            .await??;
            let resolver = &resolver;
            let lookups = addresses.iter().take(16).map(|ip| async move {
                timeout(Duration::from_secs(2), resolver.reverse_lookup(ip)).await
            });
            for records in join_all(lookups).await.into_iter().flatten().flatten() {
                for name in records.iter() {
                    if let Ok(url) =
                        Url::parse(&format!("https://{}", name.to_utf8().trim_end_matches('.')))
                    {
                        servers.push(url);
                    }
                }
            }
        }
        servers.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        servers.dedup();
        servers.shuffle(&mut rand::rng());
        if servers.is_empty() {
            bail!("No Radio Browser mirrors discovered; saved streams remain usable");
        }
        *self.mirrors.write().await = Some(MirrorCache {
            created: Instant::now(),
            servers: servers.clone(),
        });
        Ok(servers)
    }

    async fn get<T: DeserializeOwned>(
        &self,
        segments: &[&str],
        params: &[(String, String)],
    ) -> Result<T> {
        timeout(Duration::from_secs(12), async {
            let mut last_error = anyhow!("Radio Browser unavailable");
            for mut url in self.servers().await? {
                url.path_segments_mut()
                    .map_err(|_| anyhow!("Invalid mirror URL"))?
                    .clear()
                    .extend(segments);
                let result = async {
                    let response = self
                        .client
                        .get(url)
                        .query(params)
                        .send()
                        .await?
                        .error_for_status()?;
                    let mut chunks = response.bytes_stream();
                    let mut body = Vec::new();
                    while let Some(chunk) = chunks.next().await {
                        let chunk = chunk?;
                        if body.len() + chunk.len() > 8 * 1024 * 1024 {
                            bail!("Directory response exceeds 8 MiB");
                        }
                        body.extend_from_slice(&chunk);
                    }
                    Ok::<T, anyhow::Error>(serde_json::from_slice(&body)?)
                }
                .await;
                match result {
                    Ok(value) => return Ok(value),
                    Err(error) => last_error = error,
                }
            }
            // Rediscover after exhaustion instead of retaining a dead mirror set.
            *self.mirrors.write().await = None;
            Err(last_error)
        })
        .await
        .context("Radio Browser request timed out; saved streams remain usable")?
    }
}
