#!/usr/bin/env python3
"""Wait for the selected Compose project's services, using labels as identity."""

import json
import subprocess
import sys
import time


def capture(*command):
    return subprocess.check_output(command, text=True, stderr=subprocess.PIPE, timeout=30).strip()


def diagnostics(config, containers):
    """Log only failed containers from the last project-scoped inspection."""
    errors = problems(config, containers)
    for container in containers:
        service = container["Config"]["Labels"].get("com.docker.compose.service")
        if any(error.startswith(f"{service}:") for error in errors):
            print(f"--- {container['Id']}: last 50 log lines ---", file=sys.stderr)
            try:
                subprocess.run(["docker", "logs", "--tail", "50", container["Id"]], timeout=10, check=False)
            except (OSError, subprocess.SubprocessError) as error:
                print(f"Logs unavailable: {error}", file=sys.stderr)


def problems(config, containers):
    completed = {name for service in config["services"].values()
                 for name, dependency in service.get("depends_on", {}).items()
                 if dependency.get("condition") == "service_completed_successfully"}
    required = {"postgres", "g5-api"} & config["services"].keys()
    seen = set()
    errors = []
    for container in containers:
        service = container["Config"]["Labels"].get("com.docker.compose.service")
        seen.add(service)
        state = container["State"]
        if service in completed and state.get("Status") == "exited" and state.get("ExitCode") == 0:
            continue
        if (not state.get("Running") or state.get("Paused") or state.get("Restarting")
                or state.get("Health", {}).get("Status", "healthy") != "healthy"):
            errors.append(f"{service}: {state.get('Status')}, health={state.get('Health', {}).get('Status', 'none')}")
    errors += [f"{service}: missing" for service in sorted(required - seen)]
    return errors or ([] if containers else ["no project containers exist"])


def main():
    compose, timeout, project = sys.argv[1], float(sys.argv[2]), sys.argv[3]
    config = json.loads(capture("docker", "compose", "-p", project, "-f", compose, "config", "--format", "json"))
    deadline = time.monotonic() + timeout
    containers = []
    while True:
        try:
            ids = capture("docker", "ps", "-aq", "--filter", f"label=com.docker.compose.project={project}").split()
            containers = json.loads(capture("docker", "inspect", *ids)) if ids else []
            errors = problems(config, containers)
        except (subprocess.SubprocessError, OSError, ValueError) as error:
            errors = [f"container inspection changed or is unavailable; retrying ({error})"]
        if not errors:
            print(f"Project {project}: services healthy.")
            return 0
        if time.monotonic() >= deadline:
            print(f"Project {project} is not ready: " + "; ".join(errors), file=sys.stderr)
            diagnostics(config, containers)
            return 1
        time.sleep(min(2, max(0, deadline - time.monotonic())))


if __name__ == "__main__":
    sys.exit(main())
