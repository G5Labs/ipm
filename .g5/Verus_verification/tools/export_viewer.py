#!/usr/bin/env python3
"""Publish saved consolidated, playbook and inherited evidence using the G5 viewer format."""

import json
import os
import re
from collections import defaultdict
from pathlib import Path

V = Path(__file__).resolve().parents[1]
R = V.parents[1]
UP = V.parent / "uplift/consolidated"


def load(path):
    return json.loads(path.read_text())


def dump(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")


def link(folder, path, label):
    return f"[{label}]({os.path.relpath(path, folder)})"


def generate():
    metadata = {}
    for page in (V.parent / "ontology").rglob("*.g5.md"):
        for section in re.split(
            r"(?m)^### ", page.read_text().partition("\n## Metadata")[2]
        )[1:]:
            sid, _, footer = section.partition("\n")
            metadata[sid] = footer
    concerns = {}
    rows = []

    def family(key, title, question, owners, refs, detail=""):
        folder = V / "models" / key
        folder.mkdir(exist_ok=True, parents=True)
        concerns[key] = {
            "title": title,
            "question": question,
            "ontology_ids": sorted(set(owners) & metadata.keys()),
            "runs": [],
        }
        lines = [
            f"# {title}",
            question,
            detail,
            "The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.",
        ]
        for label, path in refs:
            if path.exists():
                lines.append(link(folder, path, label))
        (folder / "BINDINGS.md").write_text("\n\n".join(x for x in lines if x) + "\n")

    def row(
        key,
        label,
        kind,
        run_id,
        outcome,
        assessment,
        receipt,
        expect=None,
        variant="as-built",
        **extra,
    ):
        item = {"label": label, "kind": kind, "variant": variant, "expect": expect}
        rows.append(
            {
                **item,
                "concern": key,
                "run_id": run_id,
                "outcome": outcome,
                "assessment": assessment,
                "receipt_path": receipt.relative_to(R).as_posix() if receipt else None,
                **extra,
            }
        )

    crate = V / "crates/ipm-load-history"
    family(
        "commit-soundness", "Commit marker soundness",
        "Does the marker follow retained writes for a live top-level load without a caller transaction?",
        ["history.committed.marking"],
        [("Commit-soundness definition and lemmas", crate / "src/proofs.rs"),
         ("Product bindings and assumptions", crate / "BINDINGS.md"),
         ("Coverage and reliance", V / "coverage-map.md")],
        "The proof covers the modeled live top-level load with no caller transaction. It permits the interval between commit and marker write. Refusal policy, nested callers and whole-product correctness remain qualified in the coverage map. The receipt reports the whole crate's obligation count.",
    )
    family(
        "load-outcomes", "Load outcome contracts",
        "Which load status and namespace end states satisfy the recorded scoped contracts?",
        ["load", "history.committed"],
        [("Product bindings and modeled domain", crate / "BINDINGS.md"),
         ("Coverage and known violations", V / "coverage-map.md")],
        "These bounded explorations include known violations of the intended outcome contracts. Read each expectation and variant. They do not establish every loading option or nested history rule.",
    )
    selected = {
        "asbuilt-proofs-r3": "commit-soundness",
        "final-control-03-verus": "commit-soundness",
        "asbuilt-l1-normal-conforming-domain": "load-outcomes",
        "asbuilt-l1-ns-all-conforming-domain": "load-outcomes",
        "asbuilt-l1-end": "load-outcomes",
        "asbuilt-commit-exact": "load-outcomes",
    }
    latest = {}
    for line in (V / "results/index.jsonl").read_text().splitlines():
        if not line.strip(): continue
        r = json.loads(line)
        if r["label"] not in selected: continue
        previous = latest.get(r["label"])
        if previous is None or r["run_id"] >= previous["run_id"]:
            latest[r["label"]] = r
    for label, r in sorted(latest.items()):
        outcome = r["outcome"]
        expected = r.get("expect") or ""
        path = V / "results/verus" / r["run_id"] / "receipt.json"
        counts = {}
        match = re.search(r"verified (\d+), errors (\d+)", outcome, re.I)
        if match: counts = {"verified": int(match[1]), "errors": int(match[2])}
        if re.search("incomplete|timeout|resource|rlimit|superseded", outcome, re.I): assessment = "incomplete"
        elif label == "final-control-03-verus" and "verification errors 2" in outcome and "case_c08_committed_without_change" in outcome and "lemma_lnm0_a" in outcome: assessment = "matched"
        elif expected == outcome or (match and match[2] == "0" and expected == "verified, 0 errors"): assessment = "matched"
        else: assessment = "unknown"
        row(selected[label], label, r.get("kind"), r["run_id"], outcome, assessment, path, expected, r.get("variant"), source_concern=r["concern"], **counts)

    for directory in sorted(UP.glob("iter[0-9]*")):
        if not directory.name[4:].isdigit():
            continue
        evidence = directory / "proofs/models.json"
        if not evidence.exists():
            continue
        models = load(evidence).get("models", [])
        bindings = directory / "bindings.json"
        binding_data = load(bindings) if bindings.exists() else []
        binding_rows = (
            binding_data.get("rows", [])
            if isinstance(binding_data, dict)
            else binding_data
        )
        for model in models:
            source = (
                V / "consolidated" / directory.name / model["file"]
                if "file" in model
                else R / model["model"]
            )
            key = directory.name + "-" + source.stem.replace("_", "-")
            bound = [
                r
                for r in binding_rows
                if Path(r.get("model") or "").name == source.name
            ]
            owners = {r.get("owner", "").split("#")[-1] for r in bound}
            owners.update(
                sid
                for sid, meta in metadata.items()
                if f"/{directory.name}/{source.name}" in meta
            )
            scopes = list(dict.fromkeys(r["scope"] for r in bound if r.get("scope")))
            refs = [
                ("Formal declaration", source),
                ("Product correspondence", bindings),
                ("Product symbols and lines", directory / "exact-bindings.json"),
                ("Proof and controls", evidence),
                ("Iteration handoff", directory / "HANDOFF.md"),
                ("Sealed project map", directory / "project-map.json"),
            ]
            claims = "\n".join("- " + s for s in scopes)
            family(
                key,
                source.stem.replace("_", " ") + " · " + directory.name,
                scopes[0] if scopes else "What does the saved " + source.name + " model establish?",
                owners,
                refs,
                claims,
            )
            base = model.get("baseline") or model.get("proof") or {}
            run_prefix = key if model.get("variant") or model.get("native_receipt") else directory.name
            for label, kind, item in [("proof", "proof", base)] + [
                (c.get("id", c.get("name", "control")), "control", c)
                for c in model.get("controls", [])
            ]:
                log = Path(item.get("log") or "")
                logfile = directory / "proofs" / log.name
                if "proofs" in log.parts:
                    logfile = (
                        directory
                        / "proofs"
                        / Path(*log.parts[log.parts.index("proofs") + 1 :])
                    )
                receipt = logfile if logfile.is_file() else evidence
                rejected = item.get(
                    "assertion_rejected", item.get("rejected_at_assertion")
                )
                incomplete = bool(
                    re.search(
                        "resource|rlimit|timed out", str(item.get("stderr", "")), re.I
                    )
                    or item.get("timed_out")
                )
                good = (
                    (item.get("exit") == 0 and item.get("errors", 0) == 0)
                    if kind == "proof"
                    else rejected
                )
                assessment = (
                    "incomplete"
                    if incomplete
                    else "matched"
                    if good
                    else "mismatch"
                    if good is False
                    else "unknown"
                )
                outcome = "exit " + str(item.get("exit", "unknown"))
                if rejected:
                    outcome += "; assertion rejected"
                row(
                    key,
                    label,
                    kind,
                    run_prefix + "-" + label,
                    outcome,
                    assessment,
                    receipt,
                    "verified" if kind == "proof" else "named assertion rejected",
                    variant=model.get("variant", "as-built"),
                    verified=item.get("verified"),
                    errors=item.get("errors"),
                )
            if model.get("native_receipt"):
                receipt = directory / model["native_receipt"]
                observations = load(receipt)["observations"]
                row(
                    key, "native comparison", "native", key + "-native",
                    "native fields compared", "matched", receipt, cases=len(observations),
                )
            run = directory / "run.json"
            if run.exists() and len(models) == 1:
                summary = load(run)
                if summary.get("native_fields"):
                    row(
                        key,
                        "native comparison",
                        "native",
                        directory.name + "-native",
                        "native fields compared",
                        "matched" if summary.get("ok") else "unknown",
                        run,
                        cases=summary["native_fields"],
                    )
    pb = R / "playbook-uplift/models/verus"
    for source in sorted(pb.glob("*.rs")):
        name = source.stem
        key = "playbook-" + name.replace("_", "-")
        owners = [sid for sid, meta in metadata.items() if name + ".rs" in meta]
        receipt = pb / "results" / (name + ".json")
        family(
            key,
            name + " · inherited playbook",
            "What did the inherited " + name + " model verify?",
            owners,
            [
                ("Formal declaration", source),
                ("Trust and scope", source.with_suffix(".TRUST.md")),
                ("Recorded results", receipt),
            ],
        )
        if not receipt.exists():
            continue
        data = load(receipt)
        baseline = data.get("baseline")
        if isinstance(baseline, list) and len(baseline) == 2:
            row(
                key,
                "baseline",
                "proof",
                "baseline",
                f"{baseline[0]} verified, {baseline[1]} errors",
                "matched" if baseline[1] == 0 else "mismatch",
                receipt,
                verified=baseline[0],
                errors=baseline[1],
            )
        for c in data.get("controls", []):
            if not isinstance(c, dict):
                continue
            status = c.get("status", "unknown")
            row(
                key,
                c.get("name", "control"),
                "control",
                c.get("name", "control"),
                status,
                "matched"
                if status == "killed"
                and set(c.get("errors_in", [])) & set(c.get("expected_in", []))
                else "unknown",
                receipt,
            )
    audit = UP / "findings-audit-iter28/proofs"
    for check in load(audit / "receipt.json"):
        source = Path(check["path"])
        prefix = (
            "playbook" if source.parts[0] == "playbook-uplift" else source.parent.name
        )
        key = prefix + "-" + source.stem.replace("_", "-")
        receipt = audit / (check["name"] + ".log")
        saved = receipt.read_text()
        counts = re.search(r"(\d+) verified, (\d+) errors", saved)
        if not counts or key not in concerns:
            raise ValueError(f"Unmapped audit proof {source}")
        verified, errors = map(int, counts.groups())
        limited = "resource limit" in saved.lower()
        assessment = (
            "incomplete"
            if limited
            else "matched"
            if check["exit"] == 0 and not errors
            else "mismatch"
        )
        row(
            key,
            "baseline" if prefix == "playbook" else "proof",
            "proof",
            "iter28-audit-" + check["name"],
            counts[0] + (" (resource limit)" if limited else ""),
            assessment,
            receipt,
            "verified",
            verified=verified,
            errors=errors,
        )
    # The active projection contains the latest saved execution of each declared check.
    latest_rows = {}
    for item in rows:
        latest_rows[(item["concern"], item["label"])] = item
    rows = list(latest_rows.values())
    for key, manifest in concerns.items():
        by_label = {
            r["label"]: {k: r[k] for k in ("label", "kind", "variant", "expect")}
            for r in rows
            if r["concern"] == key
        }
        manifest["runs"] = list(by_label.values())
        dump(V / "models" / key / "manifest.json", manifest)
    (V / "results/viewer-index.jsonl").write_text(
        "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in rows)
    )
    print(f"{len(concerns)} concerns and {len(rows)} saved verification rows")


if __name__ == "__main__":
    generate()
