use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Row, Table, TableState, Wrap},
};

use crate::{
    app::{App, Playback, View},
    model::display_text,
};

fn panel(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::DarkGray))
}

pub(crate) fn render(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    if area.width < 80 || area.height < 24 {
        frame.render_widget(Paragraph::new("rmusic needs at least 80x24. Resize to continue browsing.\nSpace stop/reconnect · +/- volume · q or Ctrl+C quit").wrap(Wrap { trim: false }).block(panel(" rmusic ")), area);
        return;
    }
    let notice = !app.notice.is_empty();
    let warning = app.warning.is_some();
    let regions = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(if warning { 1 } else { 0 }),
        Constraint::Length(if notice { 2 } else { 0 }),
        Constraint::Length(if app.form_open { 8 } else { 0 }),
        Constraint::Min(4),
        Constraint::Length(4),
        Constraint::Length(2),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " rmusic ",
                Style::default()
                    .fg(Color::LightGreen)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("/ internet radio · Radio Browser"),
        ])),
        regions[0],
    );
    let tab = |label: String, active: bool| {
        Span::styled(
            label,
            if active {
                Style::default().bg(Color::LightGreen).fg(Color::Black)
            } else {
                Style::default().fg(Color::Gray)
            },
        )
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            tab(
                format!(" Favorites [{}] ", app.saved.favorites.len()),
                app.view == View::Favorites,
            ),
            Span::raw(" "),
            tab(" Search ".into(), app.view == View::Search),
            Span::raw("  Tab switch · / filters"),
        ])),
        regions[1],
    );
    if let Some(warning) = &app.warning {
        frame.render_widget(
            Paragraph::new(display_text(warning)).style(Style::default().fg(Color::Yellow)),
            regions[2],
        );
    }
    if notice {
        frame.render_widget(
            Paragraph::new(display_text(&app.notice))
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: true }),
            regions[3],
        );
    }
    if app.form_open {
        render_form(frame, regions[4], app);
    }
    let parts = Layout::horizontal([Constraint::Min(35), Constraint::Length(29)]).split(regions[5]);
    render_table(frame, parts[0], app);
    render_inspector(frame, parts[1], app);
    render_player(frame, regions[6], app);
    frame.render_widget(Paragraph::new(" ↑↓ j/k move · Enter play · Space stop/reconnect · f favorite · +/- volume\n / search · Tab switch · PgUp/PgDn page · ? help · q quit · Ctrl+Z suspend").style(Style::default().fg(Color::Gray)), regions[7]);
    if app.help_open {
        let popup = Rect::new(area.x + 8, area.y + 3, area.width - 16, area.height - 6);
        frame.render_widget(Clear, popup);
        frame.render_widget(Paragraph::new("Navigate: ↑↓ or j/k. Highlighting never changes playback.\nEnter plays the highlighted station.\nSpace stops/cancels or reconnects the last station live.\nf toggles a favorite. Removing it does not stop audio.\n+/- changes only app volume.\nTab switches Favorites/Search. / opens search filters.\nIn filters: Tab next field, Enter submit, Escape close.\nPgUp/PgDn browse search pages.\nCtrl+Z suspends silently; fg returns without autoplay.\nq or Ctrl+C quits and stops the player.\n\n? or Escape closes help.").block(panel(" Keyboard guide ")).wrap(Wrap { trim: true }), popup);
    }
}

fn render_form(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let values = [
        format!("Name         {}", display_text(&app.draft.name)),
        format!("Country code {}", display_text(&app.draft.country)),
        format!("Language     {}", display_text(&app.draft.language)),
        format!("Tag          {}", display_text(&app.draft.tag)),
        format!(
            "Hide broken  [{}]  Space toggles",
            if app.draft.hide_broken { "x" } else { " " }
        ),
    ];
    let mut lines: Vec<Line<'_>> = values
        .into_iter()
        .enumerate()
        .map(|(i, value)| {
            Line::styled(
                format!("{} {value}", if i == app.field { ">" } else { " " }),
                if i == app.field {
                    Style::default().fg(Color::LightGreen)
                } else {
                    Style::default()
                },
            )
        })
        .collect();
    lines.push(Line::styled(
        " Tab next · Enter search · Esc close · exact country/language/tag",
        Style::default().fg(Color::Gray),
    ));
    frame.render_widget(Paragraph::new(lines).block(panel(" Search filters ")), area);
}

fn render_table(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let title = match app.view {
        View::Favorites => format!(" Favorites · {} saved ", app.stations().len()),
        View::Search => format!(
            " Search · {} · page {}{} ",
            app.results_query
                .as_ref()
                .map(|q| display_text(&q.label()))
                .unwrap_or_else(|| "not submitted".into()),
            app.page + 1,
            if app.loading { " · loading" } else { "" }
        ),
    };
    let wide = area.width >= 70;
    let headers = if wide {
        vec!["", "STATION", "CC", "KBPS", "TAG", "CLICKS"]
    } else {
        vec!["", "STATION", "CC", "KBPS"]
    };
    let rows = app.stations().iter().map(|station| {
        let playing = app.playback == Playback::Playing
            && app
                .saved
                .last_station
                .as_ref()
                .is_some_and(|s| s.stationuuid == station.stationuuid);
        let favorite = app
            .saved
            .favorites
            .iter()
            .any(|s| s.stationuuid == station.stationuuid);
        let marker = format!(
            "{}{}",
            if playing { ">" } else { " " },
            if favorite { "*" } else { " " }
        );
        let mut cells = vec![
            marker,
            display_text(&station.name),
            display_text(&station.countrycode),
            station.bitrate.to_string(),
        ];
        if wide {
            cells.push(display_text(&station.tags));
            cells.push(station.clickcount.to_string());
        }
        Row::new(cells)
    });
    let widths = if wide {
        vec![
            Constraint::Length(2),
            Constraint::Min(15),
            Constraint::Length(2),
            Constraint::Length(5),
            Constraint::Length(12),
            Constraint::Length(7),
        ]
    } else {
        vec![
            Constraint::Length(2),
            Constraint::Min(15),
            Constraint::Length(2),
            Constraint::Length(5),
        ]
    };
    let table = Table::new(rows, widths)
        .header(Row::new(headers).style(Style::default().fg(Color::Gray)))
        .block(panel(&title))
        .column_spacing(1)
        .row_highlight_style(Style::default().bg(Color::LightGreen).fg(Color::Black));
    let mut state = TableState::default().with_selected(if app.stations().is_empty() {
        None
    } else {
        Some(app.selected)
    });
    frame.render_stateful_widget(table, area, &mut state);
    if app.stations().is_empty() && area.height > 4 {
        let inner = Rect::new(
            area.x + 2,
            area.y + 2,
            area.width.saturating_sub(4),
            area.height.saturating_sub(3),
        );
        frame.render_widget(
            Paragraph::new(if app.loading {
                "Searching Radio Browser…"
            } else if app.view == View::Favorites {
                "No favorites yet.\nPress / to find a station, then f to save it."
            } else {
                "No results.\nPress / to submit or adjust a search."
            })
            .wrap(Wrap { trim: true }),
            inner,
        );
    }
}

fn render_inspector(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let text = match app.highlighted() {
        Some(s) => format!(
            "{}\n{} / {}\n{} · {} kbps\nTag: {}\nClicks/24h: {}\n\nEnter play · f save/remove",
            display_text(&s.name),
            display_text(&s.countrycode),
            display_text(&s.language),
            display_text(&s.codec),
            s.bitrate,
            display_text(&s.tags),
            s.clickcount
        ),
        None => {
            "No highlighted station.\n\nBrowse with ↑↓ or j/k.\nEnter plays your selection.".into()
        }
    };
    frame.render_widget(
        Paragraph::new(text)
            .block(panel(" Highlighted station "))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_player(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let station = app
        .saved
        .last_station
        .as_ref()
        .map(|s| display_text(&s.name))
        .unwrap_or_else(|| "Ready when you are".into());
    let label = match app.playback {
        Playback::Stopped => "Stopped",
        Playback::Connecting => "Connecting",
        Playback::Playing => "Playing",
        Playback::Retrying => "Retrying",
        Playback::Failed => "Failed",
    };
    let text = vec![
        Line::from(vec![
            Span::styled(
                format!("{label} · "),
                Style::default().fg(Color::LightGreen),
            ),
            Span::raw(station),
            Span::raw(format!("  |  Volume {}%", app.saved.volume)),
        ]),
        Line::styled(
            app.metadata
                .as_deref()
                .unwrap_or("Track information unavailable"),
            Style::default().fg(Color::Gray),
        ),
    ];
    frame.render_widget(
        Paragraph::new(text).block(panel(" Player · Space stop/reconnect ")),
        area,
    );
}
