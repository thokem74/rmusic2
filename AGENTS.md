# Rust development instructions

These instructions apply throughout this repository. Follow the official
[Rust Style Guide](https://doc.rust-lang.org/style-guide/) for formatting and
[Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) for API design
and naming. The workflow and engineering rules below are repository policy.

## Before making changes

- Read the relevant Cargo manifests, nearby code, and any more specific
  `AGENTS.md` instructions before editing.
- Respect the configured Rust edition, minimum supported Rust version (MSRV),
  toolchain, features, and lint settings. When none exist, use stable Rust;
  introduce toolchain or edition changes only when the task requires them.
- This repository has no Cargo manifest yet. Apply Cargo checks once a Rust
  package exists; creating this instructions file does not require scaffolding one.

## Formatting

- Run `cargo fmt --all` and let rustfmt handle layout and import formatting.
  Honor repository rustfmt configuration; otherwise use its defaults.
- Default style uses four spaces for indentation and a maximum line width of
  100 characters. Let rustfmt decide wrapping and trailing commas.
- Use `//` for ordinary comments, `///` for item documentation, and `//!` for
  crate or module documentation. Explain intent and constraints in comments.

## Naming

Follow the [official naming guidelines](https://rust-lang.github.io/api-guidelines/naming.html).

| Item | Convention | Example |
| --- | --- | --- |
| Modules, functions, methods, fields, variables | `snake_case` | `track_count` |
| Structs, enums, traits, type aliases, enum variants | `UpperCamelCase` | `AudioTrack` |
| Constants and statics | `SCREAMING_SNAKE_CASE` | `MAX_TRACKS` |
| Type parameters | Concise `UpperCamelCase` | `T`, `Reader` |
| Lifetimes | Short lowercase names | `'a`, `'src` |
| Macros | `snake_case!` | `audio_frame!` |

- Treat acronyms as words: `HttpClient`, `Uuid`, and `http_client`.
- Use `new` for general constructors and `with_*` for descriptive alternatives.
- Name cheap borrowed views `as_*`, conversions that perform work `to_*`, and
  consuming conversions `into_*`. Implement standard conversion traits when suitable.
- Name ordinary getters after the property, such as `title()` and `title_mut()`.
  Reserve `get`/`get_mut` for conventions such as checked collection access.
- Use `iter`, `iter_mut`, and `into_iter` for collection iteration by shared
  reference, mutable reference, and ownership respectively.
- Use descriptive domain terms. Avoid redundant `rust` or `rs` crate-name affixes.

## Implementation and APIs

- Keep visibility as narrow as practical; expose only the API callers need.
- Prefer borrowing and slices (`&str`, `&[T]`) when ownership is unnecessary.
  Make allocations and cloning deliberate.
- Use `Result` for recoverable failures, `Option` for expected absence, and `?`
  for propagation. Reserve panics for broken invariants; explain any production
  `expect` or `unwrap` through an evident invariant or a nearby comment.
- Prefer safe Rust. Any necessary `unsafe` block must have a `// SAFETY:` comment
  explaining why its preconditions hold; document caller obligations on unsafe APIs.
- Document public APIs with examples where useful, plus applicable `# Errors`,
  `# Panics`, and `# Safety` sections.
- Keep changes focused. Add dependencies only when justified by the task.

## Verification

For Rust code changes, run the following from the Cargo workspace root:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets
cargo test --workspace
```

- Follow configured lint levels and resolve warnings introduced by the change.
  Keep lint exceptions narrowly scoped and explain their reason. Consult
  [Clippy usage](https://doc.rust-lang.org/clippy/usage.html) when changing lint policy.
- Add or update tests for changed behavior and regressions. Include relevant
  edge cases and failure paths; use documentation tests for public examples.
- Test additional feature combinations or targets when affected by the change.
  Enable all features together only when they are compatible.
- Before finishing, inspect the diff and report checks run, their outcomes, and
  any checks that could not run with the reason.
