//! Shipped `g5d` binary entrypoint.

fn main() {
    std::process::exit(g5d::cli::run_process_entrypoint(g5d::run));
}
