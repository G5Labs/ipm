"""Replay the #281 implementation slice: Verus proofs and controls, source bindings, and the product's integration tests.

From the IPM checkout root:
  python3 .g5/Verus_verification/consolidated/iter69/run.py               proofs, bindings and native run
  python3 .g5/Verus_verification/consolidated/iter69/run.py --proofs-only
  python3 .g5/Verus_verification/consolidated/iter69/run.py --native-only
  python3 .g5/Verus_verification/consolidated/iter69/run.py --check        saved evidence against current inputs

The native run compiles the candidate's changed classes into a running IRIS container (IPM_IRIS_CONTAINER, default
ipm281-d1-iris) whose network has the oras and notary services, then runs Test.PM.Integration.OrasContentTrust.
"""

import argparse
import concurrent.futures
import hashlib
import json
import os
import re
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
OUT = ROOT / ".g5/uplift/consolidated/iter69"
BASE = "c1c055003731649021ef651c57f2cbf87bafe0cd"
VERUS = Path(os.getenv("VERUS", Path.home() / ".local/share/g5-verification/verus-0.2026.09.27.3cf1832/verus-x86-linux/verus")).resolve()
CONTAINER = os.getenv("IPM_IRIS_CONTAINER", "ipm281-d1-iris")
CONTRACT = HERE.parent / "iter68/admission.rs"
TEST_CASE = "Test.PM.Integration.OrasContentTrust"
TEST_CLASS = ROOT / "tests/integration_tests/Test/PM/Integration/OrasContentTrust.cls"
FIXTURES = ROOT / "tests/integration_tests/Test/PM/Integration/_data/oras-content-trust"
# Product classes the candidate adds or changes, compiled before the native run.
PRODUCT = [
    "src/cls/IPM/Repo/UniversalSettings.cls",
    "src/cls/IPM/Repo/Oras/ContentTrust.cls",
    "src/cls/IPM/Repo/Oras/Definition.cls",
    "src/cls/IPM/Repo/Oras/PackageService.cls",
]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def member_span(text, name):
    """1-based start and end lines of a class member's declaration through its closing brace, with its doc comment."""
    lines = text.splitlines(keepends=True)
    pattern = re.compile(r"^(?:ClassMethod|Method|Property|Parameter|Query|XData)\s+" + re.escape(name) + r"(?=[\s(;]|$)")
    for index, line in enumerate(lines):
        if pattern.match(line):
            start = index
            while start > 0 and lines[start - 1].startswith("///"):
                start -= 1
            end = index
            if not line.rstrip().endswith(";"):
                while end < len(lines) and lines[end].rstrip() != "}":
                    end += 1
            return start + 1, end + 1
    raise AssertionError(f"member {name} not found")


def exact_bindings():
    bindings = json.loads((OUT / "bindings.json").read_text())
    model = (HERE / bindings["model"]).read_text()
    rows = []
    for row in bindings["rows"]:
        owner, _, anchor = row["owner"].partition("#")
        assert f'id="{anchor}"' in (ROOT / owner).read_text(), row["owner"]
        for symbol in row["symbols"]:
            assert re.search(rf"\b(?:fn|struct|enum)\s+{symbol}\b", model), (row["id"], symbol)
        members = []
        for locator, role in row["members"]:
            path, _, name = locator.partition("::")
            blob = (ROOT / path).read_bytes()
            start, end = member_span(blob.decode(), name)
            lines = blob.decode().splitlines(keepends=True)
            members.append({"member": locator, "role": role, "start": start, "end": end,
                            "file_sha256": digest(blob),
                            "span_sha256": digest("".join(lines[start - 1:end]).encode())})
        rows.append({**{k: v for k, v in row.items() if k != "members"}, "members": members})
    return {"base_revision": BASE, "model": bindings["model"], "rows": rows}


def rejected_functions(raw, source):
    """Proof functions whose obligations an error points into."""
    lines = source.splitlines()
    names = set()
    for number in re.findall(r"--> \S+?\.rs:(\d+):\d+", raw):
        for line in reversed(lines[:int(number)]):
            found = re.match(r"\s*(?:pub )?proof fn (\w+)", line)
            if found:
                names.add(found[1])
                break
    return sorted(names)


def verify(path, log):
    result = subprocess.run([str(VERUS), str(path), "--triggers-mode", "silent"],
                            cwd=log.parent, capture_output=True, text=True, timeout=600)
    source = path.read_text()
    raw = (result.stdout + result.stderr).replace(str(path), path.name).replace(str(ROOT) + "/", "")
    raw = raw.replace(str(CONTRACT.resolve()), "../iter68/admission.rs")
    log.write_text(raw.rstrip() + "\n")
    summary = re.search(r"verification results:: (\d+) verified, (\d+) errors", raw)
    if not summary:
        raise AssertionError(f"No verifier summary in {log}")
    return {
        "log": log.relative_to(ROOT).as_posix(), "log_sha256": digest(log.read_bytes()),
        "exit": result.returncode, "verified": int(summary[1]), "errors": int(summary[2]),
        "assertion_rejected": result.returncode != 0 and (
            "postcondition not satisfied" in raw or "assertion failed" in raw
        ) and not re.search(r"resource limit|timed out", raw, re.I),
        "rejected_functions": rejected_functions(raw, source),
    }


def proofs():
    controls = json.loads((HERE / "controls.json").read_text())
    proof_dir = OUT / "proofs"
    proof_dir.mkdir(parents=True, exist_ok=True)
    file, variants = next(iter(controls.items()))
    source = HERE / file
    body = source.read_text()
    baseline = verify(source, proof_dir / (file + ".log"))
    assert baseline["exit"] == 0 and baseline["errors"] == 0, baseline

    def control(item):
        name, (old, new) = item
        assert body.count(old) == 1, (name, old)
        with tempfile.TemporaryDirectory() as temp:
            mutant = Path(temp) / (name.replace("-", "_") + ".rs")
            mutant.write_text(body.replace(old, new, 1).replace("../iter68/admission.rs", str(CONTRACT.resolve())))
            receipt = verify(mutant, proof_dir / (name + ".log"))
        assert receipt["assertion_rejected"], (name, receipt)
        return {"id": name, **receipt}

    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        results = list(pool.map(control, variants.items()))
    inputs = [HERE / "controls.json", HERE / "run.py", CONTRACT]
    write_json(proof_dir / "models.json", {
        "tool_sha256": {name: digest((VERUS.parent / name).read_bytes()) for name in
                        (VERUS.name, "rust_verify", "z3", "vstd.vir")},
        "inputs_sha256": {path.relative_to(ROOT).as_posix(): digest(path.read_bytes()) for path in inputs},
        "models": [{"file": file, "sha256": digest(source.read_bytes()), "baseline": baseline, "controls": results,
                    "native_receipt": "native/observations.json"}],
        "implementation_proofs": 0,
    })
    write_json(OUT / "exact-bindings.json", exact_bindings())
    print(f"Verus: {file} {baseline['verified']} verified, {len(results)} controls rejected")


def session(namespace, script):
    result = subprocess.run(["docker", "exec", "-i", CONTAINER, "iris", "session", "IRIS", "-U", namespace],
                            input=script + "\nhalt\n", capture_output=True, text=True, timeout=1800)
    return result.stdout + result.stderr


def inputs_sha():
    files = [ROOT / p for p in PRODUCT] + [TEST_CLASS, ROOT / "tests/notary/server-config.json"]
    files += sorted(p for p in FIXTURES.rglob("*") if p.is_file())
    return {p.relative_to(ROOT).as_posix(): digest(p.read_bytes()) for p in files}


def native():
    out = OUT / "native"
    out.mkdir(parents=True, exist_ok=True)
    loads = "\n".join(
        f'set sc = $system.OBJ.Load("/home/irisowner/zpm/{path}", "ck-d") write !,"COMPILE {path} ",$select(sc:"OK",1:$system.Status.GetErrorText(sc))'
        for path in PRODUCT)
    compile_log = session("USER", loads)
    assert compile_log.count(" OK") == len(PRODUCT), compile_log
    run_log = session("USER", f'zpm "verify zpm -only -quiet -DUnitTest.Case={TEST_CASE}"')
    results = session("USER-VERIFY", """
set rs = ##class(%SQL.Statement).%ExecDirect(,"select TOP 1 ID from %UnitTest_Result.TestInstance order by ID desc")
do rs.%Next() set id = rs.%Get("ID")
set rs = ##class(%SQL.Statement).%ExecDirect(,"select TestCase->Name as CaseName, Name, Status from %UnitTest_Result.TestMethod where TestCase->TestSuite->TestInstance = ? order by Name", id)
while rs.%Next() { write !,"RESULT ",rs.%Get("CaseName")," ",rs.%Get("Name")," ",rs.%Get("Status") }""")
    methods = {name: int(status) for case, name, status in re.findall(r"RESULT (\S+) (\S+) (\d)", results)
               if case == TEST_CASE}
    summary = re.search(r"Methods: (\d+) total, (\d+) passed, (\d+) failed\s+Assertions: (\d+) total, (\d+) passed, (\d+) failed", run_log)
    assert summary, run_log
    (out / "compile.log").write_text(compile_log)
    (out / "session.log").write_text(run_log)
    expected = json.loads((HERE / "native-expected.json").read_text())
    assert sorted(methods) == sorted(expected["methods"]), (sorted(methods), sorted(expected["methods"]))
    assert all(methods[m] == 1 for m in methods), methods
    write_json(out / "observations.json", {
        "container_image": subprocess.run(["docker", "inspect", "-f", "{{.Image}}", CONTAINER],
                                          capture_output=True, text=True).stdout.strip(),
        "inputs_sha256": inputs_sha(), "runner_sha256": digest(Path(__file__).read_bytes()),
        "expected_sha256": digest((HERE / "native-expected.json").read_bytes()),
        "observations": {name: {"passed": status == 1, "requirements": expected["methods"][name]}
                         for name, status in methods.items()},
        "summary": dict(zip(["methods", "methods_passed", "methods_failed", "assertions", "assertions_passed",
                             "assertions_failed"], [int(x) for x in summary.groups()])),
        "raw_sha256": {name: digest((out / name).read_bytes()) for name in ("compile.log", "session.log")},
    })
    print(f"Native: {len(methods)} methods passed, {summary[5]} assertions")


def check():
    proof = json.loads((OUT / "proofs/models.json").read_text())
    for path, expected in proof["inputs_sha256"].items():
        assert digest((ROOT / path).read_bytes()) == expected, f"STALE formal input {path}: rerun --proofs-only"
    for model in proof["models"]:
        assert digest((HERE / model["file"]).read_bytes()) == model["sha256"], f"STALE model {model['file']}: rerun --proofs-only"
        for result in [model["baseline"], *model["controls"]]:
            assert digest((ROOT / result["log"]).read_bytes()) == result["log_sha256"], result["log"]
    def spans(bindings):
        return {(row["id"], member["member"]): member["span_sha256"] for row in bindings["rows"] for member in row["members"]}
    saved = spans(json.loads((OUT / "exact-bindings.json").read_text()))
    current = spans(exact_bindings())
    stale = sorted(key for key in saved.keys() | current.keys() if saved.get(key) != current.get(key))
    assert not stale, f"STALE correspondence {stale}: reread the source, update bindings.json and rerun"
    native_receipt = json.loads((OUT / "native/observations.json").read_text())
    changed = [p for p, h in inputs_sha().items() if native_receipt["inputs_sha256"].get(p) != h]
    assert not changed, f"STALE native receipt for {changed}: rerun --native-only"
    assert native_receipt["runner_sha256"] == digest(Path(__file__).read_bytes()), "Runner changed: rerun --native-only"
    for name, expected in native_receipt["raw_sha256"].items():
        assert digest((OUT / "native" / name).read_bytes()) == expected, name
    print("Saved evidence matches the model, controls, source bindings, product inputs and test fixtures")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
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
