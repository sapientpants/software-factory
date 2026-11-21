# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- **`GLOSSARY.md`** at the repo root. It is the one home of the spec's terms (D-21).
- **The decisions table of `SPEC.md`, section 23.1.** It is this repo's only decision record. Read the rows that touch the area you're about to work in.

This repo is single-context: there is no `GLOSSARY-MAP.md`.

## File structure

```
/
├── GLOSSARY.md      ← the terms
├── SPEC.md          ← the design; decisions in section 23.1
└── docs/research/   ← the evidence behind the spec
```

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor proposal, a hypothesis, a test name), use the term as defined in `GLOSSARY.md`. Don't drift to synonyms the glossary explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal: either you're inventing language the project doesn't use (reconsider) or there's a real gap (note it for `/domain-modeling`).

## Changing the glossary

`GLOSSARY.md` is protected with the spec (BOOT-4), and the rules of "Editing the spec" in `AGENTS.md` apply to it. Unlike the skill's default format, a definition here cites the ids and section numbers of `SPEC.md`, so that it restates no rule.

## Decisions go in the spec, not in ADRs

This repo keeps no `docs/adr/` through M1 (D-21). When a skill would write an ADR, record the decision as a row of `SPEC.md` 23.1 and cite its id where the design depends on it.

## Flag conflicts with a settled decision

If your output contradicts a row of 23.1, surface it explicitly rather than silently overriding:

> _Contradicts D-18 (`SPEC.md` stays one file through M1), but worth reopening because…_
