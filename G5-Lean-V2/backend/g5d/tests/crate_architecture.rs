//! Package boundaries and type compatibility.

include!("architecture/source_contracts.rs");

#[test]
fn public_nested_git_credentials_path_remains_source_compatible() {
    fn accepts_legacy_type(_: Option<g5d::git::credentials::ScmCredential>) {}

    accepts_legacy_type(None);
    let _: Option<g5d::git::ScmCredential> = None;
}

#[test]
fn workspace_vm_host_surfaces_are_exported() {
    let _: Option<g5d::host::runtime::WorkspaceHostState> = None;
    let _: Option<g5d::host::runtime::IsolationVmLauncher> = None;
    let _: Option<g5d::host::workspace_vm::WorkspaceVmRuntime> = None;
    let _: Option<g5d::host::ctl::HostCommandProcessor> = None;
}

#[test]
fn semlift_config_load_preserves_the_historical_error_contract() {
    fn accepts_legacy_signature(
        _: fn(&Path) -> Result<Option<g5d::config::SemliftConfig>, g5d::config::G5dError>,
    ) {
    }

    accepts_legacy_signature(g5d::config::SemliftConfig::load);

    let workspace = tempfile::tempdir().expect("temporary workspace should be created");
    std::fs::write(workspace.path().join("semlift.yaml"), "views: [invalid]\n")
        .expect("invalid fixture should be written");
    let error = g5d::config::SemliftConfig::load(workspace.path())
        .expect_err("invalid configuration must fail through the compatibility boundary");
    assert!(error.to_string().starts_with("configuration error:"));
}
