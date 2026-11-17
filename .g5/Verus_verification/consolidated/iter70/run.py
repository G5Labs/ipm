"""Replay the repaired #281 implementation slice: Verus proofs and controls, source bindings, and the product's tests.

From the IPM checkout root:
  python3 .g5/Verus_verification/consolidated/iter70/run.py               proofs, bindings and native run
  python3 .g5/Verus_verification/consolidated/iter70/run.py --proofs-only
  python3 .g5/Verus_verification/consolidated/iter70/run.py --native-only
  python3 .g5/Verus_verification/consolidated/iter70/run.py --check        saved evidence against current inputs

The native run compiles the candidate's changed classes and their subclasses into a running IRIS container
(IPM_IRIS_CONTAINER, default ipm281-d1-iris) whose network has the oras and notary services, then runs
Test.PM.Integration.OrasContentTrust and Test.PM.Unit.OrasContentTrust.
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
OUT = ROOT / ".g5/uplift/consolidated/iter70"
BASE = "c1c055003731649021ef651c57f2cbf87bafe0cd"
VERUS = Path(os.getenv("VERUS", Path.home() / ".local/share/g5-verification/verus-0.2026.09.27.3cf1832/verus-x86-linux/verus")).resolve()
CONTAINER = os.getenv("IPM_IRIS_CONTAINER", "ipm281-d1-iris")
CONTRACT = HERE.parent / "iter68/admission.rs"
TESTS = {
    "Test.PM.Integration.OrasContentTrust": ("verify", ROOT / "tests/integration_tests/Test/PM/Integration/OrasContentTrust.cls"),
    "Test.PM.Unit.OrasContentTrust": ("test", ROOT / "tests/unit_tests/Test/PM/Unit/OrasContentTrust.cls"),
}
FIXTURES = ROOT / "tests/integration_tests/Test/PM/Integration/_data/oras-content-trust"
# Product classes the candidate adds or changes relative to BASE.
PRODUCT = [
    "src/cls/IPM/Repo/Definition.cls",
    "src/cls/IPM/Repo/UniversalSettings.cls",
    "src/cls/IPM/Repo/Oras/ContentTrust.cls",
    "src/cls/IPM/Repo/Oras/Definition.cls",
    "src/cls/IPM/Repo/Oras/PackageService.cls",
]
# Unchanged subclasses that inherit a changed parameter or method and so are compiled again.
DEPENDENTS = [
    "src/cls/IPM/Repo/Http/Definition.cls",
    "src/cls/IPM/Repo/Remote/Definition.cls",
    "src/cls/IPM/Repo/Filesystem/Definition.cls",
    "src/cls/IPM/Repo/Oras/PublishService.cls",
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
    models = {name: (HERE / name).read_text() for name in bindings["models"]}
    rows = []
    for row in bindings["rows"]:
        owner, _, anchor = row["owner"].partition("#")
        assert f'id="{anchor}"' in (ROOT / owner).read_text(), row["owner"]
        model = models[row["model"]]
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
    return {"base_revision": BASE, "models": bindings["models"], "rows": rows}


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


def verify(path, log, source_text):
    result = subprocess.run([str(VERUS), str(path), "--triggers-mode", "silent"],
                            cwd=path.parent, capture_output=True, text=True, timeout=600)
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
        "rejected_functions": rejected_functions(raw, source_text),
    }


def proofs():
    controls = json.loads((HERE / "controls.json").read_text())
    proof_dir = OUT / "proofs"
    proof_dir.mkdir(parents=True, exist_ok=True)
    models = []
    for file, variants in controls.items():
        source = HERE / file
        body = source.read_text()
        baseline = verify(source, proof_dir / (file + ".log"), body)
        assert baseline["exit"] == 0 and baseline["errors"] == 0, baseline

        def control(item, file=file, body=body):
            name, (old, new) = item
            assert body.count(old) == 1, (name, old)
            with tempfile.TemporaryDirectory() as temp:
                mutant = Path(temp) / (name.replace("-", "_") + ".rs")
                text = body.replace(old, new, 1).replace("../iter68/admission.rs", str(CONTRACT.resolve()))
                mutant.write_text(text)
                receipt = verify(mutant, proof_dir / (name + ".log"), text)
            assert receipt["assertion_rejected"], (name, receipt)
            return {"id": name, **receipt}

        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            results = list(pool.map(control, variants.items()))
        models.append({"file": file, "sha256": digest(source.read_bytes()), "baseline": baseline,
                       "controls": results, "native_receipt": "native/observations.json"})
        print(f"Verus: {file} {baseline['verified']} verified, {len(results)} controls rejected")
    inputs = [HERE / "controls.json", HERE / "run.py", CONTRACT]
    write_json(proof_dir / "models.json", {
        "tool_sha256": {name: digest((VERUS.parent / name).read_bytes()) for name in
                        (VERUS.name, "rust_verify", "z3", "vstd.vir")},
        "inputs_sha256": {path.relative_to(ROOT).as_posix(): digest(path.read_bytes()) for path in inputs},
        "models": models,
        "implementation_proofs": 0,
    })
    write_json(OUT / "exact-bindings.json", exact_bindings())


def session(namespace, script):
    result = subprocess.run(["docker", "exec", "-i", CONTAINER, "iris", "session", "IRIS", "-U", namespace],
                            input=script + "\nhalt\n", capture_output=True, text=True, timeout=1800)
    return result.stdout + result.stderr


def inputs_sha():
    files = [ROOT / p for p in PRODUCT + DEPENDENTS] + [path for _, path in TESTS.values()]
    files += [ROOT / "tests/notary/server-config.json"]
    files += sorted(p for p in FIXTURES.rglob("*") if p.is_file())
    return {p.relative_to(ROOT).as_posix(): digest(p.read_bytes()) for p in files}


def latest_results(namespace, case):
    """Method name -> status (1 passed) and assertion counts of the newest unit test run of case in namespace."""
    output = session(namespace, f"""
set rs = ##class(%SQL.Statement).%ExecDirect(,"select TOP 1 ID from %UnitTest_Result.TestInstance order by ID desc")
do rs.%Next() set id = rs.%Get("ID")
set rs = ##class(%SQL.Statement).%ExecDirect(,"select TestCase->Name as CaseName, Name, Status from %UnitTest_Result.TestMethod where TestCase->TestSuite->TestInstance = ? order by Name", id)
while rs.%Next() {{ write !,"RESULT ",rs.%Get("CaseName")," ",rs.%Get("Name")," ",rs.%Get("Status") }}
set rs = ##class(%SQL.Statement).%ExecDirect(,"select a.TestMethod->Name as M, a.Status as S, count(*) as N from %UnitTest_Result.TestAssert a where a.TestMethod->TestCase->TestSuite->TestInstance = ? group by a.TestMethod->Name, a.Status", id)
while rs.%Next() {{ write !,"ASSERTS ",rs.%Get("M")," ",rs.%Get("S")," ",rs.%Get("N") }}""")
    methods = {name: int(status) for found, name, status in re.findall(r"RESULT (\S+) (\S+) (\d)", output) if found == case}
    asserts = {}
    for name, status, count in re.findall(r"ASSERTS (\S+) (\d) (\d+)", output):
        asserts.setdefault(name, {})[{"0": "failed", "1": "passed", "2": "skipped"}[status]] = int(count)
    return methods, asserts


def native():
    out = OUT / "native"
    out.mkdir(parents=True, exist_ok=True)
    loads = "\n".join(
        f'set sc = $system.OBJ.Load("/home/irisowner/zpm/{path}", "ck-d") write !,"COMPILE {path} ",$select(sc:"OK",1:$system.Status.GetErrorText(sc))'
        for path in PRODUCT + DEPENDENTS)
    compile_log = session("USER", loads)
    assert compile_log.count(" OK") == len(PRODUCT + DEPENDENTS), compile_log
    (out / "compile.log").write_text(compile_log)
    expected = json.loads((HERE / "native-expected.json").read_text())
    cases = {}
    for case, (phase, _) in TESTS.items():
        log_name = f"{case.split('.')[2].lower()}-session.log"
        run_log = session("USER", f'zpm "repo -delete-all"\nzpm "{phase} zpm -only -quiet -DUnitTest.Case={case}"')
        (out / log_name).write_text(run_log)
        summary = re.search(r"Methods: (\d+) total, (\d+) passed, (\d+) failed\s+Assertions: (\d+) total, (\d+) passed, (\d+) failed", run_log)
        assert summary, run_log
        methods, asserts = latest_results("USER-VERIFY" if phase == "verify" else "USER", case)
        wanted = expected["cases"][case]["methods"]
        assert sorted(methods) == sorted(wanted), (case, sorted(methods), sorted(wanted))
        assert all(methods[m] == 1 for m in methods), (case, methods)
        assert not any(a.get("skipped") or a.get("failed") for a in asserts.values()), (case, asserts)
        cases[case] = {
            "session_log": log_name,
            "observations": {name: {"passed": status == 1, "assertions": asserts.get(name, {}), "requirements": wanted[name]}
                             for name, status in methods.items()},
            "summary": dict(zip(["methods", "methods_passed", "methods_failed", "assertions", "assertions_passed",
                                 "assertions_failed"], [int(x) for x in summary.groups()])),
        }
        print(f"Native: {case} {len(methods)} methods passed, {summary[5]} assertions, none skipped")
    write_json(out / "observations.json", {
        "container_image": subprocess.run(["docker", "inspect", "-f", "{{.Image}}", CONTAINER],
                                          capture_output=True, text=True).stdout.strip(),
        "notary_cli_sha256": subprocess.run(["docker", "exec", CONTAINER, "sha256sum", "/usr/local/bin/notary"],
                                            capture_output=True, text=True).stdout.split()[0],
        "inputs_sha256": inputs_sha(), "runner_sha256": digest(Path(__file__).read_bytes()),
        "expected_sha256": digest((HERE / "native-expected.json").read_bytes()),
        "cases": cases,
        "raw_sha256": {name: digest((out / name).read_bytes())
                       for name in ["compile.log"] + [c["session_log"] for c in cases.values()]},
    })


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
    current_inputs = inputs_sha()
    changed = sorted(p for p in native_receipt["inputs_sha256"].keys() | current_inputs.keys()
                     if native_receipt["inputs_sha256"].get(p) != current_inputs.get(p))
    assert not changed, f"STALE native receipt for {changed}: rerun --native-only"
    assert native_receipt["runner_sha256"] == digest(Path(__file__).read_bytes()), "Runner changed: rerun --native-only"
    assert native_receipt["expected_sha256"] == digest((HERE / "native-expected.json").read_bytes()), "Expected cases changed: rerun --native-only"
    for name, expected in native_receipt["raw_sha256"].items():
        assert digest((OUT / "native" / name).read_bytes()) == expected, name
    print("Saved evidence matches the models, controls, source bindings, product inputs and test fixtures")


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
