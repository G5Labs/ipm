//! `g5d question` — the workspace agent's access to referenced questions
//! (`g5.documents.question.agent-access`).
//!
//! The commands call the incarnation-authenticated internal routes of the
//! question set service through the incarnation client, which owns the
//! worker protocol. The server records every write with `agent`
//! provenance: an answer lands as a proposal the owner confirms, and the
//! write changes the live set file only — the agent's own commit and push
//! publish it. The command fails outside a workspace VM.

use std::io::Read;

use g5_contracts::error::G5Error;
use serde_json::{json, Value};

use super::{
    stable_context, GlobalFlags, QuestionAnswerArgs, QuestionArgs, QuestionCommand,
    QuestionCreateArgs,
};

pub async fn execute(args: &QuestionArgs, global: &GlobalFlags) -> Result<(), G5Error> {
    let workspace_id = workspace_id_from(|key| std::env::var(key).ok()).ok_or_else(|| {
        G5Error::Config(
            "`g5d question` runs inside a G5 workspace VM: workspace identity and \
             credentials are not available here."
                .to_string(),
        )
    })?;
    let client = stable_context::require_worker_client(global).map_err(|_| {
        G5Error::Config(
            "`g5d question` needs workspace credentials; run it from the \
             agent's workspace VM."
                .to_string(),
        )
    })?;
    let response = match &args.command {
        QuestionCommand::List(list) => {
            client
                .list_question_sets(&workspace_id, list.path.as_deref())
                .await
        }
        QuestionCommand::Create(create) => {
            client
                .create_question_set(&workspace_id, &create_body(create)?)
                .await
        }
        QuestionCommand::Answer(answer) => {
            client
                .answer_question(&workspace_id, &answer_body(answer)?)
                .await
        }
    }
    .map_err(explain_conflict)?;
    if global.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&response).unwrap_or_default()
        );
    } else {
        print!(
            "{}",
            match &args.command {
                QuestionCommand::List(_) => render_sets(&response["data"]["sets"]),
                QuestionCommand::Create(_) => render_created(&response["data"]),
                QuestionCommand::Answer(_) => render_answered(&response["data"]),
            }
        );
    }
    Ok(())
}

/// The workspace this VM runs, as its dispatcher exports it.
fn workspace_id_from(get: impl Fn(&str) -> Option<String>) -> Option<String> {
    get("G5D_WORKSPACE_ID")
        .map(|id| id.trim().to_string())
        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
}

/// A moved revision returns the current entry: show it so the agent re-reads
/// and decides again instead of retrying the same command.
fn explain_conflict(error: G5Error) -> G5Error {
    let G5Error::QuestionConflict { details } = error else {
        return error;
    };
    if let Some(current) = details.get("current") {
        eprint!("Current entry:\n{}", render_entry(current));
    }
    G5Error::Conflict {
        reason: format!(
            "{} ({})",
            details["code"].as_str().unwrap_or("question-conflict"),
            details["message"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!(
                    "question moved to revision {}; re-read it with `g5d question list`",
                    details["currentRevision"]
                ))
        ),
    }
}

/// The idempotency key a retried answer replays with: the same set,
/// question and expected revision give the same key.
pub fn default_answer_key(path: &str, question_id: &str, expected_revision: u64) -> String {
    let set = path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".g5q.json"))
        .unwrap_or(path);
    format!("agent:{set}:{question_id}:{expected_revision}")
}

fn answer_body(args: &QuestionAnswerArgs) -> Result<Value, G5Error> {
    let text = args.text.clone().unwrap_or_default();
    if args.options.is_empty() && text.trim().is_empty() {
        return Err(G5Error::ValidationError {
            details: json!({"field": "answer", "error": "pass --option and/or --text"}),
        });
    }
    let key = args.idempotency_key.clone().unwrap_or_else(|| {
        default_answer_key(&args.path, &args.question_id, args.expected_revision)
    });
    Ok(json!({
        "path": args.path,
        "questionId": args.question_id,
        "expectedRevision": args.expected_revision,
        "idempotencyKey": key,
        "selectedOptionIds": args.options,
        "text": text,
        "rationale": args.rationale.clone().unwrap_or_default(),
    }))
}

fn create_body(args: &QuestionCreateArgs) -> Result<Value, G5Error> {
    let text = if args.file.as_os_str() == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| G5Error::Internal {
                message: format!("failed to read stdin: {e}"),
            })?;
        text
    } else {
        std::fs::read_to_string(&args.file).map_err(|e| G5Error::Internal {
            message: format!("failed to read {}: {e}", args.file.display()),
        })?
    };
    let questions: Value = serde_json::from_str(&text).map_err(|e| G5Error::ValidationError {
        details: json!({"field": "file", "error": format!("questions file is not JSON: {e}")}),
    })?;
    let questions = match questions {
        Value::Array(_) => questions,
        Value::Object(mut object) => object.remove("questions").unwrap_or(Value::Null),
        _ => Value::Null,
    };
    if !questions.is_array() {
        return Err(G5Error::ValidationError {
            details: json!({"field": "file", "error": "expected a JSON array of questions (or {\"questions\": [...]})"}),
        });
    }
    Ok(json!({"setId": args.set, "title": args.title, "questions": questions}))
}

fn render_entry(entry: &Value) -> String {
    let mut out = format!(
        "  #{} {}  [{}] revision {}  decision: {}\n    {}\n",
        entry["displayNumber"],
        entry["id"].as_str().unwrap_or(""),
        entry["status"].as_str().unwrap_or(""),
        entry["revision"],
        entry["decision"].as_bool().unwrap_or(false),
        entry["prompt"]
            .as_str()
            .unwrap_or("")
            .replace('\n', "\n    "),
    );
    for option in entry["options"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "    - {} ({}){}\n",
            option["label"].as_str().unwrap_or(""),
            option["id"].as_str().unwrap_or(""),
            if option["recommended"] == true {
                " [recommended]"
            } else {
                ""
            }
        ));
    }
    let answer = &entry["answer"];
    if entry["status"] != "open" {
        let selected = answer["selectedOptionIds"]
            .as_array()
            .map(|ids| {
                ids.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        out.push_str(&format!(
            "    answer by {} {}: {}{}\n",
            answer["answeredBy"]["kind"].as_str().unwrap_or("?"),
            answer["answeredBy"]["id"].as_str().unwrap_or(""),
            selected,
            answer["text"]
                .as_str()
                .filter(|text| !text.is_empty())
                .map(|text| format!(" \"{text}\""))
                .unwrap_or_default(),
        ));
        let at = answer["answeredAtRevision"].as_u64();
        if at.is_some_and(|at| Some(at) != entry["revision"].as_u64()) {
            out.push_str(&format!(
                "    answered against revision {}\n",
                at.unwrap_or_default()
            ));
        }
    }
    out
}

fn render_set(set: &Value) -> String {
    let mut out = format!(
        "{}  {}  ({})\n",
        set["path"].as_str().unwrap_or(""),
        set["title"].as_str().unwrap_or(""),
        set["source"].as_str().unwrap_or(""),
    );
    if let Some(invalid) = set.get("invalid").filter(|value| !value.is_null()) {
        out.push_str(&format!(
            "  not a valid question set ({}): {}\n",
            invalid["code"].as_str().unwrap_or(""),
            invalid["message"].as_str().unwrap_or("")
        ));
        return out;
    }
    for entry in set["questions"].as_array().into_iter().flatten() {
        out.push_str(&render_entry(entry));
    }
    out
}

fn render_sets(sets: &Value) -> String {
    let sets = sets.as_array().cloned().unwrap_or_default();
    if sets.is_empty() {
        return "No question sets.\n".to_string();
    }
    sets.iter().map(render_set).collect()
}

fn render_created(data: &Value) -> String {
    let ids = data["questionIds"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n  ")
        })
        .unwrap_or_default();
    format!(
        "{} {} (live; commit and push it with your work)\nquestion ids:\n  {ids}\nreference fence body: {{\"path\": \"{}\", \"id\": \"<question id>\"}}\n",
        if data["created"] == true { "Created" } else { "Updated" },
        data["set"]["path"].as_str().unwrap_or(""),
        data["set"]["path"].as_str().unwrap_or(""),
    )
}

fn render_answered(data: &Value) -> String {
    format!(
        "{} proposal on {} (revision {}; the owner confirms it){}\n",
        if data["replayed"] == true {
            "Replayed"
        } else {
            "Recorded"
        },
        data["entry"]["id"].as_str().unwrap_or(""),
        data["entry"]["revision"],
        if data["publication"]["status"] == "live" {
            "; commit and push the set file with your work"
        } else {
            ""
        },
    )
}

/// Parse a `--path` argument as given: the repository-relative set path.
pub fn parse_set_path(value: &str) -> Result<String, String> {
    if g5_contracts::types::question_set::is_question_set_path(value) {
        Ok(value.to_string())
    } else {
        Err("expected docs/<workspace>/questions/<set>.g5q.json".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Command, QuestionListArgs};
    use clap::Parser;

    #[test]
    fn retried_answers_share_one_key() {
        assert_eq!(
            default_answer_key("docs/ws/questions/plan.g5q.json", "pick-0123abcd", 3),
            "agent:plan:pick-0123abcd:3"
        );
    }

    #[test]
    fn the_command_refuses_to_run_outside_a_workspace_vm() {
        assert_eq!(workspace_id_from(|_| None), None);
        assert_eq!(workspace_id_from(|_| Some("nope".into())), None);
        let id = "9a9f7c64-839b-4b07-bf9c-95b080a4c316";
        assert_eq!(workspace_id_from(|_| Some(id.into())), Some(id.into()));
    }

    #[test]
    fn subcommands_parse() {
        let parse = |args: &[&str]| match Cli::try_parse_from(args).unwrap().command {
            Command::Question(question) => question.command,
            other => panic!("parsed {other:?}"),
        };
        assert!(matches!(
            parse(&["g5d", "question", "list"]),
            QuestionCommand::List(QuestionListArgs { path: None })
        ));
        let QuestionCommand::Answer(answer) = parse(&[
            "g5d",
            "question",
            "answer",
            "--path",
            "docs/ws/questions/plan.g5q.json",
            "--id",
            "pick-0123abcd",
            "--expected-revision",
            "2",
            "--option",
            "a",
            "--option",
            "b",
        ]) else {
            panic!("answer");
        };
        let body = answer_body(&answer).unwrap();
        assert_eq!(body["selectedOptionIds"], json!(["a", "b"]));
        assert_eq!(body["idempotencyKey"], "agent:plan:pick-0123abcd:2");
        assert!(body.get("answeredBy").is_none());
        assert!(Cli::try_parse_from([
            "g5d",
            "question",
            "answer",
            "--path",
            "docs/../questions/plan.g5q.json",
            "--id",
            "q",
            "--expected-revision",
            "1",
        ])
        .is_err());
        assert!(answer_body(&QuestionAnswerArgs {
            path: "docs/ws/questions/plan.g5q.json".into(),
            question_id: "q".into(),
            expected_revision: 1,
            options: Vec::new(),
            text: None,
            rationale: None,
            idempotency_key: None,
        })
        .is_err());
    }

    #[test]
    fn entries_render_decisions_and_earlier_revisions() {
        let rendered = render_entry(&json!({
            "displayNumber": 1,
            "id": "pick",
            "status": "answered",
            "revision": 4,
            "decision": true,
            "prompt": "Pick?",
            "options": [{"id": "a", "label": "A", "recommended": true}],
            "answer": {"selectedOptionIds": ["a"], "text": "", "answeredBy": {"kind": "user", "id": "u"}, "answeredAtRevision": 2},
        }));
        assert!(rendered.contains("decision: true"));
        assert!(rendered.contains("[recommended]"));
        assert!(rendered.contains("answered against revision 2"));
    }
}
