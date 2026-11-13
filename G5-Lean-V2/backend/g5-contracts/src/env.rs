//! Environment-variable helpers with production-safe defaults.
//!
//! This module exists to enforce a single architectural rule for any
//! environment-variable-driven feature flag that gates dev/test/auth-relaxed
//! behavior:
//!
//! > **Production-safe defaults.** The absence of an environment variable
//! > MUST mean the dev-only / auth-relaxed / test-only behavior is OFF.
//! > A missing variable, an unset variable, an invalid value, or any value
//! > other than the explicit literal `"true"` MUST result in `false`.
//!
//! The naive pattern
//!
//! ```ignore
//! // ⚠ Avoid in new code: reads as "default to whatever this evaluates to"
//! //   and forces every reviewer to recompute the safe-default semantics.
//! let dev_login_enabled = std::env::var("G5_E2E_AUTH").unwrap_or_default() == "true";
//! ```
//!
//! is *functionally* correct (`unwrap_or_default()` for `String` yields `""`,
//! which compares unequal to `"true"`, giving `false`) but visually alarming —
//! it makes the safe-default semantics implicit. A future refactor that swaps
//! the default to `String::from("...")`, changes the comparator, or
//! collapses to `is_ok()` could silently re-enable dev behavior in production.
//!
//! Use [`is_env_flag_enabled`] instead — it makes the rule mechanical.

/// Returns `true` if and only if the environment variable is set to the
/// exact literal `"true"`. Any other value (unset, empty, `"false"`, `"1"`,
/// `"yes"`, etc.) returns `false`.
///
/// Use this for any flag that gates dev / test / auth-relaxed behavior.
/// The strict equality with `"true"` is intentional: it forces opt-in to be
/// unambiguous and removes the temptation to add fuzzy parsing later.
///
/// # Examples
///
/// ```
/// # use g5_contracts::env::is_env_flag_enabled;
/// // SAFETY ASSERTION:
/// // The absence of the variable must always evaluate to `false`.
/// std::env::remove_var("G5_TEST_FLAG_A");
/// assert!(!is_env_flag_enabled("G5_TEST_FLAG_A"));
/// ```
#[must_use]
pub fn is_env_flag_enabled(name: &str) -> bool {
    matches!(std::env::var(name).as_deref(), Ok("true"))
}

/// Returns `true` if and only if the environment variable is set to the
/// exact literal `"false"`. Any other value (unset, empty, `"0"`, `"no"`,
/// `"FALSE"`, etc.) returns `false` — i.e. the governed feature stays ON.
///
/// This is the default-TRUE counterpart of [`is_env_flag_enabled`] for
/// **product default-on kill-switches**: flags whose feature is meant to be
/// enabled everywhere and whose variable exists only so an operator can turn
/// it off (system_invariants/config_safe_defaults D6; e.g.
/// `G5_ENABLE_PROVIDER_ACCOUNTS`). It must NEVER be used for dev / test /
/// auth-relaxed gates — those keep the fail-closed [`is_env_flag_enabled`]
/// read. The strict equality with `"false"` mirrors its sibling: opt-OUT is
/// unambiguous, with no fuzzy parsing.
///
/// # Examples
///
/// ```
/// # use g5_contracts::env::is_env_flag_disabled;
/// // SAFETY ASSERTION:
/// // The absence of the variable must keep the shipped feature enabled.
/// std::env::remove_var("G5_TEST_FLAG_B");
/// assert!(!is_env_flag_disabled("G5_TEST_FLAG_B"));
/// ```
#[must_use]
pub fn is_env_flag_disabled(name: &str) -> bool {
    matches!(std::env::var(name).as_deref(), Ok("false"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: run `body` with `var` set to `value` for the duration of the
    /// call, and remove it afterwards. Tests in this module run sequentially
    /// (cargo runs them on a single thread for env mutations) but using a
    /// scope-guard would be safer for concurrent test runners.
    fn with_env_var(name: &str, value: Option<&str>, body: impl FnOnce()) {
        match value {
            Some(v) => std::env::set_var(name, v),
            None => std::env::remove_var(name),
        }
        body();
        std::env::remove_var(name);
    }

    #[test]
    fn missing_var_is_false() {
        with_env_var("G5_TEST_FLAG_MISSING", None, || {
            assert!(!is_env_flag_enabled("G5_TEST_FLAG_MISSING"));
        });
    }

    #[test]
    fn empty_var_is_false() {
        with_env_var("G5_TEST_FLAG_EMPTY", Some(""), || {
            assert!(!is_env_flag_enabled("G5_TEST_FLAG_EMPTY"));
        });
    }

    #[test]
    fn literal_true_is_true() {
        with_env_var("G5_TEST_FLAG_TRUE", Some("true"), || {
            assert!(is_env_flag_enabled("G5_TEST_FLAG_TRUE"));
        });
    }

    #[test]
    fn literal_false_is_false() {
        with_env_var("G5_TEST_FLAG_FALSE", Some("false"), || {
            assert!(!is_env_flag_enabled("G5_TEST_FLAG_FALSE"));
        });
    }

    #[test]
    fn fuzzy_truthy_values_are_false() {
        // Strict equality with "true" — these all evaluate to `false` to
        // keep opt-in unambiguous.
        for value in &["1", "yes", "TRUE", "True", "  true  ", "on"] {
            with_env_var("G5_TEST_FLAG_FUZZY", Some(value), || {
                assert!(
                    !is_env_flag_enabled("G5_TEST_FLAG_FUZZY"),
                    "fuzzy value {value:?} must not enable a dev flag"
                );
            });
        }
    }

    #[test]
    fn disabled_missing_var_keeps_feature_enabled() {
        with_env_var("G5_TEST_KILL_SWITCH_MISSING", None, || {
            assert!(!is_env_flag_disabled("G5_TEST_KILL_SWITCH_MISSING"));
        });
    }

    #[test]
    fn disabled_literal_false_disables() {
        with_env_var("G5_TEST_KILL_SWITCH_FALSE", Some("false"), || {
            assert!(is_env_flag_disabled("G5_TEST_KILL_SWITCH_FALSE"));
        });
    }

    #[test]
    fn disabled_fuzzy_falsy_values_keep_feature_enabled() {
        // Strict equality with "false" — opt-out must be unambiguous.
        for value in &["", "0", "no", "FALSE", "False", "  false  ", "off", "true"] {
            with_env_var("G5_TEST_KILL_SWITCH_FUZZY", Some(value), || {
                assert!(
                    !is_env_flag_disabled("G5_TEST_KILL_SWITCH_FUZZY"),
                    "fuzzy value {value:?} must not disable a default-on feature"
                );
            });
        }
    }
}
