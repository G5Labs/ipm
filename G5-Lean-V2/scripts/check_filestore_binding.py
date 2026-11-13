#!/usr/bin/env python3
"""Refuse ordinary deploys that would change an existing Filestore binding.

The manual, quiesced migration owns storage switches. This read-only check runs
immediately before Helm (including its SQL hooks), after cluster authentication.
Deployment templates are authoritative here: missing Pods or zero replicas are
valid recovery states. An entirely empty installation may bootstrap normally.
"""
from __future__ import annotations

import argparse
import ipaddress
import json
import re
import subprocess
import sys


class BindingError(Exception):
    """The selected storage identity could not be proved."""


def require(condition, message):
    if not condition:
        raise BindingError(message)


def inventory(resource, namespace=None):
    command = ["kubectl", "get", resource, "--output=json", "--request-timeout=30s"]
    if namespace:
        command.extend(["--namespace", namespace])
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=40)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise BindingError(f"Could not read {resource}: {type(error).__name__}") from error
    require(result.returncode == 0,
            f"Could not read {resource} (kubectl exit {result.returncode}); lookup failures are not absence")
    try:
        document = json.loads(result.stdout)
    except ValueError as error:
        raise BindingError(f"Invalid JSON returned for {resource}") from error
    require(isinstance(document, dict) and isinstance(document.get("items"), list),
            f"Invalid {resource} inventory")
    objects = {}
    for item in document["items"]:
        require(isinstance(item, dict) and isinstance(item.get("metadata"), dict),
                f"Invalid object returned for {resource}")
        name = item["metadata"].get("name")
        require(isinstance(name, str) and name and name not in objects,
                f"Missing or duplicate name returned for {resource}")
        objects[name] = item
    return objects


def check(namespace, pvc_name, pv_name, server, share):
    ipaddress.ip_address(server)
    require(bool(re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*", share)), "Invalid NFS share name")
    require(all(value and not value.startswith("-") for value in (namespace, pvc_name, pv_name)),
            "Namespace and storage names must be nonempty")
    deployments = inventory("deployments", namespace)
    claims = inventory("persistentvolumeclaims", namespace)
    volumes = inventory("persistentvolumes")
    workloads = [deployments.get(name) for name in ("g5-api", "g5-daemon")]
    # A retained old claim/volume means this is not a new installation, even if
    # a stale build selects different names and both Deployments were deleted.
    if not any(workloads) and not claims and not volumes:
        return "Empty installation: Filestore bootstrap permitted"
    require(all(workloads), "Both g5-api and g5-daemon Deployments must exist outside an empty bootstrap")
    for name, deployment in zip(("g5-api", "g5-daemon"), workloads):
        require(not deployment["metadata"].get("deletionTimestamp"), f"{name} is being deleted")
        template_volumes = deployment.get("spec", {}).get("template", {}).get("spec", {}).get("volumes", [])
        repos = [volume for volume in template_volumes if volume.get("name") == "repos"]
        require(len(repos) == 1, f"{name} must have exactly one repos volume")
        claim = repos[0].get("persistentVolumeClaim", {})
        require(claim.get("claimName") == pvc_name and not claim.get("readOnly", False),
                f"{name} repos volume does not use writable PVC {pvc_name}; manual cutover required")

    pvc, pv = claims.get(pvc_name), volumes.get(pv_name)
    require(pvc is not None and pv is not None, "Selected PVC and PV must both exist")
    for kind, item in (("PVC", pvc), ("PV", pv)):
        require(not item["metadata"].get("deletionTimestamp"), f"Selected {kind} is being deleted")
        require(item.get("status", {}).get("phase") == "Bound", f"Selected {kind} is not Bound")
    require(pvc["metadata"].get("namespace") == namespace, "PVC namespace differs from the deployment")
    require(pvc.get("spec", {}).get("volumeName") == pv_name, "PVC is bound to a different PV")
    uid = pvc["metadata"].get("uid")
    require(isinstance(uid, str) and bool(uid), "PVC has no UID")
    ref = pv.get("spec", {}).get("claimRef", {})
    require((ref.get("namespace"), ref.get("name"), ref.get("uid")) == (namespace, pvc_name, uid),
            "PV claimRef does not identify this PVC incarnation")
    nfs = pv.get("spec", {}).get("nfs", {})
    require(nfs.get("server") == server and nfs.get("path") == f"/{share}" and not nfs.get("readOnly", False),
            "PV NFS server/share differs from selected Filestore; manual cutover required")
    require(pv.get("spec", {}).get("persistentVolumeReclaimPolicy") == "Retain",
            "Selected PV must retain its data")
    return f"Filestore binding verified: {namespace}/{pvc_name} -> {pv_name} -> {server}:/{share}"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("namespace", "pvc", "pv", "server", "share"):
        parser.add_argument(f"--{name}", required=True)
    args = parser.parse_args(argv)
    try:
        print(check(args.namespace, args.pvc, args.pv, args.server, args.share))
    except (BindingError, ValueError, TypeError, AttributeError) as error:
        print(f"Filestore deploy refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
