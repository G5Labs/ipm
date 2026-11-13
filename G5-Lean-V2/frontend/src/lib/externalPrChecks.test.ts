import { describe, expect, it } from "vitest";

import {
  aggregateVerdict,
  checksRefetchInterval,
  hasExternalRepos,
  hasPendingChecks,
  parseChecksSnapshot,
} from "./externalPrChecks";

function snapshot(repos: unknown[]): unknown {
  return { source: "github", repositories: repos };
}

/** A bucket name, or `[bucket, false]` for a check the base branch does not require. */
type Row = string | [string, boolean];

function repo(availability: string, rows: Row[]): unknown {
  return {
    repository: "acme/api",
    number: 7,
    availability,
    baseRefName: "main",
    checks: rows.map((row) => {
      const [bucket, required] = typeof row === "string" ? [row, true] : row;
      return { kind: "check_run", name: "ci", bucket, required };
    }),
  };
}

describe("parseChecksSnapshot", () => {
  it("returns null for non-object or missing repositories", () => {
    expect(parseChecksSnapshot(null)).toBeNull();
    expect(parseChecksSnapshot({ source: "github" })).toBeNull();
    expect(parseChecksSnapshot("nope")).toBeNull();
  });

  it("parses repositories and checks, coercing missing fields", () => {
    const parsed = parseChecksSnapshot(snapshot([repo("ready", ["pass"])]));
    expect(parsed?.repositories).toHaveLength(1);
    expect(parsed?.repositories[0].checks[0]).toMatchObject({
      name: "ci",
      bucket: "pass",
      required: true,
    });
    expect(parsed?.repositories[0].baseRefName).toBe("main");
    const loose = parseChecksSnapshot(
      snapshot([{ availability: "ready", checks: [{ name: "x" }] }]),
    );
    expect(loose?.repositories[0].checks[0].required).toBe(false);
    expect(loose?.repositories[0].baseRefName).toBeNull();
  });
});

describe("aggregateVerdict", () => {
  it("passing: ready repo with no checks, or all passing/skipping", () => {
    expect(aggregateVerdict(parseChecksSnapshot(snapshot([])))).toBe("passing");
    expect(
      aggregateVerdict(parseChecksSnapshot(snapshot([repo("ready", [])]))),
    ).toBe("passing");
    expect(
      aggregateVerdict(
        parseChecksSnapshot(snapshot([repo("ready", ["pass", "skipping"])])),
      ),
    ).toBe("passing");
  });

  it("failing: any fail/cancel wins over pending, required or not", () => {
    expect(
      aggregateVerdict(
        parseChecksSnapshot(
          snapshot([repo("ready", ["pass", ["fail", false]])]),
        ),
      ),
    ).toBe("failing");
    expect(
      aggregateVerdict(
        parseChecksSnapshot(snapshot([repo("ready", ["pending", "fail"])])),
      ),
    ).toBe("failing");
    expect(
      aggregateVerdict(
        parseChecksSnapshot(snapshot([repo("ready", ["cancel"])])),
      ),
    ).toBe("failing");
  });

  it("a pending check the base branch does not require is advisory", () => {
    expect(
      aggregateVerdict(
        parseChecksSnapshot(
          snapshot([repo("ready", ["pass", ["pending", false]])]),
        ),
      ),
    ).toBe("passing");
  });

  it("pending: a required pending check or a not-ready repo blocks", () => {
    expect(
      aggregateVerdict(
        parseChecksSnapshot(snapshot([repo("ready", ["pass", "pending"])])),
      ),
    ).toBe("pending");
    // Fresh PR, head not yet readable: must not read as passing.
    expect(
      aggregateVerdict(parseChecksSnapshot(snapshot([repo("pending", [])]))),
    ).toBe("pending");
    expect(
      aggregateVerdict(
        parseChecksSnapshot(snapshot([repo("unavailable", [])])),
      ),
    ).toBe("pending");
  });

  it("worst-wins across repositories", () => {
    expect(
      aggregateVerdict(
        parseChecksSnapshot(
          snapshot([repo("ready", ["pass"]), repo("ready", ["fail"])]),
        ),
      ),
    ).toBe("failing");
    expect(
      aggregateVerdict(
        parseChecksSnapshot(
          snapshot([repo("ready", ["pass"]), repo("pending", [])]),
        ),
      ),
    ).toBe("pending");
  });

  it("a null snapshot is pending, never a pass", () => {
    expect(aggregateVerdict(null)).toBe("pending");
  });
});

describe("hasPendingChecks", () => {
  it("is true while any reported check is pending, required or not", () => {
    expect(hasPendingChecks(null)).toBe(false);
    expect(
      hasPendingChecks(
        parseChecksSnapshot(snapshot([repo("ready", ["pass"])])),
      ),
    ).toBe(false);
    expect(
      hasPendingChecks(
        parseChecksSnapshot(
          snapshot([repo("ready", ["pass", ["pending", false]])]),
        ),
      ),
    ).toBe(true);
  });
});

describe("hasExternalRepos", () => {
  it("is true only when a provider repo is present", () => {
    expect(hasExternalRepos(null)).toBe(false);
    expect(hasExternalRepos(parseChecksSnapshot(snapshot([])))).toBe(false);
    expect(
      hasExternalRepos(parseChecksSnapshot(snapshot([repo("ready", [])]))),
    ).toBe(true);
  });
});

describe("parseChecksSnapshot rate limits", () => {
  it("reads rateLimited and retryAt per repository", () => {
    const parsed = parseChecksSnapshot(
      snapshot([
        {
          ...(repo("unavailable", []) as object),
          rateLimited: true,
          retryAt: "2026-10-31T12:05:00Z",
        },
        repo("ready", ["pass"]),
      ]),
    );
    expect(parsed?.repositories[0]).toMatchObject({
      rateLimited: true,
      retryAt: "2026-10-31T12:05:00Z",
    });
    expect(parsed?.repositories[1]).toMatchObject({
      rateLimited: false,
      retryAt: null,
    });
  });
});

describe("checksRefetchInterval", () => {
  const now = Date.parse("2026-10-31T12:00:00Z");
  const limited = (retryAt: string | null): unknown => ({
    ...(repo("unavailable", []) as object),
    rateLimited: true,
    retryAt,
  });
  const interval = (repos: unknown[]) =>
    checksRefetchInterval(parseChecksSnapshot(snapshot(repos)), now);

  it("waits until the latest reported reset, clamped to one to five minutes", () => {
    expect(
      interval([
        limited("2026-10-31T12:01:30Z"),
        limited("2026-10-31T12:02:00Z"),
      ]),
    ).toBe(120_000);
    expect(interval([limited("2026-10-31T12:00:10Z")])).toBe(60_000);
    expect(interval([limited("2026-10-31T11:59:00Z")])).toBe(60_000);
    expect(interval([limited("2026-10-31T13:00:00Z")])).toBe(300_000);
    expect(interval([limited(null)])).toBe(60_000);
    expect(interval([limited("not a time")])).toBe(60_000);
    // A rate limit wins over pending checks elsewhere.
    expect(
      interval([limited("2026-10-31T12:03:00Z"), repo("ready", ["pending"])]),
    ).toBe(180_000);
  });

  it("waits a minute while unavailable, polls pending checks every 30 s and stops when settled", () => {
    expect(
      interval([repo("unavailable", []), repo("ready", ["pending"])]),
    ).toBe(60_000);
    expect(interval([repo("ready", ["pending"])])).toBe(30_000);
    expect(interval([repo("ready", ["pass", ["pending", false]])])).toBe(
      30_000,
    );
    expect(interval([repo("pending", [])])).toBe(30_000);
    expect(interval([repo("ready", ["pass"])])).toBe(false);
    expect(interval([repo("ready", ["fail"])])).toBe(false);
    expect(checksRefetchInterval(null, now)).toBe(false);
    expect(checksRefetchInterval(undefined, now)).toBe(false);
  });
});

describe("local repository pull requests", () => {
  // g5.git.pr-no-github: a member not on GitHub is reported beside GitHub's,
  // ready with no checks, and never counts as an external repository.
  const local = {
    repository: "docs",
    tracker: "g5",
    number: null,
    g5Number: "PR000012",
    availability: "ready",
    noExternalChecks: true,
    message: "No external checks apply: this repository is not on GitHub.",
    checks: [],
  };

  it("parses the marker and leaves the verdict to GitHub's repositories", () => {
    const mixed = parseChecksSnapshot(
      snapshot([repo("ready", ["pass"]), local]),
    );
    expect(mixed?.repositories[1]).toMatchObject({
      noExternalChecks: true,
      tracker: "g5",
      number: null,
      g5Number: "PR000012",
    });
    expect(aggregateVerdict(mixed)).toBe("passing");
    expect(hasExternalRepos(mixed)).toBe(true);
  });

  it("is not an external repository on its own", () => {
    const onlyLocal = parseChecksSnapshot({
      source: "g5",
      repositories: [local],
      noExternalChecks: true,
    });
    expect(hasExternalRepos(onlyLocal)).toBe(false);
    expect(aggregateVerdict(onlyLocal)).toBe("passing");
  });
});
