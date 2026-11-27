# Domain docs

This repository uses a single-context layout:

- `GLOSSARY.md`: canonical domain terminology.
- `docs/adr/`: architectural decision records, created lazily.

## Before exploring or proposing changes

Read the root glossary and ADRs relevant to the work.
If either is absent, proceed without creating placeholders.

For product design, specification, or implementation work, also read
`docs/design.md` for confirmed behavior, selected UX, acceptance
criteria, and the current implementation authorization status.

## Vocabulary and decisions

Use glossary terms in issues, proposals, documentation, and tests.
Surface missing or conflicting terminology for domain modeling.

Identify conflicts with existing ADRs explicitly rather than silently
overriding their decisions.

Update the glossary when terminology is resolved. Create an ADR only
for a consequential trade-off whose reasoning needs preserving.
