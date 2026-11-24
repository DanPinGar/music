# 0001. Record architecture decisions

- Status: accepted
- Date: 2026-11-24

## Context

Segno mixes three languages (TypeScript, Rust, Python), a desktop shell and a future cloud
deployment. Many choices (where state lives, how components talk, which licences are allowed)
are hard to reverse and easy to forget the reasons for.

## Decision

Significant decisions are recorded as short architecture decision records in `docs/adr`,
numbered in order and never rewritten: a changed decision gets a new ADR that supersedes the old
one. The decisions taken while planning are summarised in the decision log of `docs/PLAN.md`.

## Consequences

Each decision has a place to explain its context and trade-offs. Writing an ADR costs a few
minutes, so they are kept for decisions that affect more than one component or are hard to undo.
