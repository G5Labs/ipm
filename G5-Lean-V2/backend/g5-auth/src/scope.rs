//! Statically-typed OAuth scope enum.
//!
//! Every authorization scope used across the system MUST be a variant of
//! [`Scope`]. This prevents typo-driven authorization bugs and ensures
//! compile-time verification of scope references.
//!
//! Wire format (JSON / JWT / Postgres) uses the `resource:action` string
//! representation (e.g., `"workspaces:read"`). Serde and sqlx handle the
//! conversion automatically.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Statically-typed OAuth scope.
///
/// # Variants
///
/// The canonical scope taxonomy:
///
/// | Resource    | Actions                  |
/// |-------------|--------------------------|
/// | workspaces  | read, write, admin       |
/// | tasks       | read, execute            |
/// | sessions    | read, write              |
/// | billing     | admin                    |
/// | org         | read, admin              |
/// | users       | read                     |
/// | workers     | read, admin              |
/// | settings    | read, write              |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Scope {
    // -- Workspace scopes --
    #[serde(rename = "workspaces:read")]
    WorkspacesRead,
    #[serde(rename = "workspaces:write")]
    WorkspacesWrite,
    #[serde(rename = "workspaces:admin")]
    WorkspacesAdmin,

    // -- Task scopes --
    #[serde(rename = "tasks:read")]
    TasksRead,
    #[serde(rename = "tasks:execute")]
    TasksExecute,

    // -- Session (chat) scopes --
    #[serde(rename = "sessions:read")]
    SessionsRead,
    #[serde(rename = "sessions:write")]
    SessionsWrite,

    // -- Billing scopes --
    #[serde(rename = "billing:admin")]
    BillingAdmin,

    // -- Organization scopes --
    #[serde(rename = "org:read")]
    OrgRead,
    #[serde(rename = "org:admin")]
    OrgAdmin,

    // -- User scopes --
    #[serde(rename = "users:read")]
    UsersRead,

    // -- Worker scopes --
    #[serde(rename = "workers:read")]
    WorkersRead,
    #[serde(rename = "workers:admin")]
    WorkersAdmin,

    // -- Settings scopes --
    #[serde(rename = "settings:read")]
    SettingsRead,
    #[serde(rename = "settings:write")]
    SettingsWrite,

    // -- Worker-host bearer tokens (host_jwt, rev-7 §5.7) --
    //
    // Identifies the JWT as having been minted for a `worker_hosts` row, not
    // a user account. The presence of this scope is the discriminant between
    // a user JWT and a host JWT; the auth middleware routes the request to
    // `AuthContext::Host` when the scope is present. See
    // `g5/g5-api/worker_hosts` [I1] for why the scope set is the
    // discriminant rather than a `subject_type` field.
    #[serde(rename = "worker_host")]
    WorkerHost,
}

impl Scope {
    /// Returns the canonical string representation (e.g., `"workspaces:read"`).
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::WorkspacesRead => "workspaces:read",
            Self::WorkspacesWrite => "workspaces:write",
            Self::WorkspacesAdmin => "workspaces:admin",
            Self::TasksRead => "tasks:read",
            Self::TasksExecute => "tasks:execute",
            Self::SessionsRead => "sessions:read",
            Self::SessionsWrite => "sessions:write",
            Self::BillingAdmin => "billing:admin",
            Self::OrgRead => "org:read",
            Self::OrgAdmin => "org:admin",
            Self::UsersRead => "users:read",
            Self::WorkersRead => "workers:read",
            Self::WorkersAdmin => "workers:admin",
            Self::SettingsRead => "settings:read",
            Self::SettingsWrite => "settings:write",
            Self::WorkerHost => "worker_host",
        }
    }

    /// Returns all scope variants.
    #[must_use]
    pub const fn all() -> &'static [Scope] {
        &[
            Self::WorkspacesRead,
            Self::WorkspacesWrite,
            Self::WorkspacesAdmin,
            Self::TasksRead,
            Self::TasksExecute,
            Self::SessionsRead,
            Self::SessionsWrite,
            Self::BillingAdmin,
            Self::OrgRead,
            Self::OrgAdmin,
            Self::UsersRead,
            Self::WorkersRead,
            Self::WorkersAdmin,
            Self::SettingsRead,
            Self::SettingsWrite,
            Self::WorkerHost,
        ]
    }

    /// Returns all scope string values (for DB validation / CHECK constraints).
    #[must_use]
    pub fn all_strings() -> Vec<&'static str> {
        Self::all().iter().map(|s| s.as_str()).collect()
    }
}

impl Scope {
    /// Whether this scope only permits reading. Additive grants on a viewer
    /// membership are limited to these (`g5.organization.viewer-ceiling`).
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.as_str().ends_with(":read")
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Scope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "workspaces:read" => Ok(Self::WorkspacesRead),
            "workspaces:write" => Ok(Self::WorkspacesWrite),
            "workspaces:admin" => Ok(Self::WorkspacesAdmin),
            "tasks:read" => Ok(Self::TasksRead),
            "tasks:execute" => Ok(Self::TasksExecute),
            "sessions:read" => Ok(Self::SessionsRead),
            "sessions:write" => Ok(Self::SessionsWrite),
            "billing:admin" => Ok(Self::BillingAdmin),
            "org:read" => Ok(Self::OrgRead),
            "org:admin" => Ok(Self::OrgAdmin),
            "users:read" => Ok(Self::UsersRead),
            "workers:read" => Ok(Self::WorkersRead),
            "workers:admin" => Ok(Self::WorkersAdmin),
            "settings:read" => Ok(Self::SettingsRead),
            "settings:write" => Ok(Self::SettingsWrite),
            "worker_host" => Ok(Self::WorkerHost),
            _ => Err(format!("unknown scope: {s}")),
        }
    }
}

impl AsRef<str> for Scope {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

// ---------------------------------------------------------------------------
// sqlx integration (behind feature gate)
// ---------------------------------------------------------------------------

#[cfg(feature = "sqlx")]
mod sqlx_impl {
    use super::Scope;

    impl sqlx::Type<sqlx::Postgres> for Scope {
        fn type_info() -> sqlx::postgres::PgTypeInfo {
            <String as sqlx::Type<sqlx::Postgres>>::type_info()
        }

        fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
            <String as sqlx::Type<sqlx::Postgres>>::compatible(ty)
        }
    }

    impl<'q> sqlx::Encode<'q, sqlx::Postgres> for Scope {
        fn encode_by_ref(
            &self,
            buf: &mut sqlx::postgres::PgArgumentBuffer,
        ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
            <&str as sqlx::Encode<'q, sqlx::Postgres>>::encode(self.as_str(), buf)
        }
    }

    impl<'r> sqlx::Decode<'r, sqlx::Postgres> for Scope {
        fn decode(
            value: sqlx::postgres::PgValueRef<'r>,
        ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
            let s = <&str as sqlx::Decode<'r, sqlx::Postgres>>::decode(value)?;
            s.parse::<Scope>().map_err(|e| e.into())
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_variants_have_unique_strings() {
        let strs: Vec<&str> = Scope::all().iter().map(|s| s.as_str()).collect();
        let unique: std::collections::HashSet<&str> = strs.iter().copied().collect();
        assert_eq!(strs.len(), unique.len(), "Duplicate scope strings detected");
    }

    #[test]
    fn test_roundtrip_from_str() {
        for scope in Scope::all() {
            let s = scope.as_str();
            let parsed: Scope = s.parse().unwrap_or_else(|e| panic!("{}", e));
            assert_eq!(*scope, parsed, "Roundtrip failed for {s}");
        }
    }

    #[test]
    fn test_display() {
        assert_eq!(Scope::WorkspacesRead.to_string(), "workspaces:read");
        assert_eq!(Scope::WorkerHost.to_string(), "worker_host");
    }

    #[test]
    fn test_serde_roundtrip() {
        for scope in Scope::all() {
            let json = serde_json::to_string(scope).unwrap();
            assert_eq!(json, format!("\"{}\"", scope.as_str()));
            let parsed: Scope = serde_json::from_str(&json).unwrap();
            assert_eq!(*scope, parsed);
        }
    }

    #[test]
    fn test_serde_vec_roundtrip() {
        let scopes = vec![Scope::WorkspacesRead, Scope::SessionsWrite];
        let json = serde_json::to_string(&scopes).unwrap();
        assert_eq!(json, r#"["workspaces:read","sessions:write"]"#);
        let parsed: Vec<Scope> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, scopes);
    }

    #[test]
    fn test_unknown_scope_fails() {
        let result = "admin:full".parse::<Scope>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown scope"));
    }

    #[test]
    fn test_serde_unknown_scope_fails() {
        let json = r#""not:a:scope""#;
        let result: Result<Scope, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_all_count() {
        assert_eq!(Scope::all().len(), 16, "Expected 16 scope variants");
    }

    #[test]
    fn test_worker_host_scope_round_trip() {
        let s = Scope::WorkerHost.as_str();
        assert_eq!(s, "worker_host");

        let parsed: Scope = s.parse().unwrap();
        assert_eq!(parsed, Scope::WorkerHost);

        let json = serde_json::to_string(&Scope::WorkerHost).unwrap();
        assert_eq!(json, "\"worker_host\"");
        let de: Scope = serde_json::from_str(&json).unwrap();
        assert_eq!(de, Scope::WorkerHost);
    }

    #[test]
    fn test_copy_semantics() {
        let a = Scope::WorkspacesRead;
        let b = a; // Copy
        assert_eq!(a, b);
    }
}
