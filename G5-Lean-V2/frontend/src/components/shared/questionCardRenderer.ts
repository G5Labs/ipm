// =============================================================================
// Question cards inside sandboxed HTML explainers (`g5.documents.question.html`)
// =============================================================================
// An explainer holds referenced questions as `data-g5-question="<set path>#<id>"`
// placeholders. The host injects this renderer script ahead of the authored
// bytes; the frame holds no credentials and never calls the API. The host
// supplies each card's data by message (`question-data`), so an answer or a
// vote update refreshes the cards in place without regenerating the frame's
// `srcDoc`, and unsent input survives while the entry's revision is
// unchanged. An Answer or Vote press inside the frame only PROPOSES
// (`propose-question-answer`, `propose-question-vote`) and the host validates
// the message. A person confirms an answer in trusted host controls
// (`QuestionAnswerBar`); a validated vote is applied with that one press,
// and the bar shows only a refused vote. Votes
// (`g5.documents.question.votes`) are open to every reader; the card shows
// the tally per option with the voters' names.
// =============================================================================

import {
  optionVoterNames,
  type QuestionEntry,
  type VoteTally,
} from "../../lib/questionSet";

export const QUESTION_CARD_RENDERER_ATTR = "data-g5-question-renderer";
export const QUESTION_MSG_SOURCE_CHILD = "g5-html-doc";
export const QUESTION_MSG_SOURCE_HOST = "g5-html-doc-host";
export const QUESTION_BRIDGE_VERSION = 1;
/** Card status while a vote press is saved; the host and the frame both set it. */
export const QUESTION_VOTE_SAVING = "Saving your vote…";
export const QUESTION_VOTE_WITHDRAWING = "Withdrawing your vote…";

/** One question's tally as the frame shows it: names only, never ids. */
export interface QuestionCardVotes {
  total: number;
  /** The viewer's own vote, when cast. */
  viewer: { selectedOptionIds: string[]; text: string } | null;
  /** Option id → voter display names, in casting order. */
  byOption: Record<string, string[]>;
  withText: { name: string; text: string }[];
}

/** One placeholder's data as the host supplies it. */
export type QuestionCardData =
  | { state: "loading" }
  | { state: "missing"; message: string }
  | { state: "invalid"; message: string }
  | {
      state: "ok";
      answerable: boolean;
      /** Every reader may vote; false only while the viewer is unknown. */
      votable: boolean;
      setTitle: string;
      /** The tally, or why there is none yet; zero counts come only from a
       * successful read. */
      votes: QuestionCardVotes | { pending: "loading" | "unavailable" };
      entry: {
        id: string;
        displayNumber: number;
        revision: number;
        status: QuestionEntry["status"];
        decision: boolean;
        prompt: string;
        context: string;
        severity: string;
        multiSelect: boolean;
        allowFreeText: boolean;
        options: {
          id: string;
          label: string;
          description: string;
          recommended: boolean;
        }[];
        answer: {
          selectedOptionIds: string[];
          text: string;
          rationale: string;
          answeredBy: string;
          answeredByKind: string;
          answeredAgainstRevision: number | null;
        };
      };
    };

/** `<set path>#<id>` → card data. */
export type QuestionCardsPayload = Record<string, QuestionCardData>;

export function cardVotes(tally: VoteTally): QuestionCardVotes {
  const byOption: Record<string, string[]> = {};
  for (const id of Object.keys(tally.byOption)) {
    byOption[id] = optionVoterNames(tally, id);
  }
  return {
    total: tally.total,
    viewer: tally.viewerVote
      ? {
          selectedOptionIds: tally.viewerVote.selectedOptionIds,
          text: tally.viewerVote.text,
        }
      : null,
    byOption,
    withText: tally.withText.map((vote) => ({
      name: vote.displayName,
      text: vote.text,
    })),
  };
}

export function cardDataForEntry(
  entry: QuestionEntry,
  answerable: boolean,
  setTitle: string,
  tally: VoteTally | "loading" | "unavailable" = "loading",
  votable = true,
): QuestionCardData {
  const at = entry.answer.answeredAtRevision;
  return {
    state: "ok",
    answerable,
    votable,
    setTitle,
    votes: typeof tally === "string" ? { pending: tally } : cardVotes(tally),
    entry: {
      id: entry.id,
      displayNumber: entry.displayNumber,
      revision: entry.revision,
      status: entry.status,
      decision: entry.decision,
      prompt: entry.prompt,
      context: entry.context ?? "",
      severity: entry.severity ?? "",
      multiSelect: entry.multiSelect,
      allowFreeText: entry.allowFreeText,
      options: entry.options.map((option) => ({
        id: option.id,
        label: option.label,
        description: option.description ?? "",
        recommended: option.recommended === true,
      })),
      answer: {
        selectedOptionIds: entry.answer.selectedOptionIds,
        text: entry.answer.text,
        rationale: entry.answer.rationale,
        answeredBy:
          entry.answer.answeredBy?.email ?? entry.answer.answeredBy?.id ?? "",
        answeredByKind: entry.answer.answeredBy?.kind ?? "",
        answeredAgainstRevision:
          entry.status !== "open" && at != null && at < entry.revision
            ? at
            : null,
      },
    },
  };
}

/**
 * The in-frame renderer. Plain ES5 with DOM construction through
 * `createElement`/`textContent` only: card text is agent- or person-authored
 * and is never parsed as markup.
 */
export const SANDBOXED_QUESTION_CARD_JS = `(function () {
  var HOST = ${JSON.stringify(QUESTION_MSG_SOURCE_HOST)};
  var CHILD = ${JSON.stringify(QUESTION_MSG_SOURCE_CHILD)};
  var VERSION = ${QUESTION_BRIDGE_VERSION};
  var VOTE_SAVING = ${JSON.stringify(QUESTION_VOTE_SAVING)};
  var VOTE_WITHDRAWING = ${JSON.stringify(QUESTION_VOTE_WITHDRAWING)};
  var generation = null;
  var cards = null;
  var drafts = {};
  var statuses = {};
  // Frame-only notes (an empty press). A host update replaces the statuses
  // but drops a note only when that card's host status changes.
  var notes = {};
  var hostStatuses = {};
  var seq = 0;
  function el(tag, style, text) {
    var node = document.createElement(tag);
    if (style) node.setAttribute("style", style);
    if (text != null) node.textContent = String(text);
    return node;
  }
  function keyOf(root) {
    return (root.getAttribute("data-g5-question") || "").replace(/^\\s+|\\s+$/g, "");
  }
  function captureDraft(key, root, card) {
    if (!card || card.state !== "ok") return;
    var draft = drafts[key];
    if (!draft || draft.revision !== card.entry.revision) return;
    var inputs = root.querySelectorAll("input[data-g5-option]");
    var selected = [];
    for (var i = 0; i < inputs.length; i++) {
      if (inputs[i].checked) selected.push(inputs[i].getAttribute("data-g5-option"));
    }
    draft.selected = selected;
    var text = root.querySelector("textarea[data-g5-text]");
    if (text) draft.text = text.value;
    var rationale = root.querySelector("input[data-g5-rationale]");
    if (rationale) draft.rationale = rationale.value;
  }
  function tallyLine(card, optionId) {
    var votes = card.votes;
    if (!votes || votes.pending || votes.total === 0) return null;
    var names = votes.byOption[optionId] || [];
    var line = el("div", "display:flex;align-items:center;gap:8px;font-size:11px;color:#555;margin:4px 0 0 22px");
    line.setAttribute("data-g5-option-votes", String(names.length));
    var share = votes.total > 0 ? Math.round((names.length / votes.total) * 100) : 0;
    var bar = el("span", "display:inline-block;width:64px;height:4px;border-radius:2px;background:#e3e3ea;overflow:hidden");
    bar.appendChild(el("span", "display:block;height:100%;background:#4f7ddb;width:" + share + "%"));
    line.appendChild(bar);
    line.appendChild(el("span", "", names.length + (names.length === 1 ? " vote" : " votes") + (names.length ? " \\u00b7 " + names.join(", ") : "")));
    return line;
  }
  function render(root) {
    var key = keyOf(root);
    var card = cards ? cards[key] : null;
    captureDraft(key, root, root.__g5Card);
    root.__g5Card = card;
    while (root.firstChild) root.removeChild(root.firstChild);
    var box = el("div", "border:1px solid #c8c8d0;border-radius:8px;padding:12px;margin:12px 0;font:14px/1.4 system-ui,sans-serif;background:#fafafc;color:#1d1d24");
    box.setAttribute("data-g5-question-card", key);
    root.appendChild(box);
    if (!card || card.state === "loading") {
      box.appendChild(el("div", "color:#666", "Loading question\\u2026"));
      return;
    }
    if (card.state !== "ok") {
      var notice = el("div", "color:#8a5a00", card.message);
      notice.setAttribute("data-g5-question-notice", card.state);
      box.appendChild(notice);
      return;
    }
    var entry = card.entry;
    var pending = !card.votes || card.votes.pending ? (card.votes && card.votes.pending) || "loading" : null;
    var votes = pending ? { total: 0, viewer: null, byOption: {}, withText: [] } : card.votes;
    var votable = card.votable && !pending;
    var interactive = card.answerable || votable;
    box.setAttribute("data-g5-question-status", entry.status);
    box.setAttribute("data-g5-question-revision", String(entry.revision));
    box.setAttribute("data-g5-question-votes", pending ? pending : String(votes.total));
    var draft = drafts[key];
    if (!draft || draft.revision !== entry.revision) {
      var seed = votes.viewer && !card.answerable ? votes.viewer : entry.answer;
      draft = drafts[key] = {
        revision: entry.revision,
        selected: seed.selectedOptionIds.slice(),
        text: seed.text,
        rationale: entry.answer.rationale
      };
    }
    var head = el("div", "display:flex;gap:8px;align-items:center;font-size:12px;color:#555;margin-bottom:6px");
    head.appendChild(el("span", "font-family:monospace", "Q" + entry.displayNumber + " \\u00b7 " + entry.id));
    var label = entry.decision ? "Answered" : entry.status === "proposed" ? "Proposed" : "Open";
    var chip = el("span", "border-radius:999px;padding:1px 8px;background:" + (entry.decision ? "#d9f2e1" : "#fbeccb"), label);
    chip.setAttribute("data-g5-question-chip", label.toLowerCase());
    head.appendChild(chip);
    if (entry.severity) head.appendChild(el("span", "", entry.severity));
    var count = el("span", "margin-left:auto", pending === "loading" ? "Loading votes\\u2026" : pending === "unavailable" ? "Votes unavailable" : votes.total === 0 ? "No votes yet" : votes.total + (votes.total === 1 ? " vote" : " votes") + (votes.viewer ? " \\u00b7 you voted" : ""));
    count.setAttribute("data-g5-question-vote-count", pending ? pending : String(votes.total));
    head.appendChild(count);
    box.appendChild(head);
    box.appendChild(el("div", "font-weight:600;margin-bottom:4px", entry.prompt));
    if (entry.context) box.appendChild(el("div", "font-size:12px;color:#555;margin-bottom:6px", entry.context));
    var name = "g5q-" + (++seq);
    for (var i = 0; i < entry.options.length; i++) {
      var option = entry.options[i];
      var row = el("label", "display:block;border:1px solid " + (option.recommended ? "#7bc59a" : "#d8d8e0") + ";border-radius:6px;padding:6px 8px;margin:4px 0;cursor:" + (interactive ? "pointer" : "default"));
      var input = el("input", "margin-right:6px");
      input.type = entry.multiSelect ? "checkbox" : "radio";
      input.name = name;
      input.setAttribute("data-g5-option", option.id);
      input.checked = draft.selected.indexOf(option.id) >= 0;
      input.disabled = !interactive;
      row.appendChild(input);
      row.appendChild(el("span", "font-weight:500", option.label));
      if (option.recommended) row.appendChild(el("span", "margin-left:6px;font-size:10px;color:#2f8a55", "RECOMMENDED"));
      if (option.description) row.appendChild(el("div", "font-size:12px;color:#555;margin-left:22px", option.description));
      var tally = tallyLine(card, option.id);
      if (tally) row.appendChild(tally);
      box.appendChild(row);
    }
    if (votes.withText && votes.withText.length) {
      var list = el("ul", "font-size:12px;color:#555;margin:6px 0 0;padding-left:18px");
      list.setAttribute("data-g5-question-vote-texts", "");
      for (var t = 0; t < votes.withText.length; t++) {
        list.appendChild(el("li", "", votes.withText[t].name + ": " + votes.withText[t].text));
      }
      box.appendChild(list);
    }
    if (entry.status !== "open") {
      var summary = el("div", "font-size:12px;margin-top:6px;padding:6px;border-radius:6px;background:#f0f0f4");
      summary.setAttribute("data-g5-question-summary", "");
      if (entry.status === "proposed") summary.appendChild(el("div", "color:#8a5a00;font-weight:600", "Proposed by an agent \\u2014 not a decision until the owner confirms it."));
      var by = entry.answer.answeredByKind === "agent" ? "the agent" : entry.answer.answeredBy;
      summary.appendChild(el("div", "", (entry.status === "proposed" ? "Proposed" : "Answered") + (by ? " by " + by : "")));
      var chosen = [];
      for (var c = 0; c < entry.options.length; c++) {
        if (entry.answer.selectedOptionIds.indexOf(entry.options[c].id) >= 0) chosen.push(entry.options[c].label);
      }
      if (chosen.length) {
        var decision = el("div", "font-weight:600", (entry.status === "proposed" ? "Proposed: " : "Decision: ") + chosen.join(", "));
        decision.setAttribute("data-g5-question-decision", "");
        summary.appendChild(decision);
      }
      if (entry.answer.text) summary.appendChild(el("div", "", entry.answer.text));
      if (entry.answer.answeredAgainstRevision != null) summary.appendChild(el("div", "color:#8a5a00", "Answered against revision " + entry.answer.answeredAgainstRevision + "."));
      box.appendChild(summary);
    }
    if (interactive) {
      if (entry.allowFreeText) {
        var text = el("textarea", "display:block;width:100%;margin-top:6px;box-sizing:border-box");
        text.setAttribute("data-g5-text", "");
        text.setAttribute("aria-label", card.answerable ? "Answer text" : "Vote text");
        text.rows = 2;
        text.value = draft.text;
        box.appendChild(text);
      }
      if (card.answerable) {
        var rationale = el("input", "display:block;width:100%;margin-top:4px;box-sizing:border-box");
        rationale.setAttribute("data-g5-rationale", "");
        rationale.setAttribute("aria-label", "Rationale");
        rationale.placeholder = "Rationale (optional)";
        rationale.value = draft.rationale;
        box.appendChild(rationale);
      }
      var actions = el("div", "display:flex;flex-wrap:wrap;align-items:center;gap:8px;margin-top:6px");
      if (votable) {
        var voteButton = el("button", "padding:4px 10px", votes.viewer ? "Change vote" : "Vote");
        voteButton.type = "button";
        voteButton.setAttribute("data-g5-question-vote", "");
        voteButton.onclick = function () { propose(key, root, "propose-question-vote", false); };
        actions.appendChild(voteButton);
        if (votes.viewer) {
          var withdrawButton = el("button", "padding:4px 10px;color:#555", "Withdraw vote");
          withdrawButton.type = "button";
          withdrawButton.setAttribute("data-g5-question-withdraw-vote", "");
          withdrawButton.onclick = function () { propose(key, root, "propose-question-vote", true); };
          actions.appendChild(withdrawButton);
        }
      }
      if (card.answerable) {
        var button = el("button", "padding:4px 10px", entry.status === "proposed" ? "Confirm answer" : "Answer");
        button.type = "button";
        button.setAttribute("data-g5-question-answer", "");
        button.onclick = function () { propose(key, root, "propose-question-answer", false); };
        actions.appendChild(button);
      }
      var status = notes[key] || statuses[key];
      if (status) {
        var line = el("span", "font-size:12px;color:#555", status);
        line.setAttribute("data-g5-question-state", "");
        actions.appendChild(line);
      }
      box.appendChild(actions);
      if (!card.answerable) {
        box.appendChild(el("div", "font-size:11px;color:#777;margin-top:6px", "Votes inform the decision; the workspace owner answers here while the workspace is running."));
      }
    } else {
      box.appendChild(el("div", "font-size:11px;color:#777;margin-top:6px", "Read-only."));
    }
  }
  function propose(key, root, type, withdraw) {
    var card = cards && cards[key];
    if (!card || card.state !== "ok" || generation == null) return;
    captureDraft(key, root, card);
    var draft = drafts[key];
    // The host drops an empty answer or vote; say so here rather than wait.
    if (!withdraw && draft.selected.length === 0 && !(draft.text || "").trim()) {
      notes[key] = "Pick an option or write a line first.";
      renderAll();
      return;
    }
    delete notes[key];
    var requestId = "q" + Date.now().toString(36) + Math.random().toString(36).slice(2, 10);
    statuses[key] = type === "propose-question-vote"
      ? (withdraw ? VOTE_WITHDRAWING : VOTE_SAVING)
      : "Waiting for confirmation in the host bar\\u2026";
    window.parent.postMessage({
      source: CHILD, version: VERSION, generation: generation,
      type: type, requestId: requestId, ref: key,
      expectedRevision: card.entry.revision, withdraw: !!withdraw,
      selectedOptionIds: withdraw ? [] : draft.selected, text: withdraw ? "" : (draft.text || ""), rationale: draft.rationale || ""
    }, "*");
    renderAll();
  }
  function renderAll() {
    var roots = document.querySelectorAll("[data-g5-question]");
    for (var i = 0; i < roots.length; i++) render(roots[i]);
  }
  window.addEventListener("message", function (event) {
    if (event.source !== window.parent) return;
    var data = event.data;
    if (!data || data.source !== HOST || data.version !== VERSION) return;
    if (data.type === "question-data" && data.cards && typeof data.cards === "object") {
      generation = data.generation;
      cards = data.cards;
      if (data.statuses && typeof data.statuses === "object") {
        for (var noted in notes) {
          if (data.statuses[noted] !== hostStatuses[noted]) delete notes[noted];
        }
        hostStatuses = data.statuses;
        statuses = {};
        for (var status in hostStatuses) statuses[status] = hostStatuses[status];
      }
      renderAll();
    }
  });
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", renderAll);
  } else {
    renderAll();
  }
})();`;
