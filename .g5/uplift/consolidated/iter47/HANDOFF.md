# Iteration 47: test report formats (JSON, YAML, TOON)

Batch 8 lane, product `571e04a1` unchanged; not sealed or committed (the main thread integrates). [Model](../../../Verus_verification/consolidated/iter47/formats.rs), [cases and controls](../../../Verus_verification/consolidated/iter47/cases.json), [host observer](../../../Verus_verification/consolidated/iter47/report_observer.py), [bindings](bindings.json), [ledger with predictions](working-ledger.json). Owner: [build.test-output](../../../ontology/features/build-and-publish.g5.md#build.test-output).

## Gap

`Test/YamlOutput.cls` and `Test/ToonOutput.cls` had no binding. JSON and the shared tree and summary were already bound (iteration17 and earlier). `Test/Utils.cls` turned out to be namespace and user helpers with no product caller, so it is left unassessed rather than counted.

## Predictions and observations

Fixture `r47` (developer mode) has six test methods: a pass; a failure whose message contains a colon, comma, quotes and `#`; a message with LF; a message with CR LF; a message starting with two spaces; and a method that divides by zero. Each command is `test r47 -only -f <fmt> -verbose -output-file r.<fmt>`, plus `.xml` and a quiet YAML summary. Files are read as raw bytes and parsed on the host with `json`, PyYAML 6.0.1 and RFC 4180 `csv`.

| Fact | JSON | YAML | TOON | Prediction |
|---|---|---|---|---|
| methods / failed (file and device) | 6 / 5 | 6 / 5 | 6 / 5 | equal (hit) |
| punctuation message | exact | exact plus trailing `\n` | exact | YAML block-scalar newline (hit) |
| LF message | exact | exact plus trailing `\n` | exact | hit |
| CR LF message | exact (`\r` escaped) | CR missing (`one\ntwo`) | CR missing | predicted YAML parse break: miss. The CR never reaches the file, so both formats lose it |
| leading-spaces message | exact | `leading spaces\n` (spaces lost) | exact | hit |
| thrown error text | present | present | present | hit |
| rows for the erroring method | 1 method | 1 method | 2 rows (error, then `failed,,,,""`) | source-predicted |
| file written | yes | yes | yes; JUnit `.xml` present | hit |

All `test` commands return an error status because tests fail, as expected. Explore1's text-mode file reader left CR handling ambiguous. Explore2 used byte reads. The observer was then fixed to cut IPM's own phase lines from the device captures (a reader fix only). Both logs are kept in `attempts/`. Device-side CR facts are inconclusive because the capture splits lines.

## Evidence

- Formal: `formats.rs` 7 verified, 0 errors. Four controls were each rejected: `toon-keeps-cr`, `yaml-no-trailing-newline`, `yaml-keeps-leading-spaces` and `toon-single-row-for-error`.
- Native: [receipt](native/20261116T180053540899Z/observations.json), 71 fields, all equal to pre-run gates.
- Replay: 33 fields in Verus ([receipt](replay/receipt.json)). Flips of `yamlf.stripped.TestELeading`, `toonf.exact.TestDCrLf` and `toonf.rows.TestFThrow` were each rejected.
- Command: `python3 .g5/Verus_verification/consolidated/iter47/run.py` (13 s; needs host PyYAML).

## Reach candidates

Newly reached: `YamlOutput::{BuildYaml, BuildYamlRows, EscapeYamlString, OutputToDevice, WriteToFileStream}` and `ToonOutput::{BuildToonRows, EscapeToonField, OutputToDevice, WriteToFileStream}`, 9 members. Strengthened: `Abstract::{BuildResultTree, GetSummary, ToFile}` and `JsonOutput::{WriteToFileStream, OutputToDevice}`.

## Limits

The model messages are short stand-ins, and rendering is per character. Exactly where the CR is dropped (stream copy) is a native-qualified premise. YAML read-back semantics are PyYAML's. JUnit content, `-quiet` and empty suites were not compared. No ObjectScript body is verified directly.

## Proposed owner text

- **`build.test-output`, added bullet:** "All formats print the same method and assertion counts. JSON reports keep assertion and error text exactly. As built, YAML and TOON files drop carriage returns from that text, and YAML descriptions read back with a trailing newline and without the first line's leading spaces (defect C:D-next)."
- **`build.test-output`, added bullet:** "TOON writes one row per assertion. A method with an error and no assertions gets an error row and a status row."
- **Metadata:** "Iteration47 formats: [formats.rs](../../Verus_verification/consolidated/iter47/formats.rs) `fixture_rendering`, `summaries_agree`, `error_method_two_toon_rows` ([handoff](../../uplift/consolidated/iter47/HANDOFF.md))."
- **C register entry, Low · Decision: Open: "YAML and TOON test reports change assertion message text".**
  - Found by: iteration47 exploration (byte-exact file reads), then source review of `YamlOutput::BuildYamlRows` (block scalars that re-indent only LF) and `ToonOutput::BuildToonRows`, and the iteration47 model `fixture_rendering` with controls `toon-keeps-cr`, `yaml-no-trailing-newline` and `yaml-keeps-leading-spaces`.
  - Reproduced by: E2E reproduced, receipt `iter47/native/20261116T180053540899Z`. For a message `"one"_$c(13,10)_"two"`, the JSON file keeps `one\r\ntwo` while the YAML and TOON files read back `one\ntwo`. `"  leading spaces"` reads back from YAML as `leading spaces\n`, and every YAML description gains a trailing newline. Passing neighbours: JSON is exact for all messages, TOON is exact apart from CR, and the counts agree.
  - Repair, unchecked: write YAML descriptions as escaped double-quoted scalars (or with `|-` and an indentation indicator), and escape CR in TOON quoted fields.
