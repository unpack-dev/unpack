# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Layout

This is a single-context repo.

## Before exploring, read these

- `CONTEXT.md` at the repo root.
- Relevant ADRs under `docs/adr/`.

If either location does not contain relevant context, proceed silently. Do not suggest creating extra documentation upfront; domain docs should be updated when terms or decisions actually get resolved.

## Use the glossary's vocabulary

When output names a domain concept in an issue title, PRD, refactor proposal, hypothesis, or test name, use the term as defined in `CONTEXT.md`. Do not drift to synonyms the glossary explicitly avoids.

If the concept is missing from the glossary, either reconsider whether the language fits this project or note the gap for domain modeling.

## Flag ADR conflicts

If output contradicts an existing ADR, surface that conflict explicitly instead of silently overriding it.
