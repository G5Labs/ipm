# Owner request

2026-11-09: “Add Fable 5.2 to the list of models. It was just released. Update claude
code as needed. Sort the list of models in the composer picker by when they were
released. Set the default model to opus 5.5 for anyone that didn't change it yet.
Remove redundant 1M options”

Interpretation:

- Offer Claude Fable 5.2 (`claude-fable-5-2`, OpenRouter `anthropic/claude-fable-5.2`),
  released by Anthropic on 2026-11-09.
- Claude Code accepts Fable 5.2 from 2.1.321 (its model configuration contract names
  that version), so the shared runtime pin moves from 2.1.308 to 2.1.321. That release
  is npm's `latest`, not its `stable` tag (2.1.311 at the time); “as needed” selects the
  release that serves the requested model, after evaluating the delta.
- The composer picker lists Claude models by Anthropic's release day, newest first; the
  catalog records the day per family so the order is data, not a hand-kept list.
- The bundled Claude chat default becomes Opus 5.5. A person's saved choice equal to the
  previous bundled default (Opus 4.8's slug, or its `[1m]` direct id) was never changed by
  them and follows the new default; every other saved choice is kept.
- “Redundant 1M options” are the `[1m]` direct variants: Fable, Sonnet 5 and later, and
  Opus 4.7 and later run their 1M window without the suffix, so each offered family keeps
  one plain form. Saved `[1m]` selections stay resolver-known.
