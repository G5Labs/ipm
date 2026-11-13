//! Built-in explainer style templates + saved-template slug contract
//! (`software_architecture/g5-common/types/core` D26).
//!
//! The Create Explainer modal offers six built-in style briefs as prompt
//! starting points. The constants here are their source of truth.
//!
//! Saved templates are committed as `.g5/explainer_templates/<slug>.yaml`
//! (YAML shape `{ name, prompt }`); [`validate_explainer_template_slug`]
//! is the single slug contract shared by API validation and file naming,
//! and the six built-in slugs are reserved so a committed file can never
//! shadow a built-in.

/// One built-in explainer style template shipped with the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltInExplainerTemplate {
    /// Display name shown in the template picker.
    pub name: &'static str,
    /// Reserved kebab-case slug identifying the template.
    pub slug: &'static str,
    /// The style brief.
    pub prompt: &'static str,
}

/// Big picture (simplified explainer).
const BIG_PICTURE_PROMPT: &str = r#"Objective: explain the subject so a smart reader with zero context on the system fully gets it.
- Complexity: simplified explainer — no unexplained acronyms, define every term at first use, lead with a concrete analogy.
- Visuals: many — one hero diagram that captures the whole concept, a numbered timeline for flows, side-by-side comparison lanes for alternatives, myth-vs-truth cards for misconceptions.
- Length: standard — open with a 30-second TL;DR, then one focused pass; no padding.
- Precision: keep the real file, flag, and function names in code blocks so engineers can follow up.
- Ending: close on the one open decision, if there is one."#;

/// Balanced (default).
const BALANCED_PROMPT: &str = r#"Objective: a readable walkthrough of the subject for an engineer who is new to the area but comfortable with the stack.
- Complexity: practitioner — define project-specific jargon once, then use it normally.
- Visuals: balanced — add a diagram wherever structure or flow is easier to see than to read.
- Length: standard — thorough, but no padding.
- Structure: overview first, then each responsibility in the order the system exercises it."#;

/// Deep technical.
const DEEP_TECHNICAL_PROMPT: &str = r#"Objective: a dense reference on the subject for an expert who already knows the system.
- Complexity: expert — assume full context, no hand-holding.
- Visuals: minimal — diagrams only where they earn their place.
- Length: extensive — completeness beats brevity.
- Precision: exact identifiers, contracts, invariants, failure semantics, and edge conditions, kept verbatim from the evidence."#;

/// Corner cases.
const CORNER_CASES_PROMPT: &str = r#"Objective: surface the subject's edge cases, failure modes, and surprising behavior — not the happy path.
- Complexity: expert — exact identifiers, no hand-holding.
- Visuals: balanced — myth-vs-truth cards for things readers commonly assume wrong.
- Length: extensive — cover every corner worth knowing.
- Structure: for each case give the triggering condition, the observed behavior, and why the system works that way."#;

/// Demo walkthrough.
const DEMO_WALKTHROUGH_PROMPT: &str = r#"Objective: narrate the subject as a live demo of the feature it powers, for a prospect audience.
- Complexity: non-technical first — what the user sees and clicks, then what happens behind the scenes.
- Visuals: screenshot-style walkthrough frames over architecture diagrams.
- Length: brief — demo pacing, one scene per capability.
- Tone: friendly and concrete, no internal jargon."#;

/// New-hire onboarding.
const NEW_HIRE_ONBOARDING_PROMPT: &str = r#"Objective: place the subject on the map for an engineer in their first week.
- Complexity: new hire — assume day-one context, spell out repo-layout terms.
- Visuals: one orientation diagram showing the subject's neighbors and data flow.
- Length: standard — enough to navigate by, not a reference.
- Structure: where it lives, its neighbors, what to read next; link every concept to the implementing file or spec.
- Ending: short glossary."#;

/// The six built-in explainer style templates, in the catalog order the
/// picker displays them (core D26).
pub const BUILT_IN_EXPLAINER_TEMPLATES: [BuiltInExplainerTemplate; 6] = [
    BuiltInExplainerTemplate {
        name: "Big picture (simplified explainer)",
        slug: "big-picture",
        prompt: BIG_PICTURE_PROMPT,
    },
    BuiltInExplainerTemplate {
        name: "Balanced",
        slug: "balanced",
        prompt: BALANCED_PROMPT,
    },
    BuiltInExplainerTemplate {
        name: "Deep technical",
        slug: "deep-technical",
        prompt: DEEP_TECHNICAL_PROMPT,
    },
    BuiltInExplainerTemplate {
        name: "Corner cases",
        slug: "corner-cases",
        prompt: CORNER_CASES_PROMPT,
    },
    BuiltInExplainerTemplate {
        name: "Demo walkthrough",
        slug: "demo-walkthrough",
        prompt: DEMO_WALKTHROUGH_PROMPT,
    },
    BuiltInExplainerTemplate {
        name: "New-hire onboarding",
        slug: "new-hire-onboarding",
        prompt: NEW_HIRE_ONBOARDING_PROMPT,
    },
];

/// Maximum length of an explainer template slug (file-stem).
pub const EXPLAINER_TEMPLATE_SLUG_MAX_LEN: usize = 64;

/// True when `slug` is one of the six reserved built-in template slugs.
/// Saving a template under a reserved slug is always rejected so a committed
/// `.g5/explainer_templates/<slug>.yaml` can never shadow a built-in.
pub fn is_reserved_explainer_template_slug(slug: &str) -> bool {
    BUILT_IN_EXPLAINER_TEMPLATES.iter().any(|t| t.slug == slug)
}

/// Validate an explainer template slug as a safe kebab-case file-stem
/// (core D26). It must be non-empty, at most
/// [`EXPLAINER_TEMPLATE_SLUG_MAX_LEN`] chars, and contain only `[a-z0-9-]` —
/// so it can never contain `/`, `.`, or `..` and thus can never escape the
/// `.g5/explainer_templates/` directory when joined to a path. Mirrors
/// `validate_preview_profile_name` (D18).
///
/// Reserved built-in slugs pass this shape check; save paths must reject
/// them separately via [`is_reserved_explainer_template_slug`].
pub fn validate_explainer_template_slug(slug: &str) -> Result<(), String> {
    if slug.is_empty() {
        return Err("template slug must not be empty".to_string());
    }
    if slug.len() > EXPLAINER_TEMPLATE_SLUG_MAX_LEN {
        return Err(format!(
            "template slug must be at most {EXPLAINER_TEMPLATE_SLUG_MAX_LEN} characters"
        ));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(
            "template slug must contain only lowercase ASCII letters, digits, or '-'".to_string(),
        );
    }
    Ok(())
}

/// Validate a slug for SAVING a template: shape-valid per
/// [`validate_explainer_template_slug`] AND not a reserved built-in slug.
pub fn validate_explainer_template_slug_for_save(slug: &str) -> Result<(), String> {
    validate_explainer_template_slug(slug)?;
    if is_reserved_explainer_template_slug(slug) {
        return Err(format!(
            "template slug `{slug}` is reserved for a built-in template"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn six_builtin_templates_present_with_nonempty_fields() {
        assert_eq!(BUILT_IN_EXPLAINER_TEMPLATES.len(), 6);
        let expected_slugs = [
            "big-picture",
            "balanced",
            "deep-technical",
            "corner-cases",
            "demo-walkthrough",
            "new-hire-onboarding",
        ];
        for (template, expected_slug) in BUILT_IN_EXPLAINER_TEMPLATES.iter().zip(expected_slugs) {
            assert_eq!(template.slug, expected_slug);
            assert!(!template.name.trim().is_empty());
            assert!(!template.prompt.trim().is_empty());
            assert!(
                template.prompt.starts_with("Objective:"),
                "brief for `{}` must be a labeled-bullet Objective brief",
                template.slug
            );
        }
    }

    #[test]
    fn builtin_slugs_are_unique_and_shape_valid() {
        let mut seen = std::collections::BTreeSet::new();
        for template in &BUILT_IN_EXPLAINER_TEMPLATES {
            assert!(
                validate_explainer_template_slug(template.slug).is_ok(),
                "built-in slug `{}` must satisfy the slug shape contract",
                template.slug
            );
            assert!(
                seen.insert(template.slug),
                "duplicate slug {}",
                template.slug
            );
            assert!(is_reserved_explainer_template_slug(template.slug));
        }
    }

    #[test]
    fn explainer_template_slug_validation() {
        assert!(validate_explainer_template_slug("my-style").is_ok());
        assert!(validate_explainer_template_slug("a").is_ok());
        assert!(validate_explainer_template_slug("style-2").is_ok());
        assert!(validate_explainer_template_slug(&"a".repeat(64)).is_ok());

        assert!(validate_explainer_template_slug("").is_err());
        assert!(validate_explainer_template_slug(&"a".repeat(65)).is_err());
        assert!(validate_explainer_template_slug("My-Style").is_err());
        assert!(validate_explainer_template_slug("has_underscore").is_err());
        assert!(validate_explainer_template_slug("has.dot").is_err());
        assert!(validate_explainer_template_slug("has/slash").is_err());
        assert!(validate_explainer_template_slug("..").is_err());
        assert!(validate_explainer_template_slug("has space").is_err());
    }

    #[test]
    fn reserved_slugs_rejected_for_save() {
        for template in &BUILT_IN_EXPLAINER_TEMPLATES {
            let err = validate_explainer_template_slug_for_save(template.slug)
                .expect_err("reserved slug must be rejected for saves");
            assert!(err.contains("reserved"), "unexpected error: {err}");
        }
        assert!(validate_explainer_template_slug_for_save("my-style").is_ok());
        assert!(validate_explainer_template_slug_for_save("Bad_Slug").is_err());
    }
}
