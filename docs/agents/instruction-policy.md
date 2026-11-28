# Instruction policy mapping

This records the documentation-only cleanup for issue #5 against the previous
`AGENTS.md` at commit `0eb677dc600765720acbc339009f8d4b9f6bddce`.
The policies remain repository-wide. The authoritative operational entry point is
[AGENTS.md](../../AGENTS.md); judgment-based implementation/review policy lives in
[CODING_STANDARDS.md](../../CODING_STANDARDS.md).

## Rule inventory

Each row accounts for the corresponding original rule or tightly related group.
“Check” means enforced mechanically; “review” retains decisions tooling cannot make.

| Original location/rule | Classification and destination |
| --- | --- |
| Introduction: repository scope | Essential: AGENTS opening. |
| Introduction: official style and API guidance | Review: coding standards introduction and Naming and APIs. |
| Before making changes: manifests, nearby code, specific instructions | Essential: AGENTS Before editing. |
| Before making changes: edition, MSRV, toolchain, features, lints, stable fallback, task-required changes only | Essential: AGENTS Before editing; no compatibility settings changed. |
| Formatting: run rustfmt, honor configuration/defaults | Essential: AGENTS Verification; review reference introduction; check: `scripts/check` format phase. |
| Formatting: four spaces, 100 columns, wrapping, commas, imports | Redundant exposition of rustfmt defaults/layout; check: format phase. No new formatter configuration. |
| Formatting: comment/doc-comment forms and intent/constraints | Review: Documentation and scope. |
| Naming: official guidance and all table conventions, including macros/type parameters/lifetimes | Review: Naming and APIs. Familiar examples removed; conventions retained because lints do not cover every case. |
| Naming: acronyms, `new`/`with_*` | Review: Naming and APIs. |
| Naming: `as_*`/`to_*`/`into_*`, standard conversion traits | Review: Naming and APIs. |
| Naming: property getters, checked `get`/`get_mut`, iterator ownership names | Review: Naming and APIs. |
| Naming: domain terms, redundant crate affixes | Review: Naming and APIs; domain navigation retained. |
| Implementation: narrow visibility, borrowed inputs, deliberate allocation/cloning | Review: Naming and APIs. |
| Implementation: `Result`/`Option`/`?`, invariant panics, explained production unwrap/expect | Review: Errors and safety. |
| Implementation: safe Rust, safety comments and unsafe caller obligations | Review: Errors and safety; check: manifest `unsafe_code = "forbid"` via compilation. The lint remains stronger than conditional documentation rules. |
| Implementation: public API examples and Errors/Panics/Safety sections | Review: Documentation and scope. |
| Implementation: focused changes, justified dependencies | Review: Documentation and scope. |
| Verification: required entry point, root selection, authoritative sequence, CI reuse | Essential: AGENTS Verification; check: `scripts/check` and CI. Command details remain in the script rather than duplicated instructions. |
| Verification: format, workspace/all-target compilation/Clippy, warnings as errors, tests, lockfile | Check: existing shared command, unchanged. |
| Verification: configured lint levels, warning resolution, narrow explained exceptions, Clippy usage reference | Review: Verification judgments; check: configured lints and Clippy `-D warnings`. |
| Verification: behavior/regression tests, edges, failure paths, doctests for examples | Review: Verification judgments; check: executes existing workspace tests, not adequacy of coverage. |
| Verification: affected feature/target checks, compatible all-features only | Review: Verification judgments. |
| Verification: inspect diff, report checks/outcomes/unavailable reasons | Essential: AGENTS Verification. |
| Agent skills: tracker before reading/writing issues/specs | Navigation: AGENTS Issues/specs → unchanged tracker guide. |
| Agent skills: labels before assigning triage roles | Navigation: AGENTS Triage → unchanged label guide. |
| Agent skills: domain guide before exploring/proposing behavior | Navigation: AGENTS Domain → unchanged domain guide. |

The [domain guide](domain.md) still owns conditional consumers: read the root
[glossary](../../GLOSSARY.md) and relevant ADRs when present, proceed without
placeholders when absent, read [design](../design.md) for product/spec/implementation
work, surface vocabulary/ADR conflicts, and update resolved terminology or record
consequential decisions. Its single-context layout is unchanged; no ADR directory
or placeholder was created. [Tracker operations](issue-tracker.md) and
[triage labels](triage-labels.md) are unchanged.

## Discovery and enforcement

The existing code-review skill searches repository standards documents and
explicitly requires `CODING_STANDARDS.md` when present. The AGENTS pointer also
requires the full reference for review. Implementation agents consult relevant
sections for decisions without preloading every review detail. The policy mapping
is audit evidence, not another required always-loaded reference.

Issue #2 is closed and its common guardrail exists in [scripts/check](../../scripts/check),
called by [CI](../../.github/workflows/checks.yml). The command checks rustfmt,
compiles workspace/all-targets, runs workspace/all-target Clippy with `-D warnings`,
and runs workspace tests; dependency-consuming phases use `--locked`.
[Runner tests](../../scripts/test_check.py) exercise real scratch workspaces with
formatting, compile, Clippy-warning, and behavior failures, as well as wrapper
failure propagation. Naming semantics, comment intent, documentation adequacy,
ownership choices, and coverage remain review judgments rather than claims of
complete compiler/linter enforcement.

## Instruction size

| Always-loaded AGENTS.md | Before | After |
| --- | ---: | ---: |
| Lines | 100 | 28 |
| Whitespace-delimited words | 657 | 167 |
| UTF-8 bytes | 4778 | 1312 |

The entry point shrank by 72.5% in bytes. These are file measurements,
not model-token estimates. Detailed review policy and this migration audit are
loaded on demand. No global instructions, installed skills, Rust settings, check
sequence, or application code changed.

## Validation evidence

- Local navigation check: all 19 relative Markdown links in the three changed
  documents resolve; headings are unique within each document; tracker, triage,
  domain, design, standards, and check routes remain in the entry point.
- Domain, tracker, and label guides are byte-for-byte unchanged.
- `python3 scripts/test_check.py`: all 8 command-boundary tests pass, including
  controlled formatting, compilation, Clippy-warning, and behavioral failures.
- A temporary workspace using the current manifest's unsafe lint and copied
  guardrail rejects an unsafe function in the compilation phase.
- `rustfmt --print-config default` confirms four spaces, spaces rather than tabs,
  and the 100-column default. No explicit configuration was added.
- `./scripts/check`: formatting, compilation, Clippy, and tests pass (20 passed;
  the existing opt-in real-mpv test remains ignored).
- `git diff --check` passes. Hosted CI was not triggered by these local checks.
