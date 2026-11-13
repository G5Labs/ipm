#!/usr/bin/env python3
"""Run one B1 check and keep its receipt.

usage: receipt.py verus   <label> <variant> <expect> <concern> [--features F] [--verus-args ...]
       receipt.py explore <run-name> <variant> <expect> <concern>
       receipt.py cases   <label>
       receipt.py survey  <label>

Writes VV/results/verus/<UTC>-<label>/{command.txt,versions.txt,inputs.sha256,raw.log,receipt.json}
and appends one line to VV/results/index.jsonl. `expect` is the substantive verdict expected
("verified, 0 errors", "failure of <lemma>", "pass", "violation of <property>"). The observed
verdict is parsed from the raw log; a timeout or missing verdict is recorded as INCOMPLETE.
"""
import datetime, hashlib, json, os, pathlib, re, subprocess, sys

VV = pathlib.Path(__file__).resolve().parents[2]
CR = VV / "crates"
VD = pathlib.Path.home() / ".local/share/g5-verification/verus-0.2026.09.27.3cf1832/verus-x86-linux"
ENV = dict(os.environ, CARGO_NET_OFFLINE="true", CARGO_TARGET_DIR=str(CR / "target"),
           RUSTUP_TOOLCHAIN="1.98.1-x86_64-unknown-linux-gnu")


def sha(p):
    return hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()


def inputs():
    files = sorted(list((CR / "ipm-load-history").rglob("*.rs")) + [CR / "ipm-load-history/Cargo.toml", CR / "Cargo.toml", CR / "Cargo.lock"]
                   + list((VV / "vendor").rglob("*")) + list((CR / "scripts").glob("*")))
    return [(str(f.relative_to(VV)), sha(f)) for f in files if f.is_file()]


def versions():
    out = []
    for cmd in ([str(VD / "verus"), "--version"], [str(VD / "z3"), "--version"], ["rustc", "--version"], ["cargo", "--version"]):
        try:
            out.append(subprocess.run(cmd, capture_output=True, text=True, env=ENV, timeout=60).stdout.strip())
        except Exception as e:  # noqa
            out.append(f"{cmd[0]}: {e}")
    out.append("vstd =0.0.0-2026-09-20-0158; verus_state_machines_macros =0.0.0-2026-09-06-0133 (Cargo.lock)")
    return "\n".join(out)


def run(cmd, timeout):
    for f in (CR / "target/debug/.fingerprint").glob("ipm-load-history-*") if (CR / "target/debug/.fingerprint").exists() else []:
        subprocess.run(["rm", "-rf", str(f)])
    try:
        p = subprocess.run(cmd, cwd=CR, capture_output=True, text=True, env=ENV, timeout=timeout)
        return p.returncode, p.stdout + p.stderr, False
    except subprocess.TimeoutExpired as e:
        return None, (e.stdout or b"").decode() if isinstance(e.stdout, bytes) else (e.stdout or ""), True


def main():
    kind, label = sys.argv[1], sys.argv[2]
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    run_id = f"{stamp}-{label}"
    d = VV / "results/verus" / run_id
    d.mkdir(parents=True)
    rec = {"run_id": run_id, "kind": kind, "label": label, "product": "571e04a19aba9f9b4bc0ea23fbe09182038f4e55",
           "verus": "0.2026.09.27.3cf1832 (bank-of-ziniu pin)", "z3": "4.16.0 (bundled)", "rust": "1.98.1-x86_64-unknown-linux-gnu"}
    if kind == "verus":
        variant, expect, concern = sys.argv[3], sys.argv[4], sys.argv[5]
        rest = sys.argv[6:]
        feats = []
        vargs = []
        if "--features" in rest:
            i = rest.index("--features"); feats = ["--features", rest[i + 1]]; rest = rest[:i] + rest[i + 2:]
        if "--verus-args" in rest:
            i = rest.index("--verus-args"); vargs = rest[i + 1:]
        cmd = [str(VD / "cargo-verus"), "verify", "-p", "ipm-load-history", "--offline"] + feats + ["--lib", "--", "--num-threads", "4"] + vargs
        code, log, to = run(cmd, 3000)
        m = re.findall(r"verification results:: (\d+) verified, (\d+) errors", log)
        fails = sorted(set(re.findall(r"-->\s*ipm-load-history/src/(\S+?):\d+", log)))
        failed_fns = []
        # map each error's first location to its enclosing proof fn or spec fn (exec twin)
        for blk in re.split(r"\n(?=error)", log):
            if not blk.startswith("error"):
                continue
            loc = re.search(r"-->\s*ipm-load-history/src/(\S+?):(\d+):", blk)
            if not loc:
                continue
            f, ln = loc.group(1), int(loc.group(2))
            src = (CR / "ipm-load-history/src" / f).read_text().splitlines()
            for k in range(ln - 1, -1, -1):
                mt = re.match(r"\s*pub (?:open )?(?:proof|spec) fn (\w+)", src[k])
                if mt:
                    failed_fns.append(f"{f}:{mt.group(1)}"); break
        verr = len(re.findall(r"^error: (?:postcondition not satisfied|assertion failed|expression simplifies to false|precondition not satisfied|function body check)", log, re.M))
        if not to and not m and verr:
            obs = f"verification errors {verr} (no summary line: a failed evaluation lemma stops Verus before its summary); failing: {sorted(set(failed_fns))}"
        elif to or not m:
            obs = "INCOMPLETE (timeout or no verification summary)"
        else:
            v = sum(int(a) for a, _ in m); e = sum(int(b) for _, b in m)
            obs = f"verified {v}, errors {e}" + (f"; failing: {sorted(set(failed_fns))}" if e else "")
        rec.update({"variant": variant, "expect": expect, "concern": concern, "features": feats[1] if feats else None, "observed": obs, "exit_code": code})
    elif kind == "explore":
        variant, expect, concern = sys.argv[3], sys.argv[4], sys.argv[5]
        b = subprocess.run(["cargo", "build", "--release", "--offline", "-p", "ipm-load-history", "--bin", "b1"], cwd=CR, env=ENV, capture_output=True, text=True)
        cmd = [str(CR / "target/release/b1"), "explore", label]
        code, log, to = run(cmd, 900)
        r = [json.loads(l[7:]) for l in log.splitlines() if l.startswith("RESULT ")]
        obs = "INCOMPLETE (timeout)" if to else (r[0]["observed"] if r else "INCOMPLETE (no RESULT line)")
        rec.update({"variant": variant, "expect": expect, "concern": concern, "observed": obs, "exit_code": code, "explorer": r[0] if r else None,
                    "build_log_tail": b.stderr[-500:]})
    elif kind in ("cases", "survey", "repair-survey"):
        b = subprocess.run(["cargo", "build", "--release", "--offline", "-p", "ipm-load-history", "--bin", "b1"], cwd=CR, env=ENV, capture_output=True, text=True)
        out = VV / "cases/cases.json" if kind == "cases" else d / ("survey.json" if kind == "survey" else "repair-survey.json")
        out.parent.mkdir(parents=True, exist_ok=True)
        cmd = [str(CR / "target/release/b1"), kind, str(out)]
        code, log, to = run(cmd, 900)
        obs = "INCOMPLETE" if (to or code != 0 or not out.exists()) else f"written {out.relative_to(VV)} sha256 {sha(out)}"
        rec.update({"variant": "repair-103 vs as-built" if kind == "repair-survey" else "as-built", "expect": f"{kind} written", "concern": f"{kind} from the executable twin", "observed": obs, "exit_code": code,
                    "output": str(out.relative_to(VV)), "output_sha256": sha(out) if out.exists() else None})
        if kind == "cases" and out.exists():
            (d / "cases.json.sha256").write_text(f"{sha(out)}  cases/cases.json\n")
    else:
        sys.exit("unknown kind")
    (d / "command.txt").write_text(" ".join(cmd) + "\n")
    (d / "versions.txt").write_text(versions() + "\n")
    ins = inputs()
    (d / "inputs.sha256").write_text("".join(f"{h}  {f}\n" for f, h in ins))
    (d / "raw.log").write_text(log)
    rec["inputs_sha256_file"] = "inputs.sha256"
    rec["inputs_digest"] = hashlib.sha256("".join(f"{h}{f}" for f, h in ins).encode()).hexdigest()
    (d / "receipt.json").write_text(json.dumps(rec, indent=2) + "\n")
    line = {"concern": rec.get("concern"), "label": label, "outcome": rec["observed"], "run_id": run_id, "kind": {"verus": "proof" if not rec.get("features") else "control", "explore": "explore" if rec.get("variant") in ("as-built",) else "control", "cases": "cases", "survey": "explore", "repair-survey": "explore"}[kind],
            "variant": rec.get("variant"), "expect": rec.get("expect")}
    with open(VV / "results/index.jsonl", "a") as f:
        f.write(json.dumps(line) + "\n")
    print(json.dumps({"run_id": run_id, "observed": rec["observed"], "expect": rec.get("expect")}))


if __name__ == "__main__":
    main()
