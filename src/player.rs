use std::{
    ffi::OsString, os::fd::OwnedFd, os::unix::net::UnixStream, process::Stdio, time::Duration,
};

use anyhow::{Result, bail};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::mpsc,
    task::JoinHandle,
    time::timeout,
};

use crate::app::{Event, PlayerEvent};

enum Control {
    Stop,
    Volume(u8),
}

pub(crate) struct Player {
    program: OsString,
    control: Option<mpsc::UnboundedSender<Control>>,
    task: Option<JoinHandle<()>>,
}

impl Player {
    pub fn new() -> Self {
        Self::with_program("mpv")
    }

    pub fn with_program(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            control: None,
            task: None,
        }
    }

    pub async fn start(
        &mut self,
        generation: u64,
        url: &str,
        volume: u8,
        sender: mpsc::UnboundedSender<Event>,
    ) {
        self.stop().await;
        let url = url.to_owned();
        let program = self.program.clone();
        let (control, receiver) = mpsc::unbounded_channel();
        self.control = Some(control);
        self.task = Some(tokio::spawn(async move {
            let result = run(program, generation, &url, volume, &sender, receiver).await;
            if let Err(error) = result {
                let missing = error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound);
                let event = if missing {
                    PlayerEvent::Missing
                } else {
                    PlayerEvent::Failed(format!("Player failed: {error}"))
                };
                let _ = sender.send(Event::Player { generation, event });
            }
        }));
    }

    pub fn volume(&self, volume: u8) {
        if let Some(control) = &self.control {
            let _ = control.send(Control::Volume(volume));
        }
    }

    pub async fn stop(&mut self) {
        if let Some(control) = self.control.take() {
            let _ = control.send(Control::Stop);
        }
        if let Some(mut task) = self.task.take()
            && timeout(Duration::from_secs(2), &mut task).await.is_err()
        {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        if let Some(control) = self.control.take() {
            let _ = control.send(Control::Stop);
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

async fn run(
    program: OsString,
    generation: u64,
    url: &str,
    volume: u8,
    sender: &mpsc::UnboundedSender<Event>,
    mut control: mpsc::UnboundedReceiver<Control>,
) -> Result<()> {
    let parsed = reqwest::Url::parse(url)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        bail!("Only HTTP(S) station streams are supported");
    }
    let (parent, child_socket) = UnixStream::pair()?;
    parent.set_nonblocking(true)?;
    let mut child = Command::new(program)
        .args([
            "--no-config",
            "--no-video",
            "--audio-display=no",
            "--idle=yes",
            "--terminal=no",
            "--input-terminal=no",
            "--input-default-bindings=no",
            "--input-ipc-client=fd://0",
        ])
        .stdin(Stdio::from(OwnedFd::from(child_socket)))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let stream = tokio::net::UnixStream::from_std(parent)?;
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let result = async {
        for command in [json!(["observe_property", 1, "time-pos"]), json!(["observe_property", 2, "metadata"]),
            json!(["observe_property", 3, "paused-for-cache"]), json!(["set_property", "volume", volume]),
            json!(["loadfile", url, "replace"])] {
            write_command(&mut writer, command).await?;
        }
        loop {
            tokio::select! {
                command = control.recv() => match command {
                    Some(Control::Volume(volume)) => write_command(&mut writer, json!(["set_property", "volume", volume])).await?,
                    Some(Control::Stop) | None => break,
                },
                line = lines.next_line() => {
                    let Some(line) = line? else { bail!("mpv disconnected"); };
                    let Ok(message) = serde_json::from_str::<Value>(&line) else { continue };
                    let event = match message["event"].as_str() {
                        Some("playback-restart") => Some(PlayerEvent::Ready),
                        Some("end-file") if message["reason"] != "redirect" => {
                            let error = message["file_error"].as_str().or_else(|| message["error"].as_str()).unwrap_or("Stream ended");
                            let _ = sender.send(Event::Player { generation, event: PlayerEvent::Failed(error.into()) });
                            break;
                        }
                        Some("property-change") => match message["name"].as_str() {
                            Some("time-pos") => message["data"].as_f64().map(PlayerEvent::Progress),
                            Some("paused-for-cache") => message["data"].as_bool().map(PlayerEvent::Buffering),
                            Some("metadata") => Some(PlayerEvent::Metadata(track_metadata(&message["data"]))),
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some(event) = event { let _ = sender.send(Event::Player { generation, event }); }
                }
                status = child.wait() => { bail!("mpv exited: {}", status?); }
            }
        }
        Ok(())
    }.await;
    drop(writer);
    drop(lines);
    // Explicitly wait after killing so station changes never overlap old audio.
    let _ = child.kill().await;
    result
}

async fn write_command(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    command: Value,
) -> Result<()> {
    let mut bytes = serde_json::to_vec(&json!({ "command": command }))?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    Ok(())
}

fn track_metadata(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    let get = |key: &str| {
        object
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .and_then(|(_, value)| value.as_str())
            .filter(|text| !text.trim().is_empty())
    };
    if let Some(title) = get("icy-title") {
        return Some(title.into());
    }
    let title = get("title")?;
    Some(match get("artist") {
        Some(artist) => format!("{artist} — {title}"),
        None => title.into(),
    })
}
