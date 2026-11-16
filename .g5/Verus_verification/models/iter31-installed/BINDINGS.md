# installed · iter31

Installed modules dependency rows on the requested name (VersionRequirements, minus excluded names) are And-ed; LoadModuleReference refuses a repository reference that fails them. Explicit 2.0.0 and unversioned requests refused (message lists i31a and i31p), explicit satisfying 1.7.0 accepted, removing A admits 2.0.0. Graph-module exclusion is represented but undistinguished (parent never installed in checked cases). SemVer And/IsSatisfiedBy are provider premises over x.y.z release codes.

- Installed modules dependency rows on the requested name (VersionRequirements, minus excluded names) are And-ed; LoadModuleReference refuses a repository reference that fails them. Explicit 2.0.0 and unversioned requests refused (message lists i31a and i31p), explicit satisfying 1.7.0 accepted, removing A admits 2.0.0. Graph-module exclusion is represented but undistinguished (parent never installed in checked cases). SemVer And/IsSatisfiedBy are provider premises over x.y.z release codes.
- Same P manifest picks B 2.0.0 without A and 1.7.0 with A installed: installed requirements and the no-downgrade floor join the search expression; installed B kept only when it satisfies them. Highest-first repository order is a provider premise (control search-lists-lowest-first). Snapshot reload, PermitDowngrade, co-updating exclusion and fuzzy backtracking over sub-dependencies unassessed.
- Unversioned install takes the first (latest) repository result without consulting installed requirements, so it is refused although installed 1.7.0 satisfies them; explicit literal selects that version. -prompt/-keywords/registry prefix unassessed.
- load <directory> of B 2.0.0 succeeds and replaces B while installed A requires <2.0.0; the installed-requirement check is only reached through LoadModuleReference (source: two callers of GetRequiredVersionExpression). Dev-mode directory load unassessed.

The proofs verify models. Their product correspondence and trust boundaries are documented in the linked evidence.

[Formal declaration](../../consolidated/iter31/installed.rs)

[Product correspondence](../../../uplift/consolidated/iter31/bindings.json)

[Proof and controls](../../../uplift/consolidated/iter31/proofs/models.json)

[Iteration handoff](../../../uplift/consolidated/iter31/HANDOFF.md)

[Sealed project map](../../../uplift/consolidated/iter31/project-map.json)
