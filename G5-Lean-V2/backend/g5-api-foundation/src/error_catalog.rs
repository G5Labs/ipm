//! Static error code catalog matching `API_AND_STORAGE.md` §4B.
//!
//! Provides [`ErrorCatalog`] with lookup functions that map error code strings
//! to HTTP status codes and human-readable descriptions. This is the single
//! source of truth for error code → HTTP status mapping within the API server.

use http::StatusCode;

// ---------------------------------------------------------------------------
// ErrorCatalogEntry
// ---------------------------------------------------------------------------

/// A single entry in the error code catalog.
#[derive(Debug, Clone)]
pub struct ErrorCatalogEntry {
    /// Machine-readable error code (SCREAMING_SNAKE_CASE).
    pub code: &'static str,
    /// HTTP status code for this error.
    pub status: StatusCode,
    /// Human-readable description of this error.
    pub description: &'static str,
}

// ---------------------------------------------------------------------------
// Static catalog data — all 15 error codes from §4B
// ---------------------------------------------------------------------------

static CATALOG: [ErrorCatalogEntry; 15] = [
    ErrorCatalogEntry {
        code: "UNAUTHORIZED",
        status: StatusCode::UNAUTHORIZED,
        description: "Missing or invalid authentication token",
    },
    ErrorCatalogEntry {
        code: "FORBIDDEN",
        status: StatusCode::FORBIDDEN,
        description: "Valid token but insufficient scopes",
    },
    ErrorCatalogEntry {
        code: "NOT_FOUND",
        status: StatusCode::NOT_FOUND,
        description: "Resource does not exist or is not visible to caller",
    },
    ErrorCatalogEntry {
        code: "CONFLICT",
        status: StatusCode::CONFLICT,
        description: "Optimistic concurrency conflict",
    },
    ErrorCatalogEntry {
        code: "VALIDATION_ERROR",
        status: StatusCode::UNPROCESSABLE_ENTITY,
        description: "Request body fails schema validation",
    },
    ErrorCatalogEntry {
        code: "RATE_LIMITED",
        status: StatusCode::TOO_MANY_REQUESTS,
        description: "Too many requests",
    },
    ErrorCatalogEntry {
        code: "SPEND_LIMIT_REACHED",
        status: StatusCode::PAYMENT_REQUIRED,
        description: "Workspace spend cap exceeded",
    },
    ErrorCatalogEntry {
        code: "BRANCH_DIVERGED",
        status: StatusCode::CONFLICT,
        description: "Branch head_commit is behind base",
    },
    ErrorCatalogEntry {
        code: "DRAFT_CONFLICT",
        status: StatusCode::CONFLICT,
        description: "live workspace file draft cannot be committed",
    },
    ErrorCatalogEntry {
        code: "TASK_NOT_ELIGIBLE",
        status: StatusCode::CONFLICT,
        description: "Task cannot transition to requested status",
    },
    ErrorCatalogEntry {
        code: "WORKER_NOT_FOUND",
        status: StatusCode::NOT_FOUND,
        description: "Worker ID not in registry",
    },
    ErrorCatalogEntry {
        code: "WORKER_BUSY",
        status: StatusCode::CONFLICT,
        description: "Worker already assigned to a task",
    },
    ErrorCatalogEntry {
        code: "SCOPE_MISSING",
        status: StatusCode::FORBIDDEN,
        description: "Specific scope(s) required for this action",
    },
    ErrorCatalogEntry {
        code: "ORG_QUOTA_EXCEEDED",
        status: StatusCode::PAYMENT_REQUIRED,
        description: "Org-level resource quota exceeded",
    },
    ErrorCatalogEntry {
        code: "INTERNAL_ERROR",
        status: StatusCode::INTERNAL_SERVER_ERROR,
        description: "Unhandled server error",
    },
];

// ---------------------------------------------------------------------------
// ErrorCatalog — static lookup functions
// ---------------------------------------------------------------------------

/// Static error code catalog matching `API_AND_STORAGE.md` §4B.
///
/// Provides lookup functions for error code metadata. The catalog is small
/// (15 entries) so linear search is used — no heap allocation needed.
pub struct ErrorCatalog;

impl ErrorCatalog {
    /// Returns the HTTP status code for a given error code string.
    ///
    /// Returns `None` for unknown codes.
    ///
    /// # Examples
    ///
    /// ```
    /// use http::StatusCode;
    /// use g5_api_foundation::error_catalog::ErrorCatalog;
    ///
    /// assert_eq!(
    ///     ErrorCatalog::status_for_code("NOT_FOUND"),
    ///     Some(StatusCode::NOT_FOUND),
    /// );
    /// assert_eq!(ErrorCatalog::status_for_code("UNKNOWN_CODE"), None);
    /// ```
    #[must_use]
    pub fn status_for_code(code: &str) -> Option<StatusCode> {
        CATALOG
            .iter()
            .find(|entry| entry.code == code)
            .map(|entry| entry.status)
    }

    /// Returns the human-readable description for a given error code string.
    ///
    /// Returns `None` for unknown codes.
    ///
    /// # Examples
    ///
    /// ```
    /// use g5_api_foundation::error_catalog::ErrorCatalog;
    ///
    /// assert_eq!(
    ///     ErrorCatalog::description("UNAUTHORIZED"),
    ///     Some("Missing or invalid authentication token"),
    /// );
    /// assert_eq!(ErrorCatalog::description("UNKNOWN_CODE"), None);
    /// ```
    #[must_use]
    pub fn description(code: &str) -> Option<&'static str> {
        CATALOG
            .iter()
            .find(|entry| entry.code == code)
            .map(|entry| entry.description)
    }

    /// Returns a slice of all known error codes with their status codes
    /// and descriptions.
    ///
    /// The returned slice contains exactly 15 entries matching the error
    /// catalog from `API_AND_STORAGE.md` §4B.
    #[must_use]
    pub fn all_codes() -> &'static [ErrorCatalogEntry] {
        &CATALOG
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_has_15_entries() {
        assert_eq!(ErrorCatalog::all_codes().len(), 15);
    }

    #[test]
    fn test_status_for_code_unauthorized() {
        assert_eq!(
            ErrorCatalog::status_for_code("UNAUTHORIZED"),
            Some(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn test_status_for_code_forbidden() {
        assert_eq!(
            ErrorCatalog::status_for_code("FORBIDDEN"),
            Some(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn test_status_for_code_not_found() {
        assert_eq!(
            ErrorCatalog::status_for_code("NOT_FOUND"),
            Some(StatusCode::NOT_FOUND)
        );
    }

    #[test]
    fn test_status_for_code_conflict() {
        assert_eq!(
            ErrorCatalog::status_for_code("CONFLICT"),
            Some(StatusCode::CONFLICT)
        );
    }

    #[test]
    fn test_status_for_code_validation_error() {
        assert_eq!(
            ErrorCatalog::status_for_code("VALIDATION_ERROR"),
            Some(StatusCode::UNPROCESSABLE_ENTITY)
        );
    }

    #[test]
    fn test_status_for_code_rate_limited() {
        assert_eq!(
            ErrorCatalog::status_for_code("RATE_LIMITED"),
            Some(StatusCode::TOO_MANY_REQUESTS)
        );
    }

    #[test]
    fn test_status_for_code_spend_limit_reached() {
        assert_eq!(
            ErrorCatalog::status_for_code("SPEND_LIMIT_REACHED"),
            Some(StatusCode::PAYMENT_REQUIRED)
        );
    }

    #[test]
    fn test_status_for_code_branch_diverged() {
        assert_eq!(
            ErrorCatalog::status_for_code("BRANCH_DIVERGED"),
            Some(StatusCode::CONFLICT)
        );
    }

    #[test]
    fn test_status_for_code_draft_conflict() {
        assert_eq!(
            ErrorCatalog::status_for_code("DRAFT_CONFLICT"),
            Some(StatusCode::CONFLICT)
        );
    }

    #[test]
    fn test_status_for_code_task_not_eligible() {
        assert_eq!(
            ErrorCatalog::status_for_code("TASK_NOT_ELIGIBLE"),
            Some(StatusCode::CONFLICT)
        );
    }

    #[test]
    fn test_status_for_code_worker_not_found() {
        assert_eq!(
            ErrorCatalog::status_for_code("WORKER_NOT_FOUND"),
            Some(StatusCode::NOT_FOUND)
        );
    }

    #[test]
    fn test_status_for_code_worker_busy() {
        assert_eq!(
            ErrorCatalog::status_for_code("WORKER_BUSY"),
            Some(StatusCode::CONFLICT)
        );
    }

    #[test]
    fn test_status_for_code_scope_missing() {
        assert_eq!(
            ErrorCatalog::status_for_code("SCOPE_MISSING"),
            Some(StatusCode::FORBIDDEN)
        );
    }

    #[test]
    fn test_status_for_code_org_quota_exceeded() {
        assert_eq!(
            ErrorCatalog::status_for_code("ORG_QUOTA_EXCEEDED"),
            Some(StatusCode::PAYMENT_REQUIRED)
        );
    }

    #[test]
    fn test_status_for_code_internal_error() {
        assert_eq!(
            ErrorCatalog::status_for_code("INTERNAL_ERROR"),
            Some(StatusCode::INTERNAL_SERVER_ERROR)
        );
    }

    #[test]
    fn test_status_for_code_unknown() {
        assert_eq!(ErrorCatalog::status_for_code("UNKNOWN_CODE"), None);
    }

    #[test]
    fn test_description_unauthorized() {
        assert_eq!(
            ErrorCatalog::description("UNAUTHORIZED"),
            Some("Missing or invalid authentication token")
        );
    }

    #[test]
    fn test_description_not_found() {
        assert_eq!(
            ErrorCatalog::description("NOT_FOUND"),
            Some("Resource does not exist or is not visible to caller")
        );
    }

    #[test]
    fn test_description_internal_error() {
        assert_eq!(
            ErrorCatalog::description("INTERNAL_ERROR"),
            Some("Unhandled server error")
        );
    }

    #[test]
    fn test_description_unknown() {
        assert_eq!(ErrorCatalog::description("UNKNOWN_CODE"), None);
    }

    #[test]
    fn test_all_codes_are_screaming_snake_case() {
        for entry in ErrorCatalog::all_codes() {
            assert!(
                entry
                    .code
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c == '_'),
                "Error code '{}' is not SCREAMING_SNAKE_CASE",
                entry.code
            );
        }
    }

    #[test]
    fn test_all_codes_have_descriptions() {
        for entry in ErrorCatalog::all_codes() {
            assert!(
                !entry.description.is_empty(),
                "Error code '{}' has empty description",
                entry.code
            );
        }
    }

    #[test]
    fn test_all_codes_have_valid_status() {
        for entry in ErrorCatalog::all_codes() {
            assert!(
                entry.status.as_u16() >= 400,
                "Error code '{}' has non-error status {}",
                entry.code,
                entry.status
            );
        }
    }

    #[test]
    fn test_all_15_codes_present() {
        let expected_codes = [
            "UNAUTHORIZED",
            "FORBIDDEN",
            "NOT_FOUND",
            "CONFLICT",
            "VALIDATION_ERROR",
            "RATE_LIMITED",
            "SPEND_LIMIT_REACHED",
            "BRANCH_DIVERGED",
            "DRAFT_CONFLICT",
            "TASK_NOT_ELIGIBLE",
            "WORKER_NOT_FOUND",
            "WORKER_BUSY",
            "SCOPE_MISSING",
            "ORG_QUOTA_EXCEEDED",
            "INTERNAL_ERROR",
        ];
        for code in &expected_codes {
            assert!(
                ErrorCatalog::status_for_code(code).is_some(),
                "Missing error code: {}",
                code
            );
        }
    }
}
