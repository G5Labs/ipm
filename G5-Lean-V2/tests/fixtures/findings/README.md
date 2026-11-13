# Findings format fixtures

Shared by the format checker (`.agents/guidance/scripts/test_check_findings.py`)
and the viewer's parser (`frontend/src/lib/findings.test.ts`), so both read the
findings format the same way (`g5.documents.findings.files`). Each folder holds a
findings file and `expected.json`: its file-level problems and its entries as the
checker's `parse` reports them, with 1-based lines and an exclusive `end`.
