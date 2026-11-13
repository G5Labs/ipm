#!/usr/bin/env python3
"""Seed the full-stack preview through its real API; safe to run again.

Requires seed-dev-org.sh's Alex/admin and Jordan/member personas and E2E auth.
Uses G5 issues and PRs so the preview needs no external tracker credentials.
"""

import argparse
import json
import os
from urllib.error import HTTPError
from urllib.request import Request, urlopen


SCENARIOS = [
    {
        "name": "preview-issue-only",
        "title": "Keep the preview profile visible while closing",
        "phase": "se",
        "body": """Closing a preview currently clears the selected profile before the shutdown finishes. This makes it hard to tell which preview is stopping.

### Reproduce
1. Open a workspace and start the **full-stack** preview.
2. Close the preview while watching its status.
3. The profile name disappears before the preview has stopped.

### Acceptance criteria
- Keep the profile name visible throughout shutdown.
- Disable repeated Close actions while shutdown is pending.
- Clear the profile only after shutdown succeeds; preserve it if shutdown fails.""",
        "comments": [
            ("member", "I can reproduce this with both **full-stack** and **frontend-only**. The name disappears immediately, but the status still says Closing for several seconds."),
            ("owner", "The selected profile is being cleared when Close starts. We should keep the selection until the server reports that the preview has stopped."),
            ("member", "Please cover the failure path too. If cleanup fails, the profile name and the error should remain visible so I can retry without guessing which preview is still running."),
            ("owner", "Agreed. Proposed checks:\n- [ ] Successful close keeps the name until completion\n- [ ] Failed close keeps the name and allows retry\n- [ ] A second click cannot submit another close\n\nReady for implementation once these cases are covered."),
        ],
    },
    {
        "name": "preview-issue-and-pr",
        "title": "Show compute slot sizes in running workspaces",
        "phase": "ie",
        "body": """The Running compute tile shows which workspaces are active, but people cannot compare their allocated resources without opening each workspace.

### Expected behavior
Show the allocated CPU and memory beside each running workspace, using the same units as workspace settings. Starting workspaces should have a clear pending state rather than a misleading zero.

### Acceptance criteria
- A running workspace can show **4 CPUs · 16 GiB**.
- A starting workspace shows that allocation is pending.
- Missing resource information does not hide the workspace.
- Workspace links and keyboard navigation continue to work.""",
        "comments": [
            ("owner", "The API already returns `slotCpus` and `slotMemMib` for each running workspace. We can use those values without adding another request per row."),
            ("member", "For the display, please use **4 CPUs · 16 GiB**. A missing allocation should say Pending while the workspace starts; showing 0 CPUs would look like an error."),
            ("owner", "The implementation description is ready for review. It covers the resource label, pending allocations, and preserving the existing workspace link."),
            ("member", "Review checklist:\n- [ ] Compare a running workspace with a starting one\n- [ ] Check a row with missing memory information\n- [ ] Tab through workspace links\n- [ ] Check the layout at narrow widths\n\nPlease keep the issue open until the PR is reviewed."),
        ],
        "pr": """Display the workspace's allocated CPU and memory beside its name, for example **4 CPUs · 16 GiB**. Use the existing `slotCpus` and `slotMemMib` fields and convert MiB to GiB for display.

### Behavior to review
- Running workspaces show their allocation when it is available.
- Starting workspaces show a pending allocation.
- Missing values leave the workspace row and its navigation available.""",
    },
]


# Only exact original sample text is upgraded; keep any subsequent human edits.
LEGACY_ISSUE_NOTES = {
    "preview-issue-only": "\n\nThis workspace is still discussing the fix, so it has no implementation PR yet.",
    "preview-issue-and-pr": "\n\nAn open G5 PR in this workspace provides an implementation description to review.",
}
LEGACY_PR_HEADING = "## Show allocated resources in Running compute\n\n"
LEGACY_PR_NOTE = "\n\n### Preview exercise\nThis sample G5 PR exercises the implementation reader, repository commit vector, PR status, and activity filters without publishing to GitHub. Use the linked issue conversation to discuss the expected behavior before merging."


class API:
    def __init__(self, url, token=None):
        self.url = url.rstrip("/")
        self.token = token

    def call(self, method, path, body=None):
        headers = {"Content-Type": "application/json"}
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        request = Request(
            self.url + path,
            data=json.dumps(body).encode() if body is not None else None,
            method=method,
            headers=headers,
        )
        try:
            with urlopen(request, timeout=60) as response:
                return json.load(response)["data"]
        except HTTPError as error:
            text = error.read().decode("utf-8", "replace")
            try:
                detail = json.loads(text).get("error")
            except (ValueError, AttributeError):
                detail = None
            reason = (
                f"{detail.get('code', '')} {detail.get('message', '')}".strip()
                if isinstance(detail, dict)
                else text.strip()[:500]
            )
            raise RuntimeError(f"{method} {path}: HTTP {error.code}: {reason}") from None

    def persona(self, email):
        token = self.call("POST", "/api/v1/auth/test-token", {"email": email})["jwt"]
        return API(self.url, token)


def seed(url, org_slug):
    anonymous = API(url)
    owner = anonymous.persona("demo@g5labs.ai")
    member = anonymous.persona("member@g5labs.ai")
    org = next(o for o in owner.call("GET", "/api/v1/orgs") if o["orgSlug"] == org_slug)
    org_path = f"/api/v1/orgs/{org['orgId']}"
    owner.call("PUT", f"{org_path}/config", {"settings": {"external_issues_enabled": True}})
    domains = owner.call("GET", f"{org_path}/issue-domains")["domains"]
    domain = next((d for d in domains if d["provider"] == "g5"), None)
    if domain is None:
        domain = owner.call("POST", f"{org_path}/issue-domains", {"provider": "g5", "key": "DEMO"})
    if not domain["isDefaultCreateTarget"]:
        raise RuntimeError("The preview's G5 issue domain must be its default creation target")
    workspaces = owner.call("GET", f"{org_path}/workspaces")
    base = next(w for w in workspaces if w["isBase"])

    for scenario in SCENARIOS:
        name = scenario["name"]
        workspace = next((w for w in workspaces if w["name"] == name), None)
        if workspace is None:
            workspace = owner.call("POST", f"/api/v1/workspaces/{base['id']}/clone", {
                "name": name,
                "description": scenario["title"],
                "source": "durable",
                "idempotencyKey": f"preview:{name}",
            })["workspace"]
            owner.call("PATCH", f"/api/v1/workspaces/{workspace['id']}", {"phase": scenario["phase"]})
        workspace_path = f"/api/v1/workspaces/{workspace['id']}"
        issue_input = {
            "domainId": domain["id"],
            "title": scenario["title"],
            "body": scenario["body"],
            "assignees": ["demo@g5labs.ai"],
            "idempotencyKey": f"preview:{name}:issue",
        }
        created = owner.call("POST", f"{org_path}/issues", {
            **issue_input, "checkOnly": True,
        })["issue"]
        if created is None:
            created = owner.call("POST", f"{org_path}/issues", issue_input)["issue"]
        issue_path = f"{org_path}/issues/g5/{created['number']}"
        detail = owner.call("GET", issue_path)
        if detail["body"] == scenario["body"] + LEGACY_ISSUE_NOTES[name]:
            owner.call("PATCH", issue_path, {
                "body": scenario["body"],
                "expectedUpdatedAt": detail["updatedAt"],
            })
        links = owner.call("GET", f"{workspace_path}/issue-links")["links"]
        if not any(link["externalIssueId"] == created["externalIssueId"] for link in links):
            owner.call("POST", f"{workspace_path}/issue-links", {"ref": created["displayKey"], "role": "primary"})
        for index, (author, body) in enumerate(scenario["comments"]):
            {"owner": owner, "member": member}[author].call(
                "POST", f"{org_path}/issues/g5/{created['number']}/comments", {
                    "body": body,
                    "idempotencyKey": f"preview:{name}:comment:{index}",
                },
            )
        if "pr" in scenario:
            prs = owner.call("GET", f"{workspace_path}/pull-requests")
            if not prs:
                owner.call("POST", f"{workspace_path}/pull-requests", {
                    "targetWorkspaceId": base["id"],
                    "title": scenario["title"],
                    "description": scenario["pr"],
                })
            else:
                for pr in prs:
                    legacy = LEGACY_PR_HEADING + scenario["pr"] + LEGACY_PR_NOTE
                    if pr["status"] == "open" and pr.get("descriptionOverride") == legacy:
                        owner.call("POST", f"{workspace_path}/pull-requests", {
                            "mode": "update",
                            "targetWorkspaceId": pr["targetWorkspaceId"],
                            "description": scenario["pr"],
                        })
        activity = owner.call("GET", f"{workspace_path}/activity")
        if activity["issueError"] is not None or activity["primaryIssue"]["displayKey"] != created["displayKey"]:
            raise RuntimeError(f"{name}: primary issue activity could not be read")
        print(f"  ✓ {name}: {created['displayKey']}, "
              f"{len(activity['entries'])} activity entries — "
              f"/{org_slug}/{workspace['name']}", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--api-url", default=f"http://localhost:{os.environ.get('G5_PORT_API', '7500')}")
    parser.add_argument("--org", default="g5labs")
    args = parser.parse_args()
    seed(args.api_url, args.org)
