#!/usr/bin/env python3
"""Rerun an original Verus proof, control or explorer run on the current inputs, without writing
into the original results tree. Run with python3 -I -B from .g5/Verus_verification.

  python3 -I -B followup/bin/verus_check.py verus   LABEL EXPECT [--features F]
  python3 -I -B followup/bin/verus_check.py explore RUN-NAME EXPECT

The commands, toolchain pin and verdict parsing are those of crates/scripts/receipt.py (original
submission). The differences: the receipt goes to followup/results/<UTC>-<LABEL>/, the build uses
CARGO_TARGET_DIR under $IPM_UPLIFT_WORK, and results/index.jsonl is not touched.
"""
import datetime, hashlib, json, os, pathlib, re, subprocess, sys

VV = pathlib.Path(__file__).resolve().parents[2]
CR = VV / "crates"
VD = pathlib.Path.home() / ".local/share/g5-verification/verus-0.2026.09.27.3cf1832/verus-x86-linux"
WORK = pathlib.Path(os.environ.get("IPM_UPLIFT_WORK") or sys.exit("IPM_UPLIFT_WORK is not set"))
ENV = dict(os.environ, CARGO_NET_OFFLINE="true", CARGO_TARGET_DIR=str(WORK / "verus-target"),
           RUSTUP_TOOLCHAIN="1.98.1-x86_64-unknown-linux-gnu")


def sha(p):
    return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()


def inputs():
    files = sorted(list((CR / "ipm-load-history").rglob("*.rs")) + [CR / "ipm-load-history/Cargo.toml", CR / "Cargo.toml", CR / "Cargo.lock"]
                   + list((VV / "vendor").rglob("*")) + list((CR / "scripts").glob("*")))
    return [(str(f.relative_to(VV)), sha(f)) for f in files if f.is_file()]


def run(cmd, timeout):
    t = WORK / "verus-target"
    for fp in ("debug/.fingerprint", "verus-partial/debug/.fingerprint"):
        for f in (t / fp).glob("ipm-load-history-*") if (t / fp).exists() else []:
            subprocess.run(["rm", "-rf", str(f)])
    try:
        p = subprocess.run(cmd, cwd=CR, capture_output=True, text=True, env=ENV, timeout=timeout)
        return p.returncode, p.stdout + p.stderr, False
    except subprocess.TimeoutExpired as e:
        return None, (e.stdout.decode() if isinstance(e.stdout, bytes) else (e.stdout or "")), True


def main():
    kind, label, expect = sys.argv[1], sys.argv[2], sys.argv[3]
    rest = sys.argv[4:]
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    d = VV / "followup" / "results" / f"{stamp}-{label}"
    d.mkdir(parents=True)
    rec = {"run_id": d.name, "kind": kind, "label": label, "expect": expect, "product": "571e04a19aba9f9b4bc0ea23fbe09182038f4e55",
           "verus": "0.2026.09.27.3cf1832", "rust": "1.98.1-x86_64-unknown-linux-gnu", "cargo_target_dir": ENV["CARGO_TARGET_DIR"]}
    if kind == "verus":
        feats = ["--features", rest[rest.index("--features") + 1]] if "--features" in rest else []
        cmd = [str(VD / "cargo-verus"), "verify", "-p", "ipm-load-history", "--offline"] + feats + ["--lib", "--", "--num-threads", "4"]
        code, log, to = run(cmd, 3000)
        m = re.findall(r"verification results:: (\d+) verified, (\d+) errors", log)
        failed_fns = []
        for blk in re.split(r"\n(?=error)", log):
            loc = re.search(r"-->\s*ipm-load-history/src/(\S+?):(\d+):", blk) if blk.startswith("error") else None
            if not loc:
                continue
            src = (CR / "ipm-load-history/src" / loc.group(1)).read_text().splitlines()
            for k in range(int(loc.group(2)) - 1, -1, -1):
                mt = re.match(r"\s*pub (?:open )?(?:proof|spec) fn (\w+)", src[k])
                if mt:
                    failed_fns.append(f"{loc.group(1)}:{mt.group(1)}")
                    break
        verr = len(re.findall(r"^error: (?:postcondition not satisfied|assertion failed|expression simplifies to false|precondition not satisfied|function body check)", log, re.M))
        if not to and not m and verr:
            obs = f"verification errors {verr} (no summary line); failing: {sorted(set(failed_fns))}"
        elif to or not m:
            obs = "INCOMPLETE (timeout or no verification summary)"
        else:
            v = sum(int(a) for a, _ in m)
            e = sum(int(b) for _, b in m)
            obs = f"verified {v}, errors {e}" + (f"; failing: {sorted(set(failed_fns))}" if e else "")
        rec.update({"features": feats[1] if feats else None, "observed": obs, "exit_code": code})
    elif kind == "explore":
        b = subprocess.run(["cargo", "build", "--release", "--offline", "-p", "ipm-load-history", "--bin", "b1"], cwd=CR, env=ENV, capture_output=True, text=True)
        cmd = [str(WORK / "verus-target/release/b1"), "explore", label]
        code, log, to = run(cmd, 900)
        r = [json.loads(l[7:]) for l in log.splitlines() if l.startswith("RESULT ")]
        obs = "INCOMPLETE (timeout)" if to else (r[0]["observed"] if r else "INCOMPLETE (no RESULT line)")
        rec.update({"observed": obs, "exit_code": code, "explorer": r[0] if r else None, "build_rc": b.returncode, "build_log_tail": b.stderr[-500:]})
    else:
        sys.exit("unknown kind")
    (d / "command.txt").write_text(" ".join(cmd) + "\n")
    ins = inputs()
    (d / "inputs.sha256").write_text("".join(f"{h}  {f}\n" for f, h in ins))
    (d / "raw.log").write_text(log)
    rec["inputs_digest"] = hashlib.sha256("".join(f"{h}{f}" for f, h in ins).encode()).hexdigest()
    (d / "receipt.json").write_text(json.dumps(rec, indent=2) + "\n")
    print(json.dumps({"run_id": d.name, "observed": rec["observed"], "expect": expect}))


if __name__ == "__main__":
    main()
