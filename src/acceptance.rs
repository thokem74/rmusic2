use crate::app::{Action, Effect, Event, PlayerEvent};
use crate::model::Query;
use crate::{
    app::{App, Playback, View},
    model::SavedState,
};
use crate::{model::Station, store::StateStore};

#[test]
fn startup_is_silent_and_opens_favorites_at_default_volume() {
    let app = App::new(SavedState::default(), None);
    assert_eq!(app.view, View::Favorites);
    assert_eq!(app.playback, Playback::Stopped);
    assert_eq!(app.saved.volume, 50);
    assert!(app.metadata.is_none());
}

#[test]
fn highlighting_and_removing_a_favorite_do_not_interrupt_the_playing_station() {
    let mut app = App::new(
        SavedState {
            favorites: vec![station("a"), station("b")],
            ..SavedState::default()
        },
        None,
    );
    let effects = app.update(Action::Play, 0);
    assert!(effects.iter().any(|e| matches!(e, Effect::Resolve { .. })));
    let generation = app.generation();
    app.complete(
        Event::Resolved {
            generation,
            url: "https://example.test/live".into(),
        },
        1,
    );
    app.complete(
        Event::Player {
            generation,
            event: PlayerEvent::Ready,
        },
        2,
    );
    app.update(Action::Move(1), 3);
    assert_eq!(app.highlighted().unwrap().stationuuid, "b");
    assert_eq!(app.saved.last_station.as_ref().unwrap().stationuuid, "a");
    assert_eq!(app.playback, Playback::Playing);
    app.update(Action::Move(-1), 4);
    let effects = app.update(Action::ToggleFavorite, 5);
    assert!(!effects.iter().any(|e| matches!(e, Effect::Stop)));
    assert_eq!(app.playback, Playback::Playing);
    assert_eq!(app.saved.favorites.len(), 1);
}

fn station(id: &str) -> Station {
    Station {
        stationuuid: id.into(),
        name: format!("Station {id}"),
        url: "https://example.test/live".into(),
        ..Station::default()
    }
}

#[test]
fn saved_favorites_and_nonfavorite_last_station_survive_restart_without_autoplay() {
    let dir = tempfile::tempdir().unwrap();
    let state = SavedState {
        favorites: vec![station("b"), station("a")],
        last_station: Some(station("c")),
        volume: 75,
        ..SavedState::default()
    };
    {
        let mut store = StateStore::open(dir.path()).unwrap();
        store.save(&state).unwrap();
    }
    let store = StateStore::open(dir.path()).unwrap();
    let app = App::new(store.state().clone(), store.warning());
    assert_eq!(
        app.saved
            .favorites
            .iter()
            .map(|s| s.stationuuid.as_str())
            .collect::<Vec<_>>(),
        ["b", "a"]
    );
    assert_eq!(app.saved.last_station.as_ref().unwrap().stationuuid, "c");
    assert_eq!(app.saved.volume, 75);
    assert_eq!(app.playback, Playback::Stopped);
}

#[test]
fn submitted_search_ignores_old_responses_and_preserves_results_after_failure() {
    let mut app = App::new(SavedState::default(), None);
    app.update(Action::OpenSearch, 0);
    app.update(Action::Input('q'), 0);
    assert_eq!(app.draft.name, "q");
    assert!(app.form_open);
    assert_eq!(app.playback, Playback::Stopped);
    let effects = app.update(Action::SubmitSearch, 1);
    let first = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Search { id, .. } => Some(*id),
            _ => None,
        })
        .unwrap();
    app.complete(
        Event::Search {
            id: first,
            query: Query {
                name: "q".into(),
                ..Query::default()
            },
            page: 0,
            result: Ok(vec![station("a")]),
        },
        2,
    );
    app.update(Action::OpenSearch, 3);
    app.update(Action::Input('f'), 3);
    let effects = app.update(Action::SubmitSearch, 4);
    let second = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Search { id, .. } => Some(*id),
            _ => None,
        })
        .unwrap();
    app.complete(
        Event::Search {
            id: first,
            query: Query::default(),
            page: 0,
            result: Ok(vec![station("old")]),
        },
        5,
    );
    app.complete(
        Event::Search {
            id: second,
            query: app.draft.clone(),
            page: 0,
            result: Err("directory offline".into()),
        },
        6,
    );
    assert_eq!(app.stations()[0].stationuuid, "a");
    assert_eq!(app.results_query.as_ref().unwrap().name, "q");
    assert!(app.notice.contains("qf"));
    assert_eq!(app.playback, Playback::Stopped);
}

#[test]
fn recovery_is_bounded_and_stop_cancels_late_player_and_resolution_events() {
    let mut app = App::new(
        SavedState {
            favorites: vec![station("a")],
            ..SavedState::default()
        },
        None,
    );
    app.update(Action::Play, 0);
    let initial = app.generation();
    app.complete(
        Event::Resolved {
            generation: initial,
            url: "https://example.test/live".into(),
        },
        0,
    );
    app.complete(
        Event::Player {
            generation: initial,
            event: PlayerEvent::Failed("offline".into()),
        },
        0,
    );
    for (index, due) in [1000, 3000, 7000].into_iter().enumerate() {
        assert_eq!(app.playback, Playback::Retrying);
        assert!(app.tick(due - 1).is_empty());
        let effects = app.tick(due);
        assert!(
            effects.iter().any(|e| matches!(e, Effect::Start { .. })),
            "retry {index}"
        );
        let generation = app.generation();
        app.complete(
            Event::Player {
                generation,
                event: PlayerEvent::Failed("offline".into()),
            },
            due,
        );
    }
    assert_eq!(app.playback, Playback::Failed);
    assert!(app.tick(100_000).is_empty());
    app.update(Action::TogglePlayback, 100_000);
    assert_eq!(app.playback, Playback::Connecting);
    let abandoned = app.generation();
    app.update(Action::TogglePlayback, 100_001);
    assert_eq!(app.playback, Playback::Stopped);
    assert!(
        app.complete(
            Event::Resolved {
                generation: abandoned,
                url: "https://example.test/late".into()
            },
            100_002
        )
        .is_empty()
    );
    app.complete(
        Event::Player {
            generation: abandoned,
            event: PlayerEvent::Ready,
        },
        100_003,
    );
    assert_eq!(app.playback, Playback::Stopped);
}

#[test]
fn real_progress_resets_the_budget_but_metadata_cannot_hide_a_stall() {
    let mut app = App::new(
        SavedState {
            favorites: vec![station("a")],
            ..SavedState::default()
        },
        None,
    );
    app.update(Action::Play, 0);
    let generation = app.generation();
    app.complete(
        Event::Resolved {
            generation,
            url: "https://example.test/live".into(),
        },
        0,
    );
    app.complete(
        Event::Player {
            generation,
            event: PlayerEvent::Ready,
        },
        0,
    );
    for seconds in 1..=31 {
        app.complete(
            Event::Player {
                generation,
                event: PlayerEvent::Progress(seconds as f64),
            },
            seconds * 1000,
        );
        assert!(app.tick(seconds * 1000).is_empty());
    }
    app.complete(
        Event::Player {
            generation,
            event: PlayerEvent::Metadata(Some("\x1b[31mSong\x1b[0m\x07".into())),
        },
        45_999,
    );
    assert_eq!(app.metadata.as_deref(), Some("Song"));
    app.tick(46_000);
    assert_eq!(app.playback, Playback::Retrying);
    assert!(app.metadata.is_none());
    app.tick(47_000);
    assert_eq!(app.playback, Playback::Connecting);
}

async fn http_fixture(status: &str, body: String) -> (String, tokio::task::JoinHandle<String>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let status = status.to_owned();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        loop {
            let mut bytes = [0; 1024];
            let count = stream.read(&mut bytes).await.unwrap();
            if count == 0 {
                break;
            }
            request.extend_from_slice(&bytes[..count]);
            if request.windows(4).any(|s| s == b"\r\n\r\n") {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        String::from_utf8(request).unwrap()
    });
    (address, task)
}

#[tokio::test]
async fn directory_failover_preserves_exact_filters_and_pagination_on_the_wire() {
    let (bad, first) = http_fixture("503 Unavailable", "{}".into()).await;
    let (good, second) = http_fixture(
        "200 OK",
        serde_json::to_string(&vec![station("a")]).unwrap(),
    )
    .await;
    let catalog = crate::catalog::Catalog::with_servers(vec![bad, good]).unwrap();
    let query = Query {
        name: "Mira & jazz".into(),
        country: "de".into(),
        language: "german".into(),
        tag: "jazz".into(),
        hide_broken: true,
    };
    let result = catalog.search(&query, 1).await.unwrap();
    assert_eq!(result[0].stationuuid, "a");
    assert!(
        first
            .await
            .unwrap()
            .starts_with("GET /json/stations/search?")
    );
    let request = second.await.unwrap();
    let target = request.split_whitespace().nth(1).unwrap();
    let url = reqwest::Url::parse(&format!("http://fixture{target}")).unwrap();
    let params: std::collections::HashMap<_, _> = url.query_pairs().collect();
    for (key, value) in [
        ("name", "Mira & jazz"),
        ("countrycode", "DE"),
        ("languageExact", "true"),
        ("tagExact", "true"),
        ("hidebroken", "true"),
        ("order", "clickcount"),
        ("reverse", "true"),
        ("limit", "50"),
        ("offset", "50"),
    ] {
        assert_eq!(params.get(key).map(|v| v.as_ref()), Some(value), "{key}");
    }
    assert!(request.to_lowercase().contains("user-agent: rmusic/"));
}

#[test]
fn layout_a_renders_at_80_by_24_and_text_fields_do_not_trigger_shortcuts() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::{Terminal, backend::TestBackend};
    let mut app = App::new(
        SavedState {
            favorites: vec![station("a")],
            ..SavedState::default()
        },
        None,
    );
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| crate::ui::render(frame, &app))
        .unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    for label in [
        "Favorites",
        "Search",
        "Highlighted",
        "Station a",
        "Stopped",
        "Space",
    ] {
        assert!(text.contains(label), "{label} missing");
    }
    app.key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE), 0);
    for ch in ['q', 'f', ' '] {
        let effects = app.key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE), 1);
        assert!(effects.is_empty());
    }
    assert_eq!(app.draft.name, "qf ");
    assert_eq!(app.saved.favorites.len(), 1);
    terminal.backend_mut().resize(60, 12);
    terminal.autoresize().unwrap();
    terminal
        .draw(|frame| crate::ui::render(frame, &app))
        .unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("80x24"));
    assert_eq!(app.draft.name, "qf ");
}

#[tokio::test]
async fn missing_player_fails_immediately_without_scheduled_retries() {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut player = crate::player::Player::with_program("/definitely/missing/rmusic-mpv");
    let mut app = App::new(
        SavedState {
            favorites: vec![station("a")],
            ..SavedState::default()
        },
        None,
    );
    app.update(Action::Play, 0);
    let generation = app.generation();
    player
        .start(generation, "https://example.test/live", 50, sender)
        .await;
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    app.complete(event, 0);
    assert_eq!(app.playback, Playback::Failed);
    assert!(app.notice.contains("mpv"));
    assert!(app.tick(100_000).is_empty());
    player.stop().await;
}

#[test]
fn shrinking_with_a_filter_open_keeps_stop_and_quit_available_without_losing_the_draft() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut app = App::new(
        SavedState {
            favorites: vec![station("a")],
            ..SavedState::default()
        },
        None,
    );
    app.update(Action::Play, 0);
    let generation = app.generation();
    app.complete(
        Event::Player {
            generation,
            event: PlayerEvent::Ready,
        },
        1,
    );
    app.update(Action::OpenSearch, 2);
    app.update(Action::Input('q'), 2);
    app.update(Action::Resize(60, 12), 3);
    app.key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE), 4);
    assert_eq!(app.playback, Playback::Stopped);
    assert_eq!(app.draft.name, "q");
    let effects = app.key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE), 5);
    assert!(effects.iter().any(|e| matches!(e, Effect::Quit)));
    app.update(Action::Resize(80, 24), 6);
    assert!(app.form_open);
    assert_eq!(app.draft.name, "q");
}

#[test]
fn damaged_state_is_preserved_and_a_second_instance_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let original = b"{damaged favorite collection";
    std::fs::write(dir.path().join("state.json"), original).unwrap();
    let mut store = StateStore::open(dir.path()).unwrap();
    assert!(store.warning().unwrap().contains("Changes cannot be saved"));
    assert!(store.save(&SavedState::default()).is_err());
    assert_eq!(
        std::fs::read(dir.path().join("state.json")).unwrap(),
        original
    );
    assert!(
        StateStore::open(dir.path())
            .err()
            .unwrap()
            .to_string()
            .contains("Another rmusic")
    );
    drop(store);
    assert!(StateStore::open(dir.path()).is_ok());
}

#[test]
fn refresh_preserves_favorite_identity_order_and_usable_addresses() {
    let mut app = App::new(
        SavedState {
            favorites: vec![station("b"), station("a")],
            ..SavedState::default()
        },
        None,
    );
    let mut fresh = station("b");
    fresh.name = "Updated name".into();
    fresh.url.clear();
    app.complete(Event::Refreshed(Ok(vec![fresh, station("unrelated")])), 0);
    assert_eq!(
        app.saved
            .favorites
            .iter()
            .map(|s| s.stationuuid.as_str())
            .collect::<Vec<_>>(),
        ["b", "a"]
    );
    assert_eq!(app.saved.favorites[0].name, "Updated name");
    assert_eq!(
        app.saved.favorites[0].stream_url(),
        "https://example.test/live"
    );
    app.complete(Event::Refreshed(Ok(Vec::new())), 1);
    assert_eq!(app.saved.favorites.len(), 2);
}

#[tokio::test]
async fn explicit_reporting_failure_falls_back_to_the_saved_stream_address() {
    let (server, request) = http_fixture("503 Unavailable", "{}".into()).await;
    let catalog = crate::catalog::Catalog::with_servers(vec![server]).unwrap();
    assert_eq!(
        catalog.resolve(&station("a")).await,
        "https://example.test/live"
    );
    assert!(request.await.unwrap().starts_with("GET /json/url/a "));
}

#[test]
fn entering_the_playing_station_is_a_noop_and_volume_is_app_only_and_bounded() {
    let mut app = App::new(
        SavedState {
            favorites: vec![station("a")],
            ..SavedState::default()
        },
        None,
    );
    app.update(Action::Play, 0);
    let generation = app.generation();
    app.complete(
        Event::Player {
            generation,
            event: PlayerEvent::Ready,
        },
        0,
    );
    assert!(app.update(Action::Play, 1).is_empty());
    for _ in 0..30 {
        app.update(Action::Volume(5), 2);
    }
    assert_eq!(app.saved.volume, 100);
    for _ in 0..30 {
        app.update(Action::Volume(-5), 3);
    }
    assert_eq!(app.saved.volume, 0);
    assert_eq!(app.playback, Playback::Playing);
    assert_eq!(app.generation(), generation);
}

#[test]
fn only_thirty_seconds_of_stable_playback_replenishes_retries() {
    let mut app = App::new(
        SavedState {
            favorites: vec![station("a")],
            ..SavedState::default()
        },
        None,
    );
    app.update(Action::Play, 0);
    let generation = app.generation();
    app.complete(
        Event::Player {
            generation,
            event: PlayerEvent::Failed("drop".into()),
        },
        0,
    );
    app.tick(1000);
    let generation = app.generation();
    app.complete(
        Event::Player {
            generation,
            event: PlayerEvent::Ready,
        },
        1000,
    );
    for second in 1..=31 {
        app.complete(
            Event::Player {
                generation,
                event: PlayerEvent::Progress(second as f64),
            },
            1000 + second * 1000,
        );
        app.tick(1000 + second * 1000);
    }
    app.complete(
        Event::Player {
            generation,
            event: PlayerEvent::Failed("drop".into()),
        },
        32_001,
    );
    assert!(app.tick(33_000).is_empty());
    assert!(
        app.tick(33_001)
            .iter()
            .any(|e| matches!(e, Effect::Start { .. }))
    );
    let generation = app.generation();
    app.complete(
        Event::Player {
            generation,
            event: PlayerEvent::Ready,
        },
        33_002,
    );
    app.complete(
        Event::Player {
            generation,
            event: PlayerEvent::Failed("brief recovery".into()),
        },
        33_003,
    );
    assert!(app.tick(34_003).is_empty());
    assert!(
        app.tick(35_003)
            .iter()
            .any(|e| matches!(e, Effect::Start { .. }))
    );
}

#[test]
fn pagination_uses_the_submitted_query_and_a_new_search_starts_at_page_one() {
    let mut app = App::new(SavedState::default(), None);
    app.update(Action::OpenSearch, 0);
    let effects = app.update(Action::SubmitSearch, 0);
    let id = effects
        .iter()
        .find_map(|e| match e {
            Effect::Search { id, .. } => Some(*id),
            _ => None,
        })
        .unwrap();
    app.complete(
        Event::Search {
            id,
            query: Query::default(),
            page: 0,
            result: Ok((0..50).map(|i| station(&i.to_string())).collect()),
        },
        0,
    );
    let effects = app.update(Action::Page(1), 1);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Search { page: 1, .. }))
    );
    let id = effects
        .iter()
        .find_map(|e| match e {
            Effect::Search { id, .. } => Some(*id),
            _ => None,
        })
        .unwrap();
    app.complete(
        Event::Search {
            id,
            query: Query::default(),
            page: 1,
            result: Ok(vec![station("last")]),
        },
        2,
    );
    assert!(app.update(Action::Page(1), 3).is_empty());
    app.update(Action::OpenSearch, 4);
    app.update(Action::Input('j'), 4);
    let effects = app.update(Action::SubmitSearch, 5);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Search { page: 0, query, .. } if query.name == "j"))
    );
}

#[test]
fn station_change_ignores_old_metadata_and_suspend_cancels_playback() {
    let mut app = App::new(
        SavedState {
            favorites: vec![station("a"), station("b")],
            ..SavedState::default()
        },
        None,
    );
    app.update(Action::Play, 0);
    let old = app.generation();
    app.complete(
        Event::Player {
            generation: old,
            event: PlayerEvent::Ready,
        },
        0,
    );
    app.complete(
        Event::Player {
            generation: old,
            event: PlayerEvent::Metadata(Some("Old track".into())),
        },
        0,
    );
    app.update(Action::Move(1), 1);
    app.update(Action::Play, 2);
    assert!(app.metadata.is_none());
    app.complete(
        Event::Player {
            generation: old,
            event: PlayerEvent::Metadata(Some("Late old track".into())),
        },
        3,
    );
    assert!(app.metadata.is_none());
    assert_eq!(app.saved.last_station.as_ref().unwrap().stationuuid, "b");
    let effects = app.update(Action::Suspend, 4);
    assert_eq!(app.playback, Playback::Stopped);
    assert!(effects.iter().any(|e| matches!(e, Effect::Suspend)));
    assert!(app.tick(100_000).is_empty());
}

#[test]
fn long_station_names_never_hide_status_or_volume_at_minimum_size() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut last = station("a");
    last.name = "A".repeat(100);
    let app = App::new(
        SavedState {
            last_station: Some(last),
            volume: 75,
            ..SavedState::default()
        },
        None,
    );
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| crate::ui::render(frame, &app))
        .unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("Stopped"));
    assert!(
        text.contains("Volume 75%"),
        "volume was clipped by station name"
    );
}

#[test]
fn elapsed_time_without_audio_progress_does_not_replenish_the_retry_budget() {
    let mut app = App::new(
        SavedState {
            favorites: vec![station("a")],
            ..SavedState::default()
        },
        None,
    );
    app.update(Action::Play, 0);
    app.complete(
        Event::Player {
            generation: app.generation(),
            event: PlayerEvent::Failed("drop".into()),
        },
        0,
    );
    app.tick(1000);
    app.complete(
        Event::Player {
            generation: app.generation(),
            event: PlayerEvent::Ready,
        },
        1000,
    );
    app.tick(31_000);
    assert_eq!(app.playback, Playback::Retrying);
    assert!(
        app.tick(32_000).is_empty(),
        "stalled audio incorrectly reset the retry budget"
    );
    assert!(
        app.tick(33_000)
            .iter()
            .any(|effect| matches!(effect, Effect::Start { .. }))
    );
}

#[tokio::test]
#[ignore = "requires installed mpv; run explicitly for the real player smoke check"]
async fn real_mpv_plays_local_audio_and_is_reaped_on_stop() {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let directory = tempfile::tempdir().unwrap();
    let pid_file = directory.path().join("player.pid");
    let wrapper = directory.path().join("mpv-null-audio");
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\necho $$ > '{}'\nexec mpv --ao=null \"$@\"\n",
            pid_file.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/silence.wav", listener.local_addr().unwrap());
    let audio = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 8192];
        let _ = socket.read(&mut request).await;
        let data_length = 44_100_u32 * 2 * 20;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(data_length + 36).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&44_100_u32.to_le_bytes());
        wav.extend_from_slice(&88_200_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_length.to_le_bytes());
        wav.resize(wav.len() + data_length as usize, 0);
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            wav.len()
        );
        let _ = socket.write_all(header.as_bytes()).await;
        let _ = socket.write_all(&wav).await;
    });
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut player = crate::player::Player::with_program(wrapper);
    player.start(1, &url, 50, sender).await;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match receiver.recv().await.unwrap() {
                Event::Player {
                    event: PlayerEvent::Ready,
                    ..
                } => break,
                Event::Player {
                    event: PlayerEvent::Failed(error),
                    ..
                } => panic!("{error}"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    let pid: i32 = std::fs::read_to_string(pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    player.volume(0);
    player.stop().await;
    assert_eq!(
        nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
        Err(nix::errno::Errno::ESRCH)
    );
    audio.abort();
}
