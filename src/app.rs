use crate::model::{Query, SavedState, Station, display_text};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum View {
    Favorites,
    Search,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Playback {
    Stopped,
    Connecting,
    Playing,
    Retrying,
    Failed,
}

#[derive(Debug)]
pub(crate) enum Action {
    Move(i32),
    Play,
    ToggleFavorite,
    OpenSearch,
    Input(char),
    Backspace,
    NextField,
    SubmitSearch,
    CloseOverlay,
    SwitchView,
    Page(i32),
    TogglePlayback,
    Volume(i8),
    Help,
    Quit,
    Suspend,
    Resize(u16, u16),
}

#[derive(Debug)]
pub(crate) enum Effect {
    Stop,
    Resolve {
        generation: u64,
        station: Station,
    },
    Start {
        generation: u64,
        url: String,
        volume: u8,
    },
    Save,
    Search {
        id: u64,
        query: Query,
        page: usize,
    },
    SetVolume(u8),
    Refresh,
    Quit,
    Suspend,
}

#[derive(Debug)]
pub(crate) enum PlayerEvent {
    Ready,
    Failed(String),
    Missing,
    Progress(f64),
    Metadata(Option<String>),
    Buffering(bool),
}

#[derive(Debug)]
pub(crate) enum Event {
    Resolved {
        generation: u64,
        url: String,
    },
    Player {
        generation: u64,
        event: PlayerEvent,
    },
    Search {
        id: u64,
        query: Query,
        page: usize,
        result: Result<Vec<Station>, String>,
    },
    Refreshed(Result<Vec<Station>, String>),
}

pub(crate) struct App {
    pub view: View,
    pub playback: Playback,
    pub saved: SavedState,
    pub metadata: Option<String>,
    pub warning: Option<String>,
    pub selected: usize,
    pub draft: Query,
    pub form_open: bool,
    pub field: usize,
    pub results_query: Option<Query>,
    pub page: usize,
    pub loading: bool,
    pub notice: String,
    pub help_open: bool,
    pub too_small: bool,
    results: Vec<Station>,
    search_id: u64,
    generation: u64,
    retries: usize,
    retry_at: Option<u64>,
    last_progress: u64,
    stable_since: Option<u64>,
    position: Option<f64>,
    buffering: bool,
}

impl App {
    pub fn new(saved: SavedState, warning: Option<String>) -> Self {
        Self {
            view: View::Favorites,
            playback: Playback::Stopped,
            saved,
            metadata: None,
            warning,
            selected: 0,
            generation: 0,
            draft: Query::default(),
            form_open: false,
            field: 0,
            results_query: None,
            page: 0,
            loading: false,
            notice: String::new(),
            results: Vec::new(),
            search_id: 0,
            retries: 0,
            retry_at: None,
            last_progress: 0,
            stable_since: None,
            position: None,
            help_open: false,
            buffering: false,
            too_small: false,
        }
    }

    pub fn stations(&self) -> &[Station] {
        match self.view {
            View::Favorites => &self.saved.favorites,
            View::Search => &self.results,
        }
    }
    pub fn highlighted(&self) -> Option<&Station> {
        self.stations().get(self.selected)
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn update(&mut self, action: Action, now: u64) -> Vec<Effect> {
        match action {
            Action::Resize(width, height) => {
                self.too_small = width < 80 || height < 24;
                Vec::new()
            }
            Action::Help => {
                self.help_open = !self.help_open;
                Vec::new()
            }
            Action::Quit | Action::Suspend => {
                let mut effects = self.stop();
                effects.push(if matches!(action, Action::Quit) {
                    Effect::Quit
                } else {
                    Effect::Suspend
                });
                effects
            }
            Action::Volume(delta) => {
                self.saved.volume =
                    (i16::from(self.saved.volume) + i16::from(delta)).clamp(0, 100) as u8;
                vec![Effect::SetVolume(self.saved.volume), Effect::Save]
            }
            Action::OpenSearch => {
                self.view = View::Search;
                self.form_open = true;
                self.field = 0;
                Vec::new()
            }
            Action::Input(ch) => {
                if self.form_open {
                    match self.field {
                        0 if self.draft.name.len() < 1024 => self.draft.name.push(ch),
                        1 if self.draft.country.len() < 32 => self.draft.country.push(ch),
                        2 if self.draft.language.len() < 1024 => self.draft.language.push(ch),
                        3 if self.draft.tag.len() < 1024 => self.draft.tag.push(ch),
                        4 if ch == ' ' => self.draft.hide_broken = !self.draft.hide_broken,
                        _ => {}
                    }
                }
                Vec::new()
            }
            Action::Backspace => {
                let field = match self.field {
                    0 => Some(&mut self.draft.name),
                    1 => Some(&mut self.draft.country),
                    2 => Some(&mut self.draft.language),
                    3 => Some(&mut self.draft.tag),
                    _ => None,
                };
                if let Some(field) = field {
                    field.pop();
                }
                Vec::new()
            }
            Action::NextField => {
                self.field = (self.field + 1) % 5;
                Vec::new()
            }
            Action::CloseOverlay => {
                self.form_open = false;
                self.help_open = false;
                Vec::new()
            }
            Action::SwitchView => {
                self.view = if self.view == View::Favorites {
                    View::Search
                } else {
                    View::Favorites
                };
                self.selected = 0;
                if self.view == View::Favorites {
                    vec![Effect::Refresh]
                } else {
                    Vec::new()
                }
            }
            Action::SubmitSearch => {
                let country = self.draft.country.trim();
                if !country.is_empty()
                    && (country.len() != 2 || !country.chars().all(|ch| ch.is_ascii_alphabetic()))
                {
                    self.notice = "Country must be a two-letter code, for example DE.".into();
                    return Vec::new();
                }
                self.form_open = false;
                self.view = View::Search;
                self.search(self.draft.clone(), 0)
            }
            Action::Page(delta) => {
                if self.view != View::Search || self.loading {
                    return Vec::new();
                }
                if delta > 0 && self.results.len() < 50 {
                    return Vec::new();
                }
                let page = self.page.saturating_add_signed(delta as isize);
                if page == self.page {
                    return Vec::new();
                }
                self.search(self.results_query.clone().unwrap_or_default(), page)
            }
            Action::Move(delta) => {
                self.selected = (self.selected as i64 + i64::from(delta))
                    .clamp(0, self.stations().len().saturating_sub(1) as i64)
                    as usize;
                Vec::new()
            }
            Action::Play => {
                let Some(station) = self.highlighted().cloned() else {
                    return Vec::new();
                };
                if self.playback == Playback::Playing
                    && self
                        .saved
                        .last_station
                        .as_ref()
                        .is_some_and(|s| s.stationuuid == station.stationuuid)
                {
                    return Vec::new();
                }
                self.begin(station, now)
            }
            Action::TogglePlayback => {
                if matches!(
                    self.playback,
                    Playback::Connecting | Playback::Playing | Playback::Retrying
                ) {
                    self.stop()
                } else if let Some(station) = self.saved.last_station.clone() {
                    self.begin(station, now)
                } else {
                    Vec::new()
                }
            }
            Action::ToggleFavorite => {
                let Some(station) = self.highlighted().cloned() else {
                    return Vec::new();
                };
                if let Some(index) = self
                    .saved
                    .favorites
                    .iter()
                    .position(|s| s.stationuuid == station.stationuuid)
                {
                    self.saved.favorites.remove(index);
                } else {
                    self.saved.favorites.push(station);
                }
                self.selected = self.selected.min(self.stations().len().saturating_sub(1));
                vec![Effect::Save]
            }
        }
    }

    pub fn complete(&mut self, event: Event, now: u64) -> Vec<Effect> {
        match event {
            Event::Refreshed(result) => {
                match result {
                    Ok(stations) => {
                        for fresh in stations {
                            if let Some(old) = self
                                .saved
                                .favorites
                                .iter_mut()
                                .find(|s| s.stationuuid == fresh.stationuuid)
                            {
                                let mut fresh = fresh;
                                if fresh.stream_url().is_empty() {
                                    fresh.url.clone_from(&old.url);
                                    fresh.url_resolved.clone_from(&old.url_resolved);
                                }
                                if fresh.name.is_empty() {
                                    fresh.name.clone_from(&old.name);
                                }
                                *old = fresh;
                            }
                        }
                        return vec![Effect::Save];
                    }
                    Err(error) => {
                        self.notice = format!(
                            "Directory refresh unavailable: {}. Saved streams remain usable.",
                            display_text(&error)
                        )
                    }
                }
                Vec::new()
            }
            Event::Search {
                id,
                query,
                page,
                result,
            } if id == self.search_id => {
                self.loading = false;
                match result {
                    Ok(stations) => {
                        self.results = stations;
                        self.results_query = Some(query);
                        self.page = page;
                        self.selected = 0;
                        self.notice.clear();
                    }
                    Err(error) => {
                        self.notice = format!(
                            "Search failed for {}: {error}. Previous results retained.",
                            query.label()
                        )
                    }
                }
                Vec::new()
            }
            Event::Resolved { generation, url } if generation == self.generation => {
                if let Some(station) = &mut self.saved.last_station {
                    station.url_resolved = url.clone();
                }
                vec![
                    Effect::Save,
                    Effect::Start {
                        generation,
                        url,
                        volume: self.saved.volume,
                    },
                ]
            }
            Event::Player {
                generation,
                event: PlayerEvent::Ready,
            } if generation == self.generation => {
                self.playback = Playback::Playing;
                self.last_progress = now;
                self.stable_since = Some(now);
                Vec::new()
            }
            Event::Player {
                generation,
                event: PlayerEvent::Progress(position),
            } if generation == self.generation => {
                if !self.buffering && position.is_finite() && self.position != Some(position) {
                    self.position = Some(position);
                    self.last_progress = now;
                    self.stable_since.get_or_insert(now);
                }
                Vec::new()
            }
            Event::Player {
                generation,
                event: PlayerEvent::Buffering(buffering),
            } if generation == self.generation => {
                self.buffering = buffering;
                if buffering {
                    self.stable_since = None;
                }
                Vec::new()
            }
            Event::Player {
                generation,
                event: PlayerEvent::Missing,
            } if generation == self.generation => {
                let effects = self.stop();
                self.playback = Playback::Failed;
                self.notice = "mpv is missing. Install it with: sudo pacman -S mpv".into();
                effects
            }
            Event::Player {
                generation,
                event: PlayerEvent::Metadata(metadata),
            } if generation == self.generation => {
                self.metadata = metadata
                    .map(|text| display_text(&text))
                    .filter(|text| !text.is_empty());
                Vec::new()
            }
            Event::Player {
                generation,
                event: PlayerEvent::Failed(error),
            } if generation == self.generation => self.fail(error, now),
            _ => Vec::new(),
        }
    }

    pub fn tick(&mut self, now: u64) -> Vec<Effect> {
        if self.playback == Playback::Playing
            && self
                .stable_since
                .is_some_and(|start| now.saturating_sub(start) >= 30_000)
        {
            self.retries = 0;
            self.stable_since = Some(now);
        }
        if self.playback == Playback::Retrying && self.retry_at.is_some_and(|due| now >= due) {
            self.retry_at = None;
            self.generation += 1;
            self.playback = Playback::Connecting;
            self.last_progress = now;
            if let Some(station) = &self.saved.last_station {
                return vec![Effect::Start {
                    generation: self.generation,
                    url: station.stream_url().into(),
                    volume: self.saved.volume,
                }];
            }
        }
        if matches!(self.playback, Playback::Connecting | Playback::Playing)
            && now.saturating_sub(self.last_progress) >= 15_000
        {
            return self.fail("Connection timed out or stream stalled".into(), now);
        }
        Vec::new()
    }

    fn begin(&mut self, station: Station, now: u64) -> Vec<Effect> {
        self.generation += 1;
        self.retries = 0;
        self.retry_at = None;
        self.saved.last_station = Some(station.clone());
        self.metadata = None;
        self.stable_since = None;
        self.position = None;
        self.buffering = false;
        self.notice.clear();
        self.playback = Playback::Connecting;
        self.last_progress = now;
        vec![
            Effect::Stop,
            Effect::Save,
            Effect::Resolve {
                generation: self.generation,
                station,
            },
        ]
    }

    fn stop(&mut self) -> Vec<Effect> {
        self.generation += 1;
        self.playback = Playback::Stopped;
        self.retry_at = None;
        self.metadata = None;
        self.stable_since = None;
        vec![Effect::Stop]
    }

    fn fail(&mut self, error: String, now: u64) -> Vec<Effect> {
        self.generation += 1;
        self.metadata = None;
        self.stable_since = None;
        self.position = None;
        self.buffering = false;
        self.notice = display_text(&error);
        if let Some(delay) = [1000, 2000, 4000].get(self.retries) {
            self.retry_at = Some(now.saturating_add(*delay));
            self.retries += 1;
            self.playback = Playback::Retrying;
        } else {
            self.playback = Playback::Failed;
            self.retry_at = None;
        }
        vec![Effect::Stop]
    }

    fn search(&mut self, query: Query, page: usize) -> Vec<Effect> {
        self.search_id += 1;
        self.loading = true;
        self.notice.clear();
        vec![Effect::Search {
            id: self.search_id,
            query,
            page,
        }]
    }
}
