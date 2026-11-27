mod app;
mod catalog;
mod input;
mod model;
mod player;
mod runtime;
mod store;
mod ui;

use std::{
    io::{self, IsTerminal, stdout},
    path::PathBuf,
    process::ExitCode,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use crossterm::{
    event::{Event, EventStream},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures_util::StreamExt;
use nix::{
    sys::signal::{Signal, kill},
    unistd::getpid,
};
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::signal::unix::{SignalKind, signal};

use app::{Action, Effect};
use runtime::{Flow, Runtime};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rmusic: {}", model::display_text(&format!("{error:#}")));
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => {}
        [arg] if arg == "--help" || arg == "-h" => {
            println!(
                "rmusic — foreground internet radio\n\nUsage: rmusic [--help | --version]\n\nRequires mpv. Opens Favorites without autoplay.\n/ search · Enter play · Space stop/reconnect · f favorite · ? help · q quit\n\nState: $XDG_STATE_HOME/rmusic (default ~/.local/state/rmusic).\nExplicit playback reports a station selection to Radio Browser, which sees your IP.\nNo app telemetry or history beyond the last station."
            );
            return Ok(());
        }
        [arg] if arg == "--version" || arg == "-V" => {
            println!("rmusic {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        _ => bail!("Unknown arguments. Use rmusic --help"),
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("Run rmusic in an interactive terminal, or use --help");
    }
    let directory = state_directory()?;
    let mut runtime = Runtime::open(&directory)?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let mut hangup = signal(SignalKind::hangup())?;
    let mut suspend = signal(SignalKind::from_raw(nix::libc::SIGTSTP))?;
    let _guard = TerminalGuard;
    let mut terminal = setup_terminal()?;
    let size = terminal.size()?;
    runtime
        .app
        .update(Action::Resize(size.width, size.height), runtime.now());
    let mut input = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    runtime.execute(vec![Effect::Refresh]).await;
    let result = async {
        loop {
            terminal.draw(|frame| ui::render(frame, &runtime.app))?;
            let effects = tokio::select! {
                input = input.next() => match input {
                    Some(Ok(Event::Key(key))) => runtime.app.key(key, runtime.now()),
                    Some(Ok(Event::Resize(width, height))) => runtime.app.update(Action::Resize(width, height), runtime.now()),
                    Some(Ok(_)) => Vec::new(),
                    Some(Err(error)) => return Err(error.into()),
                    None => runtime.app.update(Action::Quit, runtime.now()),
                },
                event = runtime.receiver.recv() => match event {
                    Some(event) => runtime.app.complete(event, runtime.now()),
                    None => runtime.app.update(Action::Quit, runtime.now()),
                },
                _ = tick.tick() => runtime.app.tick(runtime.now()),
                _ = interrupt.recv() => runtime.app.update(Action::Quit, runtime.now()),
                _ = terminate.recv() => runtime.app.update(Action::Quit, runtime.now()),
                _ = hangup.recv() => runtime.app.update(Action::Quit, runtime.now()),
                _ = suspend.recv() => runtime.app.update(Action::Suspend, runtime.now()),
            };
            match runtime.execute(effects).await {
                Flow::Continue => {}
                Flow::Quit => return Ok(()),
                Flow::Suspend => {
                    // All threads stop with the process. Keep one event stream:
                    // replacing it can overlap the old reader worker's shutdown.
                    restore_terminal();
                    // SIGSTOP suspends even though SIGTSTP is observed by Tokio.
                    kill(getpid(), Signal::SIGSTOP)?;
                    terminal = setup_terminal()?;
                    let size = terminal.size()?;
                    runtime.app.update(Action::Resize(size.width, size.height), runtime.now());
                }
            }
        }
    }.await;
    runtime.shutdown().await;
    result
}

fn state_directory() -> Result<PathBuf> {
    if let Some(base) = std::env::var_os("XDG_STATE_HOME").filter(|s| !s.is_empty()) {
        let base = PathBuf::from(base);
        if base.is_absolute() {
            return Ok(base.join("rmusic"));
        }
    }
    let home = PathBuf::from(
        std::env::var_os("HOME").context("HOME or an absolute XDG_STATE_HOME is required")?,
    );
    if !home.is_absolute() {
        bail!("HOME must be an absolute path");
    }
    Ok(home.join(".local/state/rmusic"))
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout()))?)
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen);
}

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

#[cfg(test)]
mod acceptance;
