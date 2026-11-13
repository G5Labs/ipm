//! Canonical execution identity.
//!
//! Two forms (Agent API §4.2), both filesystem-safe scalars:
//!
//! ```text
//! task.<task_uuid>.<positive_attempt>.<vm_uuid>
//! operation.<operation_uuid>.1.<vm_uuid>
//! ```
//!
//! The dispatcher treats the token as **opaque**: it parses the shape so that
//! malformed or path-escaping identities are refused before they reach the
//! filesystem, and it never derives authorization from any embedded field. The
//! incarnation fence is the request context's `expected_vm_id`/`expected_epoch`,
//! checked separately, not the UUID spelled inside this string.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecKind {
    Task,
    Operation,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExecId(String);

impl ExecId {
    pub fn parse(raw: &str) -> Option<Self> {
        // Bound the token before any allocation or filesystem use.
        if raw.is_empty() || raw.len() > 160 {
            return None;
        }
        let mut parts = raw.split('.');
        let kind = parts.next()?;
        let id = parts.next()?;
        let attempt = parts.next()?;
        let vm = parts.next()?;
        if parts.next().is_some() {
            return None;
        }
        let kind = match kind {
            "task" => ExecKind::Task,
            "operation" => ExecKind::Operation,
            _ => return None,
        };
        if !is_canonical_uuid(id) || !is_canonical_uuid(vm) {
            return None;
        }
        if !is_positive_decimal(attempt) {
            return None;
        }
        // Task-less operations are single-attempt by contract.
        if kind == ExecKind::Operation && attempt != "1" {
            return None;
        }
        Some(Self(raw.to_owned()))
    }

    pub fn kind(&self) -> ExecKind {
        if self.0.starts_with("task.") {
            ExecKind::Task
        } else {
            ExecKind::Operation
        }
    }

    /// The task or operation segment (`<uuid>`) the identity names; for a
    /// task execution this is the task id every attempt shares.
    pub fn entity_id(&self) -> &str {
        self.0.split('.').nth(1).unwrap_or_default()
    }

    /// The incarnation segment (`<vm_uuid>`) the identity was minted for.
    pub fn vm_id(&self) -> &str {
        self.0.rsplit('.').next().unwrap_or_default()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The on-disk name for this execution's journal and disposition entries.
    /// Parsing already guarantees the token contains only lowercase hex,
    /// digits, hyphens and dots, so it cannot traverse or escape.
    pub fn file_name(&self) -> String {
        self.0.clone()
    }

    pub fn from_file_name(name: &str) -> Option<Self> {
        Self::parse(name)
    }
}

impl fmt::Display for ExecId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Lowercase hyphenated UUID exactly as the API mints it. Uppercase, braced,
/// urn-prefixed or simple forms are refused so one logical execution cannot
/// acquire two journal files.
fn is_canonical_uuid(value: &str) -> bool {
    if value.len() != 36 {
        return false;
    }
    value.bytes().enumerate().all(|(index, byte)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            byte == b'-'
        } else {
            byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
        }
    })
}

/// Positive decimal without leading zeros.
fn is_positive_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 9
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && !value.starts_with('0')
}

#[cfg(test)]
mod tests {
    use super::*;

    const VM: &str = "0199c0de-0000-7000-8000-00000000abcd";
    const ID: &str = "0199c0de-1111-7000-8000-00000000beef";

    #[test]
    fn accepts_both_canonical_forms() {
        assert!(ExecId::parse(&format!("task.{ID}.1.{VM}")).is_some());
        assert!(ExecId::parse(&format!("task.{ID}.12.{VM}")).is_some());
        assert!(ExecId::parse(&format!("operation.{ID}.1.{VM}")).is_some());
    }

    #[test]
    fn rejects_cross_kind_and_malformed_identities() {
        // An operation is single-attempt by contract.
        assert!(ExecId::parse(&format!("operation.{ID}.2.{VM}")).is_none());
        // Unknown kind, wrong arity, leading zero, uppercase UUID.
        assert!(ExecId::parse(&format!("job.{ID}.1.{VM}")).is_none());
        assert!(ExecId::parse(&format!("task.{ID}.1")).is_none());
        assert!(ExecId::parse(&format!("task.{ID}.01.{VM}")).is_none());
        assert!(ExecId::parse(&format!("task.{}.1.{VM}", ID.to_uppercase())).is_none());
        assert!(ExecId::parse(&format!("task.{ID}.0.{VM}")).is_none());
    }

    #[test]
    fn rejects_path_traversal_and_separators() {
        for raw in [
            "task.../../etc/passwd.1.x",
            "../../escape",
            "task./.1./",
            "task.0199c0de-0000-7000-8000-00000000abcd.1./etc",
        ] {
            assert!(ExecId::parse(raw).is_none(), "accepted {raw}");
        }
    }

    #[test]
    fn file_name_is_the_opaque_token() {
        let exec = ExecId::parse(&format!("task.{ID}.3.{VM}")).unwrap();
        assert_eq!(exec.file_name(), format!("task.{ID}.3.{VM}"));
        assert_eq!(exec.kind(), ExecKind::Task);
    }
}
