#!/usr/bin/env python3
"""Verify passive Claude usage with the pinned CLI and a loopback model.

Uses a temporary home and synthetic credentials. The fixture's loopback proxy
refuses external traffic; no real account or subscription allowance is used.
"""

import argparse
import http.server
import json
from pathlib import Path
import runpy
import subprocess
import tempfile
import threading
import time


REPO = Path(__file__).resolve().parents[1]
LIFECYCLE = runpy.run_path(
    str(REPO / "scripts/test-native-claude-lifecycle.py"), run_name="fixture"
)
FIXTURE = LIFECYCLE["FIXTURE"]


class Provider(LIFECYCLE["Provider"]):
    def end_headers(self):
        now = int(time.time())
        for name, value in {
            "anthropic-ratelimit-unified-status": "allowed",
            "anthropic-ratelimit-unified-representative-claim": "five_hour",
            "anthropic-ratelimit-unified-reset": str(now + 3600),
            "anthropic-ratelimit-unified-5h-utilization": "0.23",
            "anthropic-ratelimit-unified-5h-reset": str(now + 3600),
            "anthropic-ratelimit-unified-7d-utilization": "0.45",
            "anthropic-ratelimit-unified-7d-reset": str(now + 86400),
        }.items():
            self.send_header(name, value)
        super().end_headers()


def check_mode(cli, server, mode):
    with tempfile.TemporaryDirectory(prefix="g5-subscription-probe-") as home:
        env = FIXTURE["environment"](home, f"http://127.0.0.1:{server.server_port}")
        if mode == "oauth_token":
            env.pop("ANTHROPIC_API_KEY", None)
            env["CLAUDE_CODE_OAUTH_TOKEN"] = "sk-ant-oat01-synthetic-subscription-probe-token"
        identity = FIXTURE["native_identity"](home)
        status = subprocess.run(
            [cli, "auth", "status"], env=env, cwd=home, capture_output=True,
            text=True, timeout=15, check=True, **identity,
        )
        assert json.loads(status.stdout)["authMethod"] == mode
        result = subprocess.run(
            [cli, "-p", "Reply with a greeting.", "--output-format", "stream-json",
             "--verbose", "--tools", "", "--setting-sources", "",
             "--strict-mcp-config", "--mcp-config", '{"mcpServers":{}}',
             "--permission-prompts", "none", "--model", "claude-sonnet-4-6"],
            env=env, cwd=home, capture_output=True, text=True, timeout=45, **identity,
        )
        assert result.returncode == 0, result.stderr
        frames = [json.loads(line) for line in result.stdout.splitlines() if line.startswith("{")]
        rates = [f["rate_limit_info"] for f in frames if f.get("type") == "rate_limit_event"]
        if mode == "api_key":
            assert not rates, rates
        else:
            assert rates, "subscription response emitted no meter"
            assert rates[-1]["unifiedWindows"]["five_hour"]["utilization"] == 0.23, rates
            assert rates[-1]["unifiedWindows"]["seven_day"]["utilization"] == 0.45, rates


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--claude", required=True)
    cli = parser.parse_args().claude
    expected = (REPO / "scripts/claude-code-cli-version").read_text().strip()
    assert subprocess.check_output([cli, "--version"], text=True, timeout=15).split()[0] == expected
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    server.errors = []
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        for mode in ["api_key", "oauth_token"]:
            check_mode(cli, server, mode)
            print(f"PASS Claude {expected}: {mode} passive usage", flush=True)
        assert not server.errors, server.errors
    finally:
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
