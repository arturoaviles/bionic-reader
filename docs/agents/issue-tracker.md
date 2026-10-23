# Issue tracker: GitHub

Issues and specs live in GitHub Issues for `arturoaviles/bionic-reader`.
Use the `gh` CLI from this clone.

## Conventions

- Create: `gh issue create --title "..." --body-file <file>`.
- Read: `gh issue view <number> --json number,title,body,labels,comments`.
- List: `gh issue list --state open --json number,title,body,labels`.
  Apply label and state filters as needed.
- Comment: `gh issue comment <number> --body-file <file>`.
- Apply labels: `gh issue edit <number> --add-label "<label>"`.
- Remove labels: `gh issue edit <number> --remove-label "<label>"`.
- Close: `gh issue close <number>`.
- Use temporary files containing actual newlines for multiline bodies.

Infer the repository from the git remote.

## Pull requests as a triage surface

**PRs as a request surface: no.**

## Skill operations

When a skill says "publish to the issue tracker", create a GitHub issue.
When it says "fetch the relevant ticket", read the GitHub issue.

## Wayfinding

- The map is one issue labelled `wayfinder:map`, containing Notes,
  Decisions-so-far, and Fog.
- Link child tickets using GitHub sub-issues when available.
  Otherwise, list children in the map and begin each child with
  `Part of #<map>`.
- Use `wayfinder:<type>` labels for research, prototype, grilling,
  and task tickets.
- Record blockers with native GitHub issue dependencies when available.
  Otherwise, begin the child body with `Blocked by: #<number>`.
- Select the first open, unassigned child in map order with no open blockers.
- Claim with `gh issue edit <number> --add-assignee @me`.
- Resolve by commenting with the answer, closing the child, and adding
  a summary and link to the map's Decisions-so-far.
