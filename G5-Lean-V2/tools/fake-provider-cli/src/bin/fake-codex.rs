//! A fake Codex CLI for G5-Lean tests; see `docs/lean-provider-cli-contract.md`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(fake_provider_cli::codex::main_with_args(args));
}
