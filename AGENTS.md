# Repository instructions

These policies apply throughout this repository.

## Before editing

- Read relevant Cargo manifests, nearby code, and more specific `AGENTS.md` files.
- Respect configured Rust edition, MSRV, toolchain, features, and lint settings.
  Use stable Rust when unspecified; change toolchain or edition only when required
  by the task.
- Apply [coding standards](CODING_STANDARDS.md); consult relevant sections when
  making implementation or API decisions. For code review, read the full reference.

## Navigation

- Issues/specs: before reading or writing them, read
  [tracker operations](docs/agents/issue-tracker.md).
- Triage: before assigning roles, read [configured labels](docs/agents/triage-labels.md).
- Domain: before exploring behavior or proposing changes, read
  [domain guidance](docs/agents/domain.md). It routes to the glossary and relevant
  ADRs when present, and to [design](docs/design.md) for product/spec/implementation work.
- Large issues/references not already in context: use [bounded context reads](docs/context.md)
  to cache the source once and select excerpts.

## Verification

For Rust changes, run `cargo fmt --all`, then [`./scripts/check`](scripts/check).
Keep the check sequence authoritative there; CI uses the same entry point.
Before finishing, inspect the diff and report checks, outcomes, and reasons for
any checks that could not run. See the coding standards for test and lint judgments.
