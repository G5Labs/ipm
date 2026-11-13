//! Compatibility for tracing targets after implementation moves between crates.

use std::borrow::Cow;
use std::fmt;

use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::fmt::FmtContext;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::EnvFilter;

/// One historical logical module prefix and its extracted physical owner.
#[derive(Debug, Clone, Copy)]
pub struct LegacyTargetMapping {
    pub legacy_prefix: &'static str,
    pub physical_prefix: &'static str,
}

impl LegacyTargetMapping {
    #[must_use]
    pub const fn new(legacy_prefix: &'static str, physical_prefix: &'static str) -> Self {
        Self {
            legacy_prefix,
            physical_prefix,
        }
    }
}

/// A historical logical crate target and the ordered prefix mappings for its
/// extracted implementation crates. More-specific mappings win over broader
/// fallbacks in both filter expansion and rendered target normalization.
#[derive(Debug, Clone, Copy)]
pub struct LegacyTargetFamily {
    pub legacy_root: &'static str,
    pub mappings: &'static [LegacyTargetMapping],
}

/// Build the ordinary `RUST_LOG` filter plus aliases from historical logical
/// targets to every extracted physical owner. Invalid filters retain the prior
/// behavior of falling back to the configured default.
#[must_use]
pub fn legacy_compatible_env_filter(
    default: &str,
    families: &'static [LegacyTargetFamily],
) -> EnvFilter {
    match std::env::var("RUST_LOG") {
        Ok(filter) => EnvFilter::try_new(expand_filter_targets(&filter, families))
            .unwrap_or_else(|_| EnvFilter::new(default)),
        Err(_) => EnvFilter::new(default),
    }
}

/// Match tracing-subscriber's default ANSI policy. The workspace uses the
/// dependency's default `ansi` feature, so a non-empty `NO_COLOR` value is the
/// only runtime opt-out applied by the baseline formatter.
#[must_use]
pub fn default_ansi_enabled() -> bool {
    std::env::var("NO_COLOR").map_or(true, |value| value.is_empty())
}

/// Wrap an existing tracing formatter and rewrite only known physical crate
/// prefixes back to their historical logical targets. This preserves both
/// human-readable and JSON `target` output for existing dashboards.
pub struct LegacyTargetFormatter<E> {
    inner: E,
    families: &'static [LegacyTargetFamily],
}

impl<E> LegacyTargetFormatter<E> {
    #[must_use]
    pub fn new(inner: E, families: &'static [LegacyTargetFamily]) -> Self {
        Self { inner, families }
    }
}

impl<S, N, E> FormatEvent<S, N> for LegacyTargetFormatter<E>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    N: for<'writer> FormatFields<'writer> + 'static,
    E: FormatEvent<S, N>,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let mut rendered = String::new();
        self.inner
            .format_event(ctx, Writer::new(&mut rendered), event)?;
        let mut event_fields = String::new();
        if !rendered.trim_start().starts_with('{') {
            ctx.format_fields(Writer::new(&mut event_fields), event)?;
        }
        writer.write_str(&rewrite_rendered_target(
            &rendered,
            event.metadata().target(),
            self.families,
            &event_fields,
        ))
    }
}

fn expand_filter_targets(filter: &str, families: &'static [LegacyTargetFamily]) -> String {
    // The caller's directives pass through verbatim and in order, so bare
    // levels, repeated matchers and selector values keep exactly the meaning
    // tracing-subscriber gives them. Translations follow, one per matcher
    // (target plus any span or field selector), each carrying the specificity
    // of the directive it came from (the length of that directive's target):
    // on a collision the more specific origin wins, and an equally specific
    // later one replaces the earlier, which is tracing-subscriber's own rule
    // for identical matchers. A matcher the caller wrote against a physical
    // target is never translated over.
    let originals: Vec<&str> = filter
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .collect();
    let written: Vec<&str> = originals
        .iter()
        .map(|directive| split_level(directive).0)
        .collect();
    let mut translated: Vec<(String, String, usize)> = Vec::new();
    for directive in &originals {
        let (matcher, level) = split_level(directive);
        let target_end = matcher.find(['[', '{']).unwrap_or(matcher.len());
        let (target, selector) = matcher.split_at(target_end);
        let rank = target.len();
        let mut translate = |physical: &str| {
            let candidate = format!("{physical}{selector}");
            if written.contains(&candidate.as_str()) {
                return;
            }
            match translated
                .iter_mut()
                .find(|(existing, _, _)| *existing == candidate)
            {
                Some(entry) => {
                    if rank >= entry.2 {
                        entry.1 = level.to_string();
                        entry.2 = rank;
                    }
                }
                None => translated.push((candidate, level.to_string(), rank)),
            }
        };
        for family in families {
            if target == family.legacy_root {
                let mut roots = Vec::new();
                for mapping in family.mappings {
                    let physical_root = mapping
                        .physical_prefix
                        .split("::")
                        .next()
                        .expect("a physical tracing prefix has a crate root");
                    if !roots.contains(&physical_root) {
                        roots.push(physical_root);
                        translate(physical_root);
                    }
                }
                continue;
            }

            // `EnvFilter` matches a directive's target as a lexical prefix, so
            // an abbreviated historical target (`g5_api::services::workspace_`)
            // selects every mapped module it prefixes, boundary or not.
            let descendants = family
                .mappings
                .iter()
                .filter(|mapping| {
                    mapping.legacy_prefix.len() > target.len()
                        && mapping.legacy_prefix.starts_with(target)
                })
                .collect::<Vec<_>>();
            if !descendants.is_empty() {
                for mapping in family
                    .mappings
                    .iter()
                    .filter(|mapping| mapping.legacy_prefix == target)
                    .chain(descendants)
                {
                    translate(mapping.physical_prefix);
                }
                continue;
            }

            let longest_match = family
                .mappings
                .iter()
                .filter(|mapping| prefix_matches(target, mapping.legacy_prefix))
                .map(|mapping| mapping.legacy_prefix.len())
                .max();
            if let Some(longest_match) = longest_match {
                for mapping in family.mappings.iter().filter(|mapping| {
                    mapping.legacy_prefix.len() == longest_match
                        && prefix_matches(target, mapping.legacy_prefix)
                }) {
                    let module_suffix = &target[mapping.legacy_prefix.len()..];
                    translate(&format!("{}{module_suffix}", mapping.physical_prefix));
                }
            }
        }
    }
    originals
        .into_iter()
        .map(str::to_string)
        .chain(
            translated
                .into_iter()
                .map(|(matcher, level, _)| format!("{matcher}{level}")),
        )
        .collect::<Vec<_>>()
        .join(",")
}

/// A directive's matcher (target plus span/field selectors) and its level:
/// the separator is the first `=` outside the selector brackets, so a field
/// value such as `{request_id=a}` stays in the matcher.
fn split_level(directive: &str) -> (&str, &str) {
    let mut depth = 0usize;
    for (index, byte) in directive.bytes().enumerate() {
        match byte {
            b'[' | b'{' => depth += 1,
            b']' | b'}' => depth = depth.saturating_sub(1),
            b'=' if depth == 0 => return directive.split_at(index),
            _ => {}
        }
    }
    (directive, "")
}

fn rewrite_rendered_target<'a>(
    rendered: &'a str,
    physical_target: &str,
    families: &'static [LegacyTargetFamily],
    event_fields: &str,
) -> Cow<'a, str> {
    let mapping = families
        .iter()
        .flat_map(|family| family.mappings)
        .filter(|mapping| prefix_matches(physical_target, mapping.physical_prefix))
        .max_by_key(|mapping| mapping.physical_prefix.len());
    let Some(mapping) = mapping else {
        return Cow::Borrowed(rendered);
    };
    let module_suffix = &physical_target[mapping.physical_prefix.len()..];
    let legacy_target = format!("{}{module_suffix}", mapping.legacy_prefix);
    if rendered.trim_start().starts_with('{') {
        return rewrite_top_level_json_target(rendered, physical_target, &legacy_target);
    }

    // Full human formatting writes scope/span fields before metadata and event
    // fields after it. Remove the separately rendered event-field suffix from
    // consideration, then choose the last matching target in the prefix. This
    // identifies metadata even when a span field or name contains the same
    // bytes, without touching message/field payloads.
    let (plain, positions) = strip_ansi_with_offsets(rendered);
    let line = plain.strip_suffix('\n').unwrap_or(&plain);
    let Some(prefix) = line.strip_suffix(event_fields) else {
        return Cow::Borrowed(rendered);
    };
    if let Some(start) = prefix.rfind(physical_target) {
        let Some(original_start) = positions.get(start).copied() else {
            return Cow::Borrowed(rendered);
        };
        let Some(original_end) = positions
            .get(start + physical_target.len() - 1)
            .map(|last_byte| last_byte + 1)
        else {
            return Cow::Borrowed(rendered);
        };
        let mut output = rendered.to_string();
        output.replace_range(original_start..original_end, &legacy_target);
        return Cow::Owned(output);
    }
    Cow::Borrowed(rendered)
}

fn strip_ansi_with_offsets(input: &str) -> (String, Vec<usize>) {
    let bytes = input.as_bytes();
    let mut plain = Vec::with_capacity(bytes.len());
    let mut positions = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'[') {
            index += 2;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if (0x40..=0x7e).contains(&byte) {
                    break;
                }
            }
            continue;
        }
        positions.push(index);
        plain.push(bytes[index]);
        index += 1;
    }
    (
        String::from_utf8(plain).expect("removing ANSI escapes preserves UTF-8"),
        positions,
    )
}

fn rewrite_top_level_json_target<'a>(
    rendered: &'a str,
    physical_target: &str,
    legacy_target: &str,
) -> Cow<'a, str> {
    let bytes = rendered.as_bytes();
    let mut depth = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'{' | b'[' => {
                depth += 1;
                index += 1;
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                index += 1;
            }
            b'"' => {
                let key_start = index + 1;
                let Some(key_end) = json_string_end(bytes, key_start) else {
                    return Cow::Borrowed(rendered);
                };
                let mut cursor = key_end + 1;
                while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                    cursor += 1;
                }
                if depth == 1 && &rendered[key_start..key_end] == "target" {
                    if bytes.get(cursor) != Some(&b':') {
                        index = key_end + 1;
                        continue;
                    }
                    cursor += 1;
                    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                        cursor += 1;
                    }
                    if bytes.get(cursor) != Some(&b'"') {
                        return Cow::Borrowed(rendered);
                    }
                    let value_start = cursor + 1;
                    let Some(value_end) = json_string_end(bytes, value_start) else {
                        return Cow::Borrowed(rendered);
                    };
                    if &rendered[value_start..value_end] == physical_target {
                        let mut output = rendered.to_string();
                        output.replace_range(value_start..value_end, legacy_target);
                        return Cow::Owned(output);
                    }
                }
                index = key_end + 1;
            }
            _ => index += 1,
        }
    }
    Cow::Borrowed(rendered)
}

fn json_string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut escaped = false;
    for (index, byte) in bytes.iter().enumerate().skip(start) {
        if escaped {
            escaped = false;
        } else if *byte == b'\\' {
            escaped = true;
        } else if *byte == b'"' {
            return Some(index);
        }
    }
    None
}

fn prefix_matches(target: &str, prefix: &str) -> bool {
    target == prefix
        || target
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with("::"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::sync::{Arc, Mutex};

    static API_MAPPINGS: &[LegacyTargetMapping] = &[
        LegacyTargetMapping {
            legacy_prefix: "g5_api",
            physical_prefix: "g5_api_core",
        },
        LegacyTargetMapping {
            legacy_prefix: "g5_api",
            physical_prefix: "g5_api_routes",
        },
    ];
    static FAMILIES: &[LegacyTargetFamily] = &[LegacyTargetFamily {
        legacy_root: "g5_api",
        mappings: API_MAPPINGS,
    }];

    static G5D_MAPPINGS: &[LegacyTargetMapping] = &[
        LegacyTargetMapping {
            legacy_prefix: "g5d",
            physical_prefix: "g5d_foundation",
        },
        LegacyTargetMapping {
            legacy_prefix: "g5d::host::supervisor",
            physical_prefix: "g5d_host::supervisor",
        },
        LegacyTargetMapping {
            legacy_prefix: "g5d::worker::codex",
            physical_prefix: "g5d_agent::codex",
        },
        // The CLI is a module of the shipped crate again, so its historical
        // `g5d::cli::*` targets are physical and only the modules that logged
        // outside that namespace keep a mapping.
        LegacyTargetMapping {
            legacy_prefix: "g5d::config",
            physical_prefix: "g5d::cli::config",
        },
        LegacyTargetMapping {
            legacy_prefix: "g5d::agent::inbox",
            physical_prefix: "g5d_agent::inbox",
        },
        LegacyTargetMapping {
            legacy_prefix: "g5d::worker::agent_host",
            physical_prefix: "g5d_host::agent_host",
        },
        LegacyTargetMapping {
            legacy_prefix: "g5d::git::credentials",
            physical_prefix: "g5d_foundation::scm_credentials",
        },
    ];
    static G5D_FAMILIES: &[LegacyTargetFamily] = &[LegacyTargetFamily {
        legacy_root: "g5d",
        mappings: G5D_MAPPINGS,
    }];

    static MOVED_MAPPINGS: &[LegacyTargetMapping] = &[
        LegacyTargetMapping {
            legacy_prefix: "g5_api::services::org_service",
            physical_prefix: "g5_api_core::services::identity::org_service",
        },
        LegacyTargetMapping {
            legacy_prefix: "g5_api::services::chat_service",
            physical_prefix: "g5_api_core::services::agents::chat_service",
        },
        LegacyTargetMapping {
            legacy_prefix: "g5_api::services::workspace_service",
            physical_prefix: "g5_api_core::services::workspace::workspace_service",
        },
        LegacyTargetMapping {
            legacy_prefix: "g5_api::services::workspace_ops",
            physical_prefix: "g5_api_core::services::git::workspace_ops",
        },
        LegacyTargetMapping {
            legacy_prefix: "g5_api",
            physical_prefix: "g5_api_core",
        },
    ];
    static MOVED_FAMILIES: &[LegacyTargetFamily] = &[LegacyTargetFamily {
        legacy_root: "g5_api",
        mappings: MOVED_MAPPINGS,
    }];

    #[test]
    fn a_more_specific_historical_directive_wins_over_a_broad_one_in_either_order() {
        for filter in [
            "g5_api::services::org_service=trace,g5_api::services=warn",
            "g5_api::services=warn,g5_api::services::org_service=trace",
        ] {
            let expanded = expand_filter_targets(filter, MOVED_FAMILIES);
            assert!(
                expanded.contains("g5_api_core::services::identity::org_service=trace"),
                "{expanded}"
            );
            assert!(
                !expanded.contains("g5_api_core::services::identity::org_service=warn"),
                "{expanded}"
            );
            assert!(
                expanded.contains("g5_api_core::services::agents::chat_service=warn"),
                "{expanded}"
            );
        }
        // A directive written against the physical target outranks a
        // translation, whichever comes first.
        for filter in [
            "g5_api::services::org_service=trace,g5_api_core::services::identity::org_service=error",
            "g5_api_core::services::identity::org_service=error,g5_api::services::org_service=trace",
        ] {
            let expanded = expand_filter_targets(filter, MOVED_FAMILIES);
            assert!(
                expanded.contains("g5_api_core::services::identity::org_service=error"),
                "{expanded}"
            );
            assert!(
                !expanded.contains("identity::org_service=trace"),
                "{expanded}"
            );
        }
    }

    #[test]
    fn span_and_field_selectors_are_distinct_matchers() {
        for filter in [
            "off,g5_api::services::org_service[{repo_id}]=debug,g5_api::services::org_service=warn",
            "off,g5_api::services::org_service=warn,g5_api::services::org_service[{repo_id}]=debug",
        ] {
            let expanded = expand_filter_targets(filter, MOVED_FAMILIES);
            assert!(
                expanded.contains("g5_api_core::services::identity::org_service[{repo_id}]=debug"),
                "{expanded}"
            );
            assert!(
                expanded.contains("g5_api_core::services::identity::org_service=warn"),
                "{expanded}"
            );
            assert!(
                expanded.contains("g5_api::services::org_service[{repo_id}]=debug"),
                "{expanded}"
            );
        }
    }

    #[test]
    fn field_values_and_bare_levels_pass_through_as_written() {
        // Two selectors that differ only in a field value are two matchers.
        let filter = "off,g5_api::services::org_service[request{request_id=a}]=debug,g5_api::services::org_service[request{request_id=b}]=debug";
        let expanded = expand_filter_targets(filter, MOVED_FAMILIES);
        assert!(expanded.starts_with(filter), "{expanded}");
        assert!(
            expanded.contains(
                "g5_api_core::services::identity::org_service[request{request_id=a}]=debug"
            ),
            "{expanded}"
        );
        assert!(
            expanded.contains(
                "g5_api_core::services::identity::org_service[request{request_id=b}]=debug"
            ),
            "{expanded}"
        );
        // The caller's directives are never reordered or deduplicated, so a
        // repeated bare level keeps tracing-subscriber's last-wins meaning.
        assert_eq!(
            expand_filter_targets("info,off,info", MOVED_FAMILIES),
            "info,off,info"
        );
        assert_eq!(split_level("a[{x=1}]=debug"), ("a[{x=1}]", "=debug"));
        assert_eq!(split_level("warn"), ("warn", ""));
    }

    #[test]
    fn an_abbreviated_historical_target_selects_every_module_it_prefixes() {
        let expanded =
            expand_filter_targets("off,g5_api::services::workspace_=debug", MOVED_FAMILIES);
        assert!(
            expanded.contains("g5_api_core::services::workspace::workspace_service=debug"),
            "{expanded}"
        );
        assert!(
            expanded.contains("g5_api_core::services::git::workspace_ops=debug"),
            "{expanded}"
        );
        assert!(!expanded.contains("org_service"), "{expanded}");
    }

    #[test]
    fn old_filter_directive_expands_to_physical_owners() {
        assert_eq!(
            expand_filter_targets("warn,g5_api::auth=debug", FAMILIES),
            "warn,g5_api::auth=debug,g5_api_core::auth=debug,g5_api_routes::auth=debug"
        );
    }

    static API_CONTRACT_MAPPINGS: &[LegacyTargetMapping] = &[
        LegacyTargetMapping {
            legacy_prefix: "g5_api",
            physical_prefix: "g5_api_foundation",
        },
        LegacyTargetMapping {
            legacy_prefix: "g5_api",
            physical_prefix: "g5_api_foundation::contracts",
        },
    ];
    static API_CONTRACT_FAMILIES: &[LegacyTargetFamily] = &[LegacyTargetFamily {
        legacy_root: "g5_api",
        mappings: API_CONTRACT_MAPPINGS,
    }];

    #[test]
    fn folded_contracts_module_keeps_its_historical_targets_in_both_directions() {
        let expanded = expand_filter_targets("g5_api::tasks=debug", API_CONTRACT_FAMILIES);
        assert!(expanded.contains("g5_api_foundation::tasks=debug"));
        assert!(expanded.contains("g5_api_foundation::contracts::tasks=debug"));
        assert_eq!(
            rewrite_rendered_target(
                "{\"target\":\"g5_api_foundation::contracts::tasks\"}",
                "g5_api_foundation::contracts::tasks",
                API_CONTRACT_FAMILIES,
                "",
            ),
            "{\"target\":\"g5_api::tasks\"}"
        );
    }

    static ROOT_SHELL_MAPPINGS: &[LegacyTargetMapping] = &[
        LegacyTargetMapping {
            legacy_prefix: "g5d::config",
            physical_prefix: "g5d::cli::config",
        },
        LegacyTargetMapping {
            legacy_prefix: "g5d",
            physical_prefix: "g5d::process",
        },
    ];
    static ROOT_SHELL_FAMILIES: &[LegacyTargetFamily] = &[LegacyTargetFamily {
        legacy_root: "g5d",
        mappings: ROOT_SHELL_MAPPINGS,
    }];

    #[test]
    fn root_level_shell_events_survive_a_mixed_filter_that_silences_the_cli_module() {
        // Everything now lives in the `g5d` crate, so the root directive
        // needs no expansion and the module directive stays as written: the
        // config module's physical target sits under `g5d::cli` (silenced),
        // the shell's does not (kept at info by the root directive).
        let expanded = expand_filter_targets("g5d=info,g5d::cli=off", ROOT_SHELL_FAMILIES);
        let directives = expanded.split(',').collect::<Vec<_>>();
        assert!(directives.contains(&"g5d=info"), "{expanded}");
        assert!(directives.contains(&"g5d::cli=off"), "{expanded}");
        assert!(
            !directives
                .iter()
                .any(|directive| directive.starts_with("g5d::process=")),
            "no directive may address the shell target directly: {expanded}"
        );
        EnvFilter::try_new(&expanded).expect("expanded filter parses");
        let physical = |legacy: &str| {
            ROOT_SHELL_MAPPINGS
                .iter()
                .find(|mapping| mapping.legacy_prefix == legacy)
                .map(|mapping| mapping.physical_prefix)
                .expect("fixture mapping")
        };
        assert!(prefix_matches(physical("g5d::config"), "g5d::cli"));
        assert!(!prefix_matches(physical("g5d"), "g5d::cli"));
        assert_eq!(
            rewrite_rendered_target(
                "{\"target\":\"g5d::process\"}",
                "g5d::process",
                ROOT_SHELL_FAMILIES,
                "",
            ),
            "{\"target\":\"g5d\"}"
        );
    }

    #[test]
    fn folded_cli_module_keeps_its_historical_targets_in_both_directions() {
        // A module that moved under `g5d::cli` but historically logged at the
        // crate root keeps both directions through its mapping.
        assert_eq!(
            rewrite_rendered_target(
                "{\"target\":\"g5d::cli::config\"}",
                "g5d::cli::config",
                G5D_FAMILIES,
                "",
            ),
            "{\"target\":\"g5d::config\"}"
        );
        // A CLI subcommand module's physical target is already its historical
        // target: the filter selects it unchanged and rendering leaves it alone.
        let expanded = expand_filter_targets("g5d::cli::clone=debug", G5D_FAMILIES);
        assert!(expanded.starts_with("g5d::cli::clone=debug"));
        assert!(!expanded.contains("g5d::cli::config"));
        assert_eq!(
            rewrite_rendered_target(
                "{\"target\":\"g5d::cli::clone\"}",
                "g5d::cli::clone",
                G5D_FAMILIES,
                "",
            ),
            "{\"target\":\"g5d::cli::clone\"}"
        );
    }

    #[test]
    fn more_specific_module_mapping_wins_in_both_directions() {
        assert_eq!(
            expand_filter_targets("warn,g5d::host::supervisor=debug", G5D_FAMILIES),
            "warn,g5d::host::supervisor=debug,g5d_host::supervisor=debug"
        );
        assert_eq!(
            expand_filter_targets("g5d::config=trace", G5D_FAMILIES),
            "g5d::config=trace,g5d::cli::config=trace"
        );
        assert_eq!(
            rewrite_rendered_target(
                "{\"target\":\"g5d_host::supervisor\"}",
                "g5d_host::supervisor",
                G5D_FAMILIES,
                "",
            ),
            "{\"target\":\"g5d::host::supervisor\"}"
        );
        assert_eq!(
            expand_filter_targets("g5d::agent=debug", G5D_FAMILIES),
            "g5d::agent=debug,g5d_agent::inbox=debug"
        );
        let worker = expand_filter_targets("g5d::worker=debug", G5D_FAMILIES);
        assert!(worker.contains("g5d_agent::codex=debug"));
        assert!(worker.contains("g5d_host::agent_host=debug"));
        assert!(!worker.contains("g5d_agent::inbox=debug"));
        assert_eq!(
            rewrite_rendered_target(
                "{\"target\":\"g5d_agent::codex::proxy\"}",
                "g5d_agent::codex::proxy",
                G5D_FAMILIES,
                "",
            ),
            "{\"target\":\"g5d::worker::codex::proxy\"}"
        );
        assert_eq!(
            rewrite_rendered_target(
                " g5d_agent::runtime: turn",
                "g5d_agent::runtime",
                G5D_FAMILIES,
                "",
            ),
            " g5d_agent::runtime: turn",
            "agent-turn modules never had a logical prefix and render physically"
        );
        let git = expand_filter_targets("g5d::git=trace", G5D_FAMILIES);
        assert!(git.contains("g5d_foundation::scm_credentials=trace"));
    }

    #[test]
    fn rendered_json_and_text_targets_keep_the_logical_prefix() {
        assert_eq!(
            rewrite_rendered_target(
                "{\"target\":\"g5_api_core::auth::jwt\"}\n g5_api_routes::routes: ok",
                "g5_api_core::auth::jwt",
                FAMILIES,
                "",
            ),
            "{\"target\":\"g5_api::auth::jwt\"}\n g5_api_routes::routes: ok"
        );
    }

    #[test]
    fn target_rewrite_does_not_change_message_or_structured_payload() {
        let json = "{\"fields\":{\"message\":\"g5_api_core payload\",\"target\":\"g5_api_core::auth\"},\"target\":\"g5_api_core::auth\",\"span\":{\"target\":\"g5_api_core::auth\"}}\n";
        assert_eq!(
            rewrite_rendered_target(json, "g5_api_core::auth", FAMILIES, ""),
            "{\"fields\":{\"message\":\"g5_api_core payload\",\"target\":\"g5_api_core::auth\"},\"target\":\"g5_api::auth\",\"span\":{\"target\":\"g5_api_core::auth\"}}\n"
        );
        let text = " INFO span{artifact=g5_api_core::auth}: g5_api_core::auth: message g5_api_core::auth\n";
        assert_eq!(
            rewrite_rendered_target(
                text,
                "g5_api_core::auth",
                FAMILIES,
                "message g5_api_core::auth",
            ),
            " INFO span{artifact=g5_api_core::auth}: g5_api::auth: message g5_api_core::auth\n"
        );
    }

    #[derive(Clone)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

    impl io::Write for CaptureWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'writer> tracing_subscriber::fmt::MakeWriter<'writer> for Capture {
        type Writer = CaptureWriter;

        fn make_writer(&'writer self) -> Self::Writer {
            CaptureWriter(Arc::clone(&self.0))
        }
    }

    #[test]
    fn historical_filter_selects_new_target_and_json_emits_historical_target() {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .json()
            .event_format(LegacyTargetFormatter::new(
                tracing_subscriber::fmt::format()
                    .json()
                    .with_current_span(true),
                FAMILIES,
            ))
            .with_env_filter(
                EnvFilter::try_new(expand_filter_targets("warn,g5_api::auth=debug", FAMILIES))
                    .unwrap(),
            )
            .with_writer(Capture(Arc::clone(&bytes)))
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!(
                target: "g5_api_core::auth::jwt",
                "compat_span",
                artifact = "g5_api_core::auth::jwt"
            );
            let _guard = span.enter();
            tracing::debug!(
                target: "g5_api_core::auth::jwt",
                target = "g5_api_core::auth::jwt",
                "compatibility probe g5_api_core::auth::jwt"
            );
        });

        let rendered = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(value["target"], "g5_api::auth::jwt");
        assert_eq!(value["fields"]["target"], "g5_api_core::auth::jwt");
        assert_eq!(value["span"]["artifact"], "g5_api_core::auth::jwt");
        assert!(value["fields"]["message"]
            .as_str()
            .unwrap()
            .contains("g5_api_core::auth::jwt"));
    }

    #[test]
    fn human_formatter_preserves_forced_ansi_output() {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(true)
            .event_format(LegacyTargetFormatter::new(
                tracing_subscriber::fmt::format().with_ansi(true),
                FAMILIES,
            ))
            .with_writer(Capture(Arc::clone(&bytes)))
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("compat_span", artifact = "g5_api_core::auth");
            let _guard = span.enter();
            tracing::info!(
                target: "g5_api_core::auth",
                artifact = "g5_api_core::auth",
                "ansi compatibility probe g5_api_core::auth"
            );
        });

        let rendered = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        assert!(rendered.contains("\u{1b}["));
        assert!(rendered.contains("g5_api::auth"));
        assert!(rendered.matches("g5_api_core::auth").count() >= 3);
    }

    #[test]
    fn human_formatter_preserves_plain_output_when_ansi_is_disabled() {
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .event_format(LegacyTargetFormatter::new(
                tracing_subscriber::fmt::format().with_ansi(false),
                FAMILIES,
            ))
            .with_writer(Capture(Arc::clone(&bytes)))
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("compat_span", artifact = "g5_api_core::auth");
            let _guard = span.enter();
            tracing::info!(
                target: "g5_api_core::auth",
                artifact = "g5_api_core::auth",
                "plain compatibility probe g5_api_core::auth"
            );
        });

        let rendered = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        assert!(!rendered.contains("\u{1b}["));
        assert!(rendered.contains("g5_api::auth"));
        assert!(rendered.matches("g5_api_core::auth").count() >= 3);
    }
}
