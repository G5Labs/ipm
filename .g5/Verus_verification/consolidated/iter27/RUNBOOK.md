# Iteration 27 consolidated Verus runbook

Use unchanged product pin 571e04a19aba9f9b4bc0ea23fbe09182038f4e55 and a non-login shell. [Handoff](../../../uplift/consolidated/iter27/HANDOFF.md), [trust](TRUST.md) and [bindings](../../../uplift/consolidated/iter27/bindings.json) separate specification proofs, pinned source roles and native provider qualifications. Public scoring revision 844 applies; separate IRIS/unpublished evaluator inputs remain private.

```sh
python3 .g5/uplift/consolidated/iter27/project_map.py
python3 .g5/uplift/consolidated/iter27/project_map.py --check
python3 .g5/Verus_verification/consolidated/iter27/run.py
python3 .g5/Verus_verification/consolidated/iter27/run.py --no-native
python3 .g5/uplift/consolidated/iter27/validate.py /absolute/path/to/ipm-iter27/baseline/members.json
python3 .g5/Verus_verification/consolidated/iter27/freshness.py check
```

Native inputs are two plain directory modules with one persistent class each; one supplies a supported preload installer whose callback deletes its own extent when the nested purge argument is true. Do not use verify/package/export/archive delivery for setup or a reader. Deployed mapping was set aside after metadata-only SDK assessment; no producer/import was attempted. Archive investigation/modeling/probing/reproduction remains excluded.

All 111 native fields have explicit expected gates. Replay derives data count/value and selected callback fields from actual raw command alternatives and the selected callback implementation; successful IO, compilation, class/storage SDKs and SQL execution remain provider qualifications. Opposite observations must reject semantically using unchanged expressions. No admitted bodies, assumed native values or resource/type/parse error credit. Direct ObjectScript-body proofs remain zero. No convergence follows from successful proofs or locator counts.
