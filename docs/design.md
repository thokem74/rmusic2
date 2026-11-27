# Internet radio player design

## Confirmed decisions

- Build a Rust TUI for personal, keyboard-driven listening in an Omarchy Linux terminal.
- Prioritize finding a station, saving it, and returning to favorites quickly.
- The first release includes station search, country/language/tag filters, favorites,
  playback controls, volume, and available track metadata.
- Defer recording, scheduling, recommendations, and editing Radio Browser entries.
- Run as one foreground application; quitting the TUI stops playback.
- Use the installed `mpv` executable as a required audio playback dependency;
  Rust owns the interface, station discovery, and playback control.
- Provide play/stop rather than pause or time-shifting. Starting again reconnects
  to the live broadcast.
- Retry a dropped stream a few times, then show an error. Never automatically
  switch stations.
- Open favorites on startup without autoplay. Persist favorites, application
  volume, and the last station across runs.
- Support arrow keys and Vim-style navigation outside text fields. Use `/` for
  search, Enter to play, Space to stop/reconnect, `f` to toggle a favorite,
  `+`/`-` for volume, and `?` for help. Text fields accept normal typing.
- Separate Favorites and Search views, with a persistent player area for station,
  available track metadata, playback status, and volume. Navigation does not
  interrupt playback; Enter explicitly selects a station for playback.
- Submit searches explicitly. Combine name, country, language, and tag filters;
  hide stations marked broken by default, with an option to show them. Paginate
  results and order by recent click count, descending.
- Save favorite station UUIDs, names, and stream addresses. Refresh details when
  possible and allow playback attempts from saved addresses during directory outages.
- Application volume affects only this player. Stop its current playback before
  starting another station; play only one station at a time.
- Deliver a normal terminal command with Omarchy installation and dependency
  instructions. Defer global media keys, desktop notifications, background services,
  and automatic Hyprland configuration changes.
- Favorites retain insertion order. Support saving/removing Radio Browser stations;
  defer custom stream URLs, renaming, and manual reordering. Playback failure never
  automatically removes a favorite.
- Target a full interface at 80 columns by 24 rows or larger. Below that, show a
  resize message while playback and quit controls remain functional. Preserve
  search, selection, and playback across resizing.
- Name the application and command `rmusic`. Initially install from this repository
  using Cargo; document the required `mpv` dependency. Defer Arch/AUR packaging.
- Explicit playback starts report the station choice through Radio Browser's click
  endpoint. Automatic reconnects do not report additional clicks. Reporting failure
  does not block playback. Document that the service receives the station choice
  and the client's IP address.

## Confirmed implementation defaults

These defaults were confirmed during the design interview. Their confirmation does
not authorize implementation; the explicit implementation hold remains in effect.

### Interface and state

- Use Ratatui and Crossterm for the TUI, with Tokio coordinating asynchronous work.
- Keep highlighted station, currently playing station, search query, and favorites
  as separate state. Space stops the current station or reconnects the last played
  station; Enter plays the highlighted station.
- Space also cancels connecting or retrying. Enter on an already-playing station
  does nothing; Enter after failure explicitly retries with a fresh retry budget.
  Space from stopped or failed reconnects the last station with a fresh budget.
  With no last station or highlighted result, the corresponding action does nothing.
- Use Tab to switch Favorites/Search; provide a visible filter form and help screen.
  Use `q` to quit outside text fields and Ctrl+C to quit throughout the application.
- Within the filter form, Tab moves between fields, Enter submits the search, and
  Escape returns to results. `/` opens the search form. Outside the form, Tab
  switches views. Provide visible next/previous-page controls and their shortcuts.
- Show station name, country, language, codec, and bitrate where space permits.
  Keep station identity visible when track metadata is missing; do not invent song
  titles or infer them from station names.
- Clear track metadata immediately on station changes and stop. When unavailable,
  show "Track information unavailable". Render station names, metadata, and other
  remote strings as plain text with terminal control sequences removed.
- Request 50 search results per page. Editing filters resets pagination when the
  new search is submitted. Ignore stale responses from superseded searches.
- A failed search preserves the previous results and reports the failed query.
  Label retained results with the query that produced them, rather than presenting
  them as results of the failed search. Search operations never interrupt audio.
- Do not autoplay after startup or after a search. Restore the last station as
  context, without automatically selecting it for playback.

### Playback and failures

- Control one application-owned `mpv` child through its JSON IPC interface, without
  a shell. Disable user mpv configuration and video for predictable audio behavior.
- Use `--input-ipc-client=fd://N` with an inherited connection, so closing the
  controller connection causes mpv to exit. Do not use a filesystem IPC server or
  require a temporary socket directory for this design.
- Expose stopped, connecting, playing, retrying, and failed playback states. Keep
  the interface responsive during network and player operations.
- Allow 15 seconds for each connection attempt. For initial connection failures
  and unexpected stream endings, retry the same station up to three times after
  delays of 1, 2, and 4 seconds. Reset the retry budget after 30 seconds of stable
  playback, so brief successful connections do not create an endless retry loop.
- Treat prolonged buffering or a lack of playback progress for 15 seconds as a
  stall and apply the same retry budget. Use player playback/buffering observations,
  rather than metadata updates, to detect progress. Recreate mpv after a player
  crash within that budget. Missing mpv fails immediately with installation guidance.
- Stop, station changes, and quit cancel pending retries. Never mix stations or
  let a late response restart a station that the listener has left.
- Use application volume from 0 to 100, adjusted in steps of 5; default to 50 on
  first launch. Do not change system volume.
- On normal exit or handled interruption, stop and reap the owned player and
  restore terminal state. On terminal closure, terminate the owned player; the
  inherited IPC connection also prevents playback surviving controller disconnect.
- Ctrl+Z stops playback and restores terminal state before suspension. On `fg`,
  restore the interface without autoplay; the listener explicitly reconnects.
- Give actionable errors for missing mpv, failed connection, unavailable directory,
  and unreadable or unwritable saved state. Keep favorites intact on playback errors.

### Radio Browser access

- Discover mirrors through the documented DNS mechanism, randomize their order,
  use HTTPS, and try another mirror after failure. Do not pin a permanent mirror.
- Identify requests with an application/version User-Agent. Set bounded request
  timeouts and keep directory failures separate from playback failures.
- Use substring name matching and exact country-code, selected-language, and
  selected-tag matching. Combine populated filters with AND. An empty query browses
  popular stations. Serialize API exact-match flags using the documented names.
- Order search results using `order=clickcount&reverse=true`: popularity means
  reported clicks within the preceding 24 hours, not `clicktrend` or listener count.
- Use station UUIDs for identity, country codes for country filters, and resolved
  stream URLs where available. Let mpv handle supported audio and playlist formats.
- Attempt click reporting for explicit play/reconnect actions. Use its returned
  stream information when available, with saved or search-result stream addresses
  as fallback; reporting is best effort and bounded by a short timeout.
- Do not report search highlights, stop actions, or automatic retries as clicks.
- Refresh saved stations by UUID when online, preserving favorite order. If an
  entry disappears, retain its saved details and allow an attempted playback.
- Deduplicate favorites by station UUID. Removing a currently playing favorite does
  not stop audio. Refresh preserves favorite position and never replaces a usable
  saved stream address with an empty address.
- No station voting or directory editing in the first release.

### Persistence and boundaries

- Keep one versioned JSON state file under `$XDG_STATE_HOME/rmusic/`, defaulting to
  `~/.local/state/rmusic/`. Persist favorites with UUID, display details, and stream
  address, plus application volume and a last-station snapshot sufficient to attempt
  playback even when that station is not a favorite.
- Save with atomic replacement. On malformed or unsupported state, preserve the
  original file and report the problem rather than silently overwriting it.
- If saved state is corrupt, unsupported, unreadable, or unwritable, allow listening
  for the current session and show a persistent "changes cannot be saved" warning.
  Do not overwrite a damaged original or imply that in-memory changes were saved.
  Recovering or replacing the file requires explicit user action.
- Store no listening history beyond the last station. Do not add application
  telemetry; Radio Browser playback reporting is the only usage reporting planned.
- Support one running instance per state directory in the first release. Detect
  and explain a second launch rather than allowing conflicting state writes.
- Keep catalog access, player control, persistence, and TUI rendering behind small
  internal interfaces. Avoid a daemon, plugin system, or public library API.
- Use stable Rust and a committed Cargo.lock. Select supported dependency versions
  during implementation and record the chosen Rust edition in Cargo.toml.

## Verification plan

- Cover search/filter serialization, pagination and stale-response handling,
  station identity and favorite ordering, state round trips and malformed files,
  and playback retry/cancellation transitions with deterministic tests.
- Use controlled HTTP/player substitutes for automated tests; avoid depending on
  public stations or live Radio Browser availability in the test suite.
- Smoke-test the real terminal and mpv integration, including play/stop, station
  changes, resize, metadata fallback, directory outage, and exit cleanup.
- Run repository-required formatting, check, Clippy, and tests once the Rust
  package exists. Document any live behavior that cannot be verified locally.

### Observable acceptance criteria

These scenarios define the expected design behavior; they do not create a separate
specification or authorize implementation.

| Scenario | Expected result |
| --- | --- |
| Start with valid saved state | Favorites retain insertion order, saved volume and last-station context return, and no audio starts. |
| Enter a search while audio plays | Submitted filters combine with AND, broken stations are hidden by default, and audio continues. |
| Browse an empty query | Results are paginated in groups of 50 and ordered by descending 24-hour click count. |
| Edit filters and submit | Pagination resets; late responses from earlier queries cannot replace the new results. |
| Search fails after earlier success | Earlier results remain labeled with their original query, and an error identifies the failed query. |
| Type in a filter field | Characters such as `q`, `f`, and spaces edit text instead of invoking navigation/playback shortcuts. |
| Play station B while A plays | A stops before B starts; old metadata clears and A's late events cannot restart it or replace B's display. |
| Enter on the playing station | Playback continues without restarting or reporting an additional click. |
| Space while connecting or retrying | The attempt and scheduled retries are cancelled; no late event restarts playback. |
| Explicitly restart after failure | A fresh bounded retry budget begins and playback targets the chosen station. |
| Connection fails continuously | After the initial attempt and at most three retries with 1/2/4-second delays, show failed status without switching stations. |
| Stream stalls or mpv crashes | Detect a 15-second stall or player exit, apply the retry budget, and recreate the player if necessary. |
| Playback remains stable for 30 seconds | Reset the retry budget; shorter successful intervals do not reset it. |
| Radio Browser is unavailable | Saved favorites can attempt playback from stored addresses; click-reporting failure does not prevent playback. |
| Explicit playback versus automatic retry | Attempt click reporting for an explicit start; highlights, stop, no-op Enter, and automatic retries produce none. |
| Save or refresh a favorite | UUID identity prevents duplicates, insertion order remains stable, and empty refreshed addresses do not erase usable saved ones. |
| Remove a playing favorite | It leaves Favorites while its current playback continues. |
| Restart after playing a non-favorite | The saved last-station snapshot permits an explicit reconnection without autoplay. |
| Load damaged state or encounter a write failure | Listening remains usable, the warning stays visible, and damaged state is not overwritten. |
| Receive absent metadata or terminal control sequences | Show the unavailable fallback as needed; remote strings cannot issue terminal controls. |
| Resize below 80×24 and back | Show the resize message, retain playback/quit controls, and restore search, selection, and playback state. |
| Change app volume | Clamp to 0–100 in steps of 5 without changing system volume. |
| Suspend and resume with Ctrl+Z/fg | Audio stops, terminal state is restored for the shell, and the resumed interface stays silent. |
| Quit, interrupt, or close the terminal | No owned player continues playback; handled exits restore terminal state and reap the child. |
| Launch a second instance for the same state directory | Explain the conflict and leave the first instance and its saved state untouched. |
| mpv is missing | Show actionable installation guidance without retrying playback. |

## References

- [Radio Browser client guidance](https://api.radio-browser.info/)
- [Radio Browser API documentation](https://docs.radio-browser.info/)
- [Search filters](https://docs.radio-browser.info/#advanced-station-search)
- [Playback click reporting](https://docs.radio-browser.info/#station-click-counter)
- [mpv inherited IPC client and disconnect behavior](https://mpv.io/manual/stable/#options-input-ipc-client)

## Design status

Shared understanding of the product scope is confirmed. The subsequent gap review
and the user's answers to Q18–Q26 resolved the recorded gaps and confirmed the
implementation defaults. Acceptance criteria above capture the resulting behavior.

No unresolved product decisions were identified in this review. Dependency versions,
exact player event mappings, and platform mechanisms must still be verified during
future implementation; this document does not claim they have been tested.

This document is ready as input for specification. The user has explicitly instructed
that implementation must not start yet. No separate specification or implementation
has been created.
