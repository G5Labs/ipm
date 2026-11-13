use super::*;

#[test]
#[ignore = "requires exact native pins in G5_NATIVE_PROVIDER_TEST_BIN_DIR; required in E2E CI"]
fn pinned_native_catalog_and_production_launches_match() {
    let binaries = std::path::PathBuf::from(
        std::env::var("G5_NATIVE_PROVIDER_TEST_BIN_DIR").expect("set pinned CLI directory"),
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let status = std::process::Command::new("python3")
        .arg(root.join("scripts/test-native-provider-catalog.py"))
        .arg("--claude")
        .arg(binaries.join("claude"))
        .arg("--codex")
        .arg(binaries.join("codex"))
        .arg("--g5d")
        .arg(bin())
        .current_dir(&root)
        .status()
        .expect("native catalog gate starts");
    assert!(
        status.success(),
        "native model/mode/effort or production launch drift"
    );
    let status = std::process::Command::new("python3")
        .arg(root.join("scripts/test-codex-app-server.py"))
        .arg("--codex")
        .arg(binaries.join("codex"))
        .current_dir(&root)
        .status()
        .expect("native App Server schema/configuration/resume gate starts");
    assert!(
        status.success(),
        "native App Server protocol or continuation drift"
    );
    let status = std::process::Command::new("python3")
        .arg(root.join("scripts/test-native-claude-lifecycle.py"))
        .arg("--claude")
        .arg(binaries.join("claude"))
        .current_dir(root)
        .status()
        .expect("native Claude lifecycle gate starts");
    assert!(
        status.success(),
        "native Claude control/background/EOF drift"
    );
}
