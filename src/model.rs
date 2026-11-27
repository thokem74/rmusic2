use serde::{Deserialize, Serialize};

pub(crate) fn display_text(value: &str) -> String {
    strip_ansi_escapes::strip_str(value)
        .chars()
        .filter(|ch| !ch.is_control())
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Query {
    pub name: String,
    pub country: String,
    pub language: String,
    pub tag: String,
    pub hide_broken: bool,
}

impl Default for Query {
    fn default() -> Self {
        Self {
            name: String::new(),
            country: String::new(),
            language: String::new(),
            tag: String::new(),
            hide_broken: true,
        }
    }
}

impl Query {
    pub fn label(&self) -> String {
        let values: Vec<_> = [&self.name, &self.country, &self.language, &self.tag]
            .into_iter()
            .filter(|s| !s.is_empty())
            .map(String::as_str)
            .collect();
        if values.is_empty() {
            "Popular stations".into()
        } else {
            values.join(" / ")
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Station {
    pub stationuuid: String,
    pub name: String,
    pub url: String,
    pub url_resolved: String,
    pub countrycode: String,
    pub language: String,
    pub tags: String,
    pub codec: String,
    pub bitrate: u32,
    pub clickcount: u64,
}

impl Station {
    pub fn stream_url(&self) -> &str {
        if self.url_resolved.is_empty() {
            &self.url
        } else {
            &self.url_resolved
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SavedState {
    pub version: u32,
    pub volume: u8,
    pub favorites: Vec<Station>,
    pub last_station: Option<Station>,
}

impl Default for SavedState {
    fn default() -> Self {
        Self {
            version: 1,
            volume: 50,
            favorites: Vec::new(),
            last_station: None,
        }
    }
}
