#!/usr/bin/env python3
"""Publish versioned worker artifacts without overwriting earlier content.

The QA build owns execution/signing; a reviewed artifact project owns all three
durable outputs. GCS writes use generation zero, OCI uses content digests, and a
GCE image retry must prove the same source/metadata before reusing its name.
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

from release_artifacts import resolve_artifact_project


def require(condition, message):
    if not condition:
        raise ValueError(message)


def run(*args, check=True, env=None):
    result = subprocess.run(args, text=True, capture_output=True, env=env)
    if check and result.returncode:
        raise RuntimeError(f"{args[0]} {args[1]} failed: {result.stderr.strip()[:400]}")
    return result


def layout(build_project, artifact_project, image_project, region, repository, bucket_suffix, family):
    owner = resolve_artifact_project(build_project, artifact_project)
    require(image_project == owner, "GCE image and worker artifacts must share the selected owner")
    require((region, repository, bucket_suffix, family) ==
            ("us-central1", "g5-worker-images", "g5-worker-images", "g5-worker"),
            "Worker artifact location is not approved")
    return {"owner": owner, "bucket": f"{owner}-g5-worker-images",
            "repository": f"{region}-docker.pkg.dev/{owner}/{repository}/{family}",
            "latest": f"gs://{build_project}-g5-worker-images/latest.json"}


def fingerprint(path):
    sha, md5, size = hashlib.sha256(), hashlib.md5(), 0
    with Path(path).open("rb") as stream:
        while data := stream.read(8 * 1024 * 1024):
            sha.update(data)
            md5.update(data)
            size += len(data)
    return sha.hexdigest(), base64.b64encode(md5.digest()).decode(), size


def upload_create_only(path, uri, runner=run):
    sha, md5, size = fingerprint(path)
    # Large files otherwise use temporary composite objects, which need broader
    # create/delete access and produce no MD5. Keep resumable single-object
    # uploads so exact-prefix create-only permissions and retry proof both hold.
    upload_env = {**os.environ, "CLOUDSDK_STORAGE_PARALLEL_COMPOSITE_UPLOAD_ENABLED": "False"}
    result = runner("gcloud", "storage", "cp", str(path), uri,
                    "--if-generation-match=0", f"--custom-metadata=sha256={sha}",
                    check=False, env=upload_env)
    # A failed upload may be a harmless retry or race. Read authoritative object
    # metadata and prove equivalence; never classify every error as "exists".
    raw = runner("gcloud", "storage", "objects", "describe", uri, "--raw", "--format=json")
    obj = json.loads(raw.stdout)
    bucket, name = uri.removeprefix("gs://").split("/", 1)
    require(obj.get("bucket") == bucket and obj.get("name") == name
            and obj.get("md5Hash") == md5 and int(obj.get("size", -1)) == size
            and obj.get("metadata", {}).get("sha256") == sha and str(obj.get("generation", "")).isdigit(),
            f"Immutable object collision or unverifiable upload: {uri}")
    print(f"{'Published' if result.returncode == 0 else 'Verified existing'} {uri}")
    return obj


def publish_oci(stage, repository, runner=run):
    """Digest-addressed OCI 1.1 manifest; no mutable worker-version tag.

    ORAS 1.2 blob/manifest push support @digest. Uploading a digest again is
    content-idempotent; the registry cannot bind different bytes to that digest.
    """
    stage = Path(stage)
    config = stage / "empty-config.json"
    config.write_bytes(b"{}")
    layers = []
    inputs = [(config, "application/vnd.oci.empty.v1+json"),
              (stage / "manifest.json", "application/json"),
              (stage / "sbom.spdx.json", "application/spdx+json"),
              (stage / "disk-pointer.json", "application/vnd.g5.worker.disk-pointer.v1+json")]
    for path, media_type in inputs:
        sha, _, size = fingerprint(path)
        descriptor = {"mediaType": media_type, "digest": f"sha256:{sha}", "size": size}
        if path != config:
            descriptor["annotations"] = {"org.opencontainers.image.title": path.name}
        layers.append(descriptor)
        runner("oras", "blob", "push", f"{repository}@sha256:{sha}", str(path))
    manifest = {"schemaVersion": 2, "mediaType": "application/vnd.oci.image.manifest.v1+json",
                "artifactType": "application/vnd.g5.worker.image.v1+json", "config": layers[0], "layers": layers[1:]}
    payload = json.dumps(manifest, separators=(",", ":"), sort_keys=True).encode()
    manifest_path = stage / "oci-manifest.json"
    manifest_path.write_bytes(payload)
    ref = f"{repository}@sha256:{hashlib.sha256(payload).hexdigest()}"
    runner("oras", "manifest", "push", ref, str(manifest_path))
    fetched = runner("oras", "manifest", "fetch", ref).stdout
    require(hashlib.sha256(fetched.encode()).hexdigest() == ref.split("@sha256:")[1],
            "Published OCI manifest bytes differ from the expected digest")
    return ref


def import_image(owner, version, source_uri, source_sha256, source_generation,
                 build_project, build_id, commit, timestamp, oci_ref, runner=run):
    require(re.fullmatch(r"g5-worker-[a-z0-9-]{1,53}", version), "Invalid worker version")
    metadata = {"source_uri": source_uri, "source_sha256": source_sha256,
                "source_generation": str(source_generation), "oci_ref": oci_ref,
                "build_project": build_project, "build_id": build_id, "commit": commit}
    description = json.dumps(metadata, sort_keys=True, separators=(",", ":"))
    labels = {"g5_image_version": version, "g5_image_family": "g5-worker",
              "g5_build_timestamp": timestamp, "g5_git_short_sha": commit[:7],
              "g5_git_commit": commit, "g5_build_project": build_project, "g5_build_id": build_id,
              "g5_root_filesystem": "xfs", "g5_xfs_reflink": "1"}

    def existing():
        result = runner("gcloud", "compute", "images", "list", f"--project={owner}",
                        "--no-standard-images", f"--filter=name={version}", "--format=json")
        return [image for image in json.loads(result.stdout) if image.get("name") == version]

    images = existing()
    if not images:
        runner("gcloud", "compute", "images", "create", version, f"--project={owner}",
                        f"--source-uri={source_uri}", "--family=g5-worker",
                        "--guest-os-features=VIRTIO_SCSI_MULTIQUEUE,GVNIC", f"--description={description}",
                        "--labels=" + ",".join(f"{key}={value}" for key, value in labels.items()), check=False)
        images = existing()
        require(images, "GCE image creation did not produce the expected image")
    require(len(images) == 1, "Ambiguous GCE image identity")
    image = images[0]
    require(image.get("family") == "g5-worker" and image.get("status") == "READY"
            and image.get("description") == description
            and all(image.get("labels", {}).get(key) == value for key, value in labels.items())
            and {feature.get("type") for feature in image.get("guestOsFeatures", [])}
                >= {"VIRTIO_SCSI_MULTIQUEUE", "GVNIC"},
            "Existing GCE image differs from the verified build; refusing name collision")
    return f"projects/{owner}/global/images/{version}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("layout", "upload", "oci", "import"))
    parser.add_argument("--build-project", required=True)
    parser.add_argument("--artifact-project", required=True)
    parser.add_argument("--image-project", required=True)
    parser.add_argument("--region", default="us-central1")
    parser.add_argument("--repository", default="g5-worker-images")
    parser.add_argument("--bucket-suffix", default="g5-worker-images")
    parser.add_argument("--family", default="g5-worker")
    parser.add_argument("--workspace", default="/workspace")
    parser.add_argument("--files", nargs="*")
    parser.add_argument("--build-id", default="")
    parser.add_argument("--commit", default="")
    args = parser.parse_args()
    selected = layout(args.build_project, args.artifact_project, args.image_project,
                      args.region, args.repository, args.bucket_suffix, args.family)
    workspace = Path(args.workspace)
    if args.action == "layout":
        print(json.dumps(selected, sort_keys=True))
        return
    version = (workspace / "image_version.txt").read_text().strip()
    require(re.fullmatch(r"g5-worker-[a-z0-9-]{1,53}", version), "Invalid worker version")
    prefix = f"gs://{selected['bucket']}/{version}"
    if args.action == "upload":
        require(args.files, "Explicit files are required")
        for filename in args.files:
            upload_create_only(filename, f"{prefix}/{Path(filename).name}")
    elif args.action == "oci":
        ref = publish_oci(workspace / "oras-stage", selected["repository"])
        (workspace / "worker-oci-ref.txt").write_text(ref + "\n")
    else:
        require(re.fullmatch(r"[0-9a-f-]{36}", args.build_id) and re.fullmatch(r"[0-9a-f]{40}", args.commit),
                "Image import requires immutable build identity")
        tarball = workspace / "infra/packer/g5-worker/output" / version / f"{version}.tar.gz"
        obj = upload_create_only(tarball, f"{prefix}/{tarball.name}")
        print(import_image(selected["owner"], version, f"{prefix}/{tarball.name}",
                           obj["metadata"]["sha256"], obj["generation"], args.build_project,
                           args.build_id, args.commit, (workspace / "build_timestamp.txt").read_text().strip(),
                           (workspace / "worker-oci-ref.txt").read_text().strip()))


if __name__ == "__main__":
    main()
