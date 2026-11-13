//! Workspace-settings update contract.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorkspaceSettingsRequest {
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub max_spend_usd: Option<Option<f64>>,
    pub spend_period: Option<String>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub max_input_tokens: Option<Option<i64>>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub max_output_tokens: Option<Option<i64>>,
    pub idle_timeout_secs: Option<i32>,
    pub response_deadline_secs: Option<i32>,
    pub bounded_execution_slots: Option<i32>,
    pub permission_policy: Option<serde_json::Value>,
    pub agent_may_modify_other_workspaces: Option<bool>,
}

fn deserialize_double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_null_and_value_remain_distinct() {
        let absent: UpdateWorkspaceSettingsRequest = serde_json::from_str("{}").unwrap();
        let null: UpdateWorkspaceSettingsRequest =
            serde_json::from_str(r#"{"maxSpendUsd":null}"#).unwrap();
        let value: UpdateWorkspaceSettingsRequest =
            serde_json::from_str(r#"{"maxSpendUsd":12.5}"#).unwrap();
        assert_eq!(absent.max_spend_usd, None);
        assert_eq!(null.max_spend_usd, Some(None));
        assert_eq!(value.max_spend_usd, Some(Some(12.5)));
    }
}
