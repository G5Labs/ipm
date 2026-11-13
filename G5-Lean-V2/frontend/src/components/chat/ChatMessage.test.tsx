import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ChatMessage, CitationChips } from "./ChatMessage";
import { WorkspaceNamesProvider } from "../../hooks/useWorkspaceNames";
import type { ChatMessageDTO } from "../../lib/types";

const WS_ID = "019ecc9b-8ca2-73c2-9ce3-c184b5d31673";

function assistantMessage(
  overrides: Partial<ChatMessageDTO> = {},
): ChatMessageDTO {
  return {
    id: "m1",
    sessionId: "s1",
    role: "assistant",
    content: "See the spec for details.",
    artifacts: null,
    citations: [{ label: "Spec", filePath: `docs/${WS_ID}/a.md` }],
    inputTokens: null,
    outputTokens: null,
    status: "complete",
    messageIndex: 0,
    createdAt: "2026-06-15T00:00:00Z",
    ...overrides,
  };
}

describe("ChatMessage citation tooltips (D8)", () => {
  it("keeps historical node-only citations inert while file citations navigate", () => {
    const historical = { label: "Legacy spec", nodeId: "retired-node" };
    const file = { label: "Source file", filePath: "src/main.rs" };
    const navigate = vi.fn();
    render(
      <CitationChips
        citations={[historical, file]}
        onCitationClick={navigate}
      />,
    );
    expect(screen.getByText("Legacy spec").closest("button")).toBeNull();
    fireEvent.click(screen.getByText("Legacy spec"));
    expect(navigate).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Source file" }));
    expect(navigate).toHaveBeenCalledExactlyOnceWith(file);
  });

  it("renders file citations as text when navigation is unavailable", () => {
    render(
      <CitationChips
        citations={[{ label: "Source file", filePath: "src/main.rs" }]}
      />,
    );
    expect(screen.getByText("Source file")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("relabels the docs/<id> UUID to the workspace name in the citation tooltip", () => {
    render(
      <WorkspaceNamesProvider value={{ [WS_ID]: "compliance-service" }}>
        <ChatMessage message={assistantMessage()} isStreaming={false} />
      </WorkspaceNamesProvider>,
    );
    // The citation chip's hover tooltip shows the workspace name, not the UUID.
    expect(
      screen.getByTitle("Spec — docs/compliance-service/a.md"),
    ).toBeInTheDocument();
    expect(
      screen.queryByTitle(`Spec — docs/${WS_ID}/a.md`),
    ).not.toBeInTheDocument();
  });

  it("falls back to the raw UUID path without a workspace-name context", () => {
    render(<ChatMessage message={assistantMessage()} isStreaming={false} />);
    expect(screen.getByTitle(`Spec — docs/${WS_ID}/a.md`)).toBeInTheDocument();
  });
});

describe("ChatMessage error indicator (websocket D15)", () => {
  it("renders saved platform diagnostics once with a non-color error indicator", () => {
    const diagnostic = "This message was not sent: correct the tool policy.";
    render(
      <ChatMessage
        message={assistantMessage({
          status: "error",
          content: diagnostic,
          isErrorNotice: true,
        })}
        isStreaming={false}
      />,
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent(diagnostic);
    expect(alert).toHaveClass("text-[var(--g5-blocked)]");
    expect(alert.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
    expect(screen.getAllByText(diagnostic)).toHaveLength(1);
    expect(
      screen.queryByText("Error generating response"),
    ).not.toBeInTheDocument();
  });

  it("surfaces the diagnostic errorMessage when present", () => {
    render(
      <ChatMessage
        message={assistantMessage({
          status: "error",
          errorMessage: "Rate limit exceeded",
        })}
        isStreaming={false}
      />,
    );
    expect(screen.getByText("Rate limit exceeded")).toBeInTheDocument();
    expect(
      screen.queryByText("Error generating response"),
    ).not.toBeInTheDocument();
  });

  it("falls back to the generic label when no diagnostic is present", () => {
    render(
      <ChatMessage
        message={assistantMessage({ status: "error", errorMessage: null })}
        isStreaming={false}
      />,
    );
    expect(screen.getByText("Error generating response")).toBeInTheDocument();
  });
});

it("keeps sparse legacy messages readable when optional JSON fields are not arrays", () => {
  const message = {
    ...assistantMessage({ content: "Legacy answer." }),
    artifacts: {},
    citations: {},
  } as unknown as ChatMessageDTO;
  render(<ChatMessage message={message} isStreaming={false} />);
  expect(screen.getByText("Legacy answer.")).toBeVisible();
  expect(screen.getByTestId("assistant-turn-content")).toHaveClass(
    "g5-conversation-prose",
  );
});
