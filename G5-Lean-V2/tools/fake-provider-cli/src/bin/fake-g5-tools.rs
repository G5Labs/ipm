//! A scripted MCP stdio server for provider transport tests; see
//! `docs/lean-provider-cli-contract.md`.

use std::io;

use fake_provider_cli::stub_server::{serve, StubConfig};

fn main() {
    let config = match StubConfig::from_env() {
        Ok(config) => config,
        Err(err) => {
            eprintln!("fake-g5-tools: {err}");
            std::process::exit(2);
        }
    };
    let stdin = io::stdin();
    let stdout = io::stdout();
    if let Err(err) = serve(&config, stdin.lock(), stdout.lock()) {
        eprintln!("fake-g5-tools: {err}");
        std::process::exit(1);
    }
}
