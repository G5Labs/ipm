# TLC developer tooling

These utilities and the executable tutorial are for developing and testing verification tooling. They are not shipped in the customer agent harness. Project TLA+/TLC artifacts belong under `.g5/TLA_verification/`; this helper does not prescribe their format. The [verification artifact reader](verification-artifact-reader.md) describes how the ontology viewer reads them.

## Run and inspect evidence

Run these commands from the repository root:

```sh
python3 tools/tlc/scripts/model_evidence.py run MANIFEST RUN_DIRECTORY --root CHECKOUT
python3 tools/tlc/scripts/model_evidence.py check MANIFEST RUN_DIRECTORY --root CHECKOUT
```

`run` requires a new output directory. `check` verifies saved evidence without executing the model. The manifest is JSON; paths and command working directory are relative to `--root` (default: the manifest's directory):

```json
{
  "command": ["python3", "verification/tlc_adapter.py"],
  "timeout_seconds": 300,
  "inputs": {
    "formal": ["verification/Concern.tla", "verification/Concern.cfg"],
    "tool": ["verification/tlc_adapter.py", "verification/tla2tools.jar"],
    "ontology": ["ontology/contract.md"],
    "binding": ["verification/bindings.md"],
    "implementation": ["src/operation.ext"]
  },
  "expected": {"outcome": "satisfied", "properties": ["ContractInvariant"]}
}
```

Replace the illustrative paths and include all actual dependencies; the helper does not discover them. `formal` and `tool` require nonempty file lists. Include the relevant ontology, binding and implementation files for claims about current code, plus other input roles where needed. Declare relevant environment assumptions in versioned inputs.

The project-owned adapter waits for TLC to finish, preserves its raw stdout/stderr in the wrapper's logs, and writes a completed structured result to the absolute path in `G5_MODEL_RESULT`, for example `{"complete":true,"outcome":"satisfied","properties":["ContractInvariant"]}`. Supported outcomes are `satisfied`, `violated` and `witness`. The result must match the declared outcome and property set; adapters must derive this from actual checker evidence. The adapter exits zero for a successfully interpreted completed result, including a violation or witness, even if TLC uses a nonzero exit for that result; parsing or execution failures must not be normalized into success. The helper separately checks whether the reported result matches the expectation. Use separate configurations when claims need different outcomes.

A qualified [TLC adapter](scripts/tlc_adapter.py) and [executable refinement example](examples/tlc-refinement/README.md) are available for the documented TLC 2.19 message/configuration subset. This adapter also rejects mismatched outcomes itself; the wrapper independently checks them. The example exercises good refinement, wrong abstraction and write variants, weak/stuck models, path-specific witnesses and real parser/resource failures. Its driver verifies expected acceptances and rejections, not a count of safety passes. Use another qualified project adapter for unsupported versions or features; extend support with actual-output tests rather than weakening rejection checks.

Each run stores `stdout.log`, `stderr.log`, `result.json` when produced, and `receipt.json`. The helper fingerprints the manifest, declared inputs, executable and itself, detects changes between execution snapshots, and rejects missing, corrupted, incomplete, timed-out or mismatched evidence. Exit 0 means the completed evidence matches its declared expectation; exit 1 means invalid/failed evidence, and run setup errors use exit 2. Matching an expected violation remains a counterexample/control, not a safety pass.

This helper verifies evidence integrity and input identity, not model meaning, dependency completeness or runtime conformance. It trusts the adapter's interpretation, cannot detect omitted dependencies or transient edits restored between snapshots, and is not tamper-proof attestation. Isolate inputs and review the model, adapter and correspondence. See [its tests](scripts/test_model_evidence.py) for the validator's failure cases; project adapters still need their own TLC-specific validation.

## Tests

```sh
python3 -m unittest discover -s tools/tlc/scripts -p "test_*.py" -v
```

Set `TLC_JAVA` and `TLC_JAR` to the supported installed Java and TLC tools for the real-tool tests; otherwise those tests explicitly skip. See the tutorial README for the supported versions and full example command.
