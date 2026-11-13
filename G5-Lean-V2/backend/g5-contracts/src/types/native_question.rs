//! Positional owner answers survive redaction of native question/option text.
//! The API validates against the stored projection; the live adapter resolves
//! against its original provider input. No text is used as correlation identity.
use std::collections::HashSet;

use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Answer {
    selected_option_indexes: Vec<usize>,
    other_text: Option<String>,
}

/// Validate the complete positional reply and resolve each answer's value.
/// Only the live adapter may use the returned values as native provider input:
/// the API's copy of the question and option labels has already been redacted.
pub fn resolve_native_question_answers(
    payload: &Value,
    response: &Value,
) -> Result<Vec<String>, &'static str> {
    let object = response
        .as_object()
        .ok_or("validated response must be an object")?;
    let answers = object
        .get("g5AnswersByIndex")
        .and_then(Value::as_object)
        .filter(|_| object.len() == 1)
        .ok_or("question response must contain exactly one g5AnswersByIndex object")?;
    let questions = payload["questions"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or("native question payload has no questions")?;
    if answers.len() != questions.len() {
        return Err("answers must cover exactly the stored question positions");
    }
    questions
        .iter()
        .enumerate()
        .map(|(index, question)| {
            question["question"]
                .as_str()
                .filter(|v| !v.trim().is_empty())
                .ok_or("native question text is invalid")?;
            let options = question["options"]
                .as_array()
                .ok_or("native question options are invalid")?;
            let answer: Answer = serde_json::from_value(
                answers
                    .get(&index.to_string())
                    .cloned()
                    .ok_or("answer question position is missing")?,
            )
            .map_err(|_| "native answer must contain option indexes and optional text")?;
            if answer
                .other_text
                .as_ref()
                .is_some_and(|v| v.trim().is_empty())
            {
                return Err("native answer text must be nonempty");
            }
            let count =
                answer.selected_option_indexes.len() + usize::from(answer.other_text.is_some());
            if count == 0 || (!question["multiSelect"].as_bool().unwrap_or(false) && count != 1) {
                return Err("each native question requires an answer matching its selection mode");
            }
            let mut seen = HashSet::new();
            let mut parts = Vec::new();
            for option_index in answer.selected_option_indexes {
                if !seen.insert(option_index) {
                    return Err("native answer option positions must be unique");
                }
                let label = options
                    .get(option_index)
                    .and_then(|v| v.get("label"))
                    .and_then(Value::as_str)
                    .filter(|v| !v.trim().is_empty())
                    .ok_or("native answer option position is invalid")?;
                parts.push(label.to_owned());
            }
            if let Some(text) = answer.other_text {
                parts.push(text);
            }
            Ok(parts.join(", "))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload() -> Value {
        json!({"questions":[
            {"question":"Choose [REDACTED]","multiSelect":true,"options":[{"label":"[REDACTED]"},{"label":"[REDACTED]"}]},
            {"question":"Choose [REDACTED]","options":[{"label":"A"},{"label":"B"}]}
        ]})
    }

    #[test]
    fn colliding_display_text_keeps_positions_and_free_text() {
        let answers = json!({"g5AnswersByIndex":{
            "0":{"selectedOptionIndexes":[1,0],"otherText":"custom"},
            "1":{"selectedOptionIndexes":[],"otherText":" free text 🟦 "}
        }});
        assert_eq!(
            resolve_native_question_answers(&payload(), &answers).unwrap(),
            ["[REDACTED], [REDACTED], custom", " free text 🟦 "]
        );
    }

    #[test]
    fn invalid_positions_shapes_and_selection_modes_are_rejected() {
        for response in [
            json!({"answers":{"Choose [REDACTED]":"A"}}),
            json!({"g5AnswersByIndex":{}}),
            json!({"g5AnswersByIndex":{"0":{"selectedOptionIndexes":[0]}}}),
            json!({"g5AnswersByIndex":{"01":{},"1":{}}}),
            json!({"g5AnswersByIndex":{"0":{},"1":{},"2":{}}}),
        ] {
            assert!(
                resolve_native_question_answers(&payload(), &response).is_err(),
                "{response}"
            );
        }
        let duplicate_multi = json!({"g5AnswersByIndex":{
            "0":{"selectedOptionIndexes":[0,0]},"1":{"selectedOptionIndexes":[0]}
        }});
        assert_eq!(
            resolve_native_question_answers(&payload(), &duplicate_multi).unwrap_err(),
            "native answer option positions must be unique"
        );
        for answer in [
            json!(null),
            json!("A"),
            json!({}),
            json!({"selectedOptionIndexes":"0"}),
            json!({"selectedOptionIndexes":[-1]}),
            json!({"selectedOptionIndexes":[2]}),
            json!({"selectedOptionIndexes":[]}),
            json!({"selectedOptionIndexes":[0,0]}),
            json!({"selectedOptionIndexes":[0,1]}),
            json!({"selectedOptionIndexes":[0],"otherText":"custom"}),
            json!({"selectedOptionIndexes":[],"otherText":" "}),
            json!({"selectedOptionIndexes":[],"otherText":false}),
            json!({"selectedOptionIndexes":[0],"unknown":true}),
        ] {
            let response = json!({"g5AnswersByIndex":{
                "0":{"selectedOptionIndexes":[0]},"1":answer
            }});
            assert!(
                resolve_native_question_answers(&payload(), &response).is_err(),
                "{response}"
            );
        }
    }
}
