# Domain docs

## Before exploring

This repo uses a single-context layout:

- `GLOSSARY.md` at the repo root defines domain vocabulary.
- `docs/adr/` contains architecture decision records.

Read the glossary and ADRs relevant to the area being explored.
If they do not exist, proceed silently. Domain-modeling creates them
lazily when terms or decisions are resolved.

## Use domain vocabulary

Use the glossary's terms in issue titles, proposals, hypotheses,
and test names. Follow its distinctions and preferred terminology.

When a needed concept is missing, reconsider whether the project
uses it; record genuine vocabulary gaps for domain-modeling.

## Surface decision conflicts

If a proposal contradicts an existing ADR, identify the ADR and
explain why its decision should be reconsidered.
