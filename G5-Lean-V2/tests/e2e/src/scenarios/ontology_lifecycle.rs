//! Ontology Lifecycle E2E Tests
//!
//! Tests CRUD operations on the ontology tree through the public API.
//! Covers reading the ontology, verifying node relationships, and
//! checking that ontology state persists across requests.

use crate::helpers::api_client::TestApiClient;

/// Read the ontology for a fresh workspace — should be empty or minimal.
#[tokio::test]
async fn test_ontology_empty_workspace() -> anyhow::Result<()> {
    let api = TestApiClient::from_env_authed().await?;
    crate::skip_unless!(api.healthz().await?, "g5-api not healthy");

    let org = api.create_org("ont-empty-org").await?;
    let ws = api.create_workspace(org.id, "ont-empty-ws").await?;

    // Fresh workspace should have no ontology nodes
    let ontology = api.get_ontology(ws.id).await?;
    eprintln!("Ontology for empty workspace: {} nodes", ontology.len());

    // Should succeed (may return empty array or 404 handled as empty)
    assert!(
        ontology.is_empty(),
        "Fresh workspace should have no ontology nodes"
    );

    Ok(())
}
