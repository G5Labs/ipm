//! Writes the committed harness-catalog data file (g5-common/core D8):
//! the client TS catalog.
//!
//! Usage: `just gen-model-catalog` from the repository root
//! (or `cargo run -p g5-contracts --bin gen_model_catalog`).

use std::path::PathBuf;

use g5_contracts::model_catalog_gen::{
    render_harness_catalog_ts, render_native_provider_catalog_json, HARNESS_CATALOG_TS_PATH,
    NATIVE_PROVIDER_CATALOG_PATH,
};

fn write_generated(relative: &str, rendered: &str) {
    let out_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::write(&out_path, rendered)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", out_path.display()));
    println!("wrote {} ({} bytes)", out_path.display(), rendered.len());
}

fn main() {
    write_generated(HARNESS_CATALOG_TS_PATH, &render_harness_catalog_ts());
    write_generated(
        NATIVE_PROVIDER_CATALOG_PATH,
        &render_native_provider_catalog_json(),
    );
}
