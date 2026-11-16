# Iteration 28 consolidated Verus runbook

Use unchanged pin571e04a19aba9f9b4bc0ea23fbe09182038f4e55 and non-login bash. [Handoff](../../../uplift/consolidated/iter28/HANDOFF.md), [trust](TRUST.md) and [bindings](../../../uplift/consolidated/iter28/bindings.json) distinguish owned preparation/sequence rules from parser/child/IO/provider qualifications. Guidance844 applies; unpublished/private evaluator inputs remain unread.

```sh
python3 .g5/uplift/consolidated/iter28/project_map.py
python3 .g5/uplift/consolidated/iter28/project_map.py --check
python3 .g5/Verus_verification/consolidated/iter28/run.py
python3 .g5/Verus_verification/consolidated/iter28/run.py --no-native
python3 .g5/uplift/consolidated/iter28/validate.py /absolute/path/to/ipm-iter28/baseline/members.json
python3 .g5/Verus_verification/consolidated/iter28/freshness.py check
```

Each run first checks that the selected settings declarations are an exact mechanical projection of their immutable original source: full original SHA, declaration spans/hashes and complete projected text must match. Every run also checks the actual raw file hashes, literal functions, case mapping and mechanical JSON-tree adapter. No inherited proof functions are extracted or counted. Raw ASCII fixture text, ordered JSON pairs and selected child commands determine the projected inputs; JSON parsing/command metadata/child interpreter/IO remain native-qualified. Prior default-node contents are not observed; default-state independence is required for the representative current-value view.

Native uses local command files only, one configurable key and a known exec expression that increments a process-private counter before division by zero. It does not call verify/package/export/archive setup. Actual GetValue and direct `%SYS` global reads agree, and command listing is checked independently from execution. All86 native fields have expected gates. Semantic controls, full proof, unchanged replay and opposite-observation rejection are separate gates. No assumed native values, admitted bodies, SDK-body proof, private-input or convergence claim.
