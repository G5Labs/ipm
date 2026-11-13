import type { GenerateOptionsV1, PendingSyncFile } from "../../lib/types";
import { isIsolatedPendingSyncDocFile } from "../../lib/types";

/** Generate is an ordinary chat. Its complete request travels as message text,
 * because chat start does not materialize document references from `context`. */
export function generateChatPrompt(
  workspaceId: string,
  files: PendingSyncFile[],
  options: GenerateOptionsV1,
): string {
  if (
    !workspaceId ||
    files.length === 0 ||
    !files.every((file) => isIsolatedPendingSyncDocFile(file, workspaceId))
  ) {
    throw new Error(
      "Select Project Docs from the current workspace to generate from.",
    );
  }

  const sections = [
    "Generate from the selected Project Docs.",
    `Read every selected document completely in the current workspace checkout before generating anything. The selected documents bound the request, not the number of files needed to satisfy it. Resolve the repository-local paths below using the workspace's repository mounts. Project Docs means docs/${workspaceId}/ in the docs repository; use its advertised project_docs alias when available. If an input is missing or unreadable, report that blocker rather than substituting a different document.`,
    "Selected documents:\n" +
      files
        .map((file) =>
          JSON.stringify({
            repository: file.repoName,
            repoId: file.repoId,
            path: file.path,
            documentRef: file.targetDocRef,
          }),
        )
        .join("\n"),
    "Carry out this request in this chat using your normal tools. Stay on the current workspace branch, preserve unrelated edits, and handle edits, validation, and any commits according to the workspace instructions. Produce only the outputs requested below, in order. Register document intent and maintain grounding as required by the workspace instructions.",
  ];

  const add = (title: string, instruction: string) =>
    sections.push(`## ${title}\n\n${instruction}`);
  if (options.plans.ontology || options.code !== "none") {
    add(
      "Update the plan of record (ontology)",
      "Reflect the selected requirements into the governing ontology first. Create or update the spec nodes that own each requirement and validate changed specs. Keep parent intent above implementation detail; generated behavior must follow the updated ontology.",
    );
  }
  if (options.plans.designDoc) {
    add(
      "Generate a design doc",
      "Write one self-contained HTML page at Project Docs/<feature-slug>-design.html describing goals, non-goals, proposed behavior and user flows, affected areas, alternatives, and risks. Include an Open Questions section. This section designs the change; implement it only if a build section below requests implementation.",
    );
  }
  if (options.plans.questionDoc) {
    add(
      "Generate a question answering doc",
      'Collect ambiguities, conflicting requirements, and open decisions into Project Docs/<feature-slug>-questions.md. Use the workspace\'s documented [Q<N>. Title] blocks and fenced g5-question schema: 1 payloads, with evidence and answer options, or the referenced form: create the questions with `g5d question create` and put a fenced g5-question-ref block {"path": "<set path>", "id": "<question id>"} in each [Q<N>. Title] block; never edit a .g5q.json file yourself, and treat an answer as a decision only when `g5d question list` reports decision: true. Follow the autonomy policy below; never mark an agent assumption as a human answer.',
    );
  }
  if (options.plans.implPlan) {
    add(
      "Generate an implementation plan",
      "Write Project Docs/<feature-slug>-plan.md with ordered phases, stable task IDs, scope and affected files, dependencies, risks and mitigations, and acceptance criteria. Planning does not authorize code changes unless the build section below requests them.",
    );
  }
  if (options.plans.testPlan) {
    add(
      "Generate a test plan",
      "Record real-user acceptance scenarios in the governing test contract, or a Project Docs test-plan document if ontology is not selected. Describe the failing state that proves each gap, the passing state that proves the behavior, and manual-only steps. Name the required evidence without claiming unexecuted tests passed.",
    );
  }
  if (options.plans.explainer.enabled) {
    const style = {
      concise: "Concise: executive-summary depth.",
      balanced: "Balanced: overview with selective detail.",
      indepth: "In-depth: a full walkthrough of the system and its main flows.",
    }[options.plans.explainer.style];
    const format =
      options.plans.explainer.format === "slides"
        ? "a self-contained HTML slide presentation"
        : "a self-contained HTML page";
    add(
      "Generate an explainer",
      `Create ${format} at Project Docs/explainers/<feature-slug>/<YYYY-MM-DD-HHMM>-<style-slug>.html in this session. Keep it under the current workspace's managed explainers folder so it appears in the Explainers section. Preserve earlier explainers; if the dated filename already exists, append a numeric suffix instead of overwriting it. Explain the proposed system to stakeholders: what it does, who uses it, and how the main flows work. ${style} Use the workspace's explainer authoring guidance and normal file tools, and link the saved artifact in your response.`,
    );
  }
  if (options.plans.mocks) {
    add(
      "Generate mocks",
      "Create clickable, self-contained HTML mockups at Project Docs/mocks/<surface>.html using the product's visual theme. Cover realistic empty, in-progress, error, and completed states. Mockups are review artifacts; they do not authorize production code changes.",
    );
  }
  if (options.code === "stubs") {
    add(
      "Build a working preview",
      "Implement a simplified but runnable vertical slice of the main flows from the updated ontology: real pages and endpoints with enough logic to click through end to end. Clearly label preview shortcuts and make them easy to replace. Validate the preview and do not claim it is production-ready.",
    );
  } else if (options.code === "full") {
    add(
      "Build the full implementation",
      "Implement the updated ontology completely with production code and tests, without stubs or placeholders. Use the appropriate debugging, feature implementation, or design workflow. Complete every required layer, maintain traceability, run relevant validation, and report concrete blockers or remaining gaps.",
    );
  } else {
    add(
      "Do not generate code",
      "Produce only the selected plans and documents. Do not create, edit, or delete source code, test code, configuration, or build files, and do not run code-modifying commands, even if the input documents request implementation. Record necessary code work in the plan or as a remaining gap instead.",
    );
  }
  // Match the visible modal's coupling: no code disables implementation reports;
  // coverage includes a review, and code requires ontology.
  if (options.code !== "none") {
    if (options.reports.review || options.reports.coverage) {
      add(
        "Post-generation review report",
        "Review every changed artifact using the workspace's code-review guidance. Fix confident findings and rerun affected checks. Save the post-fix report at Project Docs/reports/<feature-slug>-review.md with per-dimension quality assessments, implemented work, material decisions, evidence, and remaining gaps with severity and next actions.",
      );
      if (options.reports.coverage) {
        add(
          "Requirement coverage",
          "Include a requirement-coverage matrix in that same review report: source, required behavior, required layers, evidence, and implemented | partial | missing | blocked status. Name the exact gap for every incomplete row; do not create a separate coverage document.",
        );
      }
    }
    if (options.reports.statusOverview) {
      add(
        "Refresh the status overview",
        "Update the project status overview and feature catalog with the observed completion, production readiness, test coverage, and remaining gaps. Distinguish implemented work from executed validation.",
      );
    }
    add(
      "Maintain preview profiles",
      "In changed code repositories, maintain the workspace's preview profiles for detected runnable applications so the result can be previewed. Follow the repository's documented profile format; do not invent an application or mark an untested preview as working.",
    );
  }

  add(
    "Agent autonomy",
    {
      raise_all:
        "Raise every dubious requirement or decision through the normal chat question mechanism. Do not choose answers to ambiguous requirements yourself; wait for the user when their decision is required.",
      balanced:
        "Resolve routine choices from the documents and best judgment, recording material decisions. Ask the user when a choice is irreversible, changes a public contract, or contradicts the selected requirements.",
      autonomous:
        "Work as autonomously as you can. Resolve routine choices from authoritative evidence and record material assumptions. If required authority or input is unavailable, name the precise blocker and complete other eligible work without inventing approval.",
    }[options.autonomy],
  );
  add(
    "Completion",
    "Finish with links or paths to the generated artifacts, a concise account of changes and validation actually performed, and remaining gaps. Keep the full review in its saved file if requested. Continue subsequent discussion in this same chat; task progress and completion use the normal chat lifecycle.",
  );
  return sections.join("\n\n");
}
