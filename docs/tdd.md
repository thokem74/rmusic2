# Behavioral red-green evidence

Use `scripts/tdd` for one named slice at an agreed testing boundary. For radio
behavior, keep the agreed application boundary: actions and visible state with
controlled directory/player responses and time, plus real temporary persistence.
This command does not select a new seam or replace `./scripts/check`.

Requires Linux, Python 3, and stable Cargo/Rust on PATH. From the workspace root:

```sh
# Add a compilable test that detects the missing/incorrect behavior.
./scripts/tdd red volume-bound acceptance::entering_the_playing_station_is_a_noop_and_volume_is_app_only_and_bounded --exact
# Implement the behavior, then repeat the identical selection.
./scripts/tdd green volume-bound acceptance::entering_the_playing_station_is_a_noop_and_volume_is_app_only_and_bounded --exact
```

This example names an existing, currently passing test: it will report `test_pass`
and **will not** verify red unless the selected test actually fails. Choose the
new test or narrow filter for your own slice. Without `--exact`, the filter uses
libtest's substring matching. Filters must be nonempty and cannot be runner flags.
Use `--manifest-path PATH` to select another workspace; repeat that option for
all phases. The driver always builds workspace tests with `--locked`; create a
lockfile first in a new scratch workspace.

Each invocation first runs `cargo test --workspace --locked --no-run
--message-format=json`, then executes Cargo's reported test binaries with the
selected filter, stable libtest pretty output, and one test thread. It uses each
target's package directory as the working directory. This supports ordinary
unit/integration tests using Rust's standard test harness. It does not run
doctests, ignored tests, benches, cross-target runners, or custom test harnesses.
Use the normal Cargo command for those checks; this driver offers no feature or
extra runner-argument forwarding. Test executables inherit the caller's environment;
Cargo's runtime environment setup (such as dynamic-library search paths) is not
reproduced. Use ordinary native test binaries and consistent environment/settings
throughout the slice.

| Outcome | Meaning |
| --- | --- |
| `build_failure` | Cargo's compilation command failed; never behavioral red. |
| `infrastructure_failure` | A tool could not launch, exited abnormally, or produced an ambiguous test report. |
| `zero_tests` | No selected test executed, including an ignored-only selection. |
| `test_pass` | At least one selected test ran and all passed; before red, this is not a verified cycle. |
| `test_failure` | Compilation succeeded and libtest reported a failing selected test. A red invocation records `red_verified`. |
| `test_set_mismatch` | Green executed different test identities than red, even with the same filter. |
| `interrupted` | Interrupted work cannot verify a phase. |

Exit status is 0 only for a verified phase or successful reset, 1 for an observed
but unverified phase, and 2 for invocation/evidence/storage errors. An expected
failing test returns 0 from **red** because red was verified. A failed green
attempt returns 1 and retains its failed outcome and prior verified red; it may be
retried with the same selection. Only a subsequent passing run of exactly the
same executed test names verifies green. The manifest, filter, and exact flag must
also match. Missing red evidence and mismatches are explicit errors.

Evidence lives under `.tdd-evidence/SLICE/` beside the selected manifest, ignored
by this repository. The concise console result points to that absolute directory.
`record.json` holds selection, status, per-attempt classifications, commands,
working directories, exit statuses where observed, test identities, and a bounded
diagnostic excerpt. Unique run directories retain complete command logs,
including the assertion failure. Review the red diagnostic to decide whether the
intended assertion failed rather than unrelated setup. Long diagnostics may be
truncated in the console/record; complete logs remain available locally.

A slice cannot be reused for a new red run without an explicit reset:

```sh
./scripts/tdd reset volume-bound
```

Reset archives the current record (including malformed records) and retains logs;
it does not touch application code, tests, or saved listener state. Later attempts use new log directories, preserving archived cycles. Concurrent
operations on the same slice are rejected. SIGINT/SIGTERM stops the owned subprocess group and records
an interruption when possible. A hard kill can leave `running` evidence; green
rejects it. Reset interrupted, malformed, or completed cycles explicitly before
starting over. Atomic record replacement prevents partially written JSON from
being mistaken for success. Do not hand-edit records to recover a cycle.

These records support human review. They cannot prove assertion quality, rule out
setup-caused failures, or establish development chronology outside the command.
The driver never edits implementation/tests or globally installed skills.

Test the driver through its agreed command boundary using disposable Rust workspaces:

```sh
python3 scripts/test_tdd.py
```

The tests exercise real Cargo/libtest, observed classifications, exit status,
logs, reset, and evidence, without breaking the radio app or changing user state.
