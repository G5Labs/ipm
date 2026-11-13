use std::path::Path;

struct BackgroundConsumer<'a> {
    rel_path: &'a str,
    required_token_helper_call: &'a str,
}

const BACKGROUND_CONSUMERS: &[BackgroundConsumer<'_>] = &[
    BackgroundConsumer {
        rel_path: "src/services/git/remote_integrations/worker.rs",
        required_token_helper_call: ".resolve(",
    },
    BackgroundConsumer {
        rel_path: "src/services/git/remote_advisory_pinger.rs",
        required_token_helper_call: ".resolve(",
    },
];

#[test]
fn background_scm_consumers_use_repository_credentials() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("g5-api must live directly under the generation workspace")
        .join("g5-api-core");

    let mut violations = Vec::new();
    for consumer in BACKGROUND_CONSUMERS {
        let source_path = manifest_dir.join(consumer.rel_path);
        let source = std::fs::read_to_string(&source_path).unwrap_or_else(|err| {
            panic!("read {}: {err}", source_path.display());
        });
        let source_without_line_comments = strip_line_comments(&source);

        if !source_without_line_comments.contains(consumer.required_token_helper_call) {
            violations.push(format!(
                "{} does not call RepositoryCredentials::resolve",
                consumer.rel_path
            ));
        }
        if source_without_line_comments.contains("resolve_auth(")
            || source_without_line_comments.contains("use crate::auth::{resolve_auth")
            || source_without_line_comments.contains("use crate::auth::resolve_auth")
        {
            violations.push(format!(
                "{} still uses the Phase-0 resolve_auth path",
                consumer.rel_path
            ));
        }
        if source_without_line_comments.contains(".token_for_user(")
            || source_without_line_comments.contains("github_app_binding")
            || source_without_line_comments.contains(".decrypt_token(")
            || source_without_line_comments.contains("ScmTokenManager::decrypt_token")
        {
            violations.push(format!(
                "{} directly decrypts SCM tokens instead of using RepositoryCredentials",
                consumer.rel_path
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "D51 auth-boundary violations:\n{}",
        violations.join("\n")
    );
}

fn strip_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(before, _)| before))
        .collect::<Vec<_>>()
        .join("\n")
}
