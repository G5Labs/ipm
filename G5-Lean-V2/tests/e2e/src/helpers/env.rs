//! Environment helpers for E2E tests.
//!
//! Provides utilities for checking environment capabilities (live LLM keys,
//! Docker availability, etc.) and skipping tests when prerequisites aren't met.

use std::time::Duration;

const STACK_HEALTH_TIMEOUT: Duration = Duration::from_secs(5);

/// Check if a live LLM API key is available in the environment.
/// Returns the provider name and model if found.
///
/// Priority:
/// 1. `G5D_LLM_PROVIDER` + `G5D_LLM_MODEL` env vars (explicit worker config)
/// 2. Key-based detection: ANTHROPIC → OPENROUTER → OPENAI
pub fn detect_llm_provider() -> Option<(&'static str, &'static str)> {
    // If the worker's explicit LLM config is set, use it — this matches
    // what the Docker workers actually use, avoiding misleading log messages.
    if let (Ok(provider), Ok(_model)) = (
        std::env::var("G5D_LLM_PROVIDER"),
        std::env::var("G5D_LLM_MODEL"),
    ) {
        // Determine which API key env var is needed for this provider
        let key_var = match provider.as_str() {
            "openrouter" => "OPENROUTER_API_KEY",
            "anthropic" => "ANTHROPIC_API_KEY",
            "openai" => "OPENAI_API_KEY",
            _ => "ANTHROPIC_API_KEY",
        };
        if std::env::var(key_var).is_ok() {
            // Leak the model string to get a &'static str — acceptable for
            // test helper called once per test.
            let model_static: &'static str =
                Box::leak(std::env::var("G5D_LLM_MODEL").unwrap().into_boxed_str());
            let provider_static: &'static str = match provider.as_str() {
                "openrouter" => "openrouter",
                "anthropic" => "anthropic",
                "openai" => "openai",
                _ => "anthropic",
            };
            return Some((provider_static, model_static));
        }
    }

    // Fallback: key-based detection
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        Some(("anthropic", "claude-sonnet-4-20250514"))
    } else if std::env::var("OPENROUTER_API_KEY").is_ok() {
        Some(("openrouter", "openai/gpt-5.4"))
    } else if std::env::var("OPENAI_API_KEY").is_ok() {
        Some(("openai", "gpt-4o"))
    } else {
        None
    }
}

/// Returns true if the `live-llm` feature is enabled AND an API key is present.
pub fn live_llm_available() -> bool {
    cfg!(feature = "live-llm") && detect_llm_provider().is_some()
}

/// Skip the current test with a message if the condition is false.
/// Returns `Ok(())` early from the calling test function.
#[macro_export]
macro_rules! skip_unless {
    ($cond:expr, $msg:expr) => {
        if !$cond {
            eprintln!("SKIP: {}", $msg);
            return Ok(());
        }
    };
}

async fn stack_is_healthy_at(url: &str, timeout: Duration) -> bool {
    matches!(
        tokio::time::timeout(timeout, reqwest::get(format!("{}/healthz", url))).await,
        Ok(Ok(response)) if response.status().is_success()
    )
}

/// Return whether the Docker Compose API passes a bounded anonymous probe.
pub async fn stack_is_healthy() -> bool {
    let url = super::api_client::api_base_url();
    stack_is_healthy_at(&url, STACK_HEALTH_TIMEOUT).await
}

fn stack_preflight_result(
    stack_healthy: bool,
    required: bool,
    required_flag: &str,
    scenario: &str,
    api_url: &str,
) -> anyhow::Result<bool> {
    if stack_healthy {
        return Ok(true);
    }
    if required {
        anyhow::bail!(
            "{required_flag}=true but required {scenario} cannot reach a healthy g5-api at \
             {api_url}; the bounded unauthenticated /healthz preflight failed"
        );
    }
    Ok(false)
}

/// Run the bounded anonymous stack preflight for an optionally required E2E.
///
/// An unset requirement flag lets direct local runs skip cleanly. CI sets the
/// flag to the exact literal `true`, turning the same missing prerequisite into
/// a hard failure before any test-token or OAuth request is attempted.
pub async fn require_stack_or_skip(required_flag: &str, scenario: &str) -> anyhow::Result<bool> {
    let api_url = super::api_client::api_base_url();
    let stack_healthy = stack_is_healthy().await;
    let required = g5_contracts::env::is_env_flag_enabled(required_flag);
    let should_run =
        stack_preflight_result(stack_healthy, required, required_flag, scenario, &api_url)?;
    if !should_run {
        eprintln!(
            "SKIP: {scenario}: g5-api is not healthy at {api_url}; set \
             {required_flag}=true to require this live E2E"
        );
    }
    Ok(should_run)
}

/// Return the workspace selected for the current E2E run.
///
/// Shared-dev orchestration exports both names for compatibility. Prefer the
/// explicit name, then the legacy alias, and retain `e2e-test` only as the
/// standalone/default fixture when neither was selected.
pub fn e2e_workspace_name() -> String {
    std::env::var("G5_E2E_TEST_WORKSPACE_NAME")
        .or_else(|_| std::env::var("G5_E2E_TEST_WORKSPACE"))
        .unwrap_or_else(|_| "e2e-test".to_string())
}

/// Get the worker's LLM provider env var name for the detected provider.
pub fn llm_env_var_name() -> &'static str {
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        "ANTHROPIC_API_KEY"
    } else if std::env::var("OPENROUTER_API_KEY").is_ok() {
        "OPENROUTER_API_KEY"
    } else if std::env::var("OPENAI_API_KEY").is_ok() {
        "OPENAI_API_KEY"
    } else {
        "ANTHROPIC_API_KEY" // default
    }
}

/// Return the Postgres connection URL for direct DB access in E2E tests.
///
/// Reads `G5_E2E_DATABASE_URL`.  When unset, builds the URL from
/// `G5_PORT_POSTGRES` (defaulting to `5433`) to match the dev-stack
/// `.env` configuration that avoids the standard Postgres port.
///
/// **Why not `DATABASE_URL`?**  `.cargo/config.toml` sets `DATABASE_URL`
/// to a Docker-internal IP for sqlx compile-time verification.  That
/// address is unreachable from the host, so E2E tests must never use it.
pub fn e2e_db_url() -> String {
    std::env::var("G5_E2E_DATABASE_URL").unwrap_or_else(|_| {
        let port = std::env::var("G5_PORT_POSTGRES").unwrap_or_else(|_| "5433".into());
        format!("postgres://g5:g5_dev@127.0.0.1:{port}/g5_dev")
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{stack_is_healthy_at, stack_preflight_result};

    const REQUIRED_FLAG: &str = "G5_E2E_REQUIRE_EXAMPLE";
    const SCENARIO: &str = "example live E2E";
    const API_URL: &str = "http://127.0.0.1:7501";

    #[test]
    fn optional_unhealthy_stack_skips() {
        assert!(
            stack_preflight_result(false, false, REQUIRED_FLAG, SCENARIO, API_URL)
                .is_ok_and(|should_run| !should_run)
        );
    }

    #[test]
    fn healthy_stack_runs_regardless_of_requirement() {
        assert!(
            stack_preflight_result(true, false, REQUIRED_FLAG, SCENARIO, API_URL)
                .is_ok_and(|should_run| should_run)
        );
        assert!(
            stack_preflight_result(true, true, REQUIRED_FLAG, SCENARIO, API_URL)
                .is_ok_and(|should_run| should_run)
        );
    }

    #[test]
    fn required_unhealthy_stack_fails_with_actionable_context() {
        let error = stack_preflight_result(false, true, REQUIRED_FLAG, SCENARIO, API_URL)
            .expect_err("a required live E2E must fail when the stack is unavailable")
            .to_string();

        assert!(error.contains(REQUIRED_FLAG), "{error}");
        assert!(error.contains(SCENARIO), "{error}");
        assert!(error.contains(API_URL), "{error}");
        assert!(error.contains("/healthz"), "{error}");
    }

    #[tokio::test]
    async fn health_probe_is_bounded_when_server_never_responds() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind stalled health endpoint");
        let address = listener.local_addr().expect("read listener address");
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("accept health request");
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let started = tokio::time::Instant::now();
        assert!(
            !stack_is_healthy_at(&format!("http://{address}"), Duration::from_millis(50)).await
        );
        assert!(started.elapsed() < Duration::from_secs(1));
        server.abort();
    }
}
