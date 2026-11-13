# Publishing workflow-file changes

## Human instructions

- Work on [#612](https://github.com/G5Labs/G5-Lean/issues/612): support authorized
  publication of workflow edits with an appropriately scoped token including
  `workflows: write`, without silently switching identities or requesting every App
  permission; keep ordinary pushes working without that grant; return an actionable,
  sanitized publication error instead of HTTP 500; move the premature success log.
- Issue comment (zcl-g5): is GitHub's refusal an intended protection or a
  misconfiguration? It seems impossible to change `.github/workflows` from G5.

## Interpretation

GitHub's protection stays authoritative: G5 publishes workflow changes through the App
only when the organization's installation grants Workflows write. The defect was that
G5 narrowed every write token to `contents: write` even when the grant existed, and
reported the expected refusal as an internal error.
