"""Tests for g5_verify_changed.py shallow-checkout fallback behavior."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import g5_verify_changed as gvc  # noqa: E402


def test_every_extracted_rust_owner_is_production_source():
    for crate_name in [
        "g5-api-foundation",
        "g5-api-data",
        "g5-api-core",
        "g5-api-routes",
        "g5d-foundation",
        "g5d-isolation",
        "g5d-agent",
        "g5d-host",
        "g5d-dispatch",
        "g5d",
    ]:
        assert gvc.is_production_source(f"backend/{crate_name}/src/lib.rs"), crate_name
    # The CLI library lives inside g5d now; its module root is production source.
    assert gvc.is_production_source("backend/g5d/src/cli/mod.rs")


def test_grounding_existence_reads_the_repository_root_store(monkeypatch, tmp_path):
    source = "backend/g5-api-core/src/lib.rs"
    grounding = tmp_path / "intent_grounding/code/g5-api-core__src__lib.rs.jsonl"
    grounding.parent.mkdir(parents=True)
    grounding.write_text("{}\n", encoding="utf-8")
    monkeypatch.setattr(gvc, "REPO_ROOT", tmp_path)

    assert gvc.check_grounding_existence([source]) == []


def test_resolve_base_falls_back_to_head_tree_when_history_missing(monkeypatch, capsys):
    def fake_git(*args: str) -> str:
        if args == ("merge-base", "HEAD", "origin/main"):
            raise RuntimeError("origin/main unavailable")
        raise AssertionError(f"unexpected git call: {args}")

    monkeypatch.setattr(gvc, "_git", fake_git)
    monkeypatch.setattr(gvc, "_ref_exists", lambda _ref: False)
    monkeypatch.delenv("G5_VERIFY_BASE", raising=False)

    assert gvc.resolve_base(None) is None
    assert "falling back to HEAD tree diff" in capsys.readouterr().err


def test_changed_files_uses_head_tree_diff_without_base(monkeypatch):
    calls: list[tuple[str, ...]] = []

    def fake_git(*args: str) -> str:
        calls.append(args)
        assert args == ("diff-tree", "--no-commit-id", "--name-only", "-r", "--root", "HEAD")
        return "scripts/g5_verify_changed.py\n"

    monkeypatch.setattr(gvc, "_git", fake_git)

    assert gvc.changed_files(None) == ["scripts/g5_verify_changed.py"]
    assert calls == [("diff-tree", "--no-commit-id", "--name-only", "-r", "--root", "HEAD")]


def test_changed_files_falls_back_when_base_diff_fails(monkeypatch, capsys):
    calls: list[tuple[str, ...]] = []

    def fake_git(*args: str) -> str:
        calls.append(args)
        if args == ("diff", "--name-only", "--diff-filter=d", "origin/main...HEAD"):
            raise RuntimeError("ambiguous argument")
        if args == ("diff-tree", "--no-commit-id", "--name-only", "-r", "--root", "HEAD"):
            return "cloudbuild-ci.yaml\n"
        raise AssertionError(f"unexpected git call: {args}")

    monkeypatch.setattr(gvc, "_git", fake_git)

    assert gvc.changed_files("origin/main") == ["cloudbuild-ci.yaml"]
    assert "falling back to HEAD tree diff" in capsys.readouterr().err
    assert calls == [
        ("diff", "--name-only", "--diff-filter=d", "origin/main...HEAD"),
        ("diff-tree", "--no-commit-id", "--name-only", "-r", "--root", "HEAD"),
    ]


def test_human_diff_skips_when_no_base_is_available(capsys):
    assert gvc.check_human_diff(None, [".semlift/specs/g5/example.semlift.md"]) == []
    assert "skipping <human> diff" in capsys.readouterr().err


def test_human_diff_skips_design_check_when_no_base_is_available(capsys):
    assert gvc.check_human_diff(None, ["docs/reference/designs/example.md"]) == []
    assert "skipping <human> diff" in capsys.readouterr().err


def test_human_spans_ignore_inline_code_mentions():
    text = "No `<human>` tags in this diff.\nActual block follows.\n<human>\nkept\n</human>\n"

    assert gvc._human_line_spans(text) == [(3, 5)]


def test_edge_fields_reject_non_integer_relevance(monkeypatch, tmp_path):
    grounding = tmp_path / "intent_grounding/code/demo.jsonl"
    grounding.parent.mkdir(parents=True)
    grounding.write_text(
        "\n".join(
            [
                '{"source_id":"I1","target_id":"A","relationship":"implements","relevance":1}',
                '{"source_id":"I1","target_id":"A","relationship":"implements","relevance":0.0}',
                '{"source_id":"I1","target_id":"A","relationship":"implements","relevance":50.25}',
                '{"source_id":"I1","target_id":"A","relationship":"implements","relevance":true}',
            ]
        )
        + "\n",
        encoding="utf-8",
    )
    monkeypatch.setattr(gvc, "REPO_ROOT", tmp_path)

    findings = gvc.check_edge_fields(["intent_grounding/code/demo.jsonl"])

    assert findings == [
        "intent_grounding/code/demo.jsonl:2: non-canonical relevance 0.0",
        "intent_grounding/code/demo.jsonl:3: non-canonical relevance 50.25",
        "intent_grounding/code/demo.jsonl:4: non-canonical relevance True",
    ]


def test_human_diff_checks_deletions_against_base_spans(monkeypatch, tmp_path):
    target = tmp_path / "views/example.md"
    target.parent.mkdir(parents=True)
    target.write_text("<human>\nkept\n</human>\n", encoding="utf-8")
    old_text = "<human>\nkept\nsecret\n</human>\n"
    diff = "@@ -3 +2,0 @@\n-secret\n"

    def fake_git(*args: str) -> str:
        if args == ("show", "base:views/example.md"):
            return old_text
        if args == ("diff", "--unified=0", "base...HEAD", "--", "views/example.md"):
            return diff
        raise AssertionError(f"unexpected git call: {args}")

    monkeypatch.setattr(gvc, "REPO_ROOT", tmp_path)
    monkeypatch.setattr(gvc, "_git", fake_git)

    findings = gvc.check_human_diff("base", ["views/example.md"])

    assert len(findings) == 1
    assert findings[0].startswith("views/example.md:3: deletion inside <human> block")
