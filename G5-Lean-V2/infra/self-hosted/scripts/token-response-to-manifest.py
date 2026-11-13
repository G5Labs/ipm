#!/usr/bin/env python3
"""Extract a self-hosted release manifest from a registry token response."""

from __future__ import annotations

import json
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: token-response-to-manifest.py <token-response.json> <manifest-out>", file=sys.stderr)
        return 2

    data = json.loads(Path(sys.argv[1]).read_text())
    release = data.get("release") or {}
    if "manifest" in release:
        manifest = release["manifest"]
    else:
        image_refs = release.get("image_refs") or release.get("images") or {}
        manifest = {
            "schema_version": 1,
            "product": data.get("product", "g5-self-hosted"),
            "version": release.get("version", data.get("version", "")),
            "images": {
                "api": image_refs.get("api", ""),
                "edge": image_refs.get("edge", ""),
                "worker": image_refs.get("worker", ""),
            },
            "migration": {
                "image": image_refs.get("migration") or image_refs.get("api", ""),
            },
        }

    missing = [
        key
        for key in ("api", "edge", "worker")
        if not (manifest.get("images") or {}).get(key)
    ]
    if missing:
        print(f"release manifest missing image refs: {', '.join(missing)}", file=sys.stderr)
        return 1

    Path(sys.argv[2]).write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
