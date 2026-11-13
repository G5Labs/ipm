import type {
  AgentAuthModeV1,
  AgentEgressModeV1,
  AgentProviderRouteV1,
  ProviderAccountKind,
  ProviderAccountScope,
} from "./types";

export function agentProviderRouteLabel(route: AgentProviderRouteV1): string {
  switch (route) {
    case "openrouter":
      return "OpenRouter";
    case "openai_direct":
      return "OpenAI";
    case "anthropic_direct":
      return "Anthropic";
    case "compatible_endpoint":
      return "Compatible endpoint";
    default:
      return "No external provider";
  }
}

export function agentAuthModeLabel(mode: AgentAuthModeV1): string {
  switch (mode) {
    case "open_router_api_key":
      return "OpenRouter API key";
    case "open_ai_api_key":
      return "OpenAI API key";
    case "chat_gpt_subscription":
      return "ChatGPT subscription";
    case "anthropic_api_key":
      return "Anthropic API key";
    case "anthropic_bearer":
      return "Anthropic bearer token";
    case "anthropic_oauth":
      return "Anthropic OAuth";
    default:
      return "No credential";
  }
}

export function agentEgressModeLabel(mode: AgentEgressModeV1): string {
  switch (mode) {
    case "host_llm_proxy":
      return "G5 proxy (historical launch)";
    case "direct_provider":
      return "Direct provider egress";
    default:
      return "No network egress";
  }
}

/** Closed provider-account kind catalog labels (contract D14). */
export function providerAccountKindLabel(kind: ProviderAccountKind): string {
  switch (kind) {
    case "claude_subscription":
      return "Claude subscription";
    case "anthropic_api_key":
      return "Anthropic API key";
    case "anthropic_compatible_bearer":
      return "Anthropic-compatible endpoint";
    case "openrouter_api_key":
      return "OpenRouter API key";
    case "openai_api_key":
      return "OpenAI API key";
    case "chatgpt_subscription":
      return "ChatGPT subscription";
    default:
      return "Provider account";
  }
}

/** Scope pill wording shared by the assignment controls and the chat field. */
export function providerAccountScopeLabel(scope: ProviderAccountScope): string {
  switch (scope) {
    case "org":
      return "Org";
    case "workspace":
      return "Workspace";
    default:
      return "You";
  }
}
