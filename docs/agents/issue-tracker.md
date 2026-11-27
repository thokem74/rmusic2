# Issue tracker: GitHub

Issues and specs live in GitHub Issues for `thokem74/rmusic2`.
Use the `gh` CLI from this repository.

## Operations

- Verify the target before the first write:
  `gh repo view --json nameWithOwner`.
- Read: `gh issue view <number> --json number,title,body,labels,comments`.
- List: `gh issue list --state open --json number,title,labels`.
- Create: `gh issue create --title "..." --body-file <path>`.
- Comment: `gh issue comment <number> --body-file <path>`.
- Label: `gh issue edit <number> --add-label "<label>"` or
  `--remove-label "<label>"`.
- Close: post the explanation first, then `gh issue close <number>`.

For multiline bodies, write the exact text to a temporary file and use
`--body-file`. File issues only in this repository.

## Relationships

Use native GitHub sub-issues and blocking dependencies when supported.
Otherwise, put `Part of #<parent>` and `Blocked by: #<number>` at the
top of the child issue, and add the child to the parent's task list.

An issue is unblocked when all its blockers are closed.

## Skill instructions

"Publish to the issue tracker" means create a GitHub issue.
"Fetch the relevant ticket" means read the GitHub issue.

Read `docs/agents/triage-labels.md` before applying triage labels.
Read `docs/agents/domain.md` before drafting domain-related issues.

## Pull requests as a triage surface

**PRs as a request surface: no.**
