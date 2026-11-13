//! Renders the committed client harness-catalog data file (core D8).
//!
//! Pure, deterministic projection of the enriched Claude catalog (core D7)
//! and the Codex catalog (core D9) into TypeScript data. The emitted file is
//! committed and co-located at `frontend/src/lib/harnessCatalog.gen.ts`;
//! client helpers stay hand-written.

use std::fmt::Write as _;

use crate::claude_models::{
    claude_model_catalog, CLAUDE_CHAT_MODES, CLAUDE_DEFAULT_CHAT_MODE,
    CLAUDE_DIRECT_CHAT_DEFAULT_MODEL, CLAUDE_EFFORT_ORDER, CLAUDE_OPENROUTER_CHAT_DEFAULT_MODEL,
    CLAUDE_UNKNOWN_MODEL_EFFORT,
};
use crate::codex_models::{
    codex_model_catalog, CODEX_CHAT_MODES, CODEX_DEFAULT_CHAT_MODE,
    CODEX_DIRECT_CHAT_DEFAULT_MODEL, CODEX_EFFORT_ORDER, CODEX_OPENROUTER_CHAT_DEFAULT_MODEL,
    CODEX_UNKNOWN_MODEL_EFFORT,
};

/// Path of the emitted file, relative to the `generation/` workspace root.
pub const HARNESS_CATALOG_TS_PATH: &str = "frontend/src/lib/harnessCatalog.gen.ts";
pub const NATIVE_PROVIDER_CATALOG_PATH: &str = "tests/fixtures/native-provider-catalog.json";

/// Native compatibility tests consume the same route ids and per-model controls
/// as the API and generated UI. No separately maintained test model inventory.
#[must_use]
pub fn render_native_provider_catalog_json() -> String {
    let models = |provider| {
        crate::model_catalog::offered_entries()
            .filter(|entry| entry.provider == provider)
            .map(|entry| {
                serde_json::json!({
                    "id": entry.id, "route": entry.route, "efforts": entry.controls.efforts,
                    "defaultEffort": entry.controls.default_effort,
                })
            })
            .collect::<Vec<_>>()
    };
    let claude = models(crate::types::ChatAgentProvider::ClaudeCode);
    let codex = models(crate::types::ChatAgentProvider::Codex);
    let catalog = serde_json::json!({
        "schemaVersion": 1,
        "providers": {
            "claude_code": {
                "version": crate::claude_models::CLAUDE_CODE_CLI_VERSION.trim(),
                "modes": CLAUDE_CHAT_MODES, "models": claude,
                "nativeModes": crate::claude_models::CLAUDE_NATIVE_MODES,
                "openrouterSettings": {"modelOverrides": crate::claude_models::claude_native_model_overrides(CLAUDE_OPENROUTER_CHAT_DEFAULT_MODEL).unwrap()},
            },
            "codex": {
                "version": crate::codex_models::CODEX_CLI_VERSION.trim(),
                "modes": CODEX_CHAT_MODES, "models": codex,
                "nativeModes": crate::codex_models::CODEX_NATIVE_MODES,
            },
        },
    });
    serde_json::to_string_pretty(&catalog).expect("catalog serializes") + "\n"
}

fn ts_string(value: &str) -> String {
    serde_json::to_string(value).expect("plain string always serializes")
}

fn ts_string_list(values: &[&str]) -> String {
    let quoted: Vec<String> = values.iter().map(|value| ts_string(value)).collect();
    format!("[{}]", quoted.join(", "))
}

/// Emit `<indent><name>: [...]` — inline when it fits prettier's 80-column
/// width, one element per line otherwise (matching prettier's array-fit rule
/// so the committed file is byte-stable under `prettier --check`, core D8).
fn write_string_list_field(out: &mut String, indent: &str, name: &str, values: &[&str]) {
    let inline = format!("{indent}{name}: {},", ts_string_list(values));
    if inline.len() <= 80 {
        out.push_str(&inline);
        out.push('\n');
        return;
    }
    let _ = writeln!(out, "{indent}{name}: [");
    for value in values {
        let _ = writeln!(out, "{indent}  {},", ts_string(value));
    }
    let _ = writeln!(out, "{indent}],");
}

/// Render the full TypeScript module. Byte-deterministic: a pure function of
/// the static catalog slices, iterated in declaration order, with no
/// timestamps or environment reads (core D8).
#[must_use]
pub fn render_harness_catalog_ts() -> String {
    let mut out = String::new();
    out.push_str(
        "// AUTO-GENERATED — do not edit; source: g5-contracts/src/{claude,codex}_models.rs\n\
         // (emitter: g5-contracts/src/model_catalog_gen.rs). Regenerate with `just gen-model-catalog`.\n\
         // Catalog data only (g5-common/core D7/D8/D9): client helpers stay hand-written.\n\n",
    );

    out.push_str(
        "export interface HarnessCatalogVariant {\n\
         \x20 readonly id: string;\n\
         \x20 readonly label: string;\n\
         \x20 readonly offered: boolean;\n\
         }\n\n",
    );
    out.push_str(
        "export interface HarnessCatalogFamily {\n\
         \x20 readonly openrouter: string;\n\
         \x20 readonly directPreferred: string;\n\
         \x20 readonly openrouterOffered: boolean;\n\
         \x20 readonly openrouterLabel: string | null;\n\
         \x20 readonly efforts: readonly string[];\n\
         \x20 readonly defaultEffort: string;\n\
         \x20 /** Anthropic's release day, YYYY-MM-DD; families are listed newest first. */\n\
         \x20 readonly released: string;\n\
         \x20 readonly variants: readonly HarnessCatalogVariant[];\n\
         }\n\n",
    );
    out.push_str(
        "export interface HarnessCatalog {\n\
         \x20 readonly defaultModelDirect: string;\n\
         \x20 readonly defaultModelOpenrouter: string;\n\
         \x20 readonly effortOrder: readonly string[];\n\
         \x20 readonly unknownModelEffort: string;\n\
         \x20 readonly modes: readonly string[];\n\
         \x20 readonly defaultMode: string;\n\
         \x20 readonly families: readonly HarnessCatalogFamily[];\n\
         }\n\n",
    );

    out.push_str("export const CLAUDE_HARNESS_CATALOG: HarnessCatalog = {\n");
    let _ = writeln!(
        out,
        "  defaultModelDirect: {},",
        ts_string(CLAUDE_DIRECT_CHAT_DEFAULT_MODEL)
    );
    let _ = writeln!(
        out,
        "  defaultModelOpenrouter: {},",
        ts_string(CLAUDE_OPENROUTER_CHAT_DEFAULT_MODEL)
    );
    write_string_list_field(&mut out, "  ", "effortOrder", CLAUDE_EFFORT_ORDER);
    let _ = writeln!(
        out,
        "  unknownModelEffort: {},",
        ts_string(CLAUDE_UNKNOWN_MODEL_EFFORT)
    );
    write_string_list_field(&mut out, "  ", "modes", CLAUDE_CHAT_MODES);
    let _ = writeln!(
        out,
        "  defaultMode: {},",
        ts_string(CLAUDE_DEFAULT_CHAT_MODE)
    );
    out.push_str("  families: [\n");
    for mapping in claude_model_catalog() {
        out.push_str("    {\n");
        let _ = writeln!(out, "      openrouter: {},", ts_string(mapping.openrouter));
        let _ = writeln!(
            out,
            "      directPreferred: {},",
            ts_string(mapping.direct_preferred)
        );
        let _ = writeln!(
            out,
            "      openrouterOffered: {},",
            mapping.openrouter_offered
        );
        let _ = writeln!(
            out,
            "      openrouterLabel: {},",
            mapping
                .openrouter_label
                .map_or_else(|| "null".to_string(), ts_string)
        );
        write_string_list_field(&mut out, "      ", "efforts", mapping.efforts);
        let _ = writeln!(
            out,
            "      defaultEffort: {},",
            ts_string(mapping.default_effort)
        );
        let _ = writeln!(out, "      released: {},", ts_string(mapping.released));
        out.push_str("      variants: [\n");
        for variant in mapping.variants {
            // Always multiline: prettier's objectWrap preserve keeps this shape
            // byte-stable regardless of id/label widths (core D8 determinism).
            out.push_str("        {\n");
            let _ = writeln!(out, "          id: {},", ts_string(variant.id));
            let _ = writeln!(out, "          label: {},", ts_string(variant.label));
            let _ = writeln!(out, "          offered: {},", variant.offered);
            out.push_str("        },\n");
        }
        out.push_str("      ],\n");
        out.push_str("    },\n");
    }
    out.push_str("  ],\n");
    out.push_str("};\n\n");

    out.push_str(
        "export interface CodexCatalogModel {\n\
         \x20 readonly id: string;\n\
         \x20 readonly openrouter: string | null;\n\
         \x20 readonly label: string;\n\
         \x20 readonly offered: boolean;\n\
         \x20 readonly efforts: readonly string[];\n\
         \x20 readonly defaultEffort: string;\n\
         }\n\n",
    );
    out.push_str(
        "export interface CodexHarnessCatalog {\n\
         \x20 readonly defaultModelDirect: string;\n\
         \x20 readonly defaultModelOpenrouter: string;\n\
         \x20 readonly effortOrder: readonly string[];\n\
         \x20 readonly unknownModelEffort: string;\n\
         \x20 readonly modes: readonly string[];\n\
         \x20 readonly defaultMode: string;\n\
         \x20 readonly models: readonly CodexCatalogModel[];\n\
         }\n\n",
    );

    out.push_str("export const CODEX_HARNESS_CATALOG: CodexHarnessCatalog = {\n");
    let _ = writeln!(
        out,
        "  defaultModelDirect: {},",
        ts_string(CODEX_DIRECT_CHAT_DEFAULT_MODEL)
    );
    let _ = writeln!(
        out,
        "  defaultModelOpenrouter: {},",
        ts_string(CODEX_OPENROUTER_CHAT_DEFAULT_MODEL)
    );
    write_string_list_field(&mut out, "  ", "effortOrder", CODEX_EFFORT_ORDER);
    let _ = writeln!(
        out,
        "  unknownModelEffort: {},",
        ts_string(CODEX_UNKNOWN_MODEL_EFFORT)
    );
    write_string_list_field(&mut out, "  ", "modes", CODEX_CHAT_MODES);
    let _ = writeln!(
        out,
        "  defaultMode: {},",
        ts_string(CODEX_DEFAULT_CHAT_MODE)
    );
    out.push_str("  models: [\n");
    for mapping in codex_model_catalog() {
        // Always multiline: prettier's objectWrap preserve keeps this shape
        // byte-stable regardless of id/label widths (core D8 determinism).
        out.push_str("    {\n");
        let _ = writeln!(out, "      id: {},", ts_string(mapping.direct));
        let _ = writeln!(
            out,
            "      openrouter: {},",
            mapping
                .openrouter
                .map_or_else(|| "null".to_string(), ts_string)
        );
        let _ = writeln!(out, "      label: {},", ts_string(mapping.label));
        let _ = writeln!(out, "      offered: {},", mapping.offered);
        write_string_list_field(&mut out, "      ", "efforts", mapping.efforts);
        let _ = writeln!(
            out,
            "      defaultEffort: {},",
            ts_string(mapping.default_effort)
        );
        out.push_str("    },\n");
    }
    out.push_str("  ],\n");
    out.push_str("};\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_ui_and_native_catalogs_match_api_backend_source() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (path, rendered) in [
            (HARNESS_CATALOG_TS_PATH, render_harness_catalog_ts()),
            (
                NATIVE_PROVIDER_CATALOG_PATH,
                render_native_provider_catalog_json(),
            ),
        ] {
            assert_eq!(
                std::fs::read_to_string(root.join(path)).expect("committed catalog exists"),
                rendered,
                "{path} drifted from API/backend controls; run just gen-model-catalog"
            );
        }
    }

    #[test]
    fn render_is_byte_deterministic() {
        assert_eq!(render_harness_catalog_ts(), render_harness_catalog_ts());
    }

    #[test]
    fn header_marks_the_file_generated_and_names_both_sources() {
        let rendered = render_harness_catalog_ts();
        let first_line = rendered.lines().next().expect("non-empty render");
        assert!(
            first_line.starts_with(
                "// AUTO-GENERATED — do not edit; source: g5-contracts/src/{claude,codex}_models.rs"
            ),
            "header contract violated: {first_line}"
        );
        assert!(rendered.contains("just gen-model-catalog"));
    }

    #[test]
    fn every_codex_model_renders_in_catalog_order_with_spark_null_slug() {
        let rendered = render_harness_catalog_ts();
        let codex_start = rendered
            .find("CODEX_HARNESS_CATALOG")
            .expect("codex catalog rendered");
        let mut cursor = codex_start;
        for mapping in codex_model_catalog() {
            let needle = format!("id: {},", ts_string(mapping.direct));
            let found = rendered[cursor..]
                .find(&needle)
                .map(|at| cursor + at)
                .unwrap_or_else(|| panic!("model {} missing or out of order", mapping.direct));
            cursor = found;
        }
        assert!(rendered[codex_start..].contains("openrouter: null,"));
    }

    #[test]
    fn every_family_and_variant_renders_in_catalog_order() {
        let rendered = render_harness_catalog_ts();

        // Pass 1: locate each family, which proves families render in catalog order.
        let mut cursor = 0usize;
        let mut family_starts = Vec::with_capacity(claude_model_catalog().len());
        for mapping in claude_model_catalog() {
            let family_needle = format!("openrouter: {},", ts_string(mapping.openrouter));
            let found = rendered[cursor..]
                .find(&family_needle)
                .map(|at| cursor + at)
                .unwrap_or_else(|| panic!("family {} missing or out of order", mapping.openrouter));
            cursor = found;
            family_starts.push(found);
        }

        // Pass 2: each variant must appear inside its OWN family's block — bounded by
        // the next family's start — so a variant emitted under a later family fails
        // instead of being found by an unbounded search to end-of-output.
        for (index, mapping) in claude_model_catalog().iter().enumerate() {
            let block_start = family_starts[index];
            let block_end = family_starts
                .get(index + 1)
                .copied()
                .unwrap_or(rendered.len());
            let family_block = &rendered[block_start..block_end];
            for variant in mapping.variants {
                let variant_needle = format!("id: {},", ts_string(variant.id));
                assert!(
                    family_block.contains(&variant_needle),
                    "variant {} missing from its own family block ({})",
                    variant.id,
                    mapping.openrouter
                );
            }
        }
    }

    #[test]
    fn bracketed_model_ids_render_as_quoted_ts_string_literals() {
        assert_eq!(ts_string("claude-opus-4-8[1m]"), "\"claude-opus-4-8[1m]\"");
        let rendered = render_harness_catalog_ts();
        assert!(rendered.contains("\"claude-opus-4-8[1m]\""));
    }
}
