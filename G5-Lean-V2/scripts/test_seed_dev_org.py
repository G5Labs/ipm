#!/usr/bin/env python3
"""Exercise seed-dev-org.sh persona seeding with the API, stub and DB stubbed."""

from __future__ import annotations

import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/seed-dev-org.sh"
API = "http://api.test"
STUB = "http://stub.test"
FIXTURE_ORGS = ("g5labs", "g5labs-member")
E2E_OPERATOR = "e2e-operator@g5labs.ai"
ROSTER = {
    "users": [
        {"id": 100001, "login": "alex-admin", "email": "demo@g5labs.ai",
         "name": "Alex", "avatar_url": "https://avatar.test/alex", "role": "admin"},
        {"id": 150001, "login": "jordan-member", "email": "member@g5labs.ai",
         "name": "Jordan", "avatar_url": "https://avatar.test/jordan", "role": "member"},
        {"id": 150002, "login": "vera-viewer", "email": "viewer@g5labs.ai",
         "name": "Vera", "avatar_url": "https://avatar.test/vera", "role": "viewer"},
    ]
}
# One fake for curl and docker. State mirrors the API semantics the seed relies
# on: test-token creates a missing fixture membership as Admin and keeps an
# existing one, and for the fixture operator persona also creates the operator
# org with it as Admin; the member API adds an existing person immediately or
# returns their current membership; /users/me reports the stored profile and
# roles.
CLIENT = r'''
import json
import os
from pathlib import Path
import sys
from urllib.parse import urlsplit

name = Path(sys.argv[0]).name
args = sys.argv[1:]
state_path = Path(os.environ["SEED_TEST_STATE"])
state = json.loads(state_path.read_text())

def save():
    state_path.write_text(json.dumps(state))

def fail():
    sys.exit(22)

def out(data):
    print(json.dumps(data))

def issue_token(email, org):
    tokens = state.setdefault("tokens", {})
    token = f"jwt:{len(tokens)}"
    tokens[token] = {"email": email, "role": state["users"][email]["roles"][org]}
    save()
    out({"data": {"jwt": token, "refreshToken": f"refresh:{email}"}})

if name == "docker":
    if args[:1] == ["inspect"]:
        sys.exit(1)
    # Persona membership changes must go through the guarded API.
    sys.exit(1)

url = next(arg for arg in args if arg.startswith("http"))
method = args[args.index("-X") + 1] if "-X" in args else ("POST" if "-d" in args else "GET")
body = json.loads(args[args.index("-d") + 1]) if "-d" in args else None
auth = next((arg for arg in args if arg.startswith("Authorization: Bearer ")), "")
token = state.get("tokens", {}).get(auth.removeprefix("Authorization: Bearer "), {})
caller = token.get("email")
path = urlsplit(url).path

if path == "/healthz":
    if os.environ.get("SEED_TEST_API_DOWN") and urlsplit(url).netloc != "stub.test":
        sys.exit(7)
    out({"status": "ok"})
elif path == "/__e2e/users":
    roster = os.environ["SEED_TEST_ROSTER"]
    if roster == "FAIL":
        fail()
    print(roster)
elif path == "/api/v1/auth/test-token":
    email = body["email"].lower()
    if os.environ.get("SEED_TEST_FAIL_TOKEN") == email:
        fail()
    if os.environ.get("SEED_TEST_NO_JWT") == email:
        out({"data": {}})
        sys.exit(0)
    state["users"].setdefault(email, {
        "name": f"E2E Test ({email})",
        "avatar": None,
        "roles": {org: "admin" for org in ["g5labs", "g5labs-member"]},
    })
    if email == "e2e-operator@g5labs.ai":
        state["users"][email]["roles"].setdefault("operator", "admin")
    issue_token(email, "g5labs")
elif path == "/api/v1/auth/refresh":
    email = body["refreshToken"].removeprefix("refresh:")
    org = {"org-1": "g5labs", "org-2": "g5labs-member", "op-1": "operator"}[body["orgId"]]
    issue_token(email, org)
elif path == "/api/v1/orgs":
    orgs = [{"orgId": "org-1", "orgSlug": "g5labs"}]
    if "operator" in state["users"][caller]["roles"]:
        orgs.append({"orgId": "op-1", "orgSlug": "operator"})
    out({"data": orgs})
elif path == "/api/v1/orgs/op-1/members" and method == "POST":
    if os.environ.get("SEED_TEST_FAIL_MEMBER"):
        fail()
    if token.get("role") != "admin" or state["users"][caller]["roles"].get("operator") != "admin":
        fail()
    user = state["users"][body["email"].lower()]
    user["roles"].setdefault("operator", body["role"])
    save()
    out({"data": {"membership": {"userId": body["email"].lower(),
                                 "role": user["roles"]["operator"]}}})
elif path.startswith("/api/v1/orgs/op-1/members/") and method == "PATCH":
    if token.get("role") != "admin" or state["users"][caller]["roles"].get("operator") != "admin":
        fail()
    state["users"][path.rsplit("/", 1)[1]]["roles"]["operator"] = body["role"]
    save()
    out({"data": {}})
elif path.startswith(("/api/v1/orgs/org-1/members/", "/api/v1/orgs/org-2/members/")) and method == "PATCH":
    org = "g5labs" if "/org-1/" in path else "g5labs-member"
    user_id = path.rsplit("/", 1)[1]
    if os.environ.get("SEED_TEST_REVOKE_ACTOR") == caller:
        state["users"][caller]["roles"][org] = "member"
        save()
    if token.get("role") != "admin" or state["users"][caller]["roles"].get(org) != "admin":
        fail()
    if os.environ.get("SEED_TEST_FAIL_ROLE") == user_id:
        fail()
    role = body["role"]
    if role not in ("admin", "member", "viewer"):
        fail()
    admins = [u for u in state["users"].values() if u["roles"].get(org) == "admin"]
    if role != "admin" and state["users"][user_id]["roles"].get(org) == "admin" and len(admins) <= 1:
        fail()
    if not os.environ.get("SEED_TEST_ROLE_NOOP"):
        state["users"][user_id]["roles"][org] = role
    state.setdefault("role_updates", []).append({
        "actor": caller, "org": org, "user": user_id, "role": role,
    })
    save()
    out({"data": {}})
elif path == "/api/v1/orgs/org-1/workspaces":
    out({"data": [{"id": "ws-1", "name": "base"}]})
elif path == "/api/v1/orgs/org-1/repos":
    out({"data": [{"id": "repo-1", "slug": "docs"}]})
elif path == "/api/v1/workspaces/ws-1/branches":
    out({"data": [{"repoId": "repo-1", "materializationStatus": "materialized", "headCommit": "a" * 40}]})
elif path == "/api/v1/users/me" and method == "PATCH":
    if os.environ.get("SEED_TEST_FAIL_PROFILE"):
        fail()
    state["users"][caller]["name"] = body["displayName"]
    state["users"][caller]["avatar"] = body.get("avatarUrl")
    save()
    out({"data": {}})
elif path == "/api/v1/users/me":
    user = state["users"][caller]
    out({"data": {
        "id": caller,
        "email": caller,
        "displayName": user["name"],
        "orgs": [{"orgSlug": org, "orgId": {"g5labs":"org-1", "g5labs-member":"org-2", "operator":"op-1"}[org], "role": role} for org, role in user["roles"].items()],
    }})
else:
    fail()
'''


@unittest.skipUnless(shutil.which("jq"), "seed-dev-org.sh requires jq")
class SeedDevOrgPersonaTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        for name in ("curl", "docker"):
            client = self.root / name
            client.write_text(f"#!{sys.executable}\n{CLIENT}")
            client.chmod(0o755)
        self.state = self.root / "state.json"
        self.state.write_text(json.dumps({"users": {}}))

    def tearDown(self):
        self.directory.cleanup()

    def run_seed(self, roster=None, **faults):
        env = {
            "PATH": f"{self.root}:{Path(shutil.which('jq')).parent}:/usr/bin:/bin",
            "SEED_TEST_STATE": str(self.state),
            "SEED_TEST_ROSTER": roster if roster is not None else json.dumps(ROSTER),
            "OAUTH_STUB_URL": STUB,
            **{f"SEED_TEST_{fault.upper()}": value for fault, value in faults.items()},
        }
        return subprocess.run(
            ["/bin/bash", str(SCRIPT), API], env=env,
            capture_output=True, text=True, timeout=30,
        )

    def user(self, email):
        return json.loads(self.state.read_text())["users"][email]

    def assert_failed_without_workspace_id(self, result):
        self.assertNotEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("ws-1", result.stdout)

    def test_seeds_personas_and_prints_only_the_workspace_id(self):
        result = self.run_seed()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "ws-1\n")
        self.assertEqual(self.user("demo@g5labs.ai"), {
            "name": "Alex", "avatar": "https://avatar.test/alex",
            "roles": {**dict.fromkeys(FIXTURE_ORGS, "admin"), "operator": "admin"},
        })
        self.assertEqual(self.user("member@g5labs.ai"), {
            "name": "Jordan", "avatar": "https://avatar.test/jordan",
            "roles": dict.fromkeys(FIXTURE_ORGS, "member"),
        })
        self.assertEqual(self.user("viewer@g5labs.ai"), {
            "name": "Vera", "avatar": "https://avatar.test/vera",
            "roles": dict.fromkeys(FIXTURE_ORGS, "viewer"),
        })
        self.assertEqual(self.user(E2E_OPERATOR)["roles"]["operator"], "admin")
        self.assertIn("demo@g5labs.ai is a platform operator", result.stderr)

    def test_owner_roster_is_refused(self):
        roster = json.loads(json.dumps(ROSTER))
        roster["users"][0]["role"] = "owner"
        self.assert_failed_without_workspace_id(self.run_seed(roster=json.dumps(roster)))

    def test_rerun_uses_current_administrator_authority(self):
        self.assertEqual(self.run_seed().returncode, 0)
        state = json.loads(self.state.read_text())
        state["users"][E2E_OPERATOR]["roles"]["g5labs"] = "member"
        self.state.write_text(json.dumps(state))
        result = self.run_seed()
        self.assertEqual(result.returncode, 0, result.stderr)
        updates = json.loads(self.state.read_text())["role_updates"][len(state["role_updates"]):]
        self.assertTrue(updates)
        self.assertTrue(all(update["actor"] != E2E_OPERATOR for update in updates
                            if update["org"] == "g5labs"))

    def test_operator_role_in_roster_converges_and_can_be_retried(self):
        for role in ("member", "viewer"):
            with self.subTest(role=role):
                self.state.write_text(json.dumps({"users": {
                    "admin@g5labs.ai": {"name": "Old Admin", "avatar": None,
                                        "roles": dict.fromkeys(FIXTURE_ORGS, "member")},
                }}))
                roster = json.loads(json.dumps(ROSTER))
                roster["users"][0]["role"] = "member"
                roster["users"].insert(0, {
                    "id": 180001, "email": E2E_OPERATOR.upper(), "name": "Operator",
                    "avatar_url": "https://avatar.test/operator", "role": role,
                })
                roster["users"].append({
                    "id": 180002, "email": "admin@g5labs.ai", "name": "Admin",
                    "avatar_url": "https://avatar.test/admin", "role": "admin",
                })
                for _ in range(2):
                    result = self.run_seed(roster=json.dumps(roster))
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(result.stdout, "ws-1\n")
                    for org in FIXTURE_ORGS:
                        self.assertEqual(self.user(E2E_OPERATOR)["roles"][org], role)
                        self.assertEqual(self.user("admin@g5labs.ai")["roles"][org], "admin")
                updates = json.loads(self.state.read_text())["role_updates"]
                for org in FIXTURE_ORGS:
                    own = [update for update in updates if update["org"] == org]
                    self.assertEqual(own[0]["user"], "admin@g5labs.ai")
                    self.assertTrue(all(update["actor"] == "admin@g5labs.ai"
                                        for update in own if update["role"] != "admin"))

    def test_rerun_without_an_authorized_fixture_admin_refuses(self):
        self.assertEqual(self.run_seed().returncode, 0)
        state = json.loads(self.state.read_text())
        for user in state["users"].values():
            for org in FIXTURE_ORGS:
                user["roles"][org] = "member"
        self.state.write_text(json.dumps(state))
        result = self.run_seed()
        self.assert_failed_without_workspace_id(result)
        self.assertIn("No authorized fixture Admin", result.stderr)
        self.assertEqual(json.loads(self.state.read_text())["role_updates"], state["role_updates"])

    def test_a_roster_without_a_retained_fixture_admin_refuses_reductions(self):
        roster = json.loads(json.dumps(ROSTER))
        roster["users"][0]["role"] = "member"
        roster["users"].append({
            "id": 180001, "email": E2E_OPERATOR, "name": "Operator",
            "avatar_url": "https://avatar.test/operator", "role": "member",
        })
        result = self.run_seed(roster=json.dumps(roster))
        self.assert_failed_without_workspace_id(result)
        self.assertIn("No authorized fixture Admin", result.stderr)
        self.assertEqual(json.loads(self.state.read_text()).get("role_updates", []), [])

    def test_authority_lost_after_selection_is_refused_and_rerun_rechecks(self):
        result = self.run_seed(revoke_actor=E2E_OPERATOR)
        self.assert_failed_without_workspace_id(result)
        self.assertIn("Could not apply role", result.stderr)
        self.assertEqual(json.loads(self.state.read_text()).get("role_updates", []), [])
        retry = self.run_seed()
        self.assertEqual(retry.returncode, 0, retry.stderr)

    def test_alex_is_an_operator_administrator(self):
        result = self.run_seed()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.user("demo@g5labs.ai")["roles"]["operator"], "admin")
        for email in ("member@g5labs.ai", "viewer@g5labs.ai"):
            self.assertNotIn("operator", self.user(email)["roles"])

    def test_rerun_restores_a_changed_operator_role(self):
        self.assertEqual(self.run_seed().returncode, 0)
        state = json.loads(self.state.read_text())
        state["users"]["demo@g5labs.ai"]["roles"]["operator"] = "member"
        self.state.write_text(json.dumps(state))
        result = self.run_seed()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.user("demo@g5labs.ai")["roles"]["operator"], "admin")

    def test_operator_failures_fail_the_seed(self):
        for faults, message in (
            ({"fail_token": E2E_OPERATOR}, "operator org not seeded"),
            ({"no_jwt": E2E_OPERATOR}, "operator org not seeded"),
            ({"fail_member": "1"}, "Could not add demo@g5labs.ai"),
        ):
            with self.subTest(faults=faults):
                self.state.write_text(json.dumps({"users": {}}))
                result = self.run_seed(**faults)
                self.assert_failed_without_workspace_id(result)
                self.assertIn(message, result.stderr)

    def test_role_application_failure_fails_the_seed(self):
        result = self.run_seed(fail_role="member@g5labs.ai")
        self.assert_failed_without_workspace_id(result)
        self.assertIn("Could not apply role", result.stderr)

    def test_role_that_did_not_apply_fails_verification(self):
        result = self.run_seed(role_noop="1")
        self.assert_failed_without_workspace_id(result)
        self.assertIn("did not verify", result.stderr)

    def test_profile_failure_fails_the_seed(self):
        result = self.run_seed(fail_profile="1")
        self.assert_failed_without_workspace_id(result)
        self.assertIn("Could not set the profile", result.stderr)

    def test_unreachable_api_fails_the_seed(self):
        result = self.run_seed(api_down="1")
        self.assert_failed_without_workspace_id(result)
        self.assertIn("Cannot reach g5-api", result.stderr)

    def test_initial_org_token_failure_fails_the_seed(self):
        for fault in ("fail_token", "no_jwt"):
            with self.subTest(fault=fault):
                result = self.run_seed(**{fault: "demo@g5labs.ai"})
                self.assert_failed_without_workspace_id(result)
                self.assertIn("dev org not seeded", result.stderr)

    def test_unusable_roster_fails_the_seed(self):
        for roster in ("FAIL", "not json", '{"users": []}', "{}"):
            with self.subTest(roster=roster):
                self.assert_failed_without_workspace_id(self.run_seed(roster=roster))

    def test_retry_after_failure_converges_to_declared_roles(self):
        self.assert_failed_without_workspace_id(self.run_seed(fail_role="member@g5labs.ai"))
        self.assertEqual(self.user("member@g5labs.ai")["roles"], dict.fromkeys(FIXTURE_ORGS, "admin"))
        result = self.run_seed()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.user("member@g5labs.ai")["roles"], dict.fromkeys(FIXTURE_ORGS, "member"))
        self.assertEqual(self.user("viewer@g5labs.ai")["roles"], dict.fromkeys(FIXTURE_ORGS, "viewer"))
        rerun = self.run_seed()
        self.assertEqual(rerun.returncode, 0, rerun.stderr)
        self.assertEqual(self.user("member@g5labs.ai")["roles"], dict.fromkeys(FIXTURE_ORGS, "member"))
        self.assertEqual(self.user("viewer@g5labs.ai")["roles"], dict.fromkeys(FIXTURE_ORGS, "viewer"))


class DevPersonaIdentityContractTests(unittest.TestCase):
    """Dev personas share the dev stack with Playwright's OAuth identities."""

    def test_extra_personas_never_reuse_an_e2e_or_derived_stub_identity(self):
        compose = (ROOT / "infra/docker/docker-compose.dev.yml").read_text()
        extra = json.loads(re.search(r"OAUTH_EXTRA_USERS: '(.*)'", compose).group(1))
        e2e = (ROOT / "frontend/e2e/oauth.ts").read_text()
        e2e_ids = {int(value) for value in re.findall(r"_ID \?\? '(\d+)'", e2e)}
        e2e_emails = set(re.findall(r"_EMAIL \?\? '([^']+)'", e2e))
        # Ids the stub derives for emails typed at consent.
        stub = (ROOT / "infra/docker/oauth-stub/server.js").read_text()
        base, span = map(int, re.search(
            r"id: (\d+) \+ \(digest\.readUInt32BE\(0\) % (\d+)\)", stub).groups())
        self.assertTrue(extra and e2e_ids and e2e_emails)
        for persona in extra:
            with self.subTest(persona=persona["email"]):
                self.assertNotIn(persona["id"], e2e_ids)
                self.assertNotIn(persona["email"], e2e_emails)
                self.assertNotIn(persona["id"], range(base, base + span))


if __name__ == "__main__":
    unittest.main()
