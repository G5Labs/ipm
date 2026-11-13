"""Fixed Lean artifact locations, independent of QA build/provenance ownership."""

QA_BUILD_PROJECT = "g5-qa-1ccc"
ADMIN_ARTIFACT_PROJECT = "g5lean-admin-1ccc"
ARTIFACT_PROJECTS = frozenset({QA_BUILD_PROJECT, ADMIN_ARTIFACT_PROJECT})


def resolve_artifact_project(build_project, artifact_project):
    if build_project != QA_BUILD_PROJECT or artifact_project not in ARTIFACT_PROJECTS:
        raise ValueError("artifacts require the QA coordinator and an approved artifact project")
    return artifact_project


def release_bucket(artifact_project):
    resolve_artifact_project(QA_BUILD_PROJECT, artifact_project)
    return f"{artifact_project}-lean-releases"


def validate_layout(build_project, artifact_project, registry_project, worker_project,
                    bucket="", require_bucket=False):
    project = resolve_artifact_project(build_project, artifact_project)
    if registry_project != project or worker_project != project:
        raise ValueError("registry and worker image projects must match the selected artifact project")
    if (bucket or require_bucket) and bucket != release_bucket(project):
        raise ValueError("release bucket must match the selected artifact project")
    return project
