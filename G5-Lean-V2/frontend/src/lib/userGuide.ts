import type { GuideDocId, TabItem } from "./types";

export interface GuideDocDefinition {
  id: GuideDocId;
  title: string;
  markdownFileName: string;
  aliases?: readonly string[];
  url: string;
}

export const USER_GUIDE_BASE_PATH = "/user-guide/";
export const DEFAULT_GUIDE_DOC_ID: GuideDocId = "getting-started";
export const GETTING_STARTED_AUTO_OPENED_STORAGE_KEY =
  "g5:getting-started:auto-opened";

export const USER_GUIDE_DOCS: readonly GuideDocDefinition[] = [
  {
    id: "getting-started",
    title: "Getting Started",
    markdownFileName: "getting-started-latest.md",
    aliases: ["getting-started.md"],
    url: `${USER_GUIDE_BASE_PATH}getting-started-latest.md`,
  },
  {
    id: "onboarding-build-from-scratch",
    title: "Using G5 to Build from Scratch",
    markdownFileName: "onboarding-build-from-scratch.md",
    url: `${USER_GUIDE_BASE_PATH}onboarding-build-from-scratch.md`,
  },
  {
    id: "onboarding-pm-poc-legacy",
    title: "Using G5 to Enhance an Existing Library",
    markdownFileName: "onboarding-PM-PoC-legacy.md",
    url: `${USER_GUIDE_BASE_PATH}onboarding-PM-PoC-legacy.md`,
  },
];

const GUIDE_BY_ID = new Map(USER_GUIDE_DOCS.map((guide) => [guide.id, guide]));
const GUIDE_ID_BY_PATH = new Map<string, GuideDocId>(
  USER_GUIDE_DOCS.flatMap((guide) =>
    [guide.markdownFileName, ...(guide.aliases ?? [])].map(
      (path): [string, GuideDocId] => [path.toLowerCase(), guide.id],
    ),
  ),
);

function stripHashAndQuery(href: string): string {
  return href.split("#", 1)[0].split("?", 1)[0];
}

function normalizeGuidePath(href: string): string {
  const trimmed = href.trim();
  if (!trimmed || trimmed.startsWith("#")) return "";

  try {
    const base =
      typeof window !== "undefined"
        ? window.location.origin
        : "http://localhost";
    const url = new URL(trimmed, `${base}${USER_GUIDE_BASE_PATH}`);
    const sameOrigin =
      typeof window !== "undefined" && url.origin === window.location.origin;
    const relativeLike =
      !/^[a-z][a-z0-9+.-]*:/i.test(trimmed) && !trimmed.startsWith("//");
    if (!sameOrigin && !relativeLike) {
      return "";
    }
    return stripHashAndQuery(url.pathname)
      .replace(/^\/user-guide\//, "")
      .replace(/^\/+/, "");
  } catch {
    return stripHashAndQuery(trimmed)
      .replace(/^\/user-guide\//, "")
      .replace(/^\.\//, "")
      .replace(/^\/+/, "");
  }
}

export function getGuideDocById(id: string): GuideDocDefinition | null {
  return GUIDE_BY_ID.get(id as GuideDocId) ?? null;
}

export function getGuideDocByHref(href: string): GuideDocDefinition | null {
  const normalized = normalizeGuidePath(href);
  if (!normalized) return null;
  const guideId = GUIDE_ID_BY_PATH.get(normalized.toLowerCase());
  return guideId ? getGuideDocById(guideId) : null;
}

export function stripGuideMarkdownComments(markdown: string): string {
  return markdown.replace(/<!--[\s\S]*?-->/g, "");
}

export function guideDocTab(guideId: string): TabItem | null {
  const guide = getGuideDocById(guideId);
  if (!guide) return null;
  return { type: "guide_doc", guideId: guide.id, name: guide.title };
}
