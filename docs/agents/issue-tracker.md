# Issue tracker: GitHub

Issues and PRDs for this repo live as GitHub issues. Use the `gh` CLI for all operations.

## Repository

Use `unpack-dev/unpack`.

When running `gh` commands from inside this clone, infer the repo from `git remote -v`.

## Conventions

- **Create an issue**: `gh issue create --title "..." --body "..."`. Use a heredoc for multi-line bodies.
- **Read an issue**: `gh issue view <number> --comments`, filtering comments by `jq` and also fetching labels when needed.
- **List issues**: `gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'` with appropriate `--label` and `--state` filters.
- **Comment on an issue**: `gh issue comment <number> --body "..."`
- **Apply / remove labels**: `gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- **Close**: `gh issue close <number> --comment "..."`

## Pull requests as a triage surface

**PRs as a request surface: no.**

Do not pull external PRs into the `/triage` queue. Treat `/triage` as an issue-only workflow for this repo unless this file is updated.

## When a skill says "publish to the issue tracker"

Create a GitHub issue in `unpack-dev/unpack`.

## When a skill says "fetch the relevant ticket"

Run `gh issue view <number> --comments` from inside this repo.
