"""Approved artifact ownership is one unit, independent of QA execution."""
import unittest
import release_artifacts as artifacts


class ArtifactLayoutTest(unittest.TestCase):
    def test_both_reviewed_layouts_keep_qa_execution(self):
        for project in artifacts.ARTIFACT_PROJECTS:
            self.assertEqual(artifacts.validate_layout(artifacts.QA_BUILD_PROJECT, project, project, project,
                f"{project}-lean-releases", require_bucket=True), project)

    def test_mixed_or_unapproved_destinations_are_rejected(self):
        qa, admin = artifacts.QA_BUILD_PROJECT, artifacts.ADMIN_ARTIFACT_PROJECT
        for args in [(admin, admin, admin, admin, f"{admin}-lean-releases"),
                     (qa, "other-project", "other-project", "other-project", "other-project-lean-releases"),
                     (qa, admin, qa, admin, f"{admin}-lean-releases"),
                     (qa, admin, admin, qa, f"{admin}-lean-releases"),
                     (qa, admin, admin, admin, f"{qa}-lean-releases")]:
            with self.subTest(args=args), self.assertRaises(ValueError):
                artifacts.validate_layout(*args, require_bucket=True)

    def test_bucket_may_only_be_empty_when_no_release_is_requested(self):
        qa = artifacts.QA_BUILD_PROJECT
        artifacts.validate_layout(qa, qa, qa, qa)
        with self.assertRaises(ValueError):
            artifacts.validate_layout(qa, qa, qa, qa, require_bucket=True)


if __name__ == "__main__":
    unittest.main()
