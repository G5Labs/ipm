use serde::{de::DeserializeOwned, Serialize};

use g5_contracts::error::G5Error;

/// JSONL parsing and serialization for intent ledgers resolved from a
/// repository snapshot. The ledger bytes themselves are read and published
/// through the Git boundary by the owning services; this type never touches
/// the filesystem.
pub struct JsonlStore;

impl JsonlStore {
    /// Parse strict JSONL bytes already resolved from a repository snapshot.
    pub fn parse_all_strict<T: DeserializeOwned>(
        content: &str,
        source_label: &str,
    ) -> Result<Vec<T>, G5Error> {
        let mut records = Vec::new();
        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let record =
                serde_json::from_str::<T>(trimmed).map_err(|error| G5Error::ValidationError {
                    details: serde_json::json!({
                        "path": source_label,
                        "line": line_num + 1,
                        "message": format!("Invalid JSONL record: {error}"),
                    }),
                })?;
            records.push(record);
        }
        Ok(records)
    }

    /// Serialize records to canonical JSONL bytes, one record per line.
    pub fn serialize_all<T: Serialize>(records: &[T]) -> Result<String, G5Error> {
        let mut content = String::new();
        for record in records {
            let line = serde_json::to_string(record).map_err(|e| G5Error::Internal {
                message: format!("Failed to serialize record: {e}"),
            })?;
            content.push_str(&line);
            content.push('\n');
        }
        Ok(content)
    }
}

/// Encode a workspace-relative file path into a JSONL file name.
///
/// Replaces path separators (`/`) with double underscore (`__`).
/// For chat sessions (no path separators), the name is used as-is.
pub fn encode_file_name(source_name: &str) -> String {
    source_name.replace('/', "__")
}

/// Decode a JSONL file stem back to a workspace-relative path.
///
/// Replaces double underscores (`__`) with path separators (`/`).
pub fn decode_file_name(encoded: &str) -> String {
    encoded.replace("__", "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_file_name() {
        assert_eq!(
            encode_file_name("src/services/task_service.rs"),
            "src__services__task_service.rs"
        );
        assert_eq!(
            encode_file_name("views/pm_views/autodev.pm_view.md"),
            "views__pm_views__autodev.pm_view.md"
        );
        assert_eq!(encode_file_name("abc-123"), "abc-123");
    }

    #[test]
    fn test_decode_file_name() {
        assert_eq!(
            decode_file_name("src__services__task_service.rs"),
            "src/services/task_service.rs"
        );
        assert_eq!(decode_file_name("abc-123"), "abc-123");
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let original = "views/pm_views/autodev.pm_view.md";
        assert_eq!(decode_file_name(&encode_file_name(original)), original);
    }
}
