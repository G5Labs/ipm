# Iteration69 receipts

Command (repository root): `VERUS=~/.local/share/g5-verification/verus-0.2026.09.27.3cf1832/verus-x86-linux/verus IPM_CALL_TIMEOUT=1500 python3 .g5/Verus_verification/coverage-20261117/iter69/run.py`

`index.jsonl` lists the baseline proof, 13 controls, the native run, the replay bridge and the three flipped fields, each with command or inputs hash, exit, verified/errors counts and raw log. The harness's own records are `../proofs/models.json`, `../replay/receipt.json`, `../run.json` and `../native/<LATEST>/observations.json`. Control logs from compute-mode rejections end after the first failing `by (compute_only)` assertion, so they carry no summary count.
