# Iteration-75 repair ledger format

One file per repair group, `ledger/<group>.json`, written by the group's builder or fork. `apply.py` turns the
ledgers into census record updates. Forks do not edit census records, ontology pages, bugs_issues.md or the
handoff.

```json
{"group": "R3", "items": "HANDOFF-72 §6 item 3",
 "entries": [{
   "id": "artifact.filter#1",
   "old_grade": "partial", "new_grade": "full" | "partial" | "absent",
   "check": "proved" | "bounded" | "refuted" | "unchecked" | "none",
   "models": ["<repo path>::<decl>", ...],          // full replacement list for the record
   "evidence_add": ["<repo path>", ...],             // appended to the record's evidence
   "support": "mutation-checked",                     // when new_grade is full
   "mutations": ["R3-m01"],                           // ids in iter75/mutations.jsonl (killed for full)
   "receipts": ["receipts/<slug>.log"],
   "product": "src/cls/IPM/...cls:NN-MM (what the product does for the neighbour)",
   "neighbour": "<lemma name>: <the case asserted>",
   "basis_note": "<one or two sentences appended after 'new:iteration-75'>",
   "gap": "<required unless full>",
   "instance_scoped": false,
   "refutation": "<as-built divergence, if any>",
   "ontology_repair": "<text repair exposed, if any>",
   "finding": "<candidate product defect, if any>",
   "metadata_link": {"page": ".g5/ontology/...g5.md", "block": "<block id>", "model": "<repo path>"}
 }],
 "bindings": [{"formal": "<repo path>::<decl>", "file": "src/cls/IPM/...cls", "member": "<Class>::<Method>",
               "lines": "NN-MM", "role": "decide|write|read|guard|neighbour", "note": "..."}],
 "not_done": [{"id": "...", "why": "..."}],
 "cost": "<wall time, tokens if visible>"}
```
