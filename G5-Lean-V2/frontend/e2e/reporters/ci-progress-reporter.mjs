import { createHash } from "node:crypto";
import path from "node:path";

const DEFAULT_HEARTBEAT_MS = 30_000;
// Beyond the project timeout that bounds each post-timeout hook/teardown slot.
const DEFAULT_BACKSTOP_GRACE_MS = 30_000;

function positiveInteger(value, fallback) {
  const parsed = Number.parseInt(value ?? "", 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

export function stableTestId(test) {
  const location = test.location ?? {};
  const file = location.file
    ? path.relative(process.cwd(), location.file).replaceAll(path.sep, "/")
    : "unknown";
  const identity = `${file}:${location.line ?? 0}:${test.titlePath().join(" > ")}`;
  return createHash("sha256").update(identity).digest("hex").slice(0, 12);
}

// Phase timings a test recorded through e2e/timings.ts (ontology
// g5.quality.delivery.ci.timing-evidence); absent when it recorded none.
export function recordedTimings(result) {
  // The test's own annotation is the last: Playwright copies annotations
  // added in a beforeAll hook onto every later test of its suite.
  const annotation = (result.annotations ?? [])
    .filter((entry) => entry.type === "g5-timings")
    .at(-1);
  if (!annotation?.description) return undefined;
  try {
    return JSON.parse(annotation.description);
  } catch {
    return undefined;
  }
}

function emit(event, fields) {
  process.stdout.write(`G5_${event} ${JSON.stringify(fields)}\n`);
}

export default class CiProgressReporter {
  constructor(options = {}) {
    this.heartbeatMs = positiveInteger(
      options.heartbeatMs ?? process.env.G5_PLAYWRIGHT_HEARTBEAT_MS,
      DEFAULT_HEARTBEAT_MS,
    );
    this.graceMs = positiveInteger(
      options.graceMs ?? process.env.G5_PLAYWRIGHT_BACKSTOP_GRACE_MS,
      DEFAULT_BACKSTOP_GRACE_MS,
    );
    this.active = new Map();
    // Declared budget per test, fixed at its first attempt: the runner
    // overwrites test.timeout with each attempt's effective value.
    this.budgets = new Map();
    this.undeclared = [];
    this.heartbeat = undefined;
  }

  // The budget Playwright enforces, as declared at load time
  // (config/project timeout, test.describe.configure({ timeout }), static
  // test.slow()). An in-body test.setTimeout is invisible here, so a raise
  // made that way is reported as undeclared at the test's end.
  declaredBudget(test) {
    const known = this.budgets.get(test.id);
    if (known !== undefined) return known;
    // Playwright triples the timeout once per inherited slow annotation.
    const slow = test.annotations.filter((annotation) => annotation.type === "slow");
    const budget = test.timeout * 3 ** slow.length;
    this.budgets.set(test.id, budget);
    return budget;
  }

  onBegin(_config, suite) {
    emit("TEST_RUN_START", {
      tests: suite.allTests().length,
      heartbeatMs: this.heartbeatMs,
      backstopGraceMs: this.graceMs,
    });
    this.heartbeat = setInterval(() => this.emitHeartbeat(), this.heartbeatMs);
    this.heartbeat.unref();
  }

  onTestBegin(test, result) {
    const id = stableTestId(test);
    const startedAt = Date.now();
    const key = `${id}:${result.workerIndex}:${result.retry}`;
    const budgetMs = this.declaredBudget(test);
    const projectTimeoutMs = test.parent.project()?.timeout ?? 0;
    const entry = {
      id,
      key,
      title: test.title,
      file: path.relative(process.cwd(), test.location.file).replaceAll(path.sep, "/"),
      line: test.location.line,
      worker: result.workerIndex,
      retry: result.retry,
      startedAt,
      budgetMs,
      windowMs: Math.max(projectTimeoutMs, budgetMs) + projectTimeoutMs,
      deadline: 0,
      timer: undefined,
    };
    this.extendDeadline(entry, startedAt + entry.windowMs);

    this.active.set(key, entry);
    emit("TEST_START", {
      id,
      title: test.title,
      file: entry.file,
      line: entry.line,
      worker: entry.worker,
      retry: entry.retry,
      budgetMs,
      timeoutMs: entry.deadline - startedAt,
    });
  }

  // Every reported step begin/end proves the test is still advancing. Each
  // Playwright time slot — the main slot (beforeEach, fixtures, body), every
  // beforeAll/afterAll hook, After Hooks, Worker Cleanup, tracing, a worker
  // fixture — is at most max(project timeout, budget) and starts at a step
  // event, except unreported (boxed) built-in fixtures, whose single project-
  // timeout slot the window also covers. So a test silent for longer than the
  // window plus grace has outlived every slot it could be in: a hang
  // Playwright's own timeouts cannot end. The bound assumes the built-in
  // fixtures these specs use; a custom fixture with an explicit timeout above
  // the window, or several slow boxed fixtures in a row, could exceed it.
  onStepBegin(test, result) {
    this.touch(test, result);
  }

  onStepEnd(test, result) {
    this.touch(test, result);
  }

  touch(test, result) {
    const entry = this.active.get(
      `${stableTestId(test)}:${result.workerIndex}:${result.retry}`,
    );
    if (entry) this.extendDeadline(entry, Date.now() + entry.windowMs);
  }

  extendDeadline(entry, slotEnd) {
    const deadline = slotEnd + this.graceMs;
    if (deadline <= entry.deadline) return;
    entry.deadline = deadline;
    if (entry.timer) clearTimeout(entry.timer);
    entry.timer = setTimeout(() => {
      emit("TEST_TIMEOUT", {
        id: entry.id,
        title: entry.title,
        worker: entry.worker,
        retry: entry.retry,
        elapsedMs: Date.now() - entry.startedAt,
        budgetMs: entry.budgetMs,
        timeoutMs: entry.deadline - entry.startedAt,
      });
      process.kill(process.pid, "SIGINT");
    }, deadline - Date.now());
    entry.timer.unref();
  }

  onTestEnd(test, result) {
    const id = stableTestId(test);
    const key = `${id}:${result.workerIndex}:${result.retry}`;
    const entry = this.active.get(key);
    if (entry?.timer) clearTimeout(entry.timer);
    this.active.delete(key);
    // The runner now reports the attempt's effective timeout. A raise past the
    // declared budget came from an in-body setTimeout/slow the backstop could
    // not see; fail the run so the budget moves to declaration time.
    const declared = this.budgets.get(test.id);
    if (declared !== undefined && test.timeout > declared) {
      const undeclared = {
        id,
        title: test.title,
        file: path.relative(process.cwd(), test.location.file).replaceAll(path.sep, "/"),
        line: test.location.line,
        budgetMs: declared,
        effectiveMs: test.timeout,
      };
      this.undeclared.push(undeclared);
      emit("TEST_BUDGET_UNDECLARED", undeclared);
    }
    emit("TEST_END", {
      id,
      title: test.title,
      worker: result.workerIndex,
      retry: result.retry,
      status: result.status,
      durationMs: result.duration,
      timings: recordedTimings(result),
      errors:
        result.status === "failed" || result.status === "timedOut"
          ? result.errors.map((error) => error.message)
          : undefined,
    });
  }

  onEnd(result) {
    if (this.heartbeat) clearInterval(this.heartbeat);
    this.emitHeartbeat();
    const status = this.undeclared.length > 0 ? "failed" : result.status;
    if (this.undeclared.length > 0) {
      process.stdout.write(
        `::error title=Undeclared test timeout::${this.undeclared.length} test(s) raised their timeout inside the test or a hook; declare it with test.describe.configure({ timeout }) so the CI backstop can see it: ${this.undeclared
          .map((test) => `${test.file}:${test.line}`)
          .join(", ")}\n`,
      );
    }
    emit("TEST_RUN_END", { status });
    return status === result.status ? undefined : { status };
  }

  printsToStdio() {
    return true;
  }

  emitHeartbeat() {
    const now = Date.now();
    emit("TEST_HEARTBEAT", {
      active: [...this.active.values()].map((entry) => ({
        id: entry.id,
        title: entry.title,
        worker: entry.worker,
        retry: entry.retry,
        elapsedMs: now - entry.startedAt,
      })),
    });
  }
}
