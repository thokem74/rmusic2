# Coding standards

Repository policy for implementation and review. Apply every relevant rule when
reviewing a diff; use [the agent entry point](AGENTS.md) for operational steps and
[the shared check command](scripts/check) for mechanical verification.

Follow the official [Rust Style Guide](https://doc.rust-lang.org/style-guide/) and
[Rust API Guidelines](https://rust-lang.github.io/api-guidelines/). Formatting is
owned by rustfmt, honoring repository configuration or its defaults.

## Naming and APIs

Follow the official [naming guidelines](https://rust-lang.github.io/api-guidelines/naming.html).
Use `snake_case` for modules, functions, methods, fields, variables, and macros;
`UpperCamelCase` for structs, enums, traits, type aliases, and enum variants;
`SCREAMING_SNAKE_CASE` for constants and statics; concise `UpperCamelCase` for type
parameters; and short lowercase lifetime names.

- Treat acronyms as words: `HttpClient`, `Uuid`, and `http_client`.
- Use `new` for general constructors and `with_*` for descriptive alternatives.
- Name cheap borrowed views `as_*`, conversions that perform work `to_*`, and
  consuming conversions `into_*`. Implement standard conversion traits when suitable.
- Name ordinary getters after the property, such as `title()` and `title_mut()`.
  Reserve `get`/`get_mut` for conventions such as checked collection access.
- Use `iter`, `iter_mut`, and `into_iter` for iteration by shared reference,
  mutable reference, and ownership respectively.
- Use descriptive domain terms. Avoid redundant `rust` or `rs` crate-name affixes.
- Keep visibility as narrow as practical; expose only the API callers need.
- Prefer borrowing and slices (`&str`, `&[T]`) when ownership is unnecessary.
  Make allocations and cloning deliberate.

## Errors and safety

- Use `Result` for recoverable failures, `Option` for expected absence, and `?`
  for propagation. Reserve panics for broken invariants; explain any production
  `expect` or `unwrap` through an evident invariant or a nearby comment.
- Prefer safe Rust. Any necessary `unsafe` block must have a `// SAFETY:` comment
  explaining why its preconditions hold; document caller obligations on unsafe APIs.
  The configured lint policy in [Cargo.toml](Cargo.toml) currently forbids unsafe code.

## Documentation and scope

- Use `//` for ordinary comments, `///` for item documentation, and `//!` for crate
  or module documentation. Explain intent and constraints in comments.
- Document public APIs with examples where useful, plus applicable `# Errors`,
  `# Panics`, and `# Safety` sections.
- Keep changes focused. Add dependencies only when justified by the task.

## Verification judgments

- Follow configured lint levels and resolve warnings introduced by the change.
  Keep lint exceptions narrowly scoped and explain their reason. Consult
  [Clippy usage](https://doc.rust-lang.org/clippy/usage.html) when changing lint policy.
- Add or update tests for changed behavior and regressions. Include relevant
  edge cases and failure paths; use documentation tests for public examples.
- Test additional feature combinations or targets when affected by the change.
  Enable all features together only when they are compatible.

The checks enforce formatting, compilation, configured lints, and observed test
results. Review still decides whether names express domain meaning, APIs have the
right ownership/visibility, exceptions are justified, and tests cover the behavior.
