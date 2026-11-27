use std::{path::Path, time::Instant};

use anyhow::Result;
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    app::{App, Effect, Event},
    catalog::Catalog,
    player::Player,
    store::StateStore,
};

#[derive(PartialEq, Eq)]
pub(crate) enum Flow {
    Continue,
    Quit,
    Suspend,
}

pub(crate) struct Runtime {
    pub app: App,
    pub receiver: mpsc::UnboundedReceiver<Event>,
    sender: mpsc::UnboundedSender<Event>,
    store: StateStore,
    catalog: Catalog,
    player: Player,
    search: Option<JoinHandle<()>>,
    resolve: Option<JoinHandle<()>>,
    refresh: Option<JoinHandle<()>>,
    started: Instant,
}

impl Runtime {
    pub fn open(directory: &Path) -> Result<Self> {
        let store = StateStore::open(directory)?;
        let app = App::new(store.state().clone(), store.warning());
        let (sender, receiver) = mpsc::unbounded_channel();
        Ok(Self {
            app,
            receiver,
            sender,
            store,
            catalog: Catalog::new()?,
            player: Player::new(),
            search: None,
            resolve: None,
            refresh: None,
            started: Instant::now(),
        })
    }

    pub fn now(&self) -> u64 {
        self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
    }

    pub async fn execute(&mut self, effects: Vec<Effect>) -> Flow {
        for effect in effects {
            match effect {
                Effect::Save => {
                    if self.store.save(&self.app.saved).is_err() {
                        self.app.warning = self.store.warning();
                    }
                }
                Effect::Stop => {
                    if let Some(task) = self.resolve.take() {
                        task.abort();
                    }
                    self.player.stop().await;
                }
                Effect::Start {
                    generation,
                    url,
                    volume,
                } => {
                    if generation == self.app.generation() {
                        self.player
                            .start(generation, &url, volume, self.sender.clone())
                            .await;
                    }
                }
                Effect::SetVolume(volume) => self.player.volume(volume),
                Effect::Resolve {
                    generation,
                    station,
                } => {
                    if let Some(task) = self.resolve.take() {
                        task.abort();
                    }
                    let catalog = self.catalog.clone();
                    let sender = self.sender.clone();
                    self.resolve = Some(tokio::spawn(async move {
                        let url = catalog.resolve(&station).await;
                        let _ = sender.send(Event::Resolved { generation, url });
                    }));
                }
                Effect::Search { id, query, page } => {
                    if let Some(task) = self.search.take() {
                        task.abort();
                    }
                    let catalog = self.catalog.clone();
                    let sender = self.sender.clone();
                    self.search = Some(tokio::spawn(async move {
                        let result = catalog
                            .search(&query, page)
                            .await
                            .map_err(|e| e.to_string());
                        let _ = sender.send(Event::Search {
                            id,
                            query,
                            page,
                            result,
                        });
                    }));
                }
                Effect::Refresh => {
                    if self
                        .refresh
                        .as_ref()
                        .is_some_and(|task| !task.is_finished())
                    {
                        continue;
                    }
                    let ids: Vec<_> = self
                        .app
                        .saved
                        .favorites
                        .iter()
                        .map(|s| s.stationuuid.clone())
                        .collect();
                    if ids.is_empty() {
                        continue;
                    }
                    let catalog = self.catalog.clone();
                    let sender = self.sender.clone();
                    self.refresh = Some(tokio::spawn(async move {
                        let result = catalog.refresh(&ids).await.map_err(|e| e.to_string());
                        let _ = sender.send(Event::Refreshed(result));
                    }));
                }
                Effect::Quit => {
                    self.shutdown().await;
                    return Flow::Quit;
                }
                Effect::Suspend => return Flow::Suspend,
            }
        }
        Flow::Continue
    }

    pub async fn shutdown(&mut self) {
        for task in [&mut self.search, &mut self.resolve, &mut self.refresh] {
            if let Some(task) = task.take() {
                task.abort();
            }
        }
        self.player.stop().await;
    }
}
