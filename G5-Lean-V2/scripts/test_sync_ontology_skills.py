"""Exercise copy drift and regeneration using disposable skill trees."""

import importlib.util
from pathlib import Path
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    "sync_ontology_skills", Path(__file__).with_name("sync-ontology-skills.py"))
sync_skills = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sync_skills)


class SkillCopyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.write(".agents/skills/ontology-planning/SKILL.md", "Planning instructions")
        self.write(".agents/skills/ontology-planning/references/example.md", "Manual example")

    def write(self, relative, content):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
        return path

    def test_check_is_read_only_and_regeneration_preserves_source_examples(self):
        self.assertTrue(sync_skills.sync(self.root, check=True))
        self.assertFalse((self.root / ".codex").exists())
        self.assertEqual(sync_skills.sync(self.root), [])
        self.assertEqual(sync_skills.sync(self.root, check=True), [])
        changed = self.write(".codex/skills/ontology-planning/references/example.md", "Drift")
        stale = self.write(".codex/skills/ontology-planning/references/removed.md", "Stale")
        self.assertTrue(sync_skills.sync(self.root, check=True))
        self.assertEqual(changed.read_text(), "Drift")
        self.assertTrue(stale.exists())
        self.assertEqual(sync_skills.sync(self.root), [])
        self.assertEqual(changed.read_text(), "Manual example")
        self.assertFalse(stale.exists())

    def test_guidance_links_resolve_in_native_and_documentation_copies(self):
        self.write(".agents/skills/ontology-planning/SKILL.md",
                   "[Format](../../../.agents/guidance/ontology-format.md)")
        self.write(".agents/skills/ontology-planning/references/model.md",
                   "[Format](../../../../.agents/guidance/ontology-format.md)")
        self.write(".agents/guidance/ontology-format.md", "Storage conventions")
        retired = [self.write(f"{mirror}/{name}/SKILL.md", "Old")
                   for mirror in sync_skills.MIRRORS
                   for name in ("ontology-authoring", "ontology-uplift", "ontology-downlift",
                                "ontology-review")]
        self.assertEqual(sync_skills.sync(self.root), [])
        self.assertTrue(all(not path.exists() for path in retired))
        import re
        for mirror in sync_skills.MIRRORS:
            for relative in ["SKILL.md", "references/model.md"]:
                page = self.root / mirror / "ontology-planning" / relative
                link = re.search(r"\]\(([^)]+)\)", page.read_text())[1]
                self.assertEqual((page.parent / link).read_text(), "Storage conventions")
        target = self.root / sync_skills.DOC_BUNDLE / ".agents/guidance/ontology-format.md"
        target.write_text("Drift")
        self.assertIn("missing or mismatched documentation guidance", sync_skills.sync(self.root, check=True))
        self.assertEqual(target.read_text(), "Drift")
        self.assertEqual(sync_skills.sync(self.root), [])
        self.assertEqual(target.read_text(), "Storage conventions")
        self.assertEqual(sync_skills.sync(self.root, check=True), [])

    def test_retired_experiment_is_reported_read_only_then_removed_from_all_mirrors(self):
        retired = [self.write(f"{mirror}/ontology-experiment/SKILL.md", "Retired")
                   for mirror in sync_skills.MIRRORS]
        errors = sync_skills.sync(self.root, check=True)
        for mirror, path in zip(sync_skills.MIRRORS, retired):
            self.assertIn(f"unexpected skill: {mirror}/ontology-experiment", errors)
            self.assertTrue(path.exists())
        self.assertEqual(sync_skills.sync(self.root), [])
        self.assertTrue(all(not path.parent.exists() for path in retired))
        self.assertEqual(sync_skills.sync(self.root, check=True), [])

    def test_retired_entrypoint_is_removed_but_unrelated_skill_is_preserved(self):
        retired = self.write(".claude/skills/ontology-design-review/SKILL.md", "Old entrypoint")
        unrelated = self.write(".claude/skills/personal-skill/SKILL.md", "Keep this")
        errors = sync_skills.sync(self.root)
        self.assertFalse(retired.exists())
        self.assertEqual(unrelated.read_text(), "Keep this")
        self.assertIn("unexpected skill: .claude/skills/personal-skill", errors)


if __name__ == "__main__":
    unittest.main()
