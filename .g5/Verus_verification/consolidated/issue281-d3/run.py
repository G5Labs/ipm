"""Replay the #281 D3+ signed-admission evidence: Verus proofs and controls, adjacent models, source bindings and
the product's tests.

From the IPM checkout root:
  python3 .g5/Verus_verification/consolidated/issue281-d3/run.py --proofs-only    D3+ models and their controls
  python3 .g5/Verus_verification/consolidated/issue281-d3/run.py --adjacent       inherited and neighbouring models
  python3 .g5/Verus_verification/consolidated/issue281-d3/run.py --bindings       resolve and hash the member bindings
  python3 .g5/Verus_verification/consolidated/issue281-d3/run.py --native-only    compile and run the test classes
  python3 .g5/Verus_verification/consolidated/issue281-d3/run.py                  all of the above
  python3 .g5/Verus_verification/consolidated/issue281-d3/run.py --check          saved evidence against current inputs

VERUS names the verifier. The native run compiles the candidate's changed classes and their subclasses into a running
IRIS container (IPM_IRIS_CONTAINER, default ipm281d3p-iris) whose network has the oras and notary services, then runs
the test classes in native-expected.json and asserts that every expected method ran and passed with nothing skipped.
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
OUT = ROOT / ".g5/uplift/consolidated/issue281-d3"
VERUS = Path(os.environ["VERUS"]).resolve() if os.getenv("VERUS") else None
CONTAINER = os.getenv("IPM_IRIS_CONTAINER", "ipm281d3p-iris")
INCLUDES = {"../iter68/admission.rs": HERE.parent / "iter68/admission.rs",
            "../iter15/oras.rs": HERE.parent / "iter15/oras.rs"}
MODELS = ["implementation.rs", "invocations.rs", "configuration.rs", "settings.rs"]
# Inherited and neighbouring models replayed on the final candidate, with why each is affected.
ADJACENT = {
    ".g5/Verus_verification/consolidated/iter68/admission.rs": "contract refined by implementation.rs",
    ".g5/Verus_verification/consolidated/iter68/as_built.rs": "archive boundary for repositories without a trust server",
    ".g5/Verus_verification/consolidated/iter15/oras.rs": "selection composed by implementation.rs (metadata_tag, tag_for)",
    ".g5/Verus_verification/consolidated/iter16/provenance.rs": "Repo/Definition.cls::Configure claims",
    ".g5/Verus_verification/consolidated/iter22/reinstall.rs": "Repo/Definition.cls::Configure claims",
    ".g5/Verus_verification/consolidated/iter41/settings.rs": "settings model revised by settings.rs",
    ".g5/Verus_verification/consolidated/iter70/implementation.rs": "superseded #281 model, D2 product",
    ".g5/Verus_verification/consolidated/iter70/settings.rs": "superseded #281 settings model",
    "playbook-uplift/models/verus/settings.rs": "inherited settings model with ModuleRoot's computed reset",
    ".g5/Verus_verification/coverage-20261117/iter75/oras_integrity.rs": "GetModule unprotected branch",
    ".g5/Verus_verification/coverage-20261117/iter75/r8a_oras.rs": "TagForReference",
    ".g5/Verus_verification/coverage-20261117/iter75/r8a_settings.rs": "UpdateOne",
    ".g5/Verus_verification/coverage-20261117/iter74/actions.rs": "LoadModuleReference refusal and history",
}
TESTS_FILE = HERE / "native-expected.json"
FIXTURES = ROOT / "tests/integration_tests/Test/PM/Integration/_data/oras-content-trust"
# Product classes the candidate adds or changes relative to the pre-PR product.
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
ENVIRONMENT = ["tests/notary/server-config.json", "tests/notary/certs/root-ca.crt", "tests/notary/certs/notary-server.crt",
               "tests/notary/certs/notary-server.key", "Dockerfile", "docker-compose.yml"]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n")


def rel(path):
    return Path(path).resolve().relative_to(ROOT).as_posix()


def function_errors(raw, sources):
    """{proof function: sorted error kinds} for the errors a verifier log points into, e.g. {"f": ["postcondition"]}."""
    kinds = {}
    for block in re.split(r"\n(?=error)", raw):
        message = re.match(r"error(?:\[\w+\])?: ([^\n]+)", block)
        location = re.search(r"--> (\S+?\.rs):(\d+):\d+", block)
        if not message or not location or Path(location[1]).name not in sources:
            continue
        kind = ("postcondition" if "postcondition not satisfied" in message[1] else
                "assertion" if "assertion failed" in message[1] else
                "rlimit" if re.search(r"resource limit|rlimit", message[1], re.I) else message[1])
        for line in reversed(sources[Path(location[1]).name][:int(location[2])]):
            found = re.match(r"\s*(?:pub )?proof fn (\w+)", line)
            if found:
                kinds.setdefault(found[1], set()).add(kind)
                break
    return {name: sorted(found) for name, found in sorted(kinds.items())}


def verify(path, log, sources=None):
    if VERUS is None:
        raise SystemExit("Set VERUS to the pinned verifier, e.g. ~/.cache/g5-verification/<version>/<platform>/verus")
    result = subprocess.run([str(VERUS), str(path), "--triggers-mode", "silent"],
                            cwd=path.parent, capture_output=True, text=True, timeout=1200)
    raw = (result.stdout + result.stderr).replace(str(path.parent) + "/", "").replace(str(ROOT) + "/", "")
    for short, full in INCLUDES.items():
        raw = raw.replace(str(full.resolve()), short)
    log.parent.mkdir(parents=True, exist_ok=True)
    log.write_text(raw.rstrip() + "\n")
    summary = re.search(r"verification results:: (\d+) verified, (\d+) errors", raw)
    if not summary:
        raise AssertionError(f"No verifier summary in {log}")
    errors = function_errors(raw, sources or {})
    return {
        "log": rel(log), "log_sha256": digest(log.read_bytes()),
        "exit": result.returncode, "verified": int(summary[1]), "errors": int(summary[2]),
        "function_errors": errors,
        # A theorem counts as rejected only through a failed postcondition or assertion, never a resource limit.
        "rejected_functions": sorted(name for name, kinds in errors.items()
                                     if "postcondition" in kinds or "assertion" in kinds),
    }


def tool_hashes():
    return {name: digest((VERUS.parent / name).read_bytes()) for name in (VERUS.name, "rust_verify", "z3", "vstd.vir")}


def proofs():
    controls = json.loads((HERE / "controls.json").read_text())
    proof_dir = OUT / "proofs"
    results = []
    for file in MODELS:
        source = HERE / file
        body = source.read_text()
        baseline = verify(source, proof_dir / (file + ".log"))
        assert baseline["exit"] == 0 and baseline["errors"] == 0, baseline

        def control(item, file=file, body=body):
            name, spec = item
            old, new = spec["replace"]
            assert body.count(old) == 1, (name, old)
            with tempfile.TemporaryDirectory() as temp:
                mutant = Path(temp) / (name.replace("-", "_") + ".rs")
                text = body.replace(old, new, 1)
                for short, full in INCLUDES.items():
                    text = text.replace(short, str(full.resolve()))
                mutant.write_text(text)
                receipt = verify(mutant, proof_dir / file.replace(".rs", "") / (name + ".log"),
                                 {mutant.name: text.splitlines()})
            missing = sorted(set(spec["expect"]) - set(receipt["rejected_functions"]))
            assert receipt["exit"] != 0 and not missing, (name, missing, receipt)
            return {"id": name, "expected": spec["expect"], **receipt}

        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            rejected = list(pool.map(control, controls[file].items()))
        results.append({"file": file, "sha256": digest(source.read_bytes()), "baseline": baseline, "controls": rejected})
        print(f"Verus: {file} {baseline['verified']} verified, {len(rejected)} controls rejected at their theorems")
    inputs = [HERE / "controls.json", HERE / "run.py"] + list(INCLUDES.values())
    write_json(proof_dir / "models.json", {
        "verus": str(VERUS.parent.parent.name), "tool_sha256": tool_hashes(),
        "inputs_sha256": {rel(path): digest(path.read_bytes()) for path in inputs},
        "models": results,
        "implementation_proofs": 0,
    })


def adjacent():
    rows = []
    for path, why in ADJACENT.items():
        source = ROOT / path
        log = OUT / "adjacent" / (path.replace("/", "__").lstrip(".") + ".log")
        receipt = verify(source, log)
        assert receipt["exit"] == 0 and receipt["errors"] == 0, (path, receipt)
        rows.append({"model": path, "why": why, "sha256": digest(source.read_bytes()), **receipt})
        print(f"Adjacent: {path} {receipt['verified']} verified")
    write_json(OUT / "adjacent/models.json", {"verus": str(VERUS.parent.parent.name), "tool_sha256": tool_hashes(),
                                              "runner_sha256": digest(Path(__file__).read_bytes()), "models": rows})


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
    models = {name: (HERE / name).read_text() for name in MODELS}
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
            text = (ROOT / path).read_text()
            start, end = member_span(text, name)
            lines = text.splitlines(keepends=True)
            members.append({"member": locator, "role": role, "start": start, "end": end,
                            "file_sha256": digest((ROOT / path).read_bytes()),
                            "span_sha256": digest("".join(lines[start - 1:end]).encode())})
        rows.append({**{k: v for k, v in row.items() if k != "members"}, "members": members})
    return {"models": MODELS, "rows": rows}


def session(namespace, script):
    result = subprocess.run(["docker", "exec", "-i", CONTAINER, "iris", "session", "IRIS", "-U", namespace],
                            input=script + "\nhalt\n", capture_output=True, text=True, timeout=3600)
    return result.stdout + result.stderr


def native_inputs():
    expected = json.loads(TESTS_FILE.read_text())
    files = [ROOT / p for p in PRODUCT + DEPENDENTS + ENVIRONMENT]
    files += [ROOT / case["file"] for case in expected["cases"].values()]
    files += sorted(p for p in FIXTURES.rglob("*") if p.is_file())
    return {rel(p): digest(p.read_bytes()) for p in files}


def latest_results(namespace, case):
    """Method name -> status (1 passed) and assertion counts of the newest unit test run of case in namespace."""
    output = session(namespace, f"""
set rs = ##class(%SQL.Statement).%ExecDirect(,"select TOP 1 ID from %UnitTest_Result.TestInstance order by ID desc")
do rs.%Next() set id = rs.%Get("ID")
set rs = ##class(%SQL.Statement).%ExecDirect(,"select TestCase->Name as CaseName, Name, Status from %UnitTest_Result.TestMethod where TestCase->TestSuite->TestInstance = ? order by Name", id)
while rs.%Next() {{ write !,"RESULT ",rs.%Get("CaseName")," ",rs.%Get("Name")," ",rs.%Get("Status") }}
set rs = ##class(%SQL.Statement).%ExecDirect(,"select a.TestMethod->TestCase->Name as C, a.TestMethod->Name as M, a.Status as S, count(*) as N from %UnitTest_Result.TestAssert a where a.TestMethod->TestCase->TestSuite->TestInstance = ? group by a.TestMethod->TestCase->Name, a.TestMethod->Name, a.Status", id)
while rs.%Next() {{ write !,"ASSERTS ",rs.%Get("C")," ",rs.%Get("M")," ",rs.%Get("S")," ",rs.%Get("N") }}""")
    methods = {name: int(status) for found, name, status in re.findall(r"RESULT (\S+) (\S+) (\d)", output) if found == case}
    asserts = {}
    for found, name, status, count in re.findall(r"ASSERTS (\S+) (\S+) (\d) (\d+)", output):
        if found == case:
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
    expected = json.loads(TESTS_FILE.read_text())
    cases = {}
    for case, spec in expected["cases"].items():
        log_name = f"{case.split('.')[2].lower()}-{case.split('.')[-1].lower()}-session.log"
        run_log = session("USER", f'zpm "repo -delete-all"\nzpm "{spec["phase"]} zpm -only -quiet -DUnitTest.Case={case}"')
        (out / log_name).write_text(run_log)
        summary = re.search(r"Methods: (\d+) total, (\d+) passed, (\d+) failed\s+Assertions: (\d+) total, (\d+) passed, (\d+) failed", run_log)
        assert summary, run_log[-3000:]
        methods, asserts = latest_results("USER-VERIFY" if spec["phase"] == "verify" else "USER", case)
        wanted = spec["methods"]
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
        "container": CONTAINER,
        "container_image": subprocess.run(["docker", "inspect", "-f", "{{.Image}}", CONTAINER],
                                          capture_output=True, text=True).stdout.strip(),
        "notary_cli_sha256": subprocess.run(["docker", "exec", CONTAINER, "sha256sum", "/usr/local/bin/notary"],
                                            capture_output=True, text=True).stdout.split()[0],
        "inputs_sha256": native_inputs(), "runner_sha256": digest(Path(__file__).read_bytes()),
        "expected_sha256": digest(TESTS_FILE.read_bytes()),
        "cases": cases,
        "raw_sha256": {name: digest((out / name).read_bytes())
                       for name in ["compile.log"] + [c["session_log"] for c in cases.values()]},
    })


def check():
    stale = []
    proof = json.loads((OUT / "proofs/models.json").read_text())
    for path, expected in proof["inputs_sha256"].items():
        if digest((ROOT / path).read_bytes()) != expected:
            stale.append(f"formal input {path}: rerun --proofs-only")
    for model in proof["models"]:
        if digest((HERE / model["file"]).read_bytes()) != model["sha256"]:
            stale.append(f"model {model['file']}: rerun --proofs-only")
        for result in [model["baseline"], *model["controls"]]:
            if digest((ROOT / result["log"]).read_bytes()) != result["log_sha256"]:
                stale.append(f"receipt {result['log']}")
    adjacent_receipt = OUT / "adjacent/models.json"
    if adjacent_receipt.exists():
        for model in json.loads(adjacent_receipt.read_text())["models"]:
            if digest((ROOT / model["model"]).read_bytes()) != model["sha256"]:
                stale.append(f"adjacent model {model['model']}: rerun --adjacent")
    else:
        stale.append("no adjacent receipt: run --adjacent")
    if (OUT / "exact-bindings.json").exists():
        def spans(bindings):
            return {(row["id"], member["member"]): member["span_sha256"] for row in bindings["rows"] for member in row["members"]}
        saved = spans(json.loads((OUT / "exact-bindings.json").read_text()))
        current = spans(exact_bindings())
        stale += [f"correspondence {key}: reread the source, update bindings.json and rerun --bindings"
                  for key in sorted(saved.keys() | current.keys()) if saved.get(key) != current.get(key)]
    else:
        stale.append("no bindings receipt: run --bindings")
    receipt_path = OUT / "native/observations.json"
    if receipt_path.exists():
        receipt = json.loads(receipt_path.read_text())
        current = native_inputs()
        stale += [f"native input {p}: rerun --native-only" for p in sorted(receipt["inputs_sha256"].keys() | current.keys())
                  if receipt["inputs_sha256"].get(p) != current.get(p)]
        if receipt["runner_sha256"] != digest(Path(__file__).read_bytes()):
            stale.append("runner changed: rerun --native-only")
        if receipt["expected_sha256"] != digest(TESTS_FILE.read_bytes()):
            stale.append("expected cases changed: rerun --native-only")
        for name, expected in receipt["raw_sha256"].items():
            if digest((OUT / "native" / name).read_bytes()) != expected:
                stale.append(f"native log {name}")
    else:
        stale.append("no native receipt: run --native-only")
    if stale:
        raise SystemExit("STALE\n" + "\n".join(stale))
    print("Saved evidence matches the models, controls, adjacent models, source bindings, product inputs and fixtures")


VIEWER = {
    "implementation.rs": ("implementation", "Does every protected ORAS retrieval offer, plan with and unpack only what an authenticated trust server signed for the selected artifact, refusing otherwise?",
                          ["repo.oras.signed-admission", "repo.oras.signed-admission.verifier", "repo.oras.signed-admission.freshness",
                           "repo.oras.signed-admission.catalog", "repo.oras.signed-admission.snapshot", "repo.oras.signed-admission.bytes",
                           "repo.oras.signed-admission.refusal"]),
    "invocations.rs": ("invocations", "Can a check accept without reaching the trust server itself, or move the pinned root back, whatever other checks of the same repository do meanwhile?",
                       ["repo.oras.signed-admission.freshness"]),
    "configuration.rs": ("configuration", "Does -trust-server reach only ORAS repositories, only as https, without a refused value changing any repository, and does every consumer keep it?",
                         ["repo.oras.signed-admission.configuration", "repo.manage"]),
    "settings.rs": ("settings", "With NotaryExecutable added, does config delete apply each key's own reset rule, including ModuleRoot's computed root?",
                    ["settings", "settings.writes", "repo.oras.signed-admission.configuration"]),
}


def publish():
    """Writes the viewer manifests and index rows of this iteration from its receipts."""
    proof = json.loads((OUT / "proofs/models.json").read_text())
    bound = (OUT / "exact-bindings.json").exists()
    variant = "as-built" if bound else "proposed"
    native_receipt = OUT / "native/observations.json"
    rows = []
    for model in proof["models"]:
        slug, question, ontology_ids = VIEWER[model["file"]]
        concern = f"issue281-d3-{slug}"
        runs = [{"label": "proof", "kind": "proof", "variant": variant, "expect": "verified"}]
        runs += [{"label": c["id"], "kind": "control", "variant": variant, "expect": "named assertion rejected"} for c in model["controls"]]
        if native_receipt.exists():
            runs.append({"label": "native comparison", "kind": "native", "variant": variant, "expect": None})
        write_json(ROOT / f".g5/Verus_verification/models/{concern}/manifest.json",
                   {"title": f"{slug} · #281 D3+", "question": question, "ontology_ids": ontology_ids, "runs": runs})
        (ROOT / f".g5/Verus_verification/models/{concern}/BINDINGS.md").write_text(
            f"# {slug} · #281 D3+\n\n{question}\n\n"
            f"[Formal declaration](../../consolidated/issue281-d3/{model['file']})\n\n"
            + ("[Product correspondence](../../../uplift/consolidated/issue281-d3/bindings.json)\n\n" if bound else
               "Product correspondence: none yet. This model states the design the implementation must meet.\n\n")
            + "[Proof and controls](../../../uplift/consolidated/issue281-d3/proofs/models.json)\n\n"
            "[Iteration handoff](../../../uplift/consolidated/issue281-d3/HANDOFF.md)\n")
        base = model["baseline"]
        rows.append({"label": "proof", "kind": "proof", "variant": variant, "expect": "verified", "concern": concern,
                     "run_id": f"{concern}-proof", "outcome": f"exit {base['exit']}", "assessment": "matched",
                     "receipt_path": base["log"], "verified": base["verified"], "errors": base["errors"]})
        for c in model["controls"]:
            rows.append({"label": c["id"], "kind": "control", "variant": variant, "expect": "named assertion rejected",
                         "concern": concern, "run_id": f"{concern}-{c['id']}",
                         "outcome": "exit 1; rejected at " + ", ".join(c["expected"]), "assessment": "matched",
                         "receipt_path": c["log"], "verified": c["verified"], "errors": c["errors"]})
        if native_receipt.exists():
            cases = sum(len(c["observations"]) for c in json.loads(native_receipt.read_text())["cases"].values())
            rows.append({"label": "native comparison", "kind": "native", "variant": variant, "expect": None, "concern": concern,
                         "run_id": f"{concern}-native", "outcome": "native fields compared", "assessment": "matched",
                         "receipt_path": rel(native_receipt), "cases": cases})
    index = ROOT / ".g5/Verus_verification/results/viewer-index.jsonl"
    kept = [line for line in index.read_text().splitlines() if line and '"concern": "issue281-d3-' not in line]
    index.write_text("\n".join(kept + [json.dumps(row) for row in rows]) + "\n")
    print(f"Published {len(proof['models'])} viewer models ({variant}) and {len(rows)} index rows")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    group = parser.add_mutually_exclusive_group()
    for flag in ("--proofs-only", "--adjacent", "--bindings", "--native-only", "--check", "--publish"):
        group.add_argument(flag, action="store_true")
    options = parser.parse_args()
    if options.check:
        check()
        return
    if options.publish:
        publish()
        return
    everything = not (options.proofs_only or options.adjacent or options.bindings or options.native_only)
    if everything or options.proofs_only:
        proofs()
    if everything or options.adjacent:
        adjacent()
    if everything or options.bindings:
        write_json(OUT / "exact-bindings.json", exact_bindings())
        print("Bindings: resolved and hashed")
    if everything or options.native_only:
        native()


if __name__ == "__main__":
    main()
