# rmusic

A keyboard-driven internet radio player for Omarchy Linux. Discover stations through
Radio Browser, save favorites, and listen through an application-owned mpv player.
rmusic opens silently and stops its audio when you quit.

## Install

Install a stable Rust toolchain and mpv. On Omarchy/Arch, the audio dependency is:

```sh
sudo pacman -S --needed mpv
```

From this repository:

```sh
cargo install --path . --locked
rmusic
```

Ensure your Cargo binary directory is on `PATH`. For development, use `cargo run`.
The interface requires an interactive terminal of at least 80 columns by 24 rows.
The installation does not change Hyprland configuration or global keybindings.

## Listen

Favorites and Search share the station ledger: a station table on the left,
highlighted-station details on the right, and a persistent player bar below.
Moving the highlight does not change what is playing.

| Key | Action |
| --- | --- |
| Up/Down or j/k | Highlight a station |
| Enter | Play highlighted station; no-op if it is already playing |
| Space | Stop/cancel playback, or reconnect the last station live |
| f | Save/remove highlighted favorite |
| + / - | Change app volume, from 0–100 in steps of 5 |
| Tab | Switch Favorites/Search |
| / | Open search filters |
| PgUp / PgDn | Previous/next search page |
| ? | Show keyboard help |
| q / Ctrl+C | Quit and stop audio |
| Ctrl+Z | Stop audio and suspend; `fg` returns silently |

In the filter form, Tab moves between fields, Enter submits, and Escape closes.
Shortcut characters type normally in text fields. Space toggles the Hide broken
checkbox when it is highlighted. Search by part of a name, an exact two-letter
country code such as `DE`, an exact language name such as `german`, and/or an exact
tag such as `jazz`. Populated filters combine with AND. An empty query browses
popular stations. Searches return 50 stations per page, ordered by clicks in the
preceding 24 hours; broken entries are hidden unless requested.

Only one station plays at a time. Stop/reconnect always returns to the live stream;
there is no pause buffer or seeking. Track metadata depends on the station. A
dropped stream, crash, or 15-second stall is retried at most three times after
1, 2, and 4 seconds. Thirty seconds of stable playback resets the retry budget.
The app never automatically chooses another station.

## Saved state and outages

Favorites retain insertion order and are identified by station UUID. Removing a
playing favorite leaves its audio running. Saved addresses allow playback attempts
even when the directory is unavailable. Disappeared directory entries remain saved.

State is stored atomically in `$XDG_STATE_HOME/rmusic/state.json`, falling back to
`~/.local/state/rmusic/state.json`. It contains favorites, app volume, and a snapshot
of the last station, including when it is not a favorite. Startup never autoplays.
Only one instance may use a state directory at a time.

Damaged, unsupported, or inaccessible state produces a persistent warning and
session-only listening. The app preserves the original and does not automatically
replace it. To recover, quit rmusic, back up the state file, and repair it or move
it aside explicitly before starting again. Changes made while saving is disabled
are not persisted. Instance locks live in a private per-user temporary directory;
the lock files may remain after exit, but the operating system releases the lock.

## Network behavior

Radio Browser mirrors are discovered through DNS and accessed over HTTPS with an
application/version User-Agent. Directory requests and reporting are time bounded.
Explicit starts/reconnects attempt the documented click endpoint, which exposes
the station selection and your IP address to Radio Browser. Automatic retries,
highlights, and stop actions do not report clicks. Reporting failure does not
prevent playback. Audio is fetched by mpv from the station's own HTTP(S) address.

There is no app telemetry, listening history beyond the last station, background
service, recording, or scheduling. A directory health check does not guarantee a
stream will play locally.

## Verify

```sh
./scripts/check
```

Requires Bash and a stable Rust toolchain with rustfmt and Clippy. The runner
selects this checkout's workspace root even when invoked from another directory.
It checks formatting without rewriting files, compiles all workspace targets,
runs Clippy with warnings treated as errors, and runs workspace tests. All
dependency-consuming phases use `--locked`. A failed phase stops verification,
identifies the phase, preserves Cargo's diagnostic, and returns its exit status.

GitHub Actions is configured to run this same command on pushes and pull requests
with stable Rust, rustfmt, and Clippy. CI also tests the runner itself; reproduce
those checks with:

```sh
python3 scripts/test_check.py
```

The runner tests use temporary workspaces to reject real formatting, compilation,
Clippy-warning, and behavior-test failures. They also check missing-tool errors,
failure propagation, workspace selection, and source/lockfile preservation. Python 3
is needed for these runner tests; the Cargo check command itself only needs Bash
and the Rust tools. Branch protection and additional process-smoke automation are
separate from this CI guardrail.

Tests exercise application actions and visible state with controlled responses and
time, real temporary-file persistence, and localhost HTTP fixtures. They do not
depend on public radio streams. The controlled HTTP/player checks need permission
to create local sockets when run in a restricted sandbox.

Additional real-process smoke checks (with mpv installed):

```sh
cargo test real_mpv_plays -- --ignored --nocapture
cargo build --locked
python3 scripts/smoke-terminal.py
```

The player check uses generated silent audio on localhost and a null audio output;
the terminal check uses isolated temporary state and pseudo-terminals.

The agreed scope and acceptance criteria are in [the design](docs/design.md), with
the published specification in [issue #1](https://github.com/thokem74/rmusic2/issues/1).
Review findings, resolutions, and verification results are recorded in the
[implementation review](docs/implementation-review.md).
