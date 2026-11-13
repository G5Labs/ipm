import { describe, expect, it } from "vitest";

import {
  SECRET_WHITESPACE_MESSAGE,
  normalizeSecret,
  secretShapeApplies,
  secretShapeError,
} from "./providerAccountSecret";

const OAUTH =
  "sk-ant-oat01-0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghij-A_cdefgh";
const API_KEY =
  "sk-ant-api03-0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghij_A-cdefAA";

describe("providerAccountSecret", () => {
  it("trims surrounding whitespace, including a trailing newline from a paste", () => {
    expect(normalizeSecret(`  ${OAUTH}\n`)).toBe(OAUTH);
    expect(normalizeSecret(`\u0085${OAUTH}\u0085`)).toBe(OAUTH);
    expect(secretShapeError("claude_subscription", `  ${OAUTH}\n`)).toBe(null);
  });

  it("accepts a well-formed credential of each shaped kind, version digits and API-key family free", () => {
    expect(secretShapeError("anthropic_api_key", API_KEY)).toBe(null);
    for (const key of [
      "sk-ant-api04-0123456789abcdefghijklmnop",
      "sk-ant-svc-0123456789abcdefghijklmnop",
    ]) {
      expect(secretShapeError("anthropic_api_key", key), key).toBe(null);
    }
    expect(
      secretShapeError(
        "claude_subscription",
        "sk-ant-oat02-0123456789abcdefghijklmnop",
      ),
    ).toBe(null);
    expect(
      secretShapeError(
        "openrouter_api_key",
        "sk-or-v1-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
      ),
    ).toBe(null);
    for (const key of [
      "sk-proj-0123456789abcdefghijT3BlbkFJ0123456789abcdefghij",
      "sk-svcacct-0123456789abcdefghijT3BlbkFJ0123456789abcdefghij",
      "sk-0123456789abcdefghijT3BlbkFJ0123456789abcdefghij",
    ]) {
      expect(secretShapeError("openai_api_key", key), key).toBe(null);
    }
  });

  it("leaves a blank value to the caller and the bearer kind unchecked", () => {
    expect(secretShapeError("claude_subscription", "   ")).toBe(null);
    expect(
      secretShapeError("anthropic_compatible_bearer", "zai-bearer-1"),
    ).toBe(null);
  });

  it("does not apply a native shape behind a custom gateway", () => {
    expect(
      secretShapeApplies(
        "anthropic_api_key",
        "https://llm.example.test/anthropic",
      ),
    ).toBe(false);
    expect(
      secretShapeError(
        "anthropic_api_key",
        "gateway-key-0001",
        "https://llm.example.test/anthropic",
      ),
    ).toBe(null);
    expect(
      secretShapeError(
        "openai_api_key",
        "gateway-key-0001",
        "https://llm.example.test/openai/v1",
      ),
    ).toBe(null);
    for (const url of [
      null,
      "",
      "https://api.anthropic.com",
      "https://api.anthropic.com/",
      "HTTPS://API.ANTHROPIC.COM/v1",
      "https://api.anthropic.com:443/v1",
    ]) {
      expect(secretShapeApplies("anthropic_api_key", url), String(url)).toBe(
        true,
      );
      expect(
        secretShapeError("anthropic_api_key", "gateway-key-0001", url),
      ).not.toBe(null);
    }
    for (const url of [
      "https://api.anthropic.com.evil.test",
      "https://evil.test/https://api.anthropic.com",
      "https://api.anthropic.com:8443",
      "http://api.anthropic.com",
      "not a url",
    ]) {
      expect(secretShapeApplies("anthropic_api_key", url), url).toBe(false);
    }
    expect(secretShapeApplies("claude_subscription", null)).toBe(true);
    expect(secretShapeApplies("anthropic_compatible_bearer", null)).toBe(false);
  });

  it("rejects a space or line break inside the value for every kind", () => {
    expect(
      secretShapeError(
        "claude_subscription",
        "sk-ant-oat01-0123456789 abcdefghijklmnop",
      ),
    ).toBe(SECRET_WHITESPACE_MESSAGE);
    expect(secretShapeError("anthropic_compatible_bearer", "zai\nbearer")).toBe(
      SECRET_WHITESPACE_MESSAGE,
    );
    expect(
      secretShapeError(
        "openai_api_key",
        "sk-proj-0123456789\tabcdefghijklmnop",
      ),
    ).toBe(SECRET_WHITESPACE_MESSAGE);
    expect(
      secretShapeError(
        "anthropic_api_key",
        "sk-ant-api03-0123456789\u0085abcdefghijklmnop",
      ),
    ).toBe(SECRET_WHITESPACE_MESSAGE);
  });

  it("names the right account for a credential of the other Anthropic kind", () => {
    // QA 2026-10-27: an API key saved as a subscription token.
    expect(secretShapeError("claude_subscription", API_KEY)).toMatch(
      /claude setup-token/,
    );
    expect(secretShapeError("anthropic_api_key", OAUTH)).toMatch(
      /sk-ant-api03-/,
    );
    // Nor is an OAuth refresh token or an Admin API key.
    for (const key of [
      "sk-ant-ort01-0123456789abcdefghijklmnop",
      "sk-ant-admin01-0123456789abcdefghijklmnop",
    ]) {
      expect(secretShapeError("anthropic_api_key", key), key).toMatch(
        /sk-ant-api03-/,
      );
    }
  });

  it("rejects a wrong prefix, a missing family dash, a truncated body or a stray character", () => {
    expect(
      secretShapeError(
        "openrouter_api_key",
        "or-v1-0123456789abcdef0123456789abcdef",
      ),
    ).toMatch(/sk-or-v1-/);
    expect(
      secretShapeError("openai_api_key", "pk-0123456789abcdefghijklmnop"),
    ).toMatch(/sk-/);
    expect(
      secretShapeError(
        "claude_subscription",
        "sk-ant-oat01-0123456789abcdefghi",
      ),
    ).toMatch(/claude setup-token/);
    expect(
      secretShapeError(
        "anthropic_api_key",
        "sk-ant-api030123456789abcdefghijklmnop",
      ),
    ).toMatch(/sk-ant-api03-/);
    expect(
      secretShapeError(
        "anthropic_api_key",
        "sk-ant-api03-0123456789abcdefghij$klmnop",
      ),
    ).toMatch(/sk-ant-api03-/);
    expect(
      secretShapeError(
        "openrouter_api_key",
        "sk-or-v1-zzzzzzzzzzzzzzzzzzzzzzzz",
      ),
    ).toMatch(/hex/);
  });
});
