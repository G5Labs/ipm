# Merge to Base and a moved target

## Human instructions

- Work on [#582](https://github.com/G5Labs/G5-Lean/issues/582), including its second
  failure mode: Merge to Base refuses after Base moved since the PR was published.
- Use the Prepare to Ship dialog's wording in instructions ("Get the latest changes
  from Base").
- "Instead of telling the user to click 'update pr', can the merge button just resolve
  the issue directly? This stale pointer seems like an unnecessary burden."

## Interpretation

The recorded publication-time target is the stale pointer. Merge to Base should verify
the target it actually merges onto at merge time instead of asking for a republication
that changes nothing on GitHub.
