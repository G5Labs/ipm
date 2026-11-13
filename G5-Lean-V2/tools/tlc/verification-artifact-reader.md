# Verification artifact reader

How the Core Ontology viewer reads a project's `TLA_verification/` folder to show a statement's verification details (`g5.documents.ontology.verification`). This is developer documentation; agents follow the [verification foundation guide](../../.agents/guidance/TLC/setup_foundation.md#artifact-paths) for these formats.

## Discovery

- A statement's metadata links state machines. Any link into `TLA_verification/models/<concern>/` (the manifest, `BINDINGS.md`, a module or a configuration) identifies that concern; links are resolved like other ontology links, and links in code examples do not count. `link_verification.py` in the guidance scripts adds the links from each manifest's `ontology_ids`.
- The folder containing `models/` is the verification root. Each linked concern's `manifest.json` is read directly, trying the link's possible roots in order; nothing is listed, so folders hidden from Files work too. The run index of each root where a manifest was found is then read once. One query does this for every statement in a file.
- A concern's evidence is found when its folder has `manifest.json` and the run index has a row whose `concern` is the folder name.
- Reads are cached for 30 seconds. Evidence older than that is read again when the ontology file changes, and Retry reads failed evidence again.

## Fields read

- **Manifest** (`models/<concern>/manifest.json`):
  - `title` (optional; otherwise the folder name with dashes as spaces, capitalized);
  - `question`;
  - `runs[].label`, `spec`, `cfg`, `kind`, `variant`, `expect`.

  `bindings` keeps its project meaning, a list of code correspondences, and is not interpreted. `BINDINGS.md` in the same folder is rendered as saved, with links resolved from its own folder.
- **Run index rows** (`results/index.jsonl`): `run_id`, `concern`, `label`, `kind`, `variant`, `expect`, `outcome`, `distinct_states`, `states_generated`, `depth`. Unknown fields are ignored; rows without `concern`, `label` and `outcome` are counted as unreadable and skipped. Empty strings count as absent, and a leading byte-order mark is ignored.
- **Receipt** path: `results/<concern>/<run_id>/receipt.json`, linked, not parsed.

## Interpretation

- **Matched expectation:** outcome `PASS` when the expectation (the row's `expect`, else the manifest run's) is `pass` or absent, or any outcome beginning `EXPECTED_`. `ERROR`, `TIMEOUT` and `RESOURCE` are incomplete runs; anything else is a mismatch. Outcomes are always shown as saved.
- **Latest run of a label:** the greatest `run_id` when every run ID of that label starts with a `YYYYMMDDTHHMMSSZ` timestamp, otherwise the last row in the index. The timestamp, when present, is also the run date shown.
- **Declared runs:** the manifest's `runs` in order. Index labels the manifest no longer declares are listed as retired runs. The kind legend explains only the kinds shown.
- The developer `model_evidence.py` format in this folder (`satisfied`, `violated`, `witness`) is not read; a manifest in that format shows the unsupported-format note.

## Limits and failures

- The index is read whole, so it must fit the 32 MiB file-read limit.
- A statement whose evidence is missing or unreadable shows Unverified, which opens no details. Within a Design verified statement's details, a missing or unreadable linked state machine is shown as such, with Retry, never as an empty result. A manifest that is not JSON or lacks `question` or `runs` still counts as found; its panel shows an unsupported-format note with a link to the file.
- Applicability, freshness and validation records are not read.
