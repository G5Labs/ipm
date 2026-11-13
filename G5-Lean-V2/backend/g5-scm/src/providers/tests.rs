use super::*;
use std::collections::BTreeSet;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
fn endpoints(server: &MockServer) -> ProviderEndpoints {
    ProviderEndpoints::new(crate::ScmProvider::GitHub, &server.uri(), &server.uri()).unwrap()
}
fn repository(server: &MockServer, capability: RepositoryCapability) -> RepositoryClient {
    ProviderClients::new().unwrap().repository(
        RepositoryGrant::new(
            endpoints(server),
            RepositoryIdentity::Id(41),
            capability,
            "selected-token-canary".into(),
        )
        .unwrap(),
    )
}
#[tokio::test]
async fn repository_grants_refuse_wrong_capabilities_before_http() {
    let server = MockServer::start().await;
    let metadata = repository(&server, RepositoryCapability::MetadataRead);
    assert!(issues::fetch_issue(&metadata, 1).await.is_err());
    let reader = repository(&server, RepositoryCapability::IssuesRead);
    assert!(matches!(
        issues::post_issue_comment(&reader, 1, "text").await,
        issues::CommentPostOutcome::Rejected(_)
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
    Mock::given(method("GET"))
        .and(path("/repositories/41"))
        .and(header("authorization", "Bearer selected-token-canary"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"id":41,"full_name":"owner/repo"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(issues::fetch_repository(&metadata).await.unwrap().id, 41);
}
#[tokio::test]
async fn installation_issuance_is_narrow_and_never_replays_unknown_or_partial_writes() {
    let server = MockServer::start().await;
    let request = InstallationTokenRequest {
        repository_ids: BTreeSet::from([41]),
        permissions: std::collections::BTreeMap::from([("issues".into(), "read".into())]),
        ..InstallationTokenRequest::default()
    };
    Mock::given(method("POST"))
        .and(path("/app/installations/9/access_tokens"))
        .and(header("authorization", "Bearer jwt-canary"))
        .and(body_json(
            serde_json::json!({"repository_ids":[41],"permissions":{"issues":"read"}}),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_string("invalid-json"))
        .expect(1)
        .mount(&server)
        .await;
    let app = ProviderClients::new()
        .unwrap()
        .app(AppGrant::new(endpoints(&server), "jwt-canary".into()));
    let error = app.installation_token(9, &request).await.unwrap_err();
    assert_eq!(error.effect, crate::FailureEffect::Partial);
    assert_eq!(error.status, Some(201));
    assert!(error.to_string().contains("inspect current state"));
    // `reset` discards mounted expectations unverified; verify the exact
    // single issuance first.
    server.verify().await;
    server.reset().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    let error = app.installation_token(9, &request).await.unwrap_err();
    assert_eq!(error.effect, crate::FailureEffect::Unknown);
    server.verify().await;
}
#[tokio::test]
async fn provider_refusal_preserves_hints_redacts_selected_secrets_and_does_not_follow_redirects() {
    let server = MockServer::start().await;
    let other = MockServer::start().await;
    let token = "private-canary-outside-known-token-formats";
    let app = ProviderClients::new()
        .unwrap()
        .app(AppGrant::new(endpoints(&server), token.into()));
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "60")
                .insert_header("x-ratelimit-reset", "1800000000")
                .insert_header("x-github-request-id", token)
                .set_body_json(serde_json::json!({"message":format!("denied {token}")})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let error = app.installation_metadata(9).await.unwrap_err();
    assert!(error.rate_limited);
    assert_eq!(error.retry_after.as_deref(), Some("60"));
    assert_eq!(error.rate_limit_reset.as_deref(), Some("1800000000"));
    assert!(!format!("{error:?} {error}").contains(token));
    server.verify().await;
    server.reset().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", other.uri()))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        app.installation_metadata(9).await.unwrap_err().status,
        Some(302)
    );
    assert!(other.received_requests().await.unwrap().is_empty());
    server.verify().await;
}
#[tokio::test]
async fn configured_api_prefix_and_independent_login_endpoint_are_preserved() {
    let integration = MockServer::start().await;
    let identity = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/enterprise/api/v3/user"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"id":12,"login":"person"})),
        )
        .expect(1)
        .mount(&integration)
        .await;
    let endpoints = ProviderEndpoints::new(
        crate::ScmProvider::GitHub,
        &format!("{}/enterprise/api/v3/", integration.uri()),
        &integration.uri(),
    )
    .unwrap();
    let factory = ProviderClients::new().unwrap();
    assert_eq!(
        factory
            .app_user(AppUserGrant::new(
                endpoints,
                "1".into(),
                "integration-token".into()
            ))
            .authenticated_user()
            .await
            .unwrap()
            .id,
        12
    );
    Mock::given(method("GET"))
        .and(path("/identity/user"))
        .and(header("authorization", "Bearer login-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"id":22,"login":"identity"})),
        )
        .expect(1)
        .mount(&identity)
        .await;
    let login = factory.login(
        LoginConfig::new(
            crate::ScmProvider::GitHub,
            &format!("{}/token", identity.uri()),
            &format!("{}/identity/user?configured=true", identity.uri()),
            "client".into(),
            "client-secret".into(),
            "https://g5.test/callback".into(),
        )
        .unwrap(),
    );
    assert!(matches!(
        login.user_info("login-token").await.unwrap(),
        LoginUser::GitHub(GithubLoginUser { id: 22, .. })
    ));
    assert_eq!(integration.received_requests().await.unwrap().len(), 1);
}
#[tokio::test]
async fn malformed_credentials_and_unsupported_providers_refuse_before_transport() {
    let server = MockServer::start().await;
    let factory = ProviderClients::new().unwrap();
    let invalid = factory.app(AppGrant::new(endpoints(&server), "bad\ncredential".into()));
    assert_eq!(
        invalid.installation_metadata(1).await.unwrap_err().effect,
        crate::FailureEffect::Refused
    );
    let gitlab =
        ProviderEndpoints::new(crate::ScmProvider::GitLab, &server.uri(), &server.uri()).unwrap();
    let unsupported = factory.app(AppGrant::new(gitlab, "token".into()));
    assert_eq!(
        unsupported.installation_metadata(1).await.unwrap_err().kind,
        ProviderErrorKind::UnsupportedCapability
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}
#[tokio::test]
async fn confirmed_status_and_hints_survive_body_timeout_without_replay() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 4096];
        let _ = socket.read(&mut request).await.unwrap();
        socket.write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 100\r\nX-GitHub-Request-Id: confirmed-write\r\n\r\n{").await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    });
    let http = GithubHttp::new().unwrap();
    let error = match http
        .send(Request {
            operation: "test issued write",
            target: None,
            url: Url::parse(&format!("http://{address}/write")).unwrap(),
            method: reqwest::Method::POST,
            token: Some("secret"),
            secrets: &[],
            payload: Payload::Empty,
            timeout: Duration::from_millis(100),
            limit: MAX_RESPONSE,
            github: true,
            media: None,
        })
        .await
    {
        Ok(_) => panic!("body must time out"),
        Err(error) => error,
    };
    assert_eq!(error.effect, crate::FailureEffect::Partial);
    assert_eq!(error.status, Some(201));
    assert_eq!(error.request_id.as_deref(), Some("confirmed-write"));
    assert_eq!(error.kind, ProviderErrorKind::Deadline);
    server.abort();
    let _ = server.await;
}
#[test]
fn token_debug_and_endpoint_errors_never_expose_credentials() {
    let tokens = OAuthTokens {
        access_token: "access-secret".into(),
        expires_in: Some(30),
        refresh_token: Some("refresh-secret".into()),
        refresh_token_expires_in: None,
    };
    assert!(!format!("{tokens:?}").contains("access-secret"));
    assert!(!format!("{tokens:?}").contains("refresh-secret"));
    for endpoint in [
        "https://user:password@example.test/api",
        "https://example.test/api?token=password",
        "file:///tmp/api",
    ] {
        let error =
            ProviderEndpoints::new(crate::ScmProvider::GitHub, endpoint, "https://example.test")
                .unwrap_err();
        assert!(!format!("{error:?}").contains("password"));
    }
}
#[tokio::test]
async fn oversized_success_is_partial_and_oauth_rejection_is_refused() {
    let server = MockServer::start().await;
    let factory = ProviderClients::new().unwrap();
    Mock::given(method("POST"))
        .and(path("/app/installations/9/access_tokens"))
        .respond_with(ResponseTemplate::new(201).set_body_string("x".repeat(MAX_RESPONSE + 1)))
        .expect(1)
        .mount(&server)
        .await;
    let app = factory.app(AppGrant::new(endpoints(&server), "jwt".into()));
    let error = app
        .installation_token(9, &InstallationTokenRequest::default())
        .await
        .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::InvalidResponse);
    assert_eq!(error.effect, crate::FailureEffect::Partial);
    assert_eq!(error.target.as_deref(), Some("installation 9"));
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"error":"bad_verification_code"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let login = factory.login(
        LoginConfig::github_app(
            &endpoints(&server),
            "client".into(),
            "secret".into(),
            "callback".into(),
        )
        .unwrap(),
    );
    let error = login
        .exchange_code("code", Some("verifier"))
        .await
        .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Rejected);
    assert_eq!(error.effect, crate::FailureEffect::Refused);
    let exchange = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .find(|request| request.url.path() == "/login/oauth/access_token")
        .unwrap();
    let accept: Vec<_> = exchange.headers.get_all("accept").iter().collect();
    assert_eq!(accept, ["application/json"], "one Accept value per request");
}

#[tokio::test]
async fn repository_access_keeps_recovery_signals_and_exact_retry_hints() {
    let server = MockServer::start().await;
    let client = ProviderClients::new().unwrap().app_user(AppUserGrant::new(
        endpoints(&server),
        "7".into(),
        "0-private-token-canary".into(),
    ));
    for (name, response) in [
        (
            "limited",
            ResponseTemplate::new(403)
                .insert_header("retry-after", "60")
                .insert_header("x-github-request-id", "request-0")
                .set_body_json(serde_json::json!({"message":"rate limit 0-private-token-canary"})),
        ),
        (
            "secondary",
            ResponseTemplate::new(403)
                .set_body_json(serde_json::json!({"message":"secondary rate limit"})),
        ),
        ("denied", ResponseTemplate::new(403)),
        ("missing", ResponseTemplate::new(404)),
        ("revoked", ResponseTemplate::new(401)),
        (
            "allowed",
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"id":41})),
        ),
        (
            "changed",
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"id":42})),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/repos/owner/{name}")))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
    }
    for name in ["limited", "secondary"] {
        let RepositoryAccessError::Failed(error) = client
            .verify_repository_access(&format!("owner/{name}"), 41)
            .await
            .unwrap_err()
        else {
            panic!("rate limit must retain its evidence");
        };
        assert!(error.rate_limited);
        assert_eq!(error.kind, ProviderErrorKind::RateLimited);
        if name == "limited" {
            assert_eq!(error.retry_after.as_deref(), Some("60"));
            assert_eq!(error.request_id.as_deref(), Some("request-0"));
            assert!(!error.to_string().contains("0-private-token-canary"));
        }
    }
    for name in ["denied", "missing"] {
        assert!(matches!(
            client
                .verify_repository_access(&format!("owner/{name}"), 41)
                .await,
            Err(RepositoryAccessError::AccessDenied)
        ));
    }
    assert!(matches!(
        client.verify_repository_access("owner/revoked", 41).await,
        Err(RepositoryAccessError::ConnectionRequired)
    ));
    assert!(matches!(
        client.verify_repository_access("owner/changed", 41).await,
        Err(RepositoryAccessError::RepositoryChanged)
    ));
    client
        .verify_repository_access("owner/allowed", 41)
        .await
        .unwrap();
}

#[tokio::test]
async fn observed_unauthorized_survives_truncated_body() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let mut received = 0;
        while !request[..received]
            .windows(4)
            .any(|bytes| bytes == b"\r\n\r\n")
        {
            let count = socket.read(&mut request[received..]).await.unwrap();
            assert_ne!(count, 0, "incomplete HTTP request headers");
            received += count;
        }
        socket
            .write_all(
                b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 100\r\nConnection: close\r\n\r\nx",
            )
            .await
            .unwrap();
    });
    let client = ProviderClients::new().unwrap().app_user(AppUserGrant::new(
        ProviderEndpoints::new(crate::ScmProvider::GitHub, &url, &url).unwrap(),
        "7".into(),
        "token".into(),
    ));
    assert!(matches!(
        client.verify_repository_access("owner/repo", 41).await,
        Err(RepositoryAccessError::ConnectionRequired)
    ));
    server.await.unwrap();
}

#[tokio::test]
async fn issue_response_links_belong_to_selected_api_and_web_bases() {
    let server = MockServer::start().await;
    let endpoints = ProviderEndpoints::new(
        crate::ScmProvider::GitHub,
        &format!("{}/api/v3", server.uri()),
        "https://selected.test/git",
    )
    .unwrap();
    let factory = ProviderClients::new().unwrap();
    let reader = factory.repository(
        RepositoryGrant::new(
            endpoints.clone(),
            RepositoryIdentity::Id(41),
            RepositoryCapability::IssuesRead,
            "token".into(),
        )
        .unwrap(),
    );
    let searcher = factory.repository(
        RepositoryGrant::new(
            endpoints,
            RepositoryIdentity::Name("renamed/repo".into()),
            RepositoryCapability::IssuesRead,
            "token".into(),
        )
        .unwrap(),
    );
    let good = serde_json::json!({"id":51,"number":7,"title":"issue","state":"open","created_at":"now","updated_at":"now","html_url":"https://selected.test/git/renamed/repo/issues/7#issue-51","repository_url":format!("{}/api/v3/repos/renamed/repo", server.uri())});
    let mut items = vec![good.clone()];
    for (field, value) in [
        (
            "html_url",
            "https://other.test/git/renamed/repo/issues/7".to_owned(),
        ),
        (
            "html_url",
            "https://selected.test:444/git/renamed/repo/issues/7".to_owned(),
        ),
        (
            "html_url",
            "https://selected.test/renamed/repo/issues/7".to_owned(),
        ),
        (
            "repository_url",
            "https://other.test/api/v3/repos/renamed/repo".to_owned(),
        ),
        (
            "repository_url",
            format!("{}/repos/renamed/repo", server.uri()),
        ),
    ] {
        let mut bad = good.clone();
        bad[field] = value.into();
        items.push(bad.clone());
        let fixture = Mock::given(method("GET"))
            .and(path("/api/v3/repositories/41/issues/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(bad))
            .mount_as_scoped(&server)
            .await;
        assert!(issues::fetch_issue(&reader, 7).await.is_err());
        drop(fixture);
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/41/issues/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(good.clone()))
        .mount(&server)
        .await;
    assert_eq!(
        issues::fetch_issue(&reader, 7).await.unwrap().html_url,
        good["html_url"]
    );
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"total_count":items.len(),"incomplete_results":false,"items":items}),
        ))
        .mount(&server)
        .await;
    let page = issues::search_repository_issues(
        &searcher,
        "",
        issues::IssueSearchFilter {
            state: issues::IssueStateFilter::All,
            assignee: issues::IssueAssigneeFilter::Any,
        },
    )
    .await
    .unwrap();
    assert_eq!(page.issues.len(), 1);
    let comment = serde_json::json!({"id":1,"created_at":"now","updated_at":"now","html_url":"https://other.test/comment/1"});
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/41/issues/7/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([comment])))
        .mount(&server)
        .await;
    assert!(issues::fetch_issue_comments(&reader, 7).await.is_err());
}

#[tokio::test]
async fn candidate_and_account_clients_use_selected_instance_without_repository_identity() {
    use wiremock::matchers::query_param;
    let server = MockServer::start().await;
    let factory = ProviderClients::new().unwrap();
    Mock::given(method("GET"))
        .and(path("/v4/user"))
        .and(header("authorization", "Bearer candidate"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"id":9,"username":"person"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let candidate = factory.candidate(
        CandidateGrant::new(
            crate::ScmProvider::GitLab,
            &format!("{}/v4", server.uri()),
            "candidate".into(),
        )
        .unwrap(),
    );
    let identity = candidate.validate().await.unwrap();
    assert_eq!(identity.provider_user_id, "9");
    assert_eq!(identity.login, "person");
    Mock::given(method("GET")).and(path("/v4/projects")).and(query_param("membership","true")).and(query_param("page","2")).and(header("authorization","Bearer connection"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([{"id":7,"path_with_namespace":"group/repo","name":"repo","namespace":{"path":"group"},"visibility":"private","http_url_to_repo":"https://selected.test/group/repo.git"}]))).expect(1).mount(&server).await;
    Mock::given(method("GET"))
        .and(path("/v4/groups"))
        .and(query_param("min_access_level", "10"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{"id":3,"name":"group"}])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let account = factory.account(
        AccountGrant::new(
            crate::ScmProvider::GitLab,
            &format!("{}/v4", server.uri()),
            "connection".into(),
        )
        .unwrap(),
    );
    let repos = account.repositories(2).await.unwrap();
    assert_eq!(repos.repositories[0].full_name, "group/repo");
    assert!(repos.next_page.is_none());
    assert_eq!(account.organizations().await.unwrap()[0].id, "3");
    assert!(account.repositories(0).await.is_err());
}

#[test]
fn pull_selectors_bind_configured_web_origin_prefix_and_repository() {
    let endpoints = ProviderEndpoints::new(
        crate::ScmProvider::GitHub,
        "https://api.selected.test/v3",
        "https://selected.test/git",
    )
    .unwrap();
    assert_eq!(
        endpoints
            .select_pull_request("#42", Some("acme/repo"))
            .unwrap(),
        (42, Some("acme/repo".into()))
    );
    assert_eq!(
        endpoints
            .select_pull_request("https://selected.test/git/acme/repo/pull/42", None)
            .unwrap()
            .0,
        42
    );
    for bad in [
        "0",
        "-1",
        "https://github.com/acme/repo/pull/42",
        "https://selected.test/acme/repo/pull/42",
        "https://selected.test:444/git/acme/repo/pull/42",
        "https://user:secret@selected.test/git/acme/repo/pull/42",
        "https://selected.test/git/acme/repo/pull/42?token=secret",
        "https://selected.test/git/acme/repo/pull/42#fragment",
    ] {
        assert!(endpoints.select_pull_request(bad, None).is_err(), "{bad}");
    }
    assert!(endpoints
        .select_pull_request(
            "https://selected.test/git/acme/repo/pull/42",
            Some("other/repo")
        )
        .is_err());
}

#[tokio::test]
async fn pull_creation_preserves_validation_details_and_never_replays_unknown_writes() {
    let server = MockServer::start().await;
    let factory = ProviderClients::new().unwrap();
    let reader = factory.repository(
        RepositoryGrant::new(
            endpoints(&server),
            RepositoryIdentity::Name("owner/repo".into()),
            RepositoryCapability::PullRequestRead,
            "selected-token".into(),
        )
        .unwrap(),
    );
    assert!(reader
        .create_pull_request(CreatePullRequest {
            title: "title",
            head: "topic",
            base: "main",
            body: "body"
        })
        .await
        .is_err());
    assert!(server.received_requests().await.unwrap().is_empty());
    let writer = factory.repository(
        RepositoryGrant::new(
            endpoints(&server),
            RepositoryIdentity::Name("owner/repo".into()),
            RepositoryCapability::PullRequestWrite,
            "selected-token".into(),
        )
        .unwrap(),
    );
    for (status, message, effect) in [
        (422, "Validation Failed", crate::FailureEffect::Refused),
        (503, "unavailable", crate::FailureEffect::Unknown),
    ] {
        let fixture=Mock::given(method("POST")).and(path("/repos/owner/repo/pulls"))
            .and(body_json(serde_json::json!({"title":"title","head":"topic","base":"main","body":"body","maintainer_can_modify":true})))
            .respond_with(ResponseTemplate::new(status).insert_header("x-github-request-id","request-id").set_body_json(serde_json::json!({"message":message,"errors":[{"message":"not all refs are readable"},{"resource":"PullRequest","field":"head","code":"invalid"},"failed https://secret@example.test/private.git selected-token"]}))).expect(1).mount_as_scoped(&server).await;
        let error = writer
            .create_pull_request(CreatePullRequest {
                title: "title",
                head: "topic",
                base: "main",
                body: "body",
            })
            .await
            .unwrap_err();
        assert_eq!(error.effect, effect);
        assert_eq!(error.status, Some(status));
        assert_eq!(error.request_id.as_deref(), Some("request-id"));
        assert!(error.message.contains("not all refs are readable"));
        assert!(error.message.contains("PullRequest head invalid"));
        assert!(!error.message.contains("selected-token"));
        assert!(!error.message.contains("secret@example"));
        drop(fixture);
    }
}

#[tokio::test]
async fn report_scope_narrows_token_and_pins_assets_to_independent_destination() {
    let server = MockServer::start().await;
    let factory = ProviderClients::new().unwrap();
    let endpoints = ProviderEndpoints::new(
        crate::ScmProvider::GitHub,
        &server.uri(),
        "https://reports.test/git",
    )
    .unwrap();
    Mock::given(method("POST")).and(path("/app/installations/9/access_tokens")).and(body_json(serde_json::json!({"repositories":["reports"],"permissions":{"issues":"write","contents":"write"}})))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({"token":"report-token"}))).expect(1).mount(&server).await;
    let token = factory
        .app(AppGrant::new(endpoints.clone(), "jwt".into()))
        .installation_token(
            9,
            &InstallationTokenRequest::report_delivery("owner/reports").unwrap(),
        )
        .await
        .unwrap()
        .token;
    assert!(InstallationTokenRequest::report_delivery("owner/../reports").is_err());
    let report =
        factory.reports(ReportGrant::new(endpoints, "owner/reports".into(), token).unwrap());
    let id = uuid::Uuid::new_v4();
    Mock::given(method("PUT")).and(path(format!("/repos/owner/reports/contents/assets/issues/{id}.png"))).and(header("authorization","Bearer report-token"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({"content":{"html_url":"https://untrusted.test/blob/main"},"commit":{"sha":"abcdef"}}))).expect(1).mount(&server).await;
    assert_eq!(
        report.put_screenshot(id, b"png").await.unwrap(),
        format!("https://reports.test/git/owner/reports/raw/abcdef/assets/issues/{id}.png")
    );
    Mock::given(method("POST")).and(path("/repos/owner/reports/issues"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({"number":1,"html_url":"https://foreign.test/owner/reports/issues/1"}))).expect(1).mount(&server).await;
    assert_eq!(
        report
            .create_issue("title", "body", &[])
            .await
            .unwrap_err()
            .effect,
        crate::FailureEffect::Partial
    );
}

#[tokio::test]
async fn check_failures_redact_owner_secrets_before_diagnostic_and_header_caps() {
    let server = MockServer::start().await;
    let client = ProviderClients::new().unwrap().repository(
        RepositoryGrant::new(
            endpoints(&server),
            RepositoryIdentity::Name("owner/repo".into()),
            RepositoryCapability::ChecksRead,
            "provider-token".into(),
        )
        .unwrap(),
    );
    let secret = "SCOPED_CONFIG_SECRET_abcdefghijklmnopqrstuvwxyz".to_owned();
    let diagnostic = format!("{} {secret}", "x".repeat(1000));
    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/pulls/1"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-github-request-id", diagnostic.clone())
                .insert_header("retry-after", "60")
                .set_body_json(serde_json::json!({"message":diagnostic})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let error = client
        .read_pull_request_checks(1, &PullRequestChecksQuery::default(), &[secret])
        .await
        .unwrap_err();
    assert!(!format!("{error:?} {error}").contains("SCOPED_CONFIG_SECRET"));
    assert_eq!(error.retry_after.as_deref(), Some("60"));
    assert_eq!(error.status, Some(403));
}

#[tokio::test]
async fn semantic_merge_refusal_is_redacted_and_retains_response_evidence() {
    let server = MockServer::start().await;
    let client = ProviderClients::new().unwrap().repository(
        RepositoryGrant::new(
            endpoints(&server),
            RepositoryIdentity::Name("owner/repo".into()),
            RepositoryCapability::PullRequestMerge,
            "selected-token-canary".into(),
        )
        .unwrap(),
    );
    for merged in [false, true] {
        let fixture = Mock::given(method("PUT")).and(path("/repos/owner/repo/pulls/1/merge"))
            .respond_with(ResponseTemplate::new(200).insert_header("x-github-request-id","merge-request")
                .set_body_json(serde_json::json!({"merged":merged,"sha":"a".repeat(40),"message":"refused selected-token-canary"})))
            .expect(1).mount_as_scoped(&server).await;
        let result = client.merge_pull_request(1, &"a".repeat(40)).await;
        if merged {
            assert!(!result
                .unwrap()
                .message
                .unwrap()
                .contains("selected-token-canary"));
        } else {
            let error = result.unwrap_err();
            assert_eq!(error.effect, crate::FailureEffect::Refused);
            assert_eq!(error.status, Some(200));
            assert_eq!(error.request_id.as_deref(), Some("merge-request"));
            assert!(!format!("{error:?} {error}").contains("selected-token-canary"));
            assert!(error.to_string().contains("made no provider changes"));
        }
        drop(fixture);
    }
}

#[tokio::test]
async fn report_create_and_marker_results_are_bound_to_repository_resource_and_number() {
    let server = MockServer::start().await;
    let endpoints = ProviderEndpoints::new(
        crate::ScmProvider::GitHub,
        &server.uri(),
        "https://reports.test/git",
    )
    .unwrap();
    let report = ProviderClients::new()
        .unwrap()
        .reports(ReportGrant::new(endpoints, "owner/reports".into(), "token".into()).unwrap());
    let id = uuid::Uuid::new_v4();
    for (suffix, valid) in [
        ("owner/reports/issues/77", true),
        ("Owner/Reports/issues/77", true),
        ("other/reports/issues/77", false),
        ("owner/reports/pull/77", false),
        ("owner/reports/issues/5", false),
        ("owner/reports/issues/77?secret=1", false),
    ] {
        let issue = serde_json::json!({"number":77,"html_url":format!("https://reports.test/git/{suffix}"),"body":format!("<!-- g5-report:{id} -->")});
        let create = Mock::given(method("POST"))
            .and(path("/repos/owner/reports/issues"))
            .respond_with(ResponseTemplate::new(201).set_body_json(&issue))
            .expect(1)
            .mount_as_scoped(&server)
            .await;
        let lookup = Mock::given(method("GET"))
            .and(path("/search/issues"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"items":[issue]})),
            )
            .expect(1)
            .mount_as_scoped(&server)
            .await;
        let result = report.create_issue("title", "body", &[]).await;
        let found = report.find_issue_by_marker(id).await;
        if valid {
            assert_eq!(result.unwrap().number, 77);
            assert_eq!(found.unwrap().unwrap().number, 77);
        } else {
            assert_eq!(result.unwrap_err().effect, crate::FailureEffect::Partial);
            assert_eq!(found.unwrap_err().effect, crate::FailureEffect::Refused);
        }
        drop((create, lookup));
    }
}

#[test]
fn configured_clone_identity_binds_origin_port_prefix_and_repository_shape() {
    let endpoints = ProviderEndpoints::new(
        crate::ScmProvider::GitHub,
        "https://enterprise.test/api/v3",
        "https://enterprise.test:8443/git",
    )
    .unwrap();
    assert_eq!(
        endpoints
            .repository_name_from_clone_url("https://enterprise.test:8443/git/Org/Repo.git")
            .as_deref(),
        Some("Org/Repo")
    );
    for value in [
        "https://github.com/git/Org/Repo.git",
        "https://enterprise.test/git/Org/Repo.git",
        "https://enterprise.test:8443/Org/Repo.git",
        "https://enterprise.test:8443/git/Org/Repo/extra",
        "https://enterprise.test:8443/git/Org/Repo/extra/",
        "https://enterprise.test:8443/git/Org/",
        "https://token@enterprise.test:8443/git/Org/Repo.git",
        "https://enterprise.test:8443/git/Org/Repo.git?x=1",
        "https://enterprise.test:8443/git/../git/Org/Repo.git",
        "https://enterprise.test:8443/git/%2e%2e/Repo.git",
    ] {
        assert!(
            endpoints.repository_name_from_clone_url(value).is_none(),
            "{value}"
        );
    }
    let public = ProviderEndpoints::new(
        crate::ScmProvider::GitHub,
        "https://api.github.com",
        "https://github.com",
    )
    .unwrap();
    for value in [
        "https://github.com/Org/Repo.git",
        "git@github.com:Org/Repo.git",
        "ssh://git@github.com/Org/Repo.git",
        // Trailing slashes name the same repository.
        "https://github.com/Org/Repo/",
        "https://github.com/Org/Repo.git/",
        "https://github.com/Org/Repo",
    ] {
        assert_eq!(
            public.repository_name_from_clone_url(value).as_deref(),
            Some("Org/Repo")
        );
    }
    let policy =
        crate::ScmOriginPolicy::default().with_github_web_url("https://enterprise.test:8443/git");
    let provider = policy
        .provider_for_url("https://enterprise.test:8443/git/Org/Repo.git")
        .unwrap();
    assert!(provider.is_github());
    for url in [
        "https://enterprise.test:8443/other/Repo.git",
        "https://enterprise.test:8443/git/../other/Repo.git",
        "https://enterprise.test:8443/git/%2e%2e/other/Repo.git",
        "https://enterprise.test:8443/git-other/Repo.git",
    ] {
        assert!(!provider.allows(url), "{url}");
        assert!(policy.provider_for_url(url).is_none(), "{url}");
        assert!(matches!(
            crate::GitAuth::scm_token_for_provider("secret".into(), provider.clone(), url),
            crate::GitAuth::None
        ));
    }

    assert!(!provider.allows("https://github.com/Org/Repo.git"));
    assert!(!provider.allows("https://enterprise.test/git/Org/Repo.git"));
    assert!(policy
        .provider_for_url("https://github.com/Org/Repo.git")
        .is_none());
    assert!(crate::ScmOriginPolicy::default()
        .with_github_web_url("http://enterprise.test")
        .provider_for_url("https://github.com/Org/Repo.git")
        .is_none());
}

#[test]
fn configured_instance_identity_parser_strips_enterprise_prefix_and_port() {
    let instance = crate::ScmOriginPolicy::default()
        .with_github_web_url("https://ghe.test:8443/prefix")
        .provider_for_url("https://ghe.test:8443/prefix/Org/Repo.git")
        .unwrap();
    assert_eq!(
        instance
            .github_repository_name("https://ghe.test:8443/prefix/Org/Repo.git")
            .as_deref(),
        Some("Org/Repo")
    );
    for other in [
        "https://ghe.test:8443/Org/Repo.git",
        "https://ghe.test/prefix/Org/Repo.git",
        "https://ghe.test:8443/prefix/Org/Repo/extra.git",
        "https://ghe.test:8443/prefix/group/sub/Repo.git",
    ] {
        assert!(instance.github_repository_name(other).is_none(), "{other}");
    }
    assert_eq!(
        crate::ScmProvider::GitHub
            .github_repository_name("https://github.com/octocat/hello-world.git")
            .as_deref(),
        Some("octocat/hello-world")
    );
    assert!(crate::ScmProvider::GitLab
        .github_repository_name("https://gitlab.com/group/repo.git")
        .is_none());
}

/// A rate-limited answer is returned at once with its hints: there is no
/// hidden sleep-and-retry inside any client.
#[tokio::test]
async fn rate_limited_responses_return_promptly_without_replay() {
    let limited = || {
        ResponseTemplate::new(403)
            .insert_header("retry-after", "60")
            .set_body_json(
                serde_json::json!({"message":"You have exceeded a secondary rate limit"}),
            )
    };
    let factory = ProviderClients::new().unwrap();
    let bound = Duration::from_secs(5);

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/app/installations/9/access_tokens"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "60"))
        .expect(1)
        .mount(&server)
        .await;
    let started = std::time::Instant::now();
    let error = factory
        .app(AppGrant::new(endpoints(&server), "jwt".into()))
        .installation_token(9, &InstallationTokenRequest::default())
        .await
        .unwrap_err();
    assert!(started.elapsed() < bound, "{:?}", started.elapsed());
    assert!(error.rate_limited);
    assert_eq!(error.retry_after.as_deref(), Some("60"));
    server.verify().await;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/pulls"))
        .respond_with(limited())
        .expect(1)
        .mount(&server)
        .await;
    let writer = factory.repository(
        RepositoryGrant::new(
            endpoints(&server),
            RepositoryIdentity::Name("owner/repo".into()),
            RepositoryCapability::PullRequestWrite,
            "token".into(),
        )
        .unwrap(),
    );
    let started = std::time::Instant::now();
    let error = writer
        .create_pull_request(CreatePullRequest {
            title: "title",
            head: "topic",
            base: "main",
            body: "body",
        })
        .await
        .unwrap_err();
    assert!(started.elapsed() < bound, "{:?}", started.elapsed());
    assert!(error.rate_limited);
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert_eq!(error.effect, crate::FailureEffect::Refused);
    assert_eq!(error.retry_after.as_deref(), Some("60"));
    server.verify().await;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repositories/41/issues/7"))
        .respond_with(limited())
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/repositories/41/issues/7/comments"))
        .respond_with(limited())
        .expect(1)
        .mount(&server)
        .await;
    let started = std::time::Instant::now();
    let error = issues::fetch_issue(&repository(&server, RepositoryCapability::IssuesRead), 7)
        .await
        .unwrap_err();
    assert_eq!(
        error.kind,
        g5_contracts::error::ExternalIssueErrorKind::RateLimited
    );
    assert!(error.message.contains("retry-after 60"), "{error:?}");
    match issues::post_issue_comment(
        &repository(&server, RepositoryCapability::IssueCommentWrite),
        7,
        "hi",
    )
    .await
    {
        issues::CommentPostOutcome::Rejected(error) => assert_eq!(
            error.kind,
            g5_contracts::error::ExternalIssueErrorKind::RateLimited
        ),
        other => panic!("a rate-limited comment is a refusal: {other:?}"),
    }
    assert!(started.elapsed() < bound, "{:?}", started.elapsed());
    server.verify().await;
}

/// A PR create whose reply is lost after GitHub read the whole request has an
/// unknown effect: the caller is told to inspect current state and the client
/// never replays the write.
#[tokio::test]
async fn lost_pull_request_create_reply_is_unknown_and_never_replayed() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::io::AsyncReadExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let accepted = Arc::new(AtomicUsize::new(0));
    let complete = Arc::new(AtomicUsize::new(0));
    let server = tokio::spawn({
        let (accepted, complete) = (accepted.clone(), complete.clone());
        async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                accepted.fetch_add(1, Ordering::SeqCst);
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                loop {
                    let count = socket.read(&mut buffer).await.unwrap();
                    assert_ne!(count, 0, "client closed before sending the request");
                    request.extend_from_slice(&buffer[..count]);
                    let text = String::from_utf8_lossy(&request);
                    if let Some(end) = text.find("\r\n\r\n") {
                        let length = text[..end]
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if request.len() >= end + 4 + length {
                            assert!(text.starts_with("POST /repos/owner/repo/pulls "));
                            complete.fetch_add(1, Ordering::SeqCst);
                            break;
                        }
                    }
                }
                // The write was received in full; the reply is lost.
                drop(socket);
            }
        }
    });
    let writer = ProviderClients::new().unwrap().repository(
        RepositoryGrant::new(
            ProviderEndpoints::new(crate::ScmProvider::GitHub, &url, &url).unwrap(),
            RepositoryIdentity::Name("owner/repo".into()),
            RepositoryCapability::PullRequestWrite,
            "token".into(),
        )
        .unwrap(),
    );
    let error = writer
        .create_pull_request(CreatePullRequest {
            title: "title",
            head: "topic",
            base: "main",
            body: "body",
        })
        .await
        .unwrap_err();
    assert_eq!(error.effect, crate::FailureEffect::Unknown);
    assert_eq!(error.kind, ProviderErrorKind::Transport);
    assert_eq!(error.status, None);
    assert!(
        error
            .to_string()
            .contains("the result is unknown; inspect current state before retrying"),
        "{error}"
    );
    // Give a hypothetical replay time to arrive before counting.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(accepted.load(Ordering::SeqCst), 1, "no replayed connection");
    assert_eq!(
        complete.load(Ordering::SeqCst),
        1,
        "exactly one complete write"
    );
    server.abort();
}
#[tokio::test]
async fn inventory_pages_follow_the_link_header_not_page_size() {
    let server = MockServer::start().await;
    let page = |start: u64, count: u64| {
        serde_json::json!({"repositories": (start..start + count)
            .map(|id| serde_json::json!({"id": id, "full_name": format!("o/r{id}"), "private": false}))
            .collect::<Vec<_>>()})
    };
    // An exactly full page without `rel="next"` is the complete inventory.
    Mock::given(method("GET"))
        .and(path("/installation/repositories"))
        .and(wiremock::matchers::query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(1, 100)))
        .expect(1)
        .mount(&server)
        .await;
    let client = ProviderClients::new()
        .unwrap()
        .installation(InstallationGrant::new(endpoints(&server), 9, "t".into()));
    assert_eq!(client.repositories().await.unwrap().len(), 100);
    server.verify().await;
    server.reset().await;
    // A short page that still advertises `rel="next"` continues.
    Mock::given(method("GET"))
        .and(path("/installation/repositories"))
        .and(wiremock::matchers::query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", "<https://example.invalid?page=2>; rel=\"next\"")
                .set_body_json(page(1, 2)),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/installation/repositories"))
        .and(wiremock::matchers::query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page(3, 1)))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(client.repositories().await.unwrap().len(), 3);
    server.verify().await;
}
#[tokio::test]
async fn same_origin_redirects_reach_a_moved_repository_and_cross_origin_ones_do_not() {
    let server = MockServer::start().await;
    let other = MockServer::start().await;
    let client = ProviderClients::new().unwrap().repository(
        RepositoryGrant::new(
            endpoints(&server),
            RepositoryIdentity::Name("old/repo".into()),
            RepositoryCapability::PullRequestRead,
            "selected-token-canary".into(),
        )
        .unwrap(),
    );
    let pull = |owner_repo: &str| {
        serde_json::json!({"number": 7, "html_url": format!("{}/{owner_repo}/pull/7", server.uri()),
            "state": "open", "head": {"sha": "a".repeat(40)}, "base": {"sha": "b".repeat(40)}})
    };
    // A renamed repository: GitHub answers the old name with a same-origin
    // redirect to the repository id, and the PR now names the new repository.
    Mock::given(method("GET"))
        .and(path("/repos/old/repo/pulls/7"))
        .respond_with(ResponseTemplate::new(307).insert_header(
            "location",
            format!("{}/repositories/41/pulls/7", server.uri()),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repositories/41/pulls/7"))
        .and(header("authorization", "Bearer selected-token-canary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pull("new/repo")))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(client.pull_request(7).await.unwrap().number, 7);
    server.verify().await;
    server.reset().await;
    // Without a redirect, a PR URL naming another repository is still refused.
    Mock::given(method("GET"))
        .and(path("/repos/old/repo/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pull("new/repo")))
        .expect(1)
        .mount(&server)
        .await;
    assert!(client.pull_request(7).await.is_err());
    server.verify().await;
    server.reset().await;
    // A cross-origin redirect is returned, and the other origin sees nothing.
    Mock::given(method("GET"))
        .and(path("/repos/old/repo/pulls/7"))
        .respond_with(ResponseTemplate::new(307).insert_header(
            "location",
            format!("{}/repositories/41/pulls/7", other.uri()),
        ))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(client.pull_request(7).await.unwrap_err().status, Some(307));
    server.verify().await;
    assert!(other.received_requests().await.unwrap().is_empty());
}

#[test]
fn graphql_endpoint_sits_beside_the_rest_root() {
    for (api, expected) in [
        ("https://api.github.com", "https://api.github.com/graphql"),
        ("https://api.github.com/", "https://api.github.com/graphql"),
        ("https://ghe.test/api/v3", "https://ghe.test/api/graphql"),
        ("https://ghe.test/api/v3/", "https://ghe.test/api/graphql"),
        (
            "http://127.0.0.1:9/prefix",
            "http://127.0.0.1:9/prefix/graphql",
        ),
    ] {
        let endpoints =
            ProviderEndpoints::new(crate::ScmProvider::GitHub, api, "https://ghe.test").unwrap();
        assert_eq!(endpoints.graphql().as_str(), expected, "{api}");
    }
}

#[tokio::test]
async fn pull_request_read_reports_review_threads_with_resolution_and_reviews() {
    use wiremock::matchers::{body_partial_json, body_string_contains};
    let server = MockServer::start().await;
    let client = ProviderClients::new().unwrap().repository(
        RepositoryGrant::new(
            endpoints(&server),
            RepositoryIdentity::Name("acme/api".into()),
            RepositoryCapability::PullRequestRead,
            "selected-token-canary".into(),
        )
        .unwrap(),
    );
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "node_id":"PR_42","number":42,"title":"Fix","state":"open","review_comments":3
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/issues/42/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;
    let comment = |id: u64, url: String| {
        serde_json::json!({"databaseId":id,"author":{"login":"coderabbitai"},"body":format!("Finding {id}"),
            "createdAt":"2026-10-08T00:00:00Z","url":url,"diffHunk":format!("@@ hunk {id}"),"commit":{"oid":"new"},"originalCommit":{"oid":"old"}})
    };
    let thread_page = |after: serde_json::Value, next: bool, thread: serde_json::Value| {
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(header("authorization", "Bearer selected-token-canary"))
            .and(body_string_contains("reviewThreads(first:25"))
            .and(body_partial_json(
                serde_json::json!({"variables":{"id":"PR_42","after":after}}),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"data":{"node":{"reviewThreads":{
                "pageInfo":{"hasNextPage":next,"endCursor":"c1"},"nodes":[thread]}}}})),
            )
            .expect(1)
    };
    thread_page(serde_json::Value::Null, true, serde_json::json!({
        "id":"PRRT_1","isResolved":true,"isOutdated":true,"path":"src/a.rs","line":null,"originalLine":7,
        "diffSide":"RIGHT","subjectType":"LINE","resolvedBy":{"login":"author"},
        "comments":{"totalCount":1,"nodes":[comment(1, format!("{}/acme/api/pull/42#discussion_r1", server.uri()))]}
    }))
    .mount(&server)
    .await;
    thread_page(
        serde_json::json!("c1"),
        false,
        serde_json::json!({
            "isResolved":false,"isOutdated":false,"path":"src/b.rs","line":12,"startLine":10,
            "diffSide":"RIGHT","subjectType":"LINE","resolvedBy":null,
            "comments":{"totalCount":2,"nodes":[comment(2, "https://evil.test/r2".into())]}
        }),
    )
    .mount(&server)
    .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("reviews(first:25"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":{"node":{"reviews":{
            "pageInfo":{"hasNextPage":false,"endCursor":null},
            "nodes":[{"id":"PRR_9","author":{"login":"claude"},"state":"CHANGES_REQUESTED","body":"Summary",
                "submittedAt":"2026-10-08T00:00:00Z","url":"https://evil.test/r9","commit":{"oid":"new"}}]}}}})))
        .expect(1)
        .mount(&server)
        .await;
    let comments = PullRequestReadScope {
        comments: true,
        reviews: false,
    };
    let pull = client.read_pull_request(42, comments).await.unwrap();
    let threads = pull["reviewThreads"].as_array().unwrap();
    assert_eq!(threads.len(), 2);
    assert_eq!(pull["reviewThreadsHasMore"], false);
    assert_eq!(threads[0]["isResolved"], true);
    assert_eq!(threads[0]["isOutdated"], true);
    // Writes name a thread by its node ID; the first comment carries the diff context.
    assert_eq!(threads[0]["id"], "PRRT_1");
    assert_eq!(threads[0]["diffHunk"], "@@ hunk 1");
    assert_eq!(threads[0]["line"], serde_json::Value::Null);
    assert_eq!(threads[0]["originalLine"], 7);
    assert_eq!(threads[0]["resolvedBy"]["login"], "author");
    assert_eq!(threads[0]["commentsHasMore"], false);
    assert_eq!(threads[0]["comments"][0]["originalCommit"]["oid"], "old");
    assert_eq!(threads[0]["comments"][0]["commit"]["oid"], "new");
    assert!(threads[0]["comments"][0]["url"]
        .as_str()
        .unwrap()
        .ends_with("discussion_r1"));
    assert_eq!(threads[1]["path"], "src/b.rs");
    assert_eq!(threads[1]["resolvedBy"], serde_json::Value::Null);
    assert_eq!(threads[1]["commentsHasMore"], true);
    assert_eq!(threads[1]["comments"][0]["url"], serde_json::Value::Null);
    assert_eq!(pull["reviews"][0]["state"], "CHANGES_REQUESTED");
    // Reviews keep gh's shape: node ID and nested commit.
    assert_eq!(pull["reviews"][0]["id"], "PRR_9");
    assert_eq!(
        pull["reviews"][0]["commit"],
        serde_json::json!({"oid":"new"})
    );
    assert!(pull["reviews"][0].get("commitOid").is_none());
    assert!(pull.get("reviewDecision").is_none());
    assert_eq!(pull["reviews"][0]["url"], serde_json::Value::Null);
    assert_eq!(pull["reviewsHasMore"], false);
    // Each query is well formed and sent as a read.
    for request in server.received_requests().await.unwrap() {
        if request.method.as_str() == "POST" {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            let query = body["query"].as_str().unwrap();
            assert_eq!(
                query.matches('{').count(),
                query.matches('}').count(),
                "{query}"
            );
        }
    }
    // Without comments, no review query is made.
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "node_id":"PR_42","number":42,"title":"Fix","state":"open"
        })))
        .mount(&server)
        .await;
    assert!(client
        .read_pull_request(42, PullRequestReadScope::default())
        .await
        .unwrap()
        .get("reviewThreads")
        .is_none());
    assert!(server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.method.as_str() == "GET"));
    // The review summary is one query, without reading comments.
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "node_id":"PR_42","number":42,"title":"Fix","state":"open"
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("latestReviews(first:100"))
        .and(body_partial_json(serde_json::json!({"variables":{"id":"PR_42"}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":{"node":{
            "reviewDecision":"CHANGES_REQUESTED",
            "latestReviews":{"pageInfo":{"hasNextPage":false},"nodes":[
                {"id":"PRR_1","author":{"login":"claude"},"state":"CHANGES_REQUESTED","body":"","commit":null}]}}}})))
        .expect(1)
        .mount(&server)
        .await;
    let pull = client
        .read_pull_request(
            42,
            PullRequestReadScope {
                comments: false,
                reviews: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(pull["reviewDecision"], "CHANGES_REQUESTED");
    assert_eq!(pull["latestReviews"][0]["id"], "PRR_1");
    assert_eq!(pull["latestReviews"][0]["commit"], serde_json::Value::Null);
    assert_eq!(pull["latestReviewsHasMore"], false);
    assert!(pull.get("comments").is_none() && pull.get("reviews").is_none());
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2, "one REST read and one GraphQL read");
    let body: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    let query = body["query"].as_str().unwrap();
    assert_eq!(
        query.matches('{').count(),
        query.matches('}').count(),
        "{query}"
    );
    // GraphQL errors arrive with HTTP 200 and still fail the read as a
    // refusal, naming the refused field. A grant without contents read gets
    // partial data with the review's commit refused (issue #486).
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/issues/42/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data":{"node":{"reviewThreads":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[
                {"id":"PRRT_1","comments":{"totalCount":1,"nodes":[{"databaseId":7,"commit":null,"originalCommit":null}]}}]}}},
            "errors":[{"type":"FORBIDDEN","path":["node","reviewThreads","nodes",0,"comments","nodes",0,"commit"],
                "message":"Resource not accessible by integration"}]
        })))
        .mount(&server)
        .await;
    let error = client.read_pull_request(42, comments).await.unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Rejected);
    let message = error.to_string();
    assert!(message.contains("no provider changes"), "{message}");
    assert!(
        message.contains(
            "Resource not accessible by integration (at node.reviewThreads.nodes.0.comments.nodes.0.commit)"
        ),
        "{message}"
    );
    assert!(!message.contains("selected-token-canary"));
}

/// GitHub's `mergeable: null` means it is still computing: an open PR is
/// re-read a bounded number of times, while a closed PR or a known answer is
/// read once.
#[tokio::test]
async fn pull_request_read_rereads_mergeability_github_is_still_computing() {
    let server = MockServer::start().await;
    let client = ProviderClients::new().unwrap().repository(
        RepositoryGrant::new(
            endpoints(&server),
            RepositoryIdentity::Name("acme/api".into()),
            RepositoryCapability::PullRequestRead,
            "selected-token-canary".into(),
        )
        .unwrap(),
    );
    let pull = |state: &str, mergeable: serde_json::Value, merge_state: &str| {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "node_id":"PR_42","number":42,"title":"Fix","state":state,
            "mergeable":mergeable,"mergeable_state":merge_state
        }))
    };
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .respond_with(pull("open", serde_json::Value::Null, "unknown"))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .respond_with(pull("open", serde_json::json!(false), "dirty"))
        .expect(1)
        .mount(&server)
        .await;
    let read = client
        .read_pull_request(42, PullRequestReadScope::default())
        .await
        .unwrap();
    assert_eq!(read["mergeable"], false);
    assert_eq!(read["mergeStateStatus"], "dirty");
    server.verify().await;
    // Still computing after the bounded re-reads: reported as unknown.
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .respond_with(pull("open", serde_json::Value::Null, "unknown"))
        .expect(4)
        .mount(&server)
        .await;
    let started = std::time::Instant::now();
    let read = client
        .read_pull_request(42, PullRequestReadScope::default())
        .await
        .unwrap();
    assert!(read["mergeable"].is_null());
    assert!(started.elapsed() < Duration::from_secs(5));
    server.verify().await;
    // A closed PR has no mergeability to wait for.
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .respond_with(pull("closed", serde_json::Value::Null, "unknown"))
        .expect(1)
        .mount(&server)
        .await;
    client
        .read_pull_request(42, PullRequestReadScope::default())
        .await
        .unwrap();
    server.verify().await;
}

#[tokio::test]
async fn pull_request_conversation_writes_are_classified_and_bound_to_the_repository() {
    use wiremock::matchers::{body_partial_json, body_string_contains};
    let server = MockServer::start().await;
    let client = |capability| {
        ProviderClients::new().unwrap().repository(
            RepositoryGrant::new(
                endpoints(&server),
                RepositoryIdentity::Name("acme/api".into()),
                capability,
                "user-token-canary".into(),
            )
            .unwrap(),
        )
    };
    let reader = client(RepositoryCapability::PullRequestRead);
    let writer = client(RepositoryCapability::PullRequestWrite);
    // A read grant never writes.
    assert!(reader.post_pull_request_comment(42, "x").await.is_err());
    assert!(reader.reply_to_review_thread("PRRT_1", "x").await.is_err());
    assert!(reader
        .set_review_thread_resolution("PRRT_1", true)
        .await
        .is_err());
    assert!(server.received_requests().await.unwrap().is_empty());

    // A discussion comment is an issue comment on the PR, as the selected user.
    Mock::given(method("POST"))
        .and(path("/repos/acme/api/issues/42/comments"))
        .and(header("authorization", "Bearer user-token-canary"))
        .and(body_json(
            serde_json::json!({"body":"@coderabbitai review"}),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": 7, "html_url": format!("{}/acme/api/pull/42#issuecomment-7", server.uri())
        })))
        .expect(1)
        .mount(&server)
        .await;
    let posted = writer
        .post_pull_request_comment(42, "@coderabbitai review")
        .await
        .unwrap();
    assert_eq!(posted.id, 7);
    // A 5xx after sending is unknown, never a refusal.
    server.reset().await;
    Mock::given(method("POST"))
        .and(path("/repos/acme/api/issues/42/comments"))
        .respond_with(ResponseTemplate::new(502))
        .expect(1)
        .mount(&server)
        .await;
    let error = writer.post_pull_request_comment(42, "x").await.unwrap_err();
    assert_eq!(error.effect, crate::FailureEffect::Unknown);
    // A comment link outside the configured origin is not accepted as proof.
    server.reset().await;
    Mock::given(method("POST"))
        .and(path("/repos/acme/api/issues/42/comments"))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(serde_json::json!({"id": 8, "html_url": "https://evil.test/c8"})),
        )
        .mount(&server)
        .await;
    let error = writer.post_pull_request_comment(42, "x").await.unwrap_err();
    assert_ne!(error.effect, crate::FailureEffect::Refused);

    // Reconciliation finds the marker among comments since submission.
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/issues/42/comments"))
        .and(wiremock::matchers::query_param("since", "2026-10-09T00:00:00Z"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"id": 5, "body": "other", "created_at": "2026-10-09T00:00:01Z",
             "html_url": format!("{}/acme/api/pull/42#issuecomment-5", server.uri())},
            {"id": 6, "body": "reply\n\n<!-- g5-comment m -->\n", "created_at": "2026-10-09T00:00:02Z",
             "html_url": format!("{}/acme/api/pull/42#issuecomment-6", server.uri())}
        ])))
        .mount(&server)
        .await;
    let found = writer
        .find_marked_pull_request_comment(42, "2026-10-09T00:00:00Z", "<!-- g5-comment m -->")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.id, 6);
    assert!(writer
        .find_marked_pull_request_comment(42, "2026-10-09T00:00:00Z", "<!-- g5-comment z -->")
        .await
        .unwrap()
        .is_none());

    // A thread is read before a write names it; another repository's is not found.
    server.reset().await;
    let thread = |repository: &str| {
        serde_json::json!({"data":{"node":{"id":"PRRT_1","isResolved":false,
            "pullRequest":{"number":42,"repository":{"nameWithOwner":repository}},
            "comments":{"nodes":[{"databaseId":11,"body":"done\n\n<!-- g5-comment m -->",
                "url":format!("{}/acme/api/pull/42#discussion_r11", server.uri())}]}}}})
    };
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("PullRequestReviewThread"))
        .and(body_partial_json(
            serde_json::json!({"variables":{"id":"PRRT_1"}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(thread("ACME/api")))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let state = writer
        .review_thread("PRRT_1", Some("<!-- g5-comment m -->"))
        .await
        .unwrap();
    assert_eq!(
        (
            state.is_resolved,
            state.pull_number,
            state.marked.map(|c| c.id)
        ),
        (false, 42, Some(11))
    );
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("PullRequestReviewThread"))
        .respond_with(ResponseTemplate::new(200).set_body_json(thread("other/api")))
        .mount(&server)
        .await;
    let error = writer.review_thread("PRRT_1", None).await.unwrap_err();
    assert_eq!(
        (error.status, error.effect),
        (Some(404), crate::FailureEffect::Refused)
    );

    // A reply is a mutation; GraphQL refusals refuse, other errors stay unknown.
    server.reset().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("addPullRequestReviewThreadReply"))
        .and(body_partial_json(
            serde_json::json!({"variables":{"id":"PRRT_1","body":"fixed"}}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":{
            "addPullRequestReviewThreadReply":{"comment":{"databaseId":12,
                "url":format!("{}/acme/api/pull/42#discussion_r12", server.uri())}}}})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    assert_eq!(
        writer
            .reply_to_review_thread("PRRT_1", "fixed")
            .await
            .unwrap()
            .id,
        12
    );
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("addPullRequestReviewThreadReply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data":null,"errors":[{"type":"FORBIDDEN","message":"Resource not accessible by integration"}]
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let error = writer
        .reply_to_review_thread("PRRT_1", "fixed")
        .await
        .unwrap_err();
    assert_eq!(error.effect, crate::FailureEffect::Refused);
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("addPullRequestReviewThreadReply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data":null,"errors":[{"message":"Something went wrong"}]
        })))
        .mount(&server)
        .await;
    let error = writer
        .reply_to_review_thread("PRRT_1", "fixed")
        .await
        .unwrap_err();
    assert_ne!(error.effect, crate::FailureEffect::Refused);

    // Resolution reports GitHub's state, in either direction.
    server.reset().await;
    for (field, resolved) in [
        ("resolveReviewThread", true),
        ("unresolveReviewThread", false),
    ] {
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .and(body_string_contains(format!("{{{field}(input")))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":{
                field:{"thread":{"id":"PRRT_1","isResolved":resolved}}}})),
            )
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(
            writer
                .set_review_thread_resolution("PRRT_1", resolved)
                .await
                .unwrap(),
            resolved
        );
    }
    for request in server.received_requests().await.unwrap() {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        let query = body["query"].as_str().unwrap();
        assert!(query.starts_with("mutation"), "{query}");
        assert_eq!(query.matches('{').count(), query.matches('}').count());
    }
}

#[tokio::test]
async fn issue_edit_sends_only_the_edited_fields_and_checks_assignees() {
    let server = MockServer::start().await;
    let reader = repository(&server, RepositoryCapability::IssuesRead);
    let edit = issues::IssueEdit {
        title: Some("Renamed".to_string()),
        assignees: Some(vec!["octo".to_string()]),
        ..issues::IssueEdit::default()
    };
    // Only an issue-write grant may edit; a reader is refused before HTTP.
    assert!(matches!(
        issues::update_issue(&reader, 12, &edit).await,
        issues::IssueUpdateOutcome::Rejected(_)
    ));
    assert!(server.received_requests().await.unwrap().is_empty());

    let writer = repository(&server, RepositoryCapability::IssueWrite);
    Mock::given(method("PATCH"))
        .and(path("/repositories/41/issues/12"))
        .and(header("authorization", "Bearer selected-token-canary"))
        .and(body_json(
            serde_json::json!({"title": "Renamed", "assignees": ["octo"]}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 9001, "number": 12, "title": "Renamed", "body": "b", "state": "open",
            "assignees": [{"id": 7, "login": "octo"}],
            "html_url": format!("{}/acme/web/issues/12", server.uri()),
            "created_at": "2026-10-13T00:00:00Z", "updated_at": "2026-10-29T00:00:00Z"
        })))
        .expect(1)
        .mount(&server)
        .await;
    match issues::update_issue(&writer, 12, &edit).await {
        issues::IssueUpdateOutcome::Updated(updated) => {
            assert_eq!(updated.title, "Renamed");
            assert_eq!(updated.assignees[0].login, "octo");
        }
        other => panic!("expected an updated issue: {other:?}"),
    }
    server.verify().await;

    // 204 is assignable, 404 is not, anything else is an error.
    for (login, status) in [("octo", 204), ("nobody", 404), ("flaky", 502)] {
        Mock::given(method("GET"))
            .and(path(format!("/repositories/41/assignees/{login}")))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
    }
    assert!(issues::is_assignable(&reader, "octo").await.unwrap());
    assert!(!issues::is_assignable(&reader, "nobody").await.unwrap());
    assert!(issues::is_assignable(&reader, "flaky").await.is_err());
}

#[tokio::test]
async fn issue_state_update_is_classified() {
    let server = MockServer::start().await;
    let issue = |state: &str, reason: Option<&str>| {
        serde_json::json!({
            "id": 9001, "number": 12, "title": "Fix it", "body": "b", "state": state,
            "state_reason": reason,
            "html_url": format!("{}/acme/web/issues/12", server.uri()),
            "created_at": "2026-10-13T00:00:00Z", "updated_at": "2026-10-15T00:00:00Z"
        })
    };
    // Only an issue-write grant may change state; a reader is refused before HTTP.
    let reader = repository(&server, RepositoryCapability::IssuesRead);
    assert!(matches!(
        issues::update_issue_state(&reader, 12, issues::IssueStateChange::Close).await,
        issues::IssueUpdateOutcome::Rejected(_)
    ));
    assert!(server.received_requests().await.unwrap().is_empty());

    // Close sends `completed`; reopen sends only the state.
    let writer = repository(&server, RepositoryCapability::IssueWrite);
    Mock::given(method("PATCH"))
        .and(path("/repositories/41/issues/12"))
        .and(header("authorization", "Bearer selected-token-canary"))
        .and(body_json(
            serde_json::json!({"state": "closed", "state_reason": "completed"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(issue("closed", Some("completed"))))
        .expect(1)
        .mount(&server)
        .await;
    match issues::update_issue_state(&writer, 12, issues::IssueStateChange::Close).await {
        issues::IssueUpdateOutcome::Updated(updated) => {
            assert_eq!(updated.state, "closed");
            assert_eq!(updated.state_reason.as_deref(), Some("completed"));
        }
        other => panic!("expected an updated issue: {other:?}"),
    }
    server.verify().await;
    server.reset().await;
    Mock::given(method("PATCH"))
        .and(path("/repositories/41/issues/12"))
        .and(body_json(serde_json::json!({"state": "open"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(issue("open", Some("reopened"))))
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        issues::update_issue_state(&writer, 12, issues::IssueStateChange::Reopen).await,
        issues::IssueUpdateOutcome::Updated(_)
    ));
    server.verify().await;

    // A transferred issue answers a write with a redirect: the write is not
    // followed into a repository the admission never checked, and the 3xx is
    // a refusal.
    server.reset().await;
    Mock::given(method("PATCH"))
        .and(path("/repositories/41/issues/12"))
        .respond_with(ResponseTemplate::new(301).insert_header(
            "location",
            format!("{}/repositories/99/issues/12", server.uri()),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/repositories/99/issues/12"))
        .respond_with(ResponseTemplate::new(200).set_body_json(issue("closed", Some("completed"))))
        .expect(0)
        .named("the redirect destination receives no write")
        .mount(&server)
        .await;
    match issues::update_issue_state(&writer, 12, issues::IssueStateChange::Close).await {
        issues::IssueUpdateOutcome::Rejected(_) => {}
        other => panic!("a redirected write must be refused: {other:?}"),
    }
    server.verify().await;

    // A refusal is final (403 keeps its access meaning, 422 is the request's);
    // a 5xx is unknown.
    for (status, body, expect) in [
        (
            403,
            serde_json::json!({"message": "Must have push access"}),
            Some(g5_contracts::error::ExternalIssueErrorKind::AccessDenied),
        ),
        (
            422,
            serde_json::json!({"message": "Validation Failed"}),
            Some(g5_contracts::error::ExternalIssueErrorKind::InvalidReference),
        ),
        (502, serde_json::json!({}), None),
    ] {
        server.reset().await;
        Mock::given(method("PATCH"))
            .and(path("/repositories/41/issues/12"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .expect(1)
            .mount(&server)
            .await;
        let outcome =
            issues::update_issue_state(&writer, 12, issues::IssueStateChange::Close).await;
        match (expect, outcome) {
            (Some(kind), issues::IssueUpdateOutcome::Rejected(error)) => {
                assert_eq!(error.kind, kind, "status {status}")
            }
            (None, issues::IssueUpdateOutcome::Unknown(_)) => {}
            (_, other) => panic!("status {status}: {other:?}"),
        }
        server.verify().await;
    }
}

#[tokio::test]
async fn issue_creation_is_classified_and_the_creator_listing_skips_pull_requests() {
    use wiremock::matchers::query_param;
    let server = MockServer::start().await;
    let issue = |id: u64, number: u64, body: &str| {
        serde_json::json!({
            "id": id, "number": number, "title": "Fix it", "body": body, "state": "open",
            "html_url": format!("{}/acme/web/issues/{number}", server.uri()),
            "created_at": "2026-10-13T00:00:00Z", "updated_at": "2026-10-13T00:00:00Z"
        })
    };
    // Only an issue-write grant creates; a reader is refused before HTTP.
    let reader = repository(&server, RepositoryCapability::IssuesRead);
    assert!(matches!(
        issues::create_issue(&reader, "t", "b", &[], &[]).await,
        issues::IssueCreateOutcome::Rejected(_)
    ));
    assert!(server.received_requests().await.unwrap().is_empty());

    let writer = repository(&server, RepositoryCapability::IssueWrite);
    Mock::given(method("POST"))
        .and(path("/repositories/41/issues"))
        .and(header("authorization", "Bearer selected-token-canary"))
        .and(body_json(serde_json::json!({
            "title": "Fix it", "body": "Body\n\n<!-- g5-issue x -->", "assignees": ["octo"]
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(issue(9001, 12, "Body")))
        .expect(1)
        .mount(&server)
        .await;
    match issues::create_issue(
        &writer,
        "Fix it",
        "Body\n\n<!-- g5-issue x -->",
        &["octo".to_string()],
        &[],
    )
    .await
    {
        issues::IssueCreateOutcome::Created(created) => {
            assert_eq!((created.id.as_str(), created.number), ("9001", 12));
        }
        other => panic!("expected a created issue: {other:?}"),
    }
    server.verify().await;

    // A validation refusal is final; a 5xx or a misplaced link is unknown.
    for (status, body, refused) in [
        (
            422,
            serde_json::json!({"message": "Validation Failed"}),
            true,
        ),
        (502, serde_json::json!({}), false),
    ] {
        server.reset().await;
        Mock::given(method("POST"))
            .and(path("/repositories/41/issues"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .expect(1)
            .mount(&server)
            .await;
        let outcome = issues::create_issue(&writer, "t", "b", &[], &[]).await;
        match (refused, outcome) {
            (true, issues::IssueCreateOutcome::Rejected(error)) => assert_eq!(
                error.kind,
                g5_contracts::error::ExternalIssueErrorKind::InvalidReference
            ),
            (false, issues::IssueCreateOutcome::Unknown(_)) => {}
            (_, other) => panic!("status {status}: {other:?}"),
        }
        server.verify().await;
    }
    server.reset().await;
    let mut outside = issue(9002, 13, "b");
    outside["html_url"] = serde_json::json!("https://evil.test/acme/web/issues/13");
    Mock::given(method("POST"))
        .and(path("/repositories/41/issues"))
        .respond_with(ResponseTemplate::new(201).set_body_json(outside))
        .mount(&server)
        .await;
    assert!(matches!(
        issues::create_issue(&writer, "t", "b", &[], &[]).await,
        issues::IssueCreateOutcome::Unknown(_)
    ));

    // The creator listing reads newest first and drops pull requests.
    server.reset().await;
    let mut pull = issue(9003, 14, "pr");
    pull["pull_request"] = serde_json::json!({"url": "x"});
    Mock::given(method("GET"))
        .and(path("/repositories/41/issues"))
        .and(query_param("creator", "octo"))
        .and(query_param("state", "all"))
        .and(query_param("sort", "created"))
        .and(query_param("direction", "desc"))
        .and(query_param("since", "2026-10-13T00:00:00Z"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!([pull, issue(9001, 12, "Body")])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (found, more) = issues::fetch_issues_created_by(&reader, "octo", "2026-10-13T00:00:00Z", 1)
        .await
        .unwrap();
    assert!(!more);
    assert_eq!(
        found.iter().map(|issue| issue.number).collect::<Vec<_>>(),
        vec![12]
    );
    // Assignable users come from the repository's assignees.
    Mock::given(method("GET"))
        .and(path("/repositories/41/assignees"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!([{"id": 5, "login": "octo"}])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let users = issues::fetch_assignable_users(&reader).await.unwrap();
    assert_eq!(users[0].login, "octo");
    server.verify().await;
}

#[tokio::test]
async fn sub_issue_attachment_is_classified_and_a_missing_parent_is_none() {
    let server = MockServer::start().await;
    let issue = |id: u64, number: u64| {
        serde_json::json!({
            "id": id, "number": number, "title": "Epic", "body": null, "state": "open",
            "html_url": format!("{}/acme/web/issues/{number}", server.uri()),
            "repository_url": format!("{}/repos/acme/web", server.uri()),
            "created_at": "2026-10-13T00:00:00Z", "updated_at": "2026-10-13T00:00:00Z"
        })
    };
    // Only an issue-write grant attaches; a reader is refused before HTTP.
    let reader = repository(&server, RepositoryCapability::IssuesRead);
    assert!(matches!(
        issues::add_sub_issue(&reader, 12, 9001).await,
        issues::SubIssueOutcome::Rejected(_)
    ));
    assert!(server.received_requests().await.unwrap().is_empty());

    // The new issue's REST id goes under the parent's number, never
    // replacing an existing parent.
    let writer = repository(&server, RepositoryCapability::IssueWrite);
    Mock::given(method("POST"))
        .and(path("/repositories/41/issues/12/sub_issues"))
        .and(header("authorization", "Bearer selected-token-canary"))
        .and(body_json(serde_json::json!({"sub_issue_id": 9001})))
        .respond_with(ResponseTemplate::new(201).set_body_json(issue(8001, 12)))
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        issues::add_sub_issue(&writer, 12, 9001).await,
        issues::SubIssueOutcome::Attached
    ));
    server.verify().await;

    // A refusal is final; a 5xx may have applied.
    for (status, refused) in [(422, true), (403, true), (502, false)] {
        server.reset().await;
        Mock::given(method("POST"))
            .and(path("/repositories/41/issues/12/sub_issues"))
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_json(serde_json::json!({"message": "Validation Failed"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        match (refused, issues::add_sub_issue(&writer, 12, 9001).await) {
            (true, issues::SubIssueOutcome::Rejected(_))
            | (false, issues::SubIssueOutcome::Unknown(_)) => {}
            (_, other) => panic!("status {status}: {other:?}"),
        }
        server.verify().await;
    }

    // The parent read: the parent issue, or none (404); other failures stay
    // errors.
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/repositories/41/issues/30/parent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(issue(8001, 12)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repositories/41/issues/31/parent"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(serde_json::json!({"message": "Not Found"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repositories/41/issues/32/parent"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let parent = issues::fetch_parent_issue(&reader, 30)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((parent.id.as_str(), parent.number), ("8001", 12));
    assert!(issues::fetch_parent_issue(&reader, 31)
        .await
        .unwrap()
        .is_none());
    assert!(issues::fetch_parent_issue(&reader, 32).await.is_err());
}

#[tokio::test]
async fn pull_description_reads_full_media_and_maps_lifecycle_from_provider_fields() {
    let server = MockServer::start().await;
    let pulls = [
        (
            1,
            serde_json::json!({"state":"open","draft":false,"merged":false,"merged_at":null}),
            PullRequestLifecycle::Open,
        ),
        (
            2,
            serde_json::json!({"state":"open","draft":true,"merged":false,"merged_at":null}),
            PullRequestLifecycle::Draft,
        ),
        (
            3,
            serde_json::json!({"state":"closed","draft":false,"merged":false,"merged_at":null}),
            PullRequestLifecycle::Closed,
        ),
        (
            4,
            serde_json::json!({"state":"closed","draft":true,"merged":true,"merged_at":"2026-10-25T00:00:00Z"}),
            PullRequestLifecycle::Merged,
        ),
    ];
    Mock::given(method("GET"))
        .and(path("/repositories/41"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"id":41,"full_name":"owner/repo"})),
        )
        .mount(&server)
        .await;
    for (number, fields, _) in &pulls {
        let mut pull = fields.clone();
        pull["number"] = serde_json::json!(number);
        pull["title"] = serde_json::json!("WIP: not a draft signal");
        pull["body"] = serde_json::json!(format!("Body {number}"));
        pull["body_html"] = serde_json::json!(format!("<p>Body {number}</p>"));
        Mock::given(method("GET"))
            .and(path(format!("/repos/owner/repo/pulls/{number}")))
            .and(header("accept", "application/vnd.github.full+json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(pull))
            .expect(1)
            .mount(&server)
            .await;
    }
    let client = ProviderClients::new().unwrap().repository(
        RepositoryGrant::new(
            endpoints(&server),
            RepositoryIdentity::Name("owner/repo".into()),
            RepositoryCapability::PullRequestRead,
            "selected-token-canary".into(),
        )
        .unwrap(),
    );
    for (number, _, lifecycle) in pulls {
        let description = client.read_pull_request_description(number).await.unwrap();
        assert_eq!(description.lifecycle, lifecycle, "PR {number}");
        assert_eq!(
            description.body.as_deref(),
            Some(format!("Body {number}").as_str())
        );
        assert_eq!(
            description.body_html.as_deref(),
            Some(format!("<p>Body {number}</p>").as_str())
        );
    }
    // A writer-only grant cannot read, and nothing reaches the provider.
    let writer = ProviderClients::new().unwrap().repository(
        RepositoryGrant::new(
            endpoints(&server),
            RepositoryIdentity::Name("owner/repo".into()),
            RepositoryCapability::IssueCommentWrite,
            "selected-token-canary".into(),
        )
        .unwrap(),
    );
    assert!(writer.read_pull_request_description(1).await.is_err());
    assert!(writer.render_markdown("x", None).await.is_err());
}

#[tokio::test]
async fn markdown_render_posts_gfm_with_optional_repository_context() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/markdown"))
        .and(body_json(
            serde_json::json!({"text":"See #12","mode":"gfm","context":"owner/repo"}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw("<p>See <a href=\"x\">#12</a></p>", "text/html"),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/markdown"))
        .and(body_json(
            serde_json::json!({"text":"See #12","mode":"gfm"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw("<p>See #12</p>", "text/html"))
        .expect(1)
        .mount(&server)
        .await;
    let client = repository(&server, RepositoryCapability::PullRequestRead);
    assert_eq!(
        client
            .render_markdown("See #12", Some("owner/repo"))
            .await
            .unwrap(),
        "<p>See <a href=\"x\">#12</a></p>"
    );
    assert_eq!(
        client.render_markdown("See #12", None).await.unwrap(),
        "<p>See #12</p>"
    );
}

/// Read a whole attachment through its stream.
async fn read_attachment(
    content: &mut issues::AttachmentContent,
) -> Result<Vec<u8>, issues::IssueError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = content.chunk().await? {
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[tokio::test]
async fn pull_request_listing_follows_gh_and_keeps_only_the_selected_repository() {
    use wiremock::matchers::{body_partial_json, body_string_contains};
    let server = MockServer::start().await;
    let client = ProviderClients::new().unwrap().repository(
        RepositoryGrant::new(
            endpoints(&server),
            RepositoryIdentity::Name("acme/api".into()),
            RepositoryCapability::PullRequestRead,
            "selected-token-canary".into(),
        )
        .unwrap(),
    );
    let node = |number: i64, repo: &str| {
        serde_json::json!({"id":format!("PR_{number}"),"number":number,"title":format!("PR {number}"),
            "url":"https://elsewhere.test/x","state":"MERGED","isDraft":false,"headRefName":"issue-816-landing",
            "headRefOid":"a".repeat(40),"baseRefName":"main","createdAt":"2026-10-20T00:00:00Z",
            "updatedAt":"2026-10-21T00:00:00Z","mergedAt":"2026-10-21T00:00:00Z","closedAt":"2026-10-21T00:00:00Z",
            "author":{"login":"octo"},"labels":{"nodes":[{"name":"bug"}]},"repository":{"nameWithOwner":repo}})
    };
    // State and branch filters use the plain listing, newest created first;
    // `closed` covers closed and merged, as gh does.
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("authorization", "Bearer selected-token-canary"))
        .and(body_string_contains("pullRequests(first:$first,states:$states"))
        .and(body_string_contains("orderBy:{field:CREATED_AT,direction:DESC}"))
        .and(body_partial_json(serde_json::json!({"variables":{
            "owner":"acme","name":"api","first":2,"states":["CLOSED","MERGED"],
            "head":"issue-816-landing","base":null}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":{"repository":{
            "pullRequests":{"pageInfo":{"hasNextPage":true},"nodes":[node(822, "acme/api"), node(7, "other/repo")]}}}})))
        .expect(1)
        .mount(&server)
        .await;
    let page = client
        .list_pull_requests(&PullRequestListFilter {
            state: PullRequestListState::Closed,
            head: Some("issue-816-landing"),
            base: None,
            author: None,
            labels: &[],
            search: None,
            limit: 2,
            with_body: false,
        })
        .await
        .unwrap();
    assert!(page.has_more);
    assert_eq!(page.pulls.len(), 1, "another repository's node is dropped");
    let pull = &page.pulls[0];
    assert_eq!(pull["number"], 822);
    assert_eq!(pull["source"], "github");
    assert_eq!(pull["repository"], "acme/api");
    assert_eq!(pull["state"], "MERGED");
    assert_eq!(pull["headRefName"], "issue-816-landing");
    assert_eq!(pull["author"]["login"], "octo");
    assert_eq!(pull["labels"][0]["name"], "bug");
    // The URL is built on the configured web origin, never taken from GitHub.
    assert_eq!(pull["url"], format!("{}/acme/api/pull/822", server.uri()));

    // Author, label and free text use GitHub search with the same qualifiers gh sends.
    server.reset().await;
    let labels = vec!["needs review".to_owned()];
    let filter = PullRequestListFilter {
        state: PullRequestListState::Merged,
        head: None,
        base: Some("main"),
        author: Some("octo"),
        labels: &labels,
        search: Some("landing page"),
        limit: 30,
        with_body: false,
    };
    assert!(filter.needs_search());
    let query = build_pull_request_search_query("acme/api", &filter);
    assert_eq!(
        query,
        "repo:acme/api is:pr is:merged base:main author:octo label:\"needs review\" landing page sort:created-desc"
    );
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains(
            "search(query:$q,type:ISSUE,first:$first)",
        ))
        .and(body_partial_json(
            serde_json::json!({"variables":{"q":query,"first":30}}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":{"search":{
            "issueCount":1,"nodes":[node(822, "acme/api"), {}]}}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let page = client.list_pull_requests(&filter).await.unwrap();
    assert!(!page.has_more);
    assert_eq!(page.pulls.len(), 1, "a non-PR search node is dropped");
    let body: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    let sent = body["query"].as_str().unwrap();
    assert_eq!(
        sent.matches('{').count(),
        sent.matches('}').count(),
        "{sent}"
    );

    // Refusals happen before any request.
    server.reset().await;
    for refused in [
        PullRequestListFilter {
            limit: 101,
            ..filter
        },
        PullRequestListFilter {
            search: Some("repo:other/repo fix"),
            ..filter
        },
        PullRequestListFilter {
            search: Some("is:open"),
            ..filter
        },
        PullRequestListFilter {
            head: Some("two words"),
            ..filter
        },
        PullRequestListFilter {
            author: Some("a\"b"),
            ..filter
        },
        PullRequestListFilter {
            search: Some("\"open quote"),
            ..filter
        },
        PullRequestListFilter {
            search: Some("fix sort:updated-asc"),
            ..filter
        },
    ] {
        let error = client.list_pull_requests(&refused).await.unwrap_err();
        assert_eq!(error.kind, ProviderErrorKind::InvalidInput, "{error}");
    }
    for label in ["a\"b", "a\\"] {
        let labels = vec![label.to_owned()];
        assert!(client
            .list_pull_requests(&PullRequestListFilter {
                labels: &labels,
                ..filter
            })
            .await
            .is_err());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
    // A plain listing without filters is not a search.
    assert!(!PullRequestListFilter {
        author: None,
        labels: &[],
        search: Some("  "),
        ..filter
    }
    .needs_search());
}

#[tokio::test]
async fn issue_attachments_come_from_github_s_rendering_without_a_credential() {
    use g5_contracts::attachments::AttachmentKind;
    let server = MockServer::start().await;
    let uri = server.uri();
    let png = b"\x89PNG\r\n\x1a\nmock image".to_vec();
    let mp4 = b"\0\0\0\x18ftypmp42\0\0\0\0mock video".to_vec();
    let html = format!(
        r#"<p><img src="{uri}/user-attachments/assets/one" alt="Help popup"></p>
<p><video src="{uri}/signed/demo.mp4?jwt=a&amp;b=c" controls></video></p>
<p><a href="{uri}/user-attachments/files/7/notes.txt">notes.txt</a>
<a href="{uri}/user-attachments/files/8/private.pdf">private.pdf</a></p>
<p><img src="{uri}/big.png"><img src="{uri}/page.html"><img src="{uri}/away"></p>
<p><img src="https://example.com/outside.png"></p>"#
    );
    Mock::given(method("GET"))
        .and(path("/repositories/41/issues/12"))
        .and(header("accept", "application/vnd.github.full+json"))
        .and(header("authorization", "Bearer selected-token-canary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 9001, "number": 12, "title": "Mocks", "state": "open",
            "body": "![Help popup](https://github.com/user-attachments/assets/one)",
            "body_html": html,
            "html_url": format!("{uri}/acme/web/issues/12"),
            "created_at": "2026-10-13T00:00:00Z", "updated_at": "2026-10-15T00:00:00Z"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user-attachments/assets/one"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/signed/one.png?jwt=t"))
        .mount(&server)
        .await;
    let mut big = png.clone();
    big.resize(issues::attachment_limit(AttachmentKind::Image) + 1, 0);
    for (at, response) in [
        (
            "/signed/one.png",
            ResponseTemplate::new(200).set_body_bytes(png.clone()),
        ),
        (
            "/signed/demo.mp4",
            ResponseTemplate::new(200).set_body_bytes(mp4.clone()),
        ),
        (
            "/user-attachments/files/7/notes.txt",
            ResponseTemplate::new(200)
                .set_body_raw(b"line one\n".to_vec(), "text/plain; charset=utf-8"),
        ),
        (
            "/user-attachments/files/8/private.pdf",
            ResponseTemplate::new(404),
        ),
        ("/big.png", ResponseTemplate::new(200).set_body_bytes(big)),
        (
            "/page.html",
            ResponseTemplate::new(200).set_body_bytes(b"<!doctype html>".to_vec()),
        ),
        (
            "/away",
            ResponseTemplate::new(302).insert_header("location", "https://example.com/x.png"),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(response)
            .mount(&server)
            .await;
    }

    let reader = repository(&server, RepositoryCapability::IssuesRead);
    let rendered = issues::fetch_rendered_issue(&reader, 12).await.unwrap();
    assert_eq!(rendered.issue.number, 12);
    assert_eq!(
        rendered.description.version,
        issues::text_version("![Help popup](https://github.com/user-attachments/assets/one)")
    );
    let found = &rendered.description.attachments;
    assert_eq!(found.len(), 8);
    assert_eq!(
        found.iter().map(|a| a.kind).collect::<Vec<_>>(),
        [
            AttachmentKind::Image,
            AttachmentKind::Video,
            AttachmentKind::File,
            AttachmentKind::File,
            AttachmentKind::Image,
            AttachmentKind::Image,
            AttachmentKind::Image,
            AttachmentKind::Image,
        ]
    );
    assert_eq!(found[0].name.as_deref(), Some("Help popup"));
    assert_eq!(found[2].name.as_deref(), Some("notes.txt"));
    assert!(found[7].unavailable.is_some());
    assert_eq!(
        found[7].external_url.as_deref(),
        Some("https://example.com/outside.png")
    );

    let mut image = issues::open_attachment(&reader, &found[0]).await.unwrap();
    assert_eq!(image.content_type, "image/png");
    assert_eq!(read_attachment(&mut image).await.unwrap(), png);
    let mut video = issues::open_attachment(&reader, &found[1]).await.unwrap();
    assert_eq!(video.content_type, "video/mp4");
    assert_eq!(read_attachment(&mut video).await.unwrap(), mp4);
    let mut file = issues::open_attachment(&reader, &found[2]).await.unwrap();
    assert_eq!(file.content_type, "text/plain");
    assert_eq!(read_attachment(&mut file).await.unwrap(), b"line one\n");
    // A private file upload: GitHub serves it only on its site.
    let private = issues::open_attachment(&reader, &found[3])
        .await
        .unwrap_err();
    assert_eq!(
        private.kind,
        g5_contracts::error::ExternalIssueErrorKind::AccessDenied
    );
    assert!(private.message.contains("only on its site"), "{private:?}");
    // Too large, not an image, redirected off GitHub's hosts, hosted elsewhere.
    for refused in &found[4..] {
        assert!(
            issues::open_attachment(&reader, refused).await.is_err(),
            "{refused:?}"
        );
    }
    // Only the API read carried the token; no content request did.
    for request in server.received_requests().await.unwrap() {
        let api = request.url.path().starts_with("/repositories/");
        assert_eq!(
            request.headers.contains_key("authorization"),
            api,
            "{}",
            request.url.path()
        );
    }
}

#[tokio::test]
async fn an_attachment_without_a_declared_length_is_cut_off_at_its_limit() {
    use g5_contracts::attachments::AttachmentKind;
    use wiremock::Respond;
    // A body sent without Content-Length (chunked) is still bounded.
    struct Chunked(Vec<u8>);
    impl Respond for Chunked {
        fn respond(&self, _: &wiremock::Request) -> ResponseTemplate {
            ResponseTemplate::new(200)
                .insert_header("transfer-encoding", "chunked")
                .set_body_bytes(self.0.clone())
        }
    }
    let server = MockServer::start().await;
    let uri = server.uri();
    let mut big = b"\x89PNG\r\n\x1a\n".to_vec();
    big.resize(issues::attachment_limit(AttachmentKind::Image) + 10, 7);
    Mock::given(method("GET"))
        .and(path("/repositories/41/issues/12"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 9001, "number": 12, "title": "Big", "state": "open", "body": "x",
            "body_html": format!(r#"<img src="{uri}/big.png">"#),
            "html_url": format!("{uri}/acme/web/issues/12"),
            "created_at": "2026-10-13T00:00:00Z", "updated_at": "2026-10-15T00:00:00Z"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/big.png"))
        .respond_with(Chunked(big))
        .mount(&server)
        .await;
    let reader = repository(&server, RepositoryCapability::IssuesRead);
    let rendered = issues::fetch_rendered_issue(&reader, 12).await.unwrap();
    let result = match issues::open_attachment(&reader, &rendered.description.attachments[0]).await
    {
        Ok(mut content) => read_attachment(&mut content).await.map(|_| ()),
        Err(error) => Err(error),
    };
    assert!(result.is_err());
}

#[tokio::test]
async fn an_attachment_comment_must_be_on_the_issue() {
    let server = MockServer::start().await;
    let uri = server.uri();
    let comment = |id: u64, issue: u64| {
        serde_json::json!({
            "id": id, "body": "see ![mock](x)",
            "body_html": format!(r#"<p><img src="{uri}/signed/c.png" alt="mock"></p>"#),
            "user": {"login": "rahul", "id": 7},
            "created_at": "2026-10-13T00:00:00Z", "updated_at": "2026-10-13T00:00:00Z",
            "html_url": format!("{uri}/acme/web/issues/{issue}#issuecomment-{id}"),
            "issue_url": format!("{uri}/repositories/41/issues/{issue}")
        })
    };
    Mock::given(method("GET"))
        .and(path("/repositories/41/issues/12/comments"))
        .and(header("accept", "application/vnd.github.full+json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([comment(5, 12)])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repositories/41/issues/comments/5"))
        .and(header("accept", "application/vnd.github.full+json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(comment(5, 12)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repositories/41/issues/comments/6"))
        .respond_with(ResponseTemplate::new(200).set_body_json(comment(6, 13)))
        .mount(&server)
        .await;
    let reader = repository(&server, RepositoryCapability::IssuesRead);
    let comments = issues::fetch_rendered_comments(&reader, 12).await.unwrap();
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].text.attachments.len(), 1);
    assert_eq!(
        comments[0].text.attachments[0].name.as_deref(),
        Some("mock")
    );
    let one = issues::fetch_rendered_comment(&reader, 12, "5")
        .await
        .unwrap();
    assert_eq!(one.comment.id, "5");
    let elsewhere = issues::fetch_rendered_comment(&reader, 12, "6")
        .await
        .unwrap_err();
    assert_eq!(
        elsewhere.kind,
        g5_contracts::error::ExternalIssueErrorKind::NotFound
    );
    assert!(issues::fetch_rendered_comment(&reader, 12, "x")
        .await
        .is_err());
}

#[tokio::test]
async fn pull_request_attachments_come_from_every_text_in_github_s_rendering() {
    let server = MockServer::start().await;
    let uri = server.uri();
    let png = b"\x89PNG\r\n\x1a\nreview shot".to_vec();
    let client = ProviderClients::new().unwrap().repository(
        RepositoryGrant::new(
            endpoints(&server),
            RepositoryIdentity::Name("acme/api".into()),
            RepositoryCapability::PullRequestRead,
            "selected-token-canary".into(),
        )
        .unwrap(),
    );
    let img = |name: &str| {
        format!(r#"<p><img src="{uri}/user-attachments/assets/{name}" alt="{name}"></p>"#)
    };
    let user = serde_json::json!({"id": 7, "login": "octo"});
    let pull_api = format!("{uri}/repos/acme/api/pulls/42");
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42"))
        .and(header("accept", "application/vnd.github.full+json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "number": 42, "title": "Fix popup", "html_url": format!("{uri}/acme/api/pull/42"),
            "user": user, "body": "desc", "body_html": img("desc"),
            "created_at": "2026-11-01T00:00:00Z"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/issues/42/comments"))
        .and(header("accept", "application/vnd.github.full+json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "id": 501, "user": user, "body": "c", "body_html": img("comment"),
                "created_at": "2026-11-01T03:00:00Z", "updated_at": "2026-11-01T03:00:00Z",
                "html_url": format!("{uri}/acme/api/pull/42#issuecomment-501"),
                "issue_url": format!("{uri}/repos/acme/api/issues/42")
            }])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42/reviews"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"id": 601, "user": user, "body": "r", "body_html": img("review"),
             "submitted_at": "2026-11-01T01:00:00Z", "pull_request_url": pull_api},
            {"id": 602, "user": user, "body": "", "body_html": "",
             "submitted_at": "2026-11-01T01:30:00Z", "pull_request_url": pull_api}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/42/comments"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "id": 701, "user": user, "body": "l", "body_html": img("line"), "path": "src/ui.rs",
                "created_at": "2026-11-01T02:00:00Z", "pull_request_url": pull_api
            }])),
        )
        .mount(&server)
        .await;
    // Read again for a fetch; one review comment has moved to another PR.
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/comments/701"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 701, "user": user, "body": "l", "body_html": img("line"), "path": "src/ui.rs",
            "created_at": "2026-11-01T02:00:00Z", "pull_request_url": pull_api
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/acme/api/pulls/comments/702"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 702, "body": "x", "body_html": "", "created_at": "2026-11-01T02:00:00Z",
            "pull_request_url": format!("{uri}/repos/acme/api/pulls/43")
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user-attachments/assets/line"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png.clone()))
        .mount(&server)
        .await;

    let description_only = client.read_rendered_pull(42, false).await.unwrap();
    assert_eq!(description_only.title, "Fix popup");
    assert!(description_only.discussion.is_empty());

    let pull = client.read_rendered_pull(42, true).await.unwrap();
    assert_eq!(pull.description.kind, PullTextKind::Description);
    assert_eq!(
        pull.description.text.attachments[0].name.as_deref(),
        Some("desc")
    );
    assert_eq!(
        pull.description.author.as_ref().map(|a| a.login.as_str()),
        Some("octo")
    );
    // Oldest first; the review without a text is left out.
    let kinds: Vec<_> = pull
        .discussion
        .iter()
        .map(|t| (t.kind, t.id.clone()))
        .collect();
    assert_eq!(
        kinds,
        [
            (PullTextKind::Review, Some("601".to_string())),
            (PullTextKind::ReviewComment, Some("701".to_string())),
            (PullTextKind::Comment, Some("501".to_string())),
        ]
    );
    assert_eq!(pull.discussion[1].path.as_deref(), Some("src/ui.rs"));
    assert!(!pull.discussion_truncated);

    let line = client
        .read_rendered_pull_text(42, PullTextKind::ReviewComment, Some("701"))
        .await
        .unwrap();
    assert_eq!(line.text.version, pull.discussion[1].text.version);
    let mut content = issues::open_attachment(&client, &line.text.attachments[0])
        .await
        .unwrap();
    assert_eq!(content.content_type, "image/png");
    assert_eq!(read_attachment(&mut content).await.unwrap(), png);
    let moved = client
        .read_rendered_pull_text(42, PullTextKind::ReviewComment, Some("702"))
        .await
        .unwrap_err();
    assert_eq!(
        moved.kind,
        g5_contracts::error::ExternalIssueErrorKind::NotFound
    );
    assert!(client
        .read_rendered_pull_text(42, PullTextKind::Comment, Some("../x"))
        .await
        .is_err());
    // Only API reads carried the token.
    for request in server.received_requests().await.unwrap() {
        let api = request.url.path().starts_with("/repos/");
        assert_eq!(
            request.headers.contains_key("authorization"),
            api,
            "{}",
            request.url.path()
        );
    }
}

#[tokio::test]
async fn issue_labels_read_the_catalog_and_change_labels_by_name() {
    use issues::labels::{self, LabelRemoveOutcome, LabelWriteOutcome, LabelsAddOutcome};
    use wiremock::matchers::query_param;
    let server = MockServer::start().await;
    let label = |id: u64, name: &str| serde_json::json!({"id": id, "name": name, "color": "D73A4A", "description": ""});
    let reader = repository(&server, RepositoryCapability::IssuesRead);
    let writer = repository(&server, RepositoryCapability::IssueWrite);

    // Writes need an issue-write grant; nothing is sent for a reader.
    assert!(matches!(
        labels::create_label(&reader, "bug", None, "").await,
        LabelWriteOutcome::Rejected(_)
    ));
    assert!(matches!(
        labels::add_issue_labels(&reader, 12, &["bug".to_string()]).await,
        LabelsAddOutcome::Rejected(_)
    ));
    assert!(server.received_requests().await.unwrap().is_empty());

    // The catalog pages report another page from GitHub's Link header.
    Mock::given(method("GET"))
        .and(path("/repositories/41/labels"))
        .and(query_param("per_page", "100"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    "<https://x/repositories/41/labels?page=2>; rel=\"next\"",
                )
                .set_body_json(serde_json::json!([
                    label(1, "bug"),
                    label(2, "priority:p0")
                ])),
        )
        .mount(&server)
        .await;
    let (page, more) = labels::fetch_label_page(&reader, 1).await.unwrap();
    assert!(more);
    assert_eq!(page[1].id, Some(2));
    assert_eq!(page[1].name, "priority:p0");
    assert_eq!(page[0].color.as_deref(), Some("d73a4a"));
    assert_eq!(page[0].description, None);

    // A label is read by its name, path-encoded; 404 is no such label.
    Mock::given(method("GET"))
        .and(path("/repositories/41/labels/good%20first%20issue"))
        .respond_with(ResponseTemplate::new(200).set_body_json(label(3, "good first issue")))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repositories/41/labels/missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    assert_eq!(
        labels::fetch_label(&reader, "good first issue")
            .await
            .unwrap()
            .unwrap()
            .id,
        Some(3)
    );
    assert!(labels::fetch_label(&reader, "missing")
        .await
        .unwrap()
        .is_none());

    // Create, rename and delete with the person's token.
    Mock::given(method("POST"))
        .and(path("/repositories/41/labels"))
        .and(header("authorization", "Bearer selected-token-canary"))
        .and(body_json(serde_json::json!({
            "name": "wip", "color": "0e8a16", "description": "In progress"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(label(4, "wip")))
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        labels::create_label(&writer, "wip", Some("0e8a16"), "In progress").await,
        LabelWriteOutcome::Written(written) if written.id == Some(4)
    ));
    Mock::given(method("PATCH"))
        .and(path("/repositories/41/labels/wip"))
        .and(body_json(serde_json::json!({"new_name": "doing"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(label(4, "doing")))
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        labels::update_label(
            &writer,
            "wip",
            &labels::LabelEdit {
                name: Some("doing".to_string()),
                ..labels::LabelEdit::default()
            }
        )
        .await,
        LabelWriteOutcome::Written(written) if written.name == "doing"
    ));
    let absent = serde_json::json!({"message": "Label does not exist"});
    for (name, status, body, expected) in [
        ("doing", 204, serde_json::Value::Null, "removed"),
        ("gone", 404, absent.clone(), "absent"),
        // A bare 404 is not proof the label is gone: refused.
        (
            "unseen",
            404,
            serde_json::json!({"message": "Not Found"}),
            "rejected",
        ),
        ("moved", 301, serde_json::Value::Null, "rejected"),
        ("flaky", 502, serde_json::Value::Null, "unknown"),
    ] {
        let template = if body.is_null() {
            ResponseTemplate::new(status)
        } else {
            ResponseTemplate::new(status).set_body_json(body)
        };
        Mock::given(method("DELETE"))
            .and(path(format!("/repositories/41/labels/{name}")))
            .respond_with(template)
            .expect(1)
            .mount(&server)
            .await;
        let outcome = match labels::delete_label(&writer, name).await {
            LabelRemoveOutcome::Removed(_) => "removed",
            LabelRemoveOutcome::Absent => "absent",
            LabelRemoveOutcome::Rejected(_) => "rejected",
            LabelRemoveOutcome::Unknown(_) => "unknown",
        };
        assert_eq!(outcome, expected, "{name}");
    }

    // An issue gains labels in one request and loses one per request; a
    // label the issue lacks is already absent.
    Mock::given(method("POST"))
        .and(path("/repositories/41/issues/12/labels"))
        .and(body_json(
            serde_json::json!({"labels": ["bug", "priority:p0"]}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            label(1, "bug"),
            label(2, "priority:p0")
        ])))
        .expect(1)
        .mount(&server)
        .await;
    match labels::add_issue_labels(&writer, 12, &["bug".to_string(), "priority:p0".to_string()])
        .await
    {
        LabelsAddOutcome::Added(now) => assert_eq!(now.len(), 2),
        other => panic!("expected added labels: {other:?}"),
    }
    Mock::given(method("DELETE"))
        .and(path("/repositories/41/issues/12/labels/priority:p0"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([label(1, "bug")])),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/repositories/41/issues/12/labels/wip"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(serde_json::json!({"message": "Label does not exist"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        labels::remove_issue_label(&writer, 12, "priority:p0").await,
        LabelRemoveOutcome::Removed(now) if now.len() == 1
    ));
    assert!(matches!(
        labels::remove_issue_label(&writer, 12, "wip").await,
        LabelRemoveOutcome::Absent
    ));
    // A deleted or moved issue answers a bare 404: never reported removed.
    Mock::given(method("DELETE"))
        .and(path("/repositories/41/issues/99/labels/wip"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(serde_json::json!({"message": "Not Found"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        labels::remove_issue_label(&writer, 99, "wip").await,
        LabelRemoveOutcome::Rejected(_)
    ));
    // A refused add is final; a server error is unknown.
    Mock::given(method("POST"))
        .and(path("/repositories/41/issues/13/labels"))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(serde_json::json!({"message": "no"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/repositories/41/issues/14/labels"))
        .respond_with(ResponseTemplate::new(502))
        .mount(&server)
        .await;
    assert!(matches!(
        labels::add_issue_labels(&writer, 13, &["bug".to_string()]).await,
        LabelsAddOutcome::Rejected(error)
            if error.kind == g5_contracts::error::ExternalIssueErrorKind::AccessDenied
    ));
    assert!(matches!(
        labels::add_issue_labels(&writer, 14, &["bug".to_string()]).await,
        LabelsAddOutcome::Unknown(_)
    ));

    // `.` and `..` never become path segments: removing ".." must not
    // reach GitHub's remove-every-label endpoint.
    let before = server.received_requests().await.unwrap().len();
    assert!(matches!(
        labels::remove_issue_label(&writer, 12, "..").await,
        LabelRemoveOutcome::Rejected(_)
    ));
    assert!(matches!(
        labels::delete_label(&writer, ".").await,
        LabelRemoveOutcome::Rejected(_)
    ));
    assert!(labels::fetch_label(&reader, "..").await.is_err());
    assert_eq!(server.received_requests().await.unwrap().len(), before);

    // Creation sends selected labels and omits the field when none are.
    Mock::given(method("POST"))
        .and(path("/repositories/41/issues"))
        .and(body_json(serde_json::json!({
            "title": "t", "body": "b", "assignees": [], "labels": ["bug"]
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": 9001, "number": 15, "title": "t", "body": "b", "state": "open",
            "labels": [label(1, "bug")],
            "html_url": format!("{}/acme/web/issues/15", server.uri()),
            "created_at": "2026-11-03T00:00:00Z", "updated_at": "2026-11-03T00:00:00Z"
        })))
        .expect(1)
        .mount(&server)
        .await;
    match issues::create_issue(&writer, "t", "b", &[], &["bug".to_string()]).await {
        issues::IssueCreateOutcome::Created(created) => {
            assert_eq!(created.labels[0].id, Some(1));
        }
        other => panic!("expected a created issue: {other:?}"),
    }
    server.verify().await;
}
