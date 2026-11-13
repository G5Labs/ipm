//! A fake Claude Code CLI for G5-Lean tests; see `docs/lean-provider-cli-contract.md`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(fake_provider_cli::claude::main_with_args(args));
}
