use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[test]
fn migration_versions_are_unique() {
    let migration_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/db/migrations");
    let mut versions = BTreeMap::<u64, Vec<String>>::new();

    for entry in fs::read_dir(&migration_dir).expect("read migration directory") {
        let entry = entry.expect("read migration entry");
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !file_name.ends_with(".sql") {
            continue;
        }

        let prefix = file_name
            .split_once('_')
            .map(|(prefix, _)| prefix)
            .expect("migration filename must contain a numeric prefix");
        let version = prefix
            .parse::<u64>()
            .expect("migration filename prefix must be numeric");
        versions.entry(version).or_default().push(file_name);
    }

    let duplicates = versions
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .collect::<Vec<_>>();

    assert!(
        duplicates.is_empty(),
        "SQLx migration versions must be unique; duplicate versions: {duplicates:?}"
    );
}
