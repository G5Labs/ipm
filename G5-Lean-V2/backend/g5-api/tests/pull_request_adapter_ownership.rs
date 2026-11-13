//! `g5.git.pr-adapter`: every pull-request surface reaches GitHub through the
//! PR service, which dispatches to the repository pull request's adapter. No
//! route builds or names the GitHub repository client, so none can bypass the
//! adapter binding or the GitHub adapter's guards (credentials, comment
//! receipts, refusals).

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn routes_never_name_the_github_repository_client() {
    let routes = Path::new(env!("CARGO_MANIFEST_DIR")).join("../g5-api-routes/src");
    let mut files = Vec::new();
    rust_files(&routes, &mut files);
    assert!(
        !files.is_empty(),
        "no route sources under {}",
        routes.display()
    );
    let offenders: Vec<String> = files
        .iter()
        .filter(|path| {
            let source = std::fs::read_to_string(path).unwrap();
            [
                "RepositoryClient",
                "conversation_client",
                "repository_protocol",
            ]
            .iter()
            .any(|name| source.contains(name))
        })
        .map(|path| path.strip_prefix(&routes).unwrap().display().to_string())
        .collect();
    assert!(
        offenders.is_empty(),
        "routes must reach GitHub PRs through the PR service: {offenders:?}"
    );
}
