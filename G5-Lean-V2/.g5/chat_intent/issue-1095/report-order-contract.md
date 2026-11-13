# Report section order ships as assignment schema 6 (2026-11-05)

## Human instruction

Issue [#1095](https://github.com/G5Labs/G5-Lean/issues/1095) section 1 asks newly generated agent review reports to start with
`## Governing prompt` and end with `## Review scope and provenance`, preserving the canonical frontmatter/schema and stored reports.

Asked how to ship it, given that published formal-review contract assets are immutable, the owner chose
**"Schema 6, same fields"**: new reviews use assignment schema 6, whose frontmatter has exactly schema 5's fields and rules
(only `schemaVersion` reads 6), executed under a new contract `g5.formal-agent-approval/v5` and new formal runtime modules.
The rejected alternatives were keeping schema 5 with contract selection by recorded contract version, and deferring item 1.

## Interpretation

"Preserve the canonical frontmatter/schema" means the frontmatter fields, validation and result vocabulary; the schema number
is the existing selector of the frozen contract and runtime module. Retries of earlier assignments keep their published assets.
