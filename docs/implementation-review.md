# Issue #1 implementation review

Specification: [rmusic foreground radio TUI](https://github.com/thokem74/rmusic2/issues/1).

The user authorized implementation after the earlier hold. The approved review
baseline is `051c7fab5db8714c79fb9d6ac955156dff63df0b`. Independent standards and spec
reviews examined implementation commit `6a11f88`; both reviewers then verified the
fixes in `c193df1`. Neither reviewer edited the implementation.

## Standards

No documented-standard violations were found beyond checks enforced by tooling.

The initial review raised one non-blocking judgement call: numeric search-field
positions coupled input, deletion, navigation, and rendering. A named `SearchField`
enum, shared query text accessors, and an explicit field list now replace those
numeric meanings. The standards reviewer confirmed the concern is resolved and
reported no new material standards issues in the follow-up diff.

## Spec

The initial review identified one P2 finding: an unbounded station name could hide
the app volume in the persistent player bar at 80×24. The specification requires
visible playback status, metadata, and volume at the supported minimum size.

The player bar now reserves a separate volume region. A regression exercises the
actual renderer with a 100-character name at 80×24. The spec reviewer confirmed the
finding is resolved and reported no new material spec findings in the follow-up.

## Additional verification findings

- A real PTY smoke check exposed an unnecessary cursor-position query during
  resume. Removing the redundant clear operation lets the newly created terminal
  buffer redraw without querying the cursor. Suspension/resume checks now pass.
- A controlled-time regression exposed retry-budget replenishment before stall
  detection after a delayed tick. Stall detection now runs first, so elapsed time
  without playback progress cannot count as stable playback.

## Validation

| Check | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Passed |
| `cargo check --workspace --all-targets` | Passed without warnings |
| `cargo clippy --workspace --all-targets` | Passed without warnings |
| `cargo test --workspace` | 19 passed, 0 failed; 1 optional real-mpv test ignored by default |
| `cargo test real_mpv_plays -- --ignored --nocapture` | Passed separately using generated local audio, null audio output, and child-reaping verification |
| `cargo build --locked` | Passed |
| `python3 scripts/smoke-terminal.py` | Passed: startup, filter typing, minimum-size fallback, quit, terminal restoration, suspension/resume, SIGINT, SIGTERM, SIGHUP |
| Live Radio Browser search in an isolated PTY | Passed: DNS discovery and HTTPS directory access populated station details without starting audio or reporting playback |

Tests use the agreed application-action boundary, controlled responses and time,
temporary real storage, and narrow HTTP/player integration checks. The default
suite needs local socket permission but does not rely on public stations or the
public directory being available.

The installed-mpv test is opt-in so the default suite can run on machines without
mpv. Physical speaker/output-device playback and an audible public broadcast were
not tested; the audio smoke test deliberately used local silence and null output.

Initial findings: Standards — 0 hard violations and 1 resolved maintenance heuristic
(numeric search fields); Spec — 1 resolved P2 finding (clipped volume). Final
outstanding review findings: 0 on each axis.
