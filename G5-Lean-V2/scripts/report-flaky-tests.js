#!/usr/bin/env node
// Flaky-run detection (issue #1824): report Playwright specs that failed at
// least once and then passed on retry, and drop a marker so teardown knows to
// capture container logs.
//
// A retry-pass leaves the lane's step result at `passed`, so `stop-e2e-stack`
// never dumped the API/worker/host logs for it. The Playwright trace alone
// shows the client side of a flake; the server window that explains it was
// discarded. That is why several open flaky issues carry "the precise race is
// not yet diagnosed" — the evidence was thrown away at teardown.
//
// Usage: node report-flaky-tests.js "<lane label>" <markerPath> [reportPath]
// Reads test-results/report.json relative to the current working directory
// unless an explicit path is given. Always exits 0: this is evidence
// collection and must never turn a green lane red or mask a real failure.
'use strict';

const fs = require('fs');

const label = process.argv[2] || 'lane';
const markerPath = process.argv[3];
const reportPath = process.argv[4] || 'test-results/report.json';

if (!markerPath) {
  console.log('[report-flaky-tests] no marker path given; skipping');
  process.exit(0);
}
if (!fs.existsSync(reportPath)) {
  console.log(`[report-flaky-tests] no report at ${reportPath}; skipping`);
  process.exit(0);
}

let report;
try {
  report = JSON.parse(fs.readFileSync(reportPath, 'utf8'));
} catch (error) {
  console.log(`[report-flaky-tests] unreadable report at ${reportPath}: ${error.message}`);
  process.exit(0);
}

const flaky = [];
const walk = (suite) => {
  (suite.suites || []).forEach(walk);
  (suite.specs || []).forEach((spec) => {
    (spec.tests || []).forEach((test) => {
      // Playwright marks a spec "flaky" when an earlier attempt failed and a
      // later one passed. Derive the same shape from the attempt list as a
      // fallback so a reporter change degrades to detecting it anyway. A run
      // whose last attempt still failed is a hard failure, not a flake — that
      // lane already reports `=failed` and triggers the dump on its own.
      const results = test.results || [];
      const retriedToGreen =
        results.length > 1 &&
        results.some((result) => result.status === 'failed') &&
        results[results.length - 1].status === 'passed';
      if (test.status === 'flaky' || retriedToGreen) {
        flaky.push(`${spec.file || suite.file || '<file>'} › ${spec.title}`);
      }
    });
  });
};
(report.suites || []).forEach(walk);

if (flaky.length === 0) {
  console.log(`[report-flaky-tests] ${label}: no flaky specs`);
  process.exit(0);
}

console.log(`[report-flaky-tests] ${label}: ${flaky.length} flaky spec(s) — teardown will capture container logs`);
flaky.forEach((entry) => console.log(`  flaky: ${entry}`));

try {
  fs.writeFileSync(markerPath, `${label}\n${flaky.join('\n')}\n`);
} catch (error) {
  console.log(`[report-flaky-tests] could not write marker ${markerPath}: ${error.message}`);
}
process.exit(0);
