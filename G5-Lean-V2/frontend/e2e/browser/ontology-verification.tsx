// Ontology Preview with statement folds, verification status and details
// (g5.documents.ontology.disclosure, g5.documents.ontology.verification),
// over a synthetic file set shaped like an uplift's served without an API stack.
// `window.ontologyTest.load(files, path, baseline?)` swaps in other files, and
// a Compare baseline, for checks.
import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { DocumentPreview } from "../../src/components/files/DocumentPreview";
import "../../src/index.css";

const params = new URLSearchParams(location.search);
document.documentElement.dataset.theme = "alpine-graphite";
document.documentElement.dataset.mode =
  params.get("mode") === "dark" ? "dark" : "light";

const tla = ".g5/TLA_verification";
const ontologyPath = ".g5/ontology/features/receiving.g5.md";
const statement = (id: string, body: string, aliases: string[] = []) =>
  `${aliases.map((a) => `<a id="${a}"></a>\n`).join("")}<a id="${id}"></a>\n[I · ${id}]\n${body}\n\n`;
const ontology =
  "# Receiving\n\nReceivers record deliveries against a purchase order.\n\n## Numbers\n\n" +
  statement(
    "receipt-number",
    "Every receipt needs one editable, suggested number, unique to its vendor.",
  ) +
  statement(
    "receipt-number.suggestion",
    "Suggest one more than the highest stored number; the receiver may replace it.",
    ["receipt-suggestion"],
  ) +
  statement(
    "receipt-number.rules",
    "A receipt number must meet these rules:\n\n- The number can contain at most 20 characters.\n- Deleting a draft does not release its number.",
  ) +
  statement(
    "receipt-draft",
    "Create receipts as drafts.\n\n- Submission makes the receipt read-only.",
  ) +
  statement("receipt-access", "Only receivers may access receipts.") +
  "## Metadata\n\n" +
  [
    [
      "receipt-number",
      "- agent\n\nSources: [Creating a receipt](../../../docs/knowledge/receiving.md#L26-L28), [guide with spaces](../../../docs/analysis/Receiving Guide.md)\n\nChildren: [suggestion](#receipt-number.suggestion); [rules](#receipt-number.rules)",
    ],
    [
      "receipt-number.suggestion",
      `- agent\n- design-verified\n\nState machines: [Receiving numbering](../../TLA_verification/models/receiving-numbering/manifest.json)\n\nFindings: [Suggested numbers can repeat after a draft is deleted](../../TLA_verification/bugs_issues.md#rcv-num-001), [One numbering sequence](../../TLA_verification/design_simplification.md#simp-num-1)\n\nExpands: [receipt-number](#receipt-number)`,
    ],
    [
      "receipt-number.rules",
      "- agent\n- test-coverage\n\nTests: [ReceiptNumberTests](../../../tests/ReceiptNumberTests.cs) cover length and reuse.\n\nExpands: [receipt-number](#receipt-number)",
    ],
    [
      "receipt-draft",
      `- agent\n- design-verified\n\nState machines: [Draft lifecycle](../../TLA_verification/models/receipt-draft/manifest.json)`,
    ],
    [
      "receipt-access",
      "- agent\n\nSource: [Access](../../docs/knowledge/receiving-access.md)",
    ],
  ]
    .map(([id, body]) => `### ${id}\n\n${body}\n`)
    .join("\n");
let files: Record<string, string> = {
  [ontologyPath]: ontology,
  "docs/knowledge/receiving.md": "# Receiving\n\nLine 2\n",
  "docs/analysis/Receiving Guide.md": "# Guide\n",
  "docs/knowledge/receiving-access.md": "# Access\n",
  [`${tla}/models/receiving-numbering/manifest.json`]: JSON.stringify({
    concern: "receiving-numbering",
    question:
      "Is the receipt number unique per vendor, kept reserved after draft deletion, and is every suggestion fresh?",
    ontology_ids: ["receipt-suggestion"],
    bindings: [{ path: "src/Receiving.cs", start: 16, end: 36 }],
    runs: [
      {
        label: "asbuilt-safety",
        spec: "Numbering.tla",
        cfg: "safety.cfg",
        kind: "safety",
        variant: "as-built",
        expect: "pass",
      },
      {
        label: "ctl-no-unique-index",
        spec: "Numbering.tla",
        cfg: "ctl.cfg",
        kind: "control",
        variant: "control",
        expect: "violation:UniquePerVendor",
      },
    ],
  }),
  [`${tla}/models/receiving-numbering/BINDINGS.md`]:
    "# receiving-numbering bindings\n\n| Model action | Code | Guard |\n|---|---|---|\n| `Suggest` | [Handler](../../../../src/Receiving.cs#L16-L36) | role |\n",
  [`${tla}/models/receiving-numbering/Numbering.tla`]:
    "---- MODULE Numbering ----\n====\n",
  [`${tla}/models/receipt-draft/manifest.json`]: JSON.stringify({
    concern: "receipt-draft",
    question: "Can a deleted draft be submitted?",
    ontology_ids: ["receipt-draft"],
    runs: [
      {
        label: "safety",
        spec: "Draft.tla",
        cfg: "s.cfg",
        kind: "safety",
        variant: "as-built",
        expect: "pass",
      },
    ],
  }),
  [`${tla}/results/index.jsonl`]: [
    {
      run_id: "20261024T014713Z-asbuilt-safety-a",
      concern: "receiving-numbering",
      label: "asbuilt-safety",
      kind: "safety",
      variant: "as-built",
      expect: "pass",
      outcome: "PASS",
      distinct_states: 26827,
      states_generated: 86478,
      depth: 13,
    },
    {
      run_id: "20261023T010000Z-asbuilt-safety-b",
      concern: "receiving-numbering",
      label: "asbuilt-safety",
      kind: "safety",
      variant: "as-built",
      expect: "pass",
      outcome: "TIMEOUT",
      distinct_states: 1200,
    },
    {
      run_id: "20261024T014740Z-ctl-no-unique-index",
      concern: "receiving-numbering",
      label: "ctl-no-unique-index",
      kind: "control",
      variant: "control",
      expect: "violation:UniquePerVendor",
      outcome: "EXPECTED_VIOLATION",
      distinct_states: 173,
      states_generated: 273,
      depth: 5,
    },
  ]
    .map((row) => JSON.stringify(row))
    .join("\n"),
  "src/Receiving.cs": "class Receiving {}\n",
  [`${tla}/bugs_issues.md`]:
    '# Bugs and issues\n\n<a id="rcv-num-001"></a>\n### Suggested numbers can repeat after a draft is deleted\n\nSeverity: High · Decision: Need fix — [G5L-7](https://g5.example/issues/7)\n\nDetails.\n',
  [`${tla}/design_simplification.md`]:
    '# Design simplifications\n\n<a id="simp-num-1"></a>\n### Use one numbering sequence for receipts and orders\n\nSeverity: Low · Decision: Open\n\nDetails.\n',
};
let currentPath = ontologyPath;
const opened: { path: string; line?: number }[] = [];
const navigated: unknown[] = [];

// Compare's baseline for the current file, when a check supplies one.
let baseline: string | undefined;
const missing = () =>
  Response.json(
    { error: { code: "NOT_FOUND", message: "Not found" } },
    { status: 404 },
  );
const realFetch = window.fetch.bind(window);
window.fetch = async (input, init) => {
  const url = new URL(
    input instanceof Request ? input.url : String(input),
    location.href,
  );
  const file = url.pathname.match(/^\/api\/v1\/workspaces\/ws\/files\/(.+)$/);
  if (file) {
    const path = file[1].split("/").map(decodeURIComponent).join("/");
    const content = files[path];
    return content === undefined
      ? missing()
      : Response.json({
          data: {
            path,
            content,
            size: content.length,
            encoding: "utf-8",
            hash: "fixture",
            source: "live",
            repoOid: "fixture",
            snapshotAt: null,
          },
        });
  }
  if (
    url.pathname === "/api/v1/workspaces/ws/source-control/ontology-baseline" &&
    baseline !== undefined
  )
    return Response.json({
      data: {
        revision: "fixture",
        workspaceId: "ws",
        repoId: "repo",
        path: currentPath,
        baselineCommit: "0123456789abcdef",
        sourceCommit: "fixture",
        parentCommit: "fixture",
        content: baseline,
      },
    });
  if (url.pathname.startsWith("/api/")) return missing();
  return realFetch(input, init);
};

const listeners = new Set<() => void>();
const client = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});
(window as unknown as { ontologyTest: unknown }).ontologyTest = {
  opened,
  navigated,
  files: () => files,
  path: () => currentPath,
  load(next: Record<string, string>, path: string, base?: string) {
    files = next;
    currentPath = path;
    baseline = base;
    client.clear();
    listeners.forEach((listener) => listener());
  },
};

function App() {
  const [, setRevision] = useState(0);
  useEffect(() => {
    const listener = () => setRevision((n) => n + 1);
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, []);
  return (
    <div style={{ maxWidth: 1100, margin: "0 auto" }}>
      <DocumentPreview
        key={currentPath}
        workspaceId="ws"
        repoId="repo"
        filePath={currentPath}
        content={files[currentPath] ?? ""}
        onNavigate={(target) => navigated.push(target)}
        onOpenWorkspaceFile={(path, _repo, line) => opened.push({ path, line })}
      />
    </div>
  );
}

createRoot(document.getElementById("root")!).render(
  <QueryClientProvider client={client}>
    <MemoryRouter>
      <App />
    </MemoryRouter>
  </QueryClientProvider>,
);
