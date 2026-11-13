//! Guest execution of the same admitted Git restore operation used by the host.
use g5_internal_api::host_ctl::VmStart;
use std::{path::Path, time::Duration};

pub async fn restore(start: &VmStart) -> anyhow::Result<()> {
    let root = Path::new("/work");
    anyhow::ensure!(
        start.spec.spec_version == g5_internal_api::host_ctl::VM_SPEC_VERSION
            && start.workspace_id == start.spec.incarnation.workspace_id
            && start.vm_id == start.spec.incarnation.vm_id
            && start.spec.incarnation.epoch != 0
            && !start.workspace_token.is_empty(),
        "invalid bootstrap incarnation"
    );
    g5d_host::workspace_vm::validate_restore_spec(&start.spec)?;
    let git = g5_scm::native::NativeGit::trusted_bootstrap(Duration::from_secs(1800))?;
    for repo in &start.spec.repos {
        git.validate_bootstrap_checkout(
            &root.join(&repo.root_path),
            &repo.branch_oid,
            &repo.git_url,
        )?;
    }
    for repo in &start.spec.repos {
        git.prepare_bootstrap_checkout(
            &root.join(&repo.root_path),
            &repo.branch_oid,
            &repo.git_url,
        )
        .await?;
    }
    // Adapt transport only; persisted membership retains the admitted URLs.
    // The host firewall permits only its configured API/cache ports.
    let mut transport_spec = start.spec.clone();
    for repo in &mut transport_spec.repos {
        if let Some(url) =
            g5d_isolation::isolation::passt::rewrite_loopback_url_for_door(&repo.git_url)
        {
            repo.git_url = url;
        }
    }
    for repo in &mut transport_spec.reference_repos {
        if let Some(url) =
            g5d_isolation::isolation::passt::rewrite_loopback_url_for_door(&repo.git_url)
        {
            repo.git_url = url;
        }
    }
    g5d_host::workspace_vm::restore_workspace_with_git(
        root,
        &transport_spec,
        &start.workspace_token,
        &git,
    )
    .await?;
    // The seed holds members only; reference checkouts, the owner's personal
    // mount included, are fetched here, beside them.
    g5d_host::workspace_vm::restore_references_with_git(
        root,
        &start.spec,
        &transport_spec,
        &start.workspace_token,
        &git,
    )
    .await?;
    let metadata = root.join(".g5");
    std::fs::create_dir_all(&metadata)?;
    let layout = if start.spec.reference_repos.is_empty() {
        serde_json::json!({"repos":start.spec.repos,"restore":start.spec.restore})
    } else {
        serde_json::json!({
            "repos":start.spec.repos,
            "restore":start.spec.restore,
            "references":start.spec.reference_repos,
        })
    };
    std::fs::write(metadata.join("layout.json"), serde_json::to_vec(&layout)?)?;
    // Project-doc aliases refer to this workspace, never the seed's original producer.
    for repo in &start.spec.repos {
        if repo.role.as_deref() == Some("docs") {
            let target = std::path::PathBuf::from(&repo.root_path)
                .join("docs")
                .join(start.workspace_id.to_string());
            std::fs::create_dir_all(root.join(&target))?;
            let alias = root.join("project_docs");
            if std::fs::symlink_metadata(&alias).is_ok() {
                std::fs::remove_file(&alias)?;
            }
            std::os::unix::fs::symlink(target, alias)?;
        }
    }
    if let Some((uid, gid)) = g5d_foundation::ownership::module_identity_from_env(&start.spec.env) {
        g5d_foundation::ownership::chown_tree(root, Some(uid), gid)?;
    }
    Ok(())
}
