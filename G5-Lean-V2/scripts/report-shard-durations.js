#!/usr/bin/env node
// Shard timing evidence (ontology g5.quality.delivery.ci.timing-evidence and
// g5.quality.delivery.ci.browser-shards): per-file first-attempt and retry
// time, slowest first, beside the shard planner's prediction, plus per-file
// totals of the named phases tests record in their `g5-timings` annotation
// (frontend/e2e/timings.ts). Shared by both playwright-e2e shard steps in
// infra/ci/cloudbuild-ci-e2e.yaml so the aggregation cannot drift.
//
// Usage: node report-shard-durations.js "<shard label>" [reportPath]
//          [--plan plan.json --shard N] [--summary out.json]
// Reads test-results/report.json relative to the current working directory
// (each shard's WORKDIR) unless an explicit path is given. --summary writes
// the same data as JSON ("kind": "g5-shard-durations"), which
// `scripts/plan_playwright_shards.py durations` accepts to refresh the
// committed duration table. Exits 0 when the report is missing or unreadable
// so a failed Playwright run never masks its own failure with a reporting
// error.
'use strict';

const fs = require('fs');
const path = require('path');

const TIMINGS_ANNOTATION = 'g5-timings';

function parseArgs(argv) {
  const options = { positional: [] };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--plan' || arg === '--shard' || arg === '--summary') {
      options[arg.slice(2)] = argv[index + 1];
      index += 1;
    } else {
      options.positional.push(arg);
    }
  }
  return options;
}

function readJson(file) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (error) {
    console.log(`[report-shard-durations] cannot read ${file}: ${error.message}; skipping`);
    return undefined;
  }
}

function phaseTimings(result) {
  // The test's own annotation is the last one; Playwright copies annotations
  // added in a beforeAll hook onto every later test of its suite.
  const annotation = (result.annotations || [])
    .filter((entry) => entry.type === TIMINGS_ANNOTATION)
    .at(-1);
  if (!annotation) return {};
  try {
    const parsed = JSON.parse(annotation.description || '{}');
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

function summarize(report, plan, shard) {
  const files = {};
  const entry = (file) => {
    files[file] ??= { firstAttemptMs: 0, retryMs: 0, attempts: 0, tests: 0, phases: {} };
    return files[file];
  };
  const walk = (suite) => {
    (suite.suites || []).forEach(walk);
    (suite.specs || []).forEach((spec) => {
      // Chromium only: the setup project's auth.setup.ts is a dependency of
      // every shard, not a placed file.
      (spec.tests || []).filter((test) => test.projectName === 'chromium').forEach((test) => {
        const results = test.results || [];
        if (results.length === 0) return;
        const file = entry(path.basename(spec.file));
        file.tests += 1;
        results.forEach((result, attempt) => {
          file.attempts += 1;
          if (attempt === 0) {
            file.firstAttemptMs += result.duration;
            for (const [phase, timing] of Object.entries(phaseTimings(result))) {
              const total = (file.phases[phase] ??= { ms: 0, count: 0 });
              total.ms += Number(timing.ms) || 0;
              total.count += Number(timing.count) || 0;
            }
          } else {
            file.retryMs += result.duration;
          }
        });
      });
    });
  };
  (report.suites || []).forEach(walk);

  const summary = { kind: 'g5-shard-durations', files };
  if (plan && Number.isInteger(shard)) {
    const weights = plan.fileWeightsMs || {};
    for (const name of plan.plannedFiles?.[shard - 1] || []) {
      if (files[name]) files[name].predictedMs = weights[name];
    }
    summary.predictedShardMs = plan.predictedShardMs?.[shard - 1];
  }
  summary.totals = Object.values(files).reduce(
    (totals, file) => ({
      firstAttemptMs: totals.firstAttemptMs + file.firstAttemptMs,
      retryMs: totals.retryMs + file.retryMs,
    }),
    { firstAttemptMs: 0, retryMs: 0 },
  );
  return summary;
}

function print(label, summary) {
  console.log(`=== ${label} per-file durations (ms), slowest first ===`);
  console.log('   total  file  first retries predicted');
  const rows = Object.entries(summary.files).sort(
    (a, b) => b[1].firstAttemptMs + b[1].retryMs - (a[1].firstAttemptMs + a[1].retryMs),
  );
  for (const [file, data] of rows) {
    const total = data.firstAttemptMs + data.retryMs;
    const predicted = data.predictedMs === undefined ? '-' : String(data.predictedMs);
    console.log(
      `${String(Math.round(total)).padStart(8)}  ${file}  ${Math.round(data.firstAttemptMs)} ${Math.round(data.retryMs)} ${predicted}`,
    );
  }
  console.log(
    `=== ${label} totals: first attempts ${Math.round(summary.totals.firstAttemptMs)} ms, ` +
      `retries ${Math.round(summary.totals.retryMs)} ms` +
      (summary.predictedShardMs === undefined
        ? ''
        : `, planner predicted ${summary.predictedShardMs} ms elapsed`) +
      ' ===',
  );
  const phased = rows.filter(([, data]) => Object.keys(data.phases).length > 0);
  if (phased.length > 0) {
    console.log(`=== ${label} recorded phases per file (first attempts, ms) ===`);
    for (const [file, data] of phased) {
      const phases = Object.entries(data.phases)
        .sort((a, b) => b[1].ms - a[1].ms)
        .map(([phase, timing]) => `${phase}=${Math.round(timing.ms)}/${timing.count}`)
        .join(' ');
      console.log(`  ${file}: ${phases}`);
    }
  }
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const label = options.positional[0] || 'shard';
  const reportPath = options.positional[1] || 'test-results/report.json';
  if (!fs.existsSync(reportPath)) {
    console.log(`[report-shard-durations] no report at ${reportPath}; skipping`);
    return;
  }
  const report = readJson(reportPath);
  if (!report) return;
  const plan = options.plan ? readJson(options.plan) : undefined;
  const shard = options.shard === undefined ? undefined : Number.parseInt(options.shard, 10);
  const summary = summarize(report, plan, shard);
  print(label, summary);
  // One machine-readable line, so the committed duration table can be
  // refreshed from build logs alone (plan_playwright_shards.py durations).
  const compact = Object.fromEntries(
    Object.entries(summary.files).map(([file, data]) => [
      file,
      { firstAttemptMs: Math.round(data.firstAttemptMs), retryMs: Math.round(data.retryMs) },
    ]),
  );
  console.log(`G5_SHARD_DURATIONS ${JSON.stringify({ kind: 'g5-shard-durations', label, files: compact })}`);
  if (options.summary) {
    fs.writeFileSync(options.summary, `${JSON.stringify({ label, ...summary }, null, 2)}\n`);
  }
}

if (require.main === module) {
  main();
}

module.exports = { summarize };
