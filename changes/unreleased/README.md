# Release-note fragments

One user-visible change adds one file here and does not touch `CHANGELOG.md`, so two pull requests
never edit the same lines.

- File name: `<issue-or-PR-number>-<short-slug>.md`, or the branch name when there is no number
  (`changelog-fragments-377.md`). It must be unique, end in `.md` and not be `README.md`.
- Content: one Markdown bullet in the style of `CHANGELOG.md`, starting `- `, with optional indented
  sub-bullets. Name the issue as `(#NNN)`. No heading, no blank-line-separated paragraphs.
- Docs-only and internal work needs no fragment.

`rust/tools/release.ps1 -Prepare -Version X.Y.Z` appends every fragment to the new `## X.Y.Z` section
in file-name order (ordinal), commits the deletions and refuses a fragment that does not start with `- `.
A `## Unreleased` section in `CHANGELOG.md`, if one is still there, is folded in first.
