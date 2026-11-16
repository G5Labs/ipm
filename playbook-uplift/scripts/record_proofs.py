#!/usr/bin/env python3
"""Verify Verus model files and record each proof function in evidence/proofs-<stem>.json.

  python3 -I scripts/record_proofs.py                    # every models/verus/*.rs
  python3 -I scripts/record_proofs.py models/verus/x.rs  # one file

Rules (each is enforced, not advisory):
- A proof function is recorded 'verified' only when its whole file verifies with 0 errors.
- A file that uses assume(, admit(, #[verifier::external_body] or #[verifier::external] is recorded 'not-verified'
  (trust inventory: no trusted items in model proofs).
- The file's controls (models/verus/controls/<stem>.json, run by models/verus/controls.py) must all be killed in the
  expected lemma; the result is copied in as controls_all_killed. The ontology gate refuses a P tag whose file has no
  controls or a surviving control.
"""
import json
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VERUS = "/Users/ziniuw/.cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus"
TRUST_RE = re.compile(r"\bassume\s*\(|\badmit\s*\(|verifier::external_body|verifier::external\b")


def record(src: Path) -> bool:
    text = src.read_text()
    t0 = time.time()
    p = subprocess.run([VERUS, str(src), "--rlimit", "30"], capture_output=True, text=True, timeout=1800)
    secs = round(time.time() - t0, 1)
    out = p.stdout + p.stderr
    m = re.search(r"verification results:: (\d+) verified, (\d+) errors", out)
    trusted = TRUST_RE.findall(re.sub(r"//.*", "", text))
    ok = p.returncode == 0 and m is not None and m.group(2) == "0" and not trusted
    names = re.findall(r"^\s*(?:pub )?proof fn (\w+)", text, re.M)
    ctl_path = ROOT / "models/verus/results" / f"{src.stem}.json"
    ctl = json.loads(ctl_path.read_text()) if ctl_path.exists() else {}
    ver = subprocess.run([VERUS, "--version"], capture_output=True, text=True).stdout.split("\n")[1].strip()
    data = {"file": str(src.relative_to(ROOT)), "verus": ver, "summary": m.group(0) if m else None, "seconds": secs,
            "trusted_items": trusted, "controls_all_killed": ctl.get("all_killed_in_expected_place"),
            "controls": len(ctl.get("controls", [])),
            "records": [{"name": n, "status": "verified" if ok else "not-verified", "file": str(src.relative_to(ROOT))} for n in names]}
    (ROOT / "evidence" / f"proofs-{src.stem}.json").write_text(json.dumps(data, indent=2) + "\n")
    (ROOT / "validation/logs" / f"40-verus-{src.stem}.log").write_text(out)
    print(f"{src.name}: {data['summary']} ({secs}s), {len(names)} proof fns, trusted={trusted or 'none'}, "
          f"controls={data['controls']} all_killed={data['controls_all_killed']}")
    return ok


def main(argv):
    files = [Path(a).resolve() for a in argv] or sorted((ROOT / "models/verus").glob("*.rs"))
    old = ROOT / "evidence" / "proofs.json"
    if old.exists():
        old.unlink()
    return 0 if all([record(f) for f in files]) else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
