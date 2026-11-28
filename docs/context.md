# Bounded context reads

Use `scripts/context` when a large issue or Markdown reference is not already
available in the working context. It retrieves a complete snapshot once, prints
only identity/metadata by default, and reuses that snapshot until explicit refresh.
It helps targeted reading; it does not replace reading required material or require
another fetch when the source is already in the conversation.

Requires Linux, Python 3, and the configured `gh` CLI for uncached issue reads.
[Tracker guidance](agents/issue-tracker.md) still applies. Issue operations are
read-only: the helper runs `gh issue view` with an explicit repository and verifies
the returned number and URL. It never creates issues, posts comments, or edits
local references. No credentials store or additional authentication setup is added.

## Invocation

```sh
# Optional: one isolated cache for this working context. Reuse this path.
export RMUSIC_CONTEXT_CACHE="$(mktemp -d "${TMPDIR:-/tmp}/rmusic-context.XXXXXX")"

./scripts/context issue '#6'
./scripts/context issue '#6' --section 'Testing Decisions'
./scripts/context file docs/design.md --section 'Playback and failures'
./scripts/context file /path/to/reference.md --excerpt --max-lines 30 --max-bytes 3000

# Refresh only when a newer snapshot is wanted.
./scripts/context issue '#6' --refresh
```

Issue selectors accept `6`, `#6`, `thokem74/rmusic2#6`, or
`https://github.com/thokem74/rmusic2/issues/6`. This repository helper is scoped to
`thokem74/rmusic2`; other repositories/hosts are rejected. Different accepted
spellings share the same cache entry. Local paths are resolved to absolute paths
(including symlinks); sources must decode as UTF-8.

`--cache-dir PATH` overrides `RMUSIC_CONTEXT_CACHE`. With neither, the default is
`rmusic-context-UID` under Python's OS temporary directory (`$TMPDIR`, otherwise
`/tmp` on this Linux system). That default may survive across sessions; use the
isolated directory above when starting a separate working context. Caches are
outside tracked source by default. Keep custom caches in temporary or ignored
storage too. Removing a cache directory explicitly starts a new context.

## Metadata, limits, and full evidence

Successful output identifies the canonical source, retrieval time in UTC, whether
this was a retrieval, refresh, or cache hit, source line/character/UTF-8 byte counts,
and the full-source path. Issues also display number and title. No body is printed
without `--section` or `--excerpt`. `--excerpt` starts at the beginning of the body.

Source output is capped at **80 lines and 8,000 UTF-8 bytes**, including metadata,
the full-source pointer, and any truncation marker. Smaller budgets are allowed:
`--max-lines` accepts 3–80 and `--max-bytes` accepts 256–8,000. Both limits apply,
so even one extremely long line is bounded. Truncated output always says
`TRUNCATED` and retains the full-source pointer. If the pointer itself cannot fit,
the command fails with an explicit budget error rather than omitting it.
Extremely large titles/metadata may themselves be truncated. Console line endings
are normalized, and byte cuts preserve complete Unicode characters.

`Full source: PATH` points to a complete decoded Markdown file, not an escaped JSON
body. Read that file directly when the full evidence is needed; the bounds affect
only console output. A sibling `record.json` stores source identity, retrieval
metadata, the body filename, and its hash. Only requested issue fields are stored;
authentication remains entirely in `gh`. Raw CLI transport output/errors are not
printed or cached because they may contain credentials. Command failures identify
`gh`'s exit code, or a launch/30-second timeout failure, and direct the contributor
to check authentication/network access through the existing CLI.

## Section contract

`--section TEXT` matches the exact, case-sensitive ATX heading text after trimming
surrounding whitespace and optional trailing closing hashes. Headings use one to
six `#` characters, with up to three leading spaces and whitespace before the text.
Inline Markdown is matched literally, without rendering. A bare `#` is an empty
heading and can be selected with `--section ''`.

The excerpt includes the selected heading and its descendants, ending before the
next heading at the same or a shallower level (or EOF). A heading without body text
returns just that heading. Heading-looking lines inside backtick/tilde fenced code
blocks are ignored. This is a deliberately small ATX selector, not a full Markdown
renderer: Setext/HTML heading semantics are not interpreted.

Missing headings and repeated matching headings are explicit errors with the full
cached-source location. Repeated headings are ambiguous even when nested under
different parents; use a unique heading, a bounded beginning excerpt, or inspect
the full file. The helper never guesses an occurrence or dumps the body on error.
`--section` and `--excerpt` are mutually exclusive.

## Freshness and recovery

A cache hit is a snapshot from the displayed retrieval timestamp, not a new network
or filesystem read. There is no automatic expiry, file-change probe, or background
refresh. Consequently, a hit remains usable after the local original is deleted
or GitHub becomes unavailable. Use `--refresh` explicitly to request current text.

A failed refresh returns nonzero, prints no source on stdout, and preserves the
previous record and body. It reports that the earlier snapshot is unchanged,
without claiming a successful fresh fetch or silently falling back. A later read
without `--refresh` explicitly reports a cache hit. Missing tools, bad responses,
wrong target identities, missing sections, and unreadable sources also fail.

Each source has a local lock so concurrent reads fetch once. Snapshot files are
private (mode 600) inside per-source directories (mode 700). Refresh writes a new
body file and atomically replaces the record only after the complete snapshot is
available. Earlier full-source files remain inspectable; delete the context cache
when no longer needed. An interrupted refresh can leave unused temporary files,
which may be removed with the cache. A malformed record, missing body, or hash
mismatch fails explicitly; repair by refreshing, or remove the cache and retrieve
again. No invalid cache triggers an implicit fetch.

## Verify the helper

```sh
python3 scripts/test_context.py
```

These command-boundary tests use temporary Markdown, temporary caches, and a
controlled `gh` fixture. They assert metadata/body separation, reuse and refresh,
section boundaries/errors, Unicode/newlines, both output bounds, failed reads,
cache damage, identity validation, and source preservation. They require no live
GitHub access or installed-skill changes. CI runs the same test command.
