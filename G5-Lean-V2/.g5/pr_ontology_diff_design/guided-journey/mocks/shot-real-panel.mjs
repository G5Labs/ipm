import { chromium } from '/Users/davide/G5-Lean/frontend/node_modules/playwright/index.mjs';
const out = '/Users/davide/G5-Lean-wt/guided-mocks/.g5/pr_ontology_diff_design/guided-journey/screenshots';
const browser = await chromium.launch();
const ctx = await browser.newContext({ viewport: { width: 1440, height: 1000 }, deviceScaleFactor: 2 });
await ctx.addInitScript(() => {
  try { localStorage.setItem('g5-user-settings', JSON.stringify({ workspaceLayout: 'alignment' })); } catch {}
  const now = Date.now();
  const marks = [
    { stepId: 'se-plan', done: true, skipped: false, markedAt: new Date(now - 2 * 60000).toISOString(), actorUserId: 'user-alex' },
    { stepId: 'se-code', done: false, skipped: true, markedAt: new Date(now - 60000).toISOString(), actorUserId: 'user-alex' },
    { stepId: 'be-open-questions', done: true, skipped: false, markedAt: new Date(now - 3600000).toISOString(), actorUserId: 'user-alex' },
  ];
  const docs = [
    { id: 'd1', filePath: 'docs/task-graph-rework/requirements.md', docType: 'doc', title: 'requirements.md' },
    { id: 'd2', filePath: 'docs/task-graph-rework/mocks/home.html', docType: 'doc', title: 'home.html' },
    { id: 'd3', filePath: 'docs/task-graph-rework/explainers/why.html', docType: 'explainer', title: 'why.html' },
  ];
  const wrap = (real) => async (input, init) => {
    const url = typeof input === 'string' ? input : input.url;
    if (/\/api\/v1\/workspaces\/[^/]+\/journey$/.test(url)) {
      return new Response(JSON.stringify({ data: { marks } }), { status: 200, headers: { 'Content-Type': 'application/json' } });
    }
    const res = await real(input, init);
    if (/\/api\/v1\/workspaces\/task-graph-rework$/.test(url) && res.ok) {
      const body = await res.clone().json();
      if (body && body.data) { body.data.phase = 'se'; body.data.docsFolderPath = 'docs/task-graph-rework'; }
      return new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } });
    }
    if (/\/api\/v1\/workspaces\/task-graph-rework\/docs(\?|$)/.test(url) && res.ok) {
      const body = await res.clone().json();
      const extra = docs.map((d) => ({ ...(Array.isArray(body.data) && body.data[0] ? body.data[0] : {}), ...d }));
      return new Response(JSON.stringify({ ...body, data: [...(Array.isArray(body.data) ? body.data : []), ...extra] }), { status: 200, headers: { 'Content-Type': 'application/json' } });
    }
    return res;
  };
  let current = window.fetch;
  Object.defineProperty(window, 'fetch', {
    configurable: true,
    get() { return current; },
    set(fn) { current = wrap(fn); },
  });
});
const page = await ctx.newPage();
page.on('console', (m) => { if (m.type() === 'error') console.log('console:', m.text().slice(0, 200)); });
await page.goto('http://localhost:5199/g5labs/task-graph-rework', { waitUntil: 'networkidle' });
await page.waitForTimeout(1500);
const guided = page.getByTestId('guided-journey');
await guided.waitFor({ timeout: 15000 });
await page.screenshot({ path: `${out}/real-panel-full.png` });
const box = await guided.boundingBox();
console.log('guided box', box);
const summary = await page.getByTestId('guided-summary').innerText();
console.log('summary:', summary.replace(/\n/g, ' | '));
const states = await page.$$eval('[data-testid^="guided-step-"][data-state]', (els) => els.map((e) => `${e.dataset.testid}:${e.dataset.state}/${e.dataset.kind}`));
console.log(states.join('\n'));
await guided.screenshot({ path: `${out}/real-panel.png` });
// Expand the finished BE phase and the IE phase, open a folded row.
await page.getByTestId('guided-phase-be-toggle').click();
await page.getByTestId('guided-step-se-code-review-toggle').click();
await page.waitForTimeout(400);
await guided.screenshot({ path: `${out}/real-panel-expanded.png` });
await browser.close(); console.log('ok');
