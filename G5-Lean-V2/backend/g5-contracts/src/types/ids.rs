//! Domain identity types: `G5Uuid`, typed ID newtypes, and `ComponentId`.
//!
//! `G5Uuid` is a newtype wrapper around `uuid::Uuid` providing UUID v7
//! time-ordered identifiers for all G5 entities.
//!
//! Typed ID newtypes (`WorkspaceId`, `UserId`, `OrgId`, etc.) wrap `Uuid`
//! to prevent accidental parameter swapping at call sites — the compiler
//! rejects `fn f(ws: WorkspaceId, user: UserId)` called as `f(user, ws)`.
//!
//! `ComponentId` is a validated, filesystem-derived hierarchical identifier
//! for ontology components (e.g., `auth/validator`, `core/engine/hashtable`).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::G5Error;

/// Canonical identity of one repository member. No branch UUID is allocated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRepoKey {
    pub org_id: uuid::Uuid,
    pub workspace_id: uuid::Uuid,
    pub repo_id: uuid::Uuid,
}

impl fmt::Display for WorkspaceRepoKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}/{}", self.org_id, self.workspace_id, self.repo_id)
    }
}

// ---------------------------------------------------------------------------
// G5Uuid
// ---------------------------------------------------------------------------

/// A newtype wrapper around [`uuid::Uuid`] providing a domain-specific UUID
/// type for all G5 entity identifiers. Uses UUID v7 for time-ordered IDs.
///
/// Serializes transparently as a plain UUID string in JSON (not wrapped in
/// an object) via `#[serde(transparent)]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct G5Uuid(uuid::Uuid);

impl G5Uuid {
    /// Creates a new UUID v7 (time-ordered).
    #[must_use]
    pub fn new() -> Self {
        G5Uuid(uuid::Uuid::now_v7())
    }

    /// Wraps an existing [`uuid::Uuid`].
    #[must_use]
    pub fn from_uuid(uuid: uuid::Uuid) -> Self {
        G5Uuid(uuid)
    }

    /// Borrows the inner [`uuid::Uuid`].
    #[must_use]
    pub fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }

    /// Consumes and returns the inner [`uuid::Uuid`].
    #[must_use]
    pub fn into_uuid(self) -> uuid::Uuid {
        self.0
    }

    /// Returns the nil UUID (all zeros), useful for testing.
    #[must_use]
    pub fn nil() -> Self {
        G5Uuid(uuid::Uuid::nil())
    }
}

impl Default for G5Uuid {
    /// Generates a new UUID v7 via [`uuid::Uuid::now_v7()`].
    fn default() -> Self {
        G5Uuid::new()
    }
}

impl fmt::Display for G5Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for G5Uuid {
    type Err = G5Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let uuid = uuid::Uuid::parse_str(s).map_err(|e| G5Error::ValidationError {
            details: serde_json::json!({
                "field": "uuid",
                "error": format!("Invalid UUID: {}", e),
            }),
        })?;
        Ok(G5Uuid(uuid))
    }
}

impl From<uuid::Uuid> for G5Uuid {
    fn from(uuid: uuid::Uuid) -> Self {
        G5Uuid(uuid)
    }
}

impl From<G5Uuid> for uuid::Uuid {
    fn from(g5: G5Uuid) -> Self {
        g5.0
    }
}

// Optional sqlx integration
#[cfg(feature = "sqlx")]
impl sqlx::Type<sqlx::Postgres> for G5Uuid {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <uuid::Uuid as sqlx::Type<sqlx::Postgres>>::type_info()
    }
}

#[cfg(feature = "sqlx")]
impl<'q> sqlx::Encode<'q, sqlx::Postgres> for G5Uuid {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Postgres as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        <uuid::Uuid as sqlx::Encode<'q, sqlx::Postgres>>::encode_by_ref(&self.0, buf)
    }
}

#[cfg(feature = "sqlx")]
impl<'r> sqlx::Decode<'r, sqlx::Postgres> for G5Uuid {
    fn decode(
        value: <sqlx::Postgres as sqlx::Database>::ValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let uuid = <uuid::Uuid as sqlx::Decode<'r, sqlx::Postgres>>::decode(value)?;
        Ok(G5Uuid(uuid))
    }
}

// ---------------------------------------------------------------------------
// Typed ID newtypes — compile-time parameter-swap prevention
// ---------------------------------------------------------------------------

/// Generates a strongly-typed ID newtype wrapping `uuid::Uuid`.
///
/// Each generated type gets: `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`,
/// `Hash`, `Serialize` (transparent), `Deserialize` (transparent), `Display`,
/// `FromStr`, `From<Uuid>`, `From<Self> -> Uuid`, and optional `sqlx` impls.
macro_rules! define_id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(uuid::Uuid);

        impl $name {
            /// Generates a new UUID v7 (time-ordered).
            #[must_use]
            pub fn new() -> Self {
                Self(uuid::Uuid::now_v7())
            }

            /// Wraps an existing [`Uuid`](uuid::Uuid).
            #[must_use]
            pub const fn from_uuid(uuid: uuid::Uuid) -> Self {
                Self(uuid)
            }

            /// Borrows the inner [`Uuid`](uuid::Uuid).
            #[must_use]
            pub const fn as_uuid(&self) -> &uuid::Uuid {
                &self.0
            }

            /// Consumes and returns the inner [`Uuid`](uuid::Uuid).
            #[must_use]
            pub const fn into_uuid(self) -> uuid::Uuid {
                self.0
            }

            /// Returns the nil UUID (all zeros), useful for testing.
            #[must_use]
            pub const fn nil() -> Self {
                Self(uuid::Uuid::nil())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl FromStr for $name {
            type Err = G5Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let uuid = uuid::Uuid::parse_str(s).map_err(|e| G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": stringify!($name),
                        "error": format!("Invalid UUID: {}", e),
                    }),
                })?;
                Ok(Self(uuid))
            }
        }

        impl From<uuid::Uuid> for $name {
            fn from(uuid: uuid::Uuid) -> Self {
                Self(uuid)
            }
        }

        impl From<$name> for uuid::Uuid {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl From<G5Uuid> for $name {
            fn from(g5: G5Uuid) -> Self {
                Self(*g5.as_uuid())
            }
        }

        impl From<$name> for G5Uuid {
            fn from(id: $name) -> Self {
                G5Uuid::from_uuid(id.0)
            }
        }

        #[cfg(feature = "sqlx")]
        impl sqlx::Type<sqlx::Postgres> for $name {
            fn type_info() -> sqlx::postgres::PgTypeInfo {
                <uuid::Uuid as sqlx::Type<sqlx::Postgres>>::type_info()
            }
        }

        #[cfg(feature = "sqlx")]
        impl<'q> sqlx::Encode<'q, sqlx::Postgres> for $name {
            fn encode_by_ref(
                &self,
                buf: &mut <sqlx::Postgres as sqlx::Database>::ArgumentBuffer<'q>,
            ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
                <uuid::Uuid as sqlx::Encode<'q, sqlx::Postgres>>::encode_by_ref(&self.0, buf)
            }
        }

        #[cfg(feature = "sqlx")]
        impl<'r> sqlx::Decode<'r, sqlx::Postgres> for $name {
            fn decode(
                value: <sqlx::Postgres as sqlx::Database>::ValueRef<'r>,
            ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
                let uuid = <uuid::Uuid as sqlx::Decode<'r, sqlx::Postgres>>::decode(value)?;
                Ok(Self(uuid))
            }
        }
    };
}

define_id_type! {
    /// Uniquely identifies a workspace (a top-level project container).
    WorkspaceId
}

define_id_type! {
    /// Uniquely identifies a user account.
    UserId
}

define_id_type! {
    /// Uniquely identifies an organization.
    OrgId
}

define_id_type! {
    /// Uniquely identifies a task (a unit of work within a workspace).
    TaskId
}

define_id_type! {
    /// Uniquely identifies a session (a user's active connection context).
    SessionId
}

define_id_type! {
    /// Uniquely identifies a repository (a git repo managed by G5).
    RepoId
}

define_id_type! {
    /// Uniquely identifies a workspace branch.
    BranchId
}

define_id_type! {
    /// Uniquely identifies a worker process.
    WorkerId
}

define_id_type! {
    /// Uniquely identifies a message in the chat system.
    MessageId
}

define_id_type! {
    /// Uniquely identifies a comment.
    CommentId
}

define_id_type! {
    /// Uniquely identifies a node in the ontology graph.
    NodeId
}

// ---------------------------------------------------------------------------
// ComponentId
// ---------------------------------------------------------------------------

/// A validated, filesystem-derived hierarchical identifier for ontology
/// components (e.g., `auth/validator`, `core/engine/hashtable`).
///
/// Stored as a `String` internally. Deserialization accepts any non-empty
/// string (to support file-path identifiers from downlift tasks); use
/// `ComponentId::new()` for strict validation of semantic component IDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ComponentId(String);

impl ComponentId {
    /// Validates and constructs a new `ComponentId`.
    ///
    /// # Validation Rules
    ///
    /// 1. Must not be empty
    /// 2. Must not contain backslashes (`\`)
    /// 3. Must not contain file extensions (no dots in any segment)
    /// 4. Must be lowercase (no uppercase ASCII characters)
    /// 5. Must not have leading or trailing forward slashes
    /// 6. Must not contain consecutive forward slashes (`//`)
    /// 7. Segments between slashes must be non-empty
    /// 8. Each segment must match `[a-z0-9_-]+`
    pub fn new(id: impl Into<String>) -> Result<Self, G5Error> {
        let id = id.into();
        Self::validate(&id)?;
        Ok(ComponentId(id))
    }

    /// Construct a `ComponentId` without validation.
    ///
    /// Use this for identifiers that are raw file paths (e.g. downlift
    /// tasks targeting source files like `src/components/Editor.tsx`).
    /// These contain dots and uppercase characters that would fail the
    /// normal validation rules, but are legitimate task targets.
    pub fn new_unchecked(id: impl Into<String>) -> Self {
        ComponentId(id.into())
    }

    /// Borrows the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Splits on `/` and iterates over segments.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }

    /// Returns the parent ID (everything before the last `/`), or `None`
    /// if this is a root-level ID (single segment).
    #[must_use]
    pub fn parent(&self) -> Option<ComponentId> {
        self.0
            .rfind('/')
            .map(|pos| ComponentId(self.0[..pos].to_string()))
    }

    /// Returns the number of segments (e.g., `auth/validator` → 2).
    #[must_use]
    pub fn depth(&self) -> usize {
        self.0.split('/').count()
    }

    /// Checks if `self` is a prefix of `other` (i.e., `self` is an ancestor
    /// of `other` in the hierarchy).
    #[must_use]
    pub fn is_ancestor_of(&self, other: &ComponentId) -> bool {
        if self.0.len() >= other.0.len() {
            return false;
        }
        other.0.starts_with(&self.0) && other.0.as_bytes()[self.0.len()] == b'/'
    }

    /// Internal validation logic.
    fn validate(id: &str) -> Result<(), G5Error> {
        if id.is_empty() {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "component_id",
                    "error": "Component ID must not be empty",
                }),
            });
        }

        if id.contains('\\') {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "component_id",
                    "error": "Component ID must not contain backslashes",
                }),
            });
        }

        if id.starts_with('/') || id.ends_with('/') {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "component_id",
                    "error": "Component ID must not have leading or trailing slashes",
                }),
            });
        }

        if id.contains("//") {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "component_id",
                    "error": "Component ID must not contain consecutive slashes",
                }),
            });
        }

        if id.bytes().any(|b| b.is_ascii_uppercase()) {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "component_id",
                    "error": "Component ID must be lowercase",
                }),
            });
        }

        for segment in id.split('/') {
            if segment.is_empty() {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "component_id",
                        "error": "Component ID segments must not be empty",
                    }),
                });
            }

            if segment.contains('.') {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "component_id",
                        "error": format!("Component ID segment '{}' must not contain dots (file extensions)", segment),
                    }),
                });
            }

            // Each segment must match [a-z0-9_-]+
            if !segment
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
            {
                return Err(G5Error::ValidationError {
                    details: serde_json::json!({
                        "field": "component_id",
                        "error": format!("Component ID segment '{}' contains invalid characters (must match [a-z0-9_-]+)", segment),
                    }),
                });
            }
        }

        Ok(())
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for ComponentId {
    type Error = G5Error;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        // Lenient deserialization: accept any non-empty string so file-path
        // identifiers from downlift tasks can round-trip through serde without
        // applying strict semantic component-id validation. Empty identifiers
        // are still invalid; callers that intentionally need unchecked raw
        // identifiers should use `ComponentId::new_unchecked()`.
        if s.is_empty() {
            return Err(G5Error::ValidationError {
                details: serde_json::json!({
                    "field": "component_id",
                    "error": "Component ID must not be empty",
                }),
            });
        }
        Ok(ComponentId(s))
    }
}

impl From<ComponentId> for String {
    fn from(id: ComponentId) -> Self {
        id.0
    }
}

impl AsRef<str> for ComponentId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
impl G5Uuid {
    /// Returns a deterministic UUID for test assertions.
    #[must_use]
    pub fn test_fixed() -> Self {
        G5Uuid::from_uuid(uuid::Uuid::from_bytes([
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10,
        ]))
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- G5Uuid tests --

    #[test]
    fn test_g5uuid_new_is_v7() {
        let id = G5Uuid::new();
        // UUID v7 has version nibble = 7 in the 13th hex digit
        let s = id.to_string();
        assert_eq!(s.chars().nth(14), Some('7'), "UUID should be v7");
    }

    #[test]
    fn test_g5uuid_nil() {
        let id = G5Uuid::nil();
        assert_eq!(id.to_string(), "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn test_g5uuid_from_uuid_roundtrip() {
        let raw = uuid::Uuid::now_v7();
        let g5 = G5Uuid::from_uuid(raw);
        assert_eq!(*g5.as_uuid(), raw);
        assert_eq!(g5.into_uuid(), raw);
    }

    #[test]
    fn test_g5uuid_from_into_uuid() {
        let raw = uuid::Uuid::now_v7();
        let g5: G5Uuid = raw.into();
        let back: uuid::Uuid = g5.into();
        assert_eq!(raw, back);
    }

    #[test]
    fn test_g5uuid_display() {
        let id = G5Uuid::test_fixed();
        let s = id.to_string();
        assert!(s.contains('-'), "Display should use hyphenated format");
        assert_eq!(s.len(), 36);
    }

    #[test]
    fn test_g5uuid_from_str() {
        let id = G5Uuid::test_fixed();
        let s = id.to_string();
        let parsed: G5Uuid = s.parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_g5uuid_from_str_invalid() {
        let result: Result<G5Uuid, _> = "not-a-uuid".parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_g5uuid_serde_transparent() {
        let id = G5Uuid::test_fixed();
        let json = serde_json::to_string(&id).unwrap();
        // Should be a plain string, not an object
        assert!(json.starts_with('"'));
        assert!(json.ends_with('"'));
        assert!(!json.contains('{'));

        let parsed: G5Uuid = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_g5uuid_default_is_v7() {
        let id = G5Uuid::default();
        let s = id.to_string();
        assert_eq!(s.chars().nth(14), Some('7'));
    }

    #[test]
    fn test_g5uuid_hash_eq() {
        let id1 = G5Uuid::test_fixed();
        let id2 = G5Uuid::test_fixed();
        assert_eq!(id1, id2);

        let mut set = std::collections::HashSet::new();
        set.insert(id1);
        assert!(set.contains(&id2));
    }

    // -- ComponentId tests --

    #[test]
    fn test_component_id_valid() {
        assert!(ComponentId::new("auth/validator").is_ok());
        assert!(ComponentId::new("core").is_ok());
        assert!(ComponentId::new("core/engine/hashtable").is_ok());
        assert!(ComponentId::new("my-component").is_ok());
        assert!(ComponentId::new("my_component").is_ok());
        assert!(ComponentId::new("abc123").is_ok());
    }

    #[test]
    fn test_component_id_empty() {
        assert!(ComponentId::new("").is_err());
    }

    #[test]
    fn test_component_id_backslash() {
        assert!(ComponentId::new("auth\\validator").is_err());
    }

    #[test]
    fn test_component_id_file_extension() {
        assert!(ComponentId::new("auth/validator.rs").is_err());
        assert!(ComponentId::new("file.py").is_err());
        assert!(ComponentId::new("a/b.md/c").is_err());
    }

    #[test]
    fn test_component_id_uppercase() {
        assert!(ComponentId::new("Auth/Validator").is_err());
        assert!(ComponentId::new("CORE").is_err());
    }

    #[test]
    fn test_component_id_leading_slash() {
        assert!(ComponentId::new("/auth/validator").is_err());
    }

    #[test]
    fn test_component_id_trailing_slash() {
        assert!(ComponentId::new("auth/validator/").is_err());
    }

    #[test]
    fn test_component_id_consecutive_slashes() {
        assert!(ComponentId::new("auth//validator").is_err());
    }

    #[test]
    fn test_component_id_as_str() {
        let id = ComponentId::new("auth/validator").unwrap();
        assert_eq!(id.as_str(), "auth/validator");
    }

    #[test]
    fn test_component_id_segments() {
        let id = ComponentId::new("auth/validator/rules").unwrap();
        let segs: Vec<&str> = id.segments().collect();
        assert_eq!(segs, vec!["auth", "validator", "rules"]);
    }

    #[test]
    fn test_component_id_parent() {
        let id = ComponentId::new("auth/validator/rules").unwrap();
        let parent = id.parent().unwrap();
        assert_eq!(parent.as_str(), "auth/validator");

        let grandparent = parent.parent().unwrap();
        assert_eq!(grandparent.as_str(), "auth");

        assert!(grandparent.parent().is_none());
    }

    #[test]
    fn test_component_id_depth() {
        assert_eq!(ComponentId::new("core").unwrap().depth(), 1);
        assert_eq!(ComponentId::new("auth/validator").unwrap().depth(), 2);
        assert_eq!(ComponentId::new("a/b/c/d").unwrap().depth(), 4);
    }

    #[test]
    fn test_component_id_is_ancestor_of() {
        let auth = ComponentId::new("auth").unwrap();
        let validator = ComponentId::new("auth/validator").unwrap();
        let rules = ComponentId::new("auth/validator/rules").unwrap();
        let other = ComponentId::new("other/thing").unwrap();

        assert!(auth.is_ancestor_of(&validator));
        assert!(auth.is_ancestor_of(&rules));
        assert!(validator.is_ancestor_of(&rules));
        assert!(!validator.is_ancestor_of(&auth));
        assert!(!auth.is_ancestor_of(&other));
        assert!(!auth.is_ancestor_of(&auth)); // not ancestor of itself
    }

    #[test]
    fn test_component_id_display() {
        let id = ComponentId::new("auth/validator").unwrap();
        assert_eq!(id.to_string(), "auth/validator");
    }

    #[test]
    fn test_component_id_try_from_string() {
        let id: ComponentId = "auth/validator".to_string().try_into().unwrap();
        assert_eq!(id.as_str(), "auth/validator");

        let bad: Result<ComponentId, _> = "".to_string().try_into();
        assert!(bad.is_err());
    }

    #[test]
    fn test_component_id_into_string() {
        let id = ComponentId::new("auth/validator").unwrap();
        let s: String = id.into();
        assert_eq!(s, "auth/validator");
    }

    #[test]
    fn test_component_id_as_ref_str() {
        let id = ComponentId::new("auth/validator").unwrap();
        let s: &str = id.as_ref();
        assert_eq!(s, "auth/validator");
    }

    #[test]
    fn test_component_id_serde_roundtrip() {
        let id = ComponentId::new("auth/validator").unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"auth/validator\"");

        let parsed: ComponentId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn test_component_id_serde_lenient_accepts_file_paths() {
        // Lenient deserialization accepts file paths (dots, uppercase) for
        // downlift tasks; strict validation is via ComponentId::new().
        let result: Result<ComponentId, _> = serde_json::from_str("\"g5-common/src/config.rs\"");
        assert!(result.is_ok());

        let result: Result<ComponentId, _> = serde_json::from_str("\"Auth/Validator\"");
        assert!(result.is_ok());

        // Empty string is still rejected.
        let result: Result<ComponentId, _> = serde_json::from_str("\"\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_component_id_hash_eq() {
        let id1 = ComponentId::new("auth/validator").unwrap();
        let id2 = ComponentId::new("auth/validator").unwrap();
        assert_eq!(id1, id2);

        let mut set = std::collections::HashSet::new();
        set.insert(id1);
        assert!(set.contains(&id2));
    }
}
