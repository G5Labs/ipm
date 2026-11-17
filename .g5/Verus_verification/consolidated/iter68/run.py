"""Replay scoped Verus checks and disposable IRIS/OCI observations."""

import argparse
import concurrent.futures
import hashlib
import json
import os
import re
import subprocess
import tempfile
import time
import uuid
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
OUT = ROOT / ".g5/uplift/consolidated/iter68"
PIN = "571e04a19aba9f9b4bc0ea23fbe09182038f4e55"
IMAGE = os.getenv("IPM_IRIS_IMAGE", "ipm-consolidated-iris:571e04a1-fresh")
VERUS = Path(os.getenv("VERUS", Path.home() / ".cache/g5-verification/0.2026.09.27.3cf1832/verus-arm64-macos/verus")).resolve()
WHEEL = "wheels/oras-0.1.30-py3-none-any.whl"
EXPECTED = {
    "configured": 1, "initial-install": 1, "callable-before": 1,
    "refused-reinstall": 0, "refusal-is-integrity": 1, "provider-extracted": 1,
    "callable-after-refusal": 1, "root-preserved": 1, "refusal-preserves-module": 1,
    "failed-history": 1, "metadata-is-revision-1": 1, "revised-reinstall": 1,
    "callable-after-retag": 2, "replaced-root": 1,
    "reset-child": 1, "load-parent": 1, "dependency-callable": 2,
    "dependency-row-revision-2": 1,
}


def run(*command, **kwargs):
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=90, **kwargs)
    if result.returncode:
        raise RuntimeError(f"{command}: {result.returncode}\n{result.stdout}\n{result.stderr}")
    return result.stdout


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def product_inputs():
    paths = run("git", "ls-tree", "-r", "--name-only", PIN, "--",
                "src", "preload", "modules", "tests", "wheels", "module.xml").splitlines()
    pinned = subprocess.check_output(["git", "diff", "--name-only", PIN, "--",
                                      "src", "preload", "modules", "tests", "wheels", "module.xml"], cwd=ROOT)
    if pinned or set(paths) != set(run("git", "ls-files", "--cached", "--others", "--exclude-standard", "--",
                                       "src", "preload", "modules", "tests", "wheels", "module.xml").splitlines()):
        raise AssertionError("Product inputs differ from the pin. Establish a new native baseline.")
    return {path: digest((ROOT / path).read_bytes()) for path in paths}


def fixture_inputs():
    fixture = HERE / "native"
    return {p.relative_to(fixture).as_posix(): digest(p.read_bytes())
            for p in sorted(fixture.rglob("*")) if p.is_file() and "__pycache__" not in p.parts}


def exact_bindings():
    bindings = json.loads((OUT / "bindings.json").read_text())
    rows = []
    for row in bindings["rows"]:
        row = dict(row)
        owner, _, anchor = row["owner"].partition("#")
        assert f'id="{anchor}"' in (ROOT / owner).read_text(), row["owner"]
        model = (HERE / row["model"]).read_text()
        assert all(re.search(rf"\bfn {symbol}\b", model) for symbol in row["symbols"]), row
        ranges = []
        for locator, start, end, role in row.pop("source_ranges"):
            path, _, member = locator.partition("!")
            blob = (ROOT / path).read_bytes()
            if member:
                with zipfile.ZipFile(ROOT / path) as wheel:
                    content = wheel.read(member)
            else:
                content = blob
            lines = content.decode().splitlines(keepends=True)
            assert 1 <= start <= end <= len(lines), locator
            ranges.append({"path": path, **({"wheel_member": member} if member else {}),
                           "start": start, "end": end, "role": role,
                           "file_sha256": digest(blob),
                           "span_sha256": digest("".join(lines[start - 1:end]).encode())})
        rows.append({**row, "source_ranges": ranges})
    return {"product_pin": PIN, "rows": rows}


def native():
    container = "ipm-iter68-" + uuid.uuid4().hex[:12]
    fixture = "/tmp/iter68"
    out = OUT / "native"
    expected_inputs = product_inputs()

    def in_container(*args, **kwargs):
        return run("docker", "exec", "-i", container, *args, **kwargs)

    def session(*lines):
        return in_container("iris", "session", "IRIS", "-U", "USER", input="\n".join((*lines, "halt", "")))

    def mode(value):
        in_container("python3", "-c", f"from pathlib import Path; Path('{fixture}/mode').write_text('{value}')")

    logs = []
    raw_provider = ""
    try:
        run("docker", "run", "-d", "--network", "none", "--name", container, IMAGE)
        for _ in range(75):
            try:
                if "ITER68-READY" in session('write "ITER68-READY",!'):
                    break
            except RuntimeError:
                pass
            time.sleep(0.4)
        else:
            raise RuntimeError("IRIS did not become ready")

        inventory = """import json, hashlib, sys
from pathlib import Path
root = Path('/home/irisowner/zpm')
print(json.dumps({p: hashlib.sha256((root/p).read_bytes()).hexdigest() for p in json.load(sys.stdin)}))
"""
        assert json.loads(in_container("python3", "-c", inventory, input=json.dumps(list(expected_inputs)))) == expected_inputs

        with zipfile.ZipFile(ROOT / WHEEL) as wheel:
            members = {name: digest(wheel.read(name)) for name in
                       ("oras/provider.py", "oras/defaults.py", "oras/utils/fileio.py")}
        installed = inventory.replace("/home/irisowner/zpm", "/usr/irissys/mgr/python")
        assert json.loads(in_container("python3", "-c", installed, input=json.dumps(list(members)))) == members
        run("docker", "cp", str(HERE / "native"), f"{container}:{fixture}")
        run("docker", "exec", "-u", "root", container, "chmod", "-R", "a+rwX", fixture)
        mode("1")
        run("docker", "exec", "-d", container, "sh", "-c",
            f"python3 {fixture}/registry.py > {fixture}/registry.log 2>&1")
        for _ in range(40):
            try:
                in_container("python3", "-c", "import urllib.request; urllib.request.urlopen('http://127.0.0.1:18292/v2/').close()")
                break
            except RuntimeError:
                time.sleep(0.2)
        else:
            raise RuntimeError("Loopback registry did not become ready")

        logs.append(session(
            f'write $system.Status.GetErrorText($system.OBJ.Load("{fixture}/Probe.cls","ck")),!',
            'do ##class(Iter68.Probe).RunCommand("configured","repo -o -n boundary -url http://127.0.0.1:18292 -namespace fixture")',
            'do ##class(Iter68.Probe).RunCommand("initial-install","install boundary/g5-admission 1.0.0")',
            'do ##class(Iter68.Probe).RememberRoot()',
            'do ##class(Iter68.Probe).ReadMetadata()',
        ))
        mode("size-mismatch")
        logs.append(session(
            'set sc=##class(Iter68.Probe).RunCommand("refused-reinstall","reinstall g5-admission")',
            'do ##class(Iter68.Probe).Observe("refusal-is-integrity",$system.Status.GetErrorText(sc)["Module size and/or hash failed validation")',
            'do ##class(Iter68.Probe).AfterRefusal()',
        ))
        markers = in_container("find", "/tmp", "/usr/irissys/mgr", "-name", "g5-iter68-extraction-marker.txt")
        logs.append(f"OBS|provider-extracted|{int(bool(markers.strip()))}\n")
        mode("2")
        logs.append(session(
            'do ##class(Iter68.Probe).RunCommand("revised-reinstall","reinstall g5-admission")',
            'do ##class(Iter68.Probe).Observe("callable-after-retag",##class(Iter68.Probe).Revision())',
            'do ##class(Iter68.Probe).Observe("replaced-root",\'##class(%File).Exists(##class(Iter68.Probe).Root()_"g5-iter68-root-sentinel"))',
        ))
        logs.append(session('do ##class(Iter68.Probe).RunCommand("reset-child","uninstall g5-admission")'))
        mode("retag")
        logs.append(session(
            'do ##class(Iter68.Probe).RunCommand("load-parent","load -nodev /tmp/iter68/parent")',
            'do ##class(Iter68.Probe).Observe("dependency-callable",##class(Iter68.Probe).Revision())',
            'set child=##class(%IPM.Storage.Module).NameOpen("g5-admission")',
            'do ##class(Iter68.Probe).Observe("dependency-row-revision-2",child.Description="revision-2")',
        ))
        raw_provider = in_container("cat", f"{fixture}/registry.log")
        raw = "".join(logs)
        pairs = re.findall(r"(?m)^OBS\|([^|]+)\|(\d+)\s*$", raw)
        observed = {key: int(value) for key, value in pairs}
        assert len(pairs) == len(observed) and observed == EXPECTED, (observed, EXPECTED)
        manifests = {
            mode: {line.split("|")[-1] for line in raw_provider.splitlines()
                   if line.startswith(f"GET|{mode}|") and "/manifests/1.0.0|200|" in line}
            for mode in ("1", "size-mismatch", "2")
        }
        assert all(len(value) == 1 for value in manifests.values())
        assert len(set.union(*manifests.values())) == len(manifests)
        retag = [line.split("|")[-1] for line in raw_provider.splitlines()
                 if line.startswith("GET|retag|") and "/manifests/1.0.0|200|" in line]
        first, second = (next(iter(manifests[mode])) for mode in ("1", "2"))
        assert retag == [first, first, second, second], retag
        write_json(out / "observations.json", {
            "pin": PIN, "image": run("docker", "image", "inspect", IMAGE, "--format", "{{.Id}}").strip(),
            "source_sha256": expected_inputs, "provider_files_sha256": members,
            "fixture_sha256": fixture_inputs(), "runner_sha256": digest(Path(__file__).read_bytes()),
            "raw_sha256": {"session.log": digest(raw.encode()), "registry.log": digest(raw_provider.encode())},
            "dependency_manifest_reads": retag,
            "observations": observed, "manifest_digests": {mode: next(iter(values)) for mode, values in manifests.items()},
        })
        write_json(OUT / "exact-bindings.json", exact_bindings())
        print(f"Native checks: {len(observed)} matched")
    finally:
        out.mkdir(parents=True, exist_ok=True)
        (out / "session.log").write_text("".join(logs))
        (out / "registry.log").write_text(raw_provider)
        subprocess.run(["docker", "rm", "-f", container], capture_output=True, timeout=30)


def verify(path, log):
    result = subprocess.run([str(VERUS), str(path), "--triggers-mode", "silent"],
                            cwd=log.parent, capture_output=True, text=True, timeout=60)
    raw = result.stdout + result.stderr
    # Record paths relative to the proof inputs so the receipts travel with the checkout.
    raw = raw.replace(str(path), path.name).replace(str(ROOT) + "/", "")
    log.write_text(raw.rstrip() + "\n")
    summary = re.search(r"verification results:: (\d+) verified, (\d+) errors", raw)
    if not summary:
        raise AssertionError(f"No verifier summary in {log}")
    return {
        "log_sha256": digest(log.read_bytes()), "exit": result.returncode, "verified": int(summary[1]), "errors": int(summary[2]),
        "log": log.relative_to(ROOT).as_posix(),
        "assertion_rejected": result.returncode != 0 and (
            "postcondition not satisfied" in raw or "assertion failed" in raw
        ) and not re.search(r"resource limit|timed out", raw, re.I),
    }


def proofs():
    control_input = HERE / "controls.json"
    variants = json.loads(control_input.read_text())
    proof_dir = OUT / "proofs"
    proof_dir.mkdir(parents=True, exist_ok=True)

    def verify_model(item):
        file, controls = item
        source = HERE / file
        body = source.read_text()
        baseline = verify(source, proof_dir / (file + ".log"))
        assert baseline["exit"] == 0 and baseline["errors"] == 0, baseline
        results = []
        with tempfile.TemporaryDirectory() as temp:
            for name, (old, new) in controls.items():
                assert body.count(old) == 1, (name, old)
                mutant = Path(temp) / (name.replace("-", "_") + ".rs")
                text = body.replace(old, new, 1)
                dependency = (HERE / "../iter2/registry.rs").resolve()
                mutant.write_text(text.replace("../iter2/registry.rs", str(dependency)))
                receipt = verify(mutant, proof_dir / (name + ".log"))
                assert receipt["assertion_rejected"], (name, receipt)
                results.append({"id": name, **receipt})
        coverage = ({"variant": "proposed"} if file == "admission.rs" else
                    {"native_receipt": "native/observations.json"})
        return {"file": file, "sha256": digest(source.read_bytes()), "baseline": baseline,
                "controls": results, **coverage}

    with concurrent.futures.ThreadPoolExecutor(max_workers=len(variants)) as pool:
        models = list(pool.map(verify_model, variants.items()))
    inputs = [control_input, HERE / "run.py", HERE.parent / "iter2/registry.rs"]
    write_json(proof_dir / "models.json", {
        "tool_sha256": {name: digest((VERUS.parent / name).read_bytes()) for name in
                        (VERUS.name, "rust_verify", "z3", "vstd.vir")},
        "inputs_sha256": {path.relative_to(ROOT).as_posix(): digest(path.read_bytes()) for path in inputs},
        "models": models, "implementation_proofs": 0,
    })
    print("Verus: " + ", ".join(
        f"{model['file']} {model['baseline']['verified']} verified, {len(model['controls'])} controls rejected"
        for model in models
    ))


def check():
    out = OUT / "native"
    native_receipt = json.loads((out / "observations.json").read_text())
    assert native_receipt["pin"] == PIN and native_receipt["source_sha256"] == product_inputs()
    assert native_receipt["runner_sha256"] == digest(Path(__file__).read_bytes())
    assert native_receipt["fixture_sha256"] == fixture_inputs()
    assert native_receipt["observations"] == EXPECTED
    for log, expected in native_receipt["raw_sha256"].items():
        assert digest((out / log).read_bytes()) == expected, log
    assert json.loads((OUT / "exact-bindings.json").read_text()) == exact_bindings()

    proof = json.loads((OUT / "proofs/models.json").read_text())
    for path, expected in proof["inputs_sha256"].items():
        assert digest((ROOT / path).read_bytes()) == expected, path
    for model in proof["models"]:
        assert digest((HERE / model["file"]).read_bytes()) == model["sha256"], model["file"]
        for result in [model["baseline"], *model["controls"]]:
            assert digest((ROOT / result["log"]).read_bytes()) == result["log_sha256"], result["log"]
    print("Saved evidence matches the model, product inputs, source bindings and fixtures")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--native-only", action="store_true")
    group.add_argument("--proofs-only", action="store_true")
    group.add_argument("--check", action="store_true")
    options = parser.parse_args()
    if options.check:
        check()
        return
    if not options.native_only:
        proofs()
    if not options.proofs_only:
        native()


if __name__ == "__main__":
    main()
