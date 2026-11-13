// The injected Content-Security-Policy meta must be the first thing the HTML
// parser places in <head>: after at most a BOM, parser whitespace and the
// document's own doctype. Anything the parser treats as text (NBSP, vertical
// tab) ahead of the doctype opens <body> first and would leave a policy placed
// after the doctype inert, so such documents get the policy before their bytes.
import { describe, expect, it } from "vitest";
import {
  SANDBOXED_HTML_CSP,
  SCRIPTLESS_PREVIEW_CSP,
  withSandboxedHtmlCsp,
  withScriptlessPreviewCsp,
} from "./sandboxedHtml";

const BODY =
  "<html><head><title>t</title></head><body><p>authored</p></body></html>";
const META_PREFIX = '<meta http-equiv="Content-Security-Policy"';

function headPolicy(html: string): string | null {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const meta = doc.head.querySelector(
    'meta[http-equiv="Content-Security-Policy"]',
  );
  return meta?.getAttribute("content") ?? null;
}

describe.each([
  [
    "withScriptlessPreviewCsp",
    withScriptlessPreviewCsp,
    SCRIPTLESS_PREVIEW_CSP,
  ],
  ["withSandboxedHtmlCsp", withSandboxedHtmlCsp, SANDBOXED_HTML_CSP],
] as const)("%s doctype placement", (_name, inject, policy) => {
  it("keeps a doctype preceded by parser whitespace first", () => {
    const out = inject(`\r\n\t <!DOCTYPE html>${BODY}`);
    expect(out.startsWith(`\r\n\t <!DOCTYPE html>${META_PREFIX}`)).toBe(true);
    expect(headPolicy(out)).toBe(policy);
  });

  it("keeps a byte-order mark and its doctype ahead of the policy", () => {
    // A browser decodes the blob's bytes and drops a leading BOM before
    // parsing; jsdom's string parser keeps it as text, so only the
    // placement is asserted here.
    const out = inject(`\uFEFF<!doctype html>${BODY}`);
    expect(out.startsWith(`\uFEFF<!doctype html>${META_PREFIX}`)).toBe(true);
  });

  it.each([
    ["a no-break space", "\u00a0"],
    ["a vertical tab", "\u000b"],
    ["an ideographic space", "\u3000"],
  ])(
    "puts the policy before a doctype preceded by %s, which the parser reads as text",
    (_label, prefix) => {
      const out = inject(`${prefix}<!doctype html>${BODY}`);
      expect(out.startsWith(META_PREFIX)).toBe(true);
      expect(headPolicy(out)).toBe(policy);
    },
  );

  it("puts the policy first when there is no doctype", () => {
    const out = inject(BODY);
    expect(out.startsWith(META_PREFIX)).toBe(true);
    expect(headPolicy(out)).toBe(policy);
  });
});
