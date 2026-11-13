//! Supply the G5 harness to a provider launch.
//!
//! The platform runtime packages `.agents/AGENTS.md`, `.agents/skills/<name>`
//! and ordinary documents/tooling under `.agents/guidance`.
//! Both clients receive the same instruction bytes in their owner-scoped homes:
//! Codex at `.codex/AGENTS.md`, Claude Code at `.claude/CLAUDE.md`. Skills go in
//! each client's native `skills` directory; guidance goes in the owner's
//! `.agents/guidance`, outside skill discovery.
//!
//! Nothing is written into a member checkout. Only provided instructions,
//! skills and guidance, G5's own `g5-tools` entry and retired G5 files change;
//! credentials, sessions, other settings and the owner's other files stay.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use toml_edit::DocumentMut;

const INSTRUCTIONS: &str = "AGENTS.md";
const SKILLS: &str = ".agents/skills";
const GUIDANCE: &str = ".agents/guidance";
const PROVIDED_GUIDANCE: &str = ".g5/provided-guidance";
const GUIDANCE_DOCUMENTS: &[&str] = &[
    "ontology.md",
    "ontology-format.md",
    "ontology-examples.md",
    "uplift.md",
    "downlift.md",
];
// Earlier bundles contain only the flat guidance. Bundles advertising the new
// workflow paths must ship both sets of entry points and their common mechanics.
const WORKFLOW_DOCUMENTS: &[&str] = &[
    "TLC/uplift.md",
    "TLC/downlift.md",
    "TLC/tla-design.md",
    "TLC/model-checking.md",
    "TLC/setup_foundation.md",
    "TLC/pr_incremental_TLA_reverification.md",
    "Verus/uplift.md",
    "Verus/downlift.md",
    "Verus/new-feature-pr.md",
    "Verus/incremental-reverification.md",
    "Verus/verification.md",
    "Verus/setup_foundation.md",
];
/// Skills the harness last put in a provider home, so a later harness can retire
/// the ones it no longer provides without touching the owner's own skills.
const PROVIDED_SKILLS: &str = ".g5-provided-skills";

fn source_dir() -> PathBuf {
    std::env::var_os("G5_HARNESS_DIR")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        // The host mounts its runtime bundle here for every workspace VM.
        .unwrap_or_else(|| PathBuf::from("/opt/g5/assets/harness"))
}

/// Why the harness could not be supplied to a launch.
#[derive(Debug)]
pub(crate) enum HarnessError {
    /// The runtime bundle has no complete harness.
    Unavailable(io::Error),
    /// The harness could not be written into the provider home.
    Install(io::Error),
}

/// Instructions a personal-workspace launch adds to the packaged harness
/// (`g5.personal.project-docs`, `g5.personal.durability`,
/// `g5.personal.follow`, `g5.personal.file-citations`).
pub(crate) const PERSONAL_GUIDANCE: &str = "

## Personal workspace

You are working in the owner's personal workspace.

- `/work/personal` is the owner's private repository and the only durable place here. Commit and push artifacts to it as you make them; anything not pushed can be lost with the VM, and the platform never commits for you.
- The owner's other workspaces also push to this branch. G5 fast-forwards this checkout when it has no commits of its own; if a push is refused as non-fast-forward, run `git pull --rebase` and push again.
- The other checkouts under `/work` are read-only references to the organization's Base repositories at the commit this VM started from; `git pull` brings in Base's newer commits. They are discarded at the next Start; never keep work in them. Use ordinary Git in them to read other workspaces' pushed branches.
- Keep G5 workflow records (`.g5/chat_intent`, plans, ontology drafts) and other documents in the private repository, never in a reference checkout. To change the codebase, the owner forks Base; that fork mounts this repository, so its agent can read these records and copy into the fork what the fork should keep.
- In this workspace's chat, write file paths as backticked plain text, not `g5://file` links: this chat also shows on Home, where those links do not open.
";

/// Refresh the harness in Claude Code's native user configuration directory.
pub(crate) fn prepare_claude(home: &Path, personal: bool) -> Result<(), HarnessError> {
    install_claude(&source_dir(), home, personal)
}

/// Refresh the harness in the Codex home shared by exec and native login.
pub(crate) fn prepare_codex(home: &Path, personal: bool) -> Result<(), HarnessError> {
    install_codex(&source_dir(), &home.join(".codex"), personal)
}

struct Harness {
    instructions: Vec<u8>,
    skills: Vec<(OsString, PathBuf)>,
    guidance: Vec<(PathBuf, Vec<u8>)>,
}

impl Harness {
    /// The packaged instructions, plus the personal guidance for a launch in
    /// a personal workspace.
    fn for_launch(mut self, personal: bool) -> Self {
        if personal {
            self.instructions
                .extend_from_slice(PERSONAL_GUIDANCE.as_bytes());
        }
        self
    }
}

fn incomplete(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, message)
}

/// Read the packaged harness, refusing an incomplete one before any write.
fn load(source: &Path) -> io::Result<Harness> {
    let instructions = std::fs::read(source.join(INSTRUCTIONS))
        .map_err(|_| incomplete("Harness instructions are missing"))?;
    let mut skills = Vec::new();
    let entries = std::fs::read_dir(source.join(SKILLS))
        .map_err(|_| incomplete("Harness skills are missing"))?;
    for entry in entries {
        let entry = entry?;
        // This workflow is installed only by the formal-review supervisor,
        // whose closed tools and frozen assignment establish its authority.
        if entry.file_name() == "g5-agent-approval-review" {
            continue;
        }
        if !entry.file_type()?.is_dir() {
            continue;
        }
        if !entry.path().join("SKILL.md").is_file() {
            return Err(incomplete("Harness skill is incomplete"));
        }
        skills.push((entry.file_name(), entry.path()));
    }
    if instructions.is_empty() || skills.is_empty() {
        return Err(incomplete("Harness is empty"));
    }
    skills.sort();
    // Old bundles remain usable. A bundle that advertises guidance must carry
    // its documents; reject a partial upgrade before changing an owner's home.
    let guidance_root = source.join(GUIDANCE);
    let mut guidance = Vec::new();
    let instructions_text = String::from_utf8_lossy(&instructions);
    if guidance_root.exists() {
        let advertises_workflows = instructions_text.contains(".agents/guidance/TLC/")
            || instructions_text.contains(".agents/guidance/Verus/");
        let workflows = if advertises_workflows { WORKFLOW_DOCUMENTS } else { &[] };
        for name in GUIDANCE_DOCUMENTS.iter().chain(workflows) {
            if std::fs::read(guidance_root.join(name))?.is_empty() {
                return Err(incomplete("Harness guidance document is empty"));
            }
        }
        read_guidance(&guidance_root, &guidance_root, &mut guidance)?;
        guidance.sort_by(|a, b| a.0.cmp(&b.0));
    } else if instructions_text.contains(".agents/guidance/") {
        return Err(incomplete("Harness guidance is missing"));
    }
    Ok(Harness {
        instructions,
        skills,
        guidance,
    })
}

fn read_guidance(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(PathBuf, Vec<u8>)>,
) -> io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            read_guidance(root, &entry.path(), files)?;
        } else if kind.is_file() {
            files.push((
                entry.path().strip_prefix(root).unwrap().to_path_buf(),
                std::fs::read(entry.path())?,
            ));
        } else {
            return Err(incomplete("Harness guidance must contain regular files"));
        }
    }
    Ok(())
}

fn install_claude(source: &Path, home: &Path, personal: bool) -> Result<(), HarnessError> {
    let harness = load(source)
        .map(|harness| harness.for_launch(personal))
        .map_err(HarnessError::Unavailable)?;
    write_claude(&harness, home).map_err(HarnessError::Install)
}

fn write_claude(harness: &Harness, home: &Path) -> io::Result<()> {
    let claude_home = home.join(".claude");
    std::fs::create_dir_all(&claude_home)?;
    write_guidance(harness, home)?;
    atomic_write(
        &claude_home.join("CLAUDE.md"),
        &harness.instructions,
        None,
        &claude_home,
    )?;
    write_skills(harness, &claude_home)?;
    // Only this directory belonged entirely to the previous G5 installer.
    // Remove it after publishing the replacement, so partial installs can retry.
    remove_path(&home.join(".g5/harness"))
}

fn install_codex(source: &Path, codex_home: &Path, personal: bool) -> Result<(), HarnessError> {
    let harness = load(source)
        .map(|harness| harness.for_launch(personal))
        .map_err(HarnessError::Unavailable)?;
    write_codex(&harness, codex_home).map_err(HarnessError::Install)
}

fn write_codex(harness: &Harness, codex_home: &Path) -> io::Result<()> {
    std::fs::create_dir_all(codex_home)?;
    if let Some(home) = codex_home.parent() {
        write_guidance(harness, home)?;
    }
    remove_retired_review_tools(codex_home)?;
    if let Some(home) = codex_home.parent() {
        remove_retired_review_credentials(home)?;
    }
    remove_legacy_profile(codex_home)?;
    atomic_write(
        &codex_home.join(INSTRUCTIONS),
        &harness.instructions,
        None,
        codex_home,
    )?;
    write_skills(harness, codex_home)
}

fn write_guidance(harness: &Harness, home: &Path) -> io::Result<()> {
    // A legacy bundle neither publishes guidance nor retires a newer bundle's
    // documents. Only paths recorded by this installer may be removed.
    if harness.guidance.is_empty() {
        return Ok(());
    }
    let target = home.join(GUIDANCE);
    std::fs::create_dir_all(&target)?;
    let record = home.join(PROVIDED_GUIDANCE);
    if let Some(parent) = record.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Ok(previous) = std::fs::read_to_string(&record) {
        for name in previous.lines() {
            let relative = Path::new(name);
            let safe = !name.is_empty()
                && relative
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_)));
            if safe && !harness.guidance.iter().any(|(path, _)| path == relative) {
                remove_path(&target.join(relative))?;
            }
        }
    }
    let mut listing = String::new();
    for (relative, bytes) in &harness.guidance {
        let path = target.join(relative);
        std::fs::create_dir_all(path.parent().unwrap())?;
        atomic_write(&path, bytes, None, &target)?;
        listing.push_str(&relative.to_string_lossy());
        listing.push('\n');
    }
    atomic_write(&record, listing.as_bytes(), None, &target)
}

fn write_skills(harness: &Harness, provider_home: &Path) -> io::Result<()> {
    // The owner may keep other skills here, so only the provided ones change.
    let skills = provider_home.join("skills");
    std::fs::create_dir_all(&skills)?;
    let record = provider_home.join(PROVIDED_SKILLS);
    if let Ok(previous) = std::fs::read_to_string(&record) {
        for name in previous.lines() {
            let simple = !name.is_empty() && name != "." && name != ".." && !name.contains('/');
            if simple && !harness.skills.iter().any(|(provided, _)| provided == name) {
                remove_path(&skills.join(name))?;
            }
        }
    }
    let mut listing = String::new();
    for (name, path) in &harness.skills {
        mirror(path, &skills.join(name), provider_home)?;
        listing.push_str(&name.to_string_lossy());
        listing.push('\n');
    }
    atomic_write(&record, listing.as_bytes(), None, provider_home)
}

/// Remove G5's retired review server from retained owner homes. Other MCP
/// servers and settings remain byte-for-byte unchanged when no removal is needed.
fn remove_retired_review_tools(codex_home: &Path) -> io::Result<()> {
    let config_path = codex_home.join("config.toml");
    let text = match std::fs::read_to_string(&config_path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    // A malformed owner configuration is left for Codex to report.
    let Ok(mut config) = text.parse::<DocumentMut>() else {
        return Ok(());
    };
    let Some(servers) = config
        .get_mut("mcp_servers")
        .and_then(|item| item.as_table_like_mut())
    else {
        return Ok(());
    };
    if servers.remove("g5-tools").is_none() {
        return Ok(());
    }
    atomic_write(
        &config_path,
        config.to_string().as_bytes(),
        None,
        codex_home,
    )
}

/// Older launches left one credential file per review MCP process.
fn remove_retired_review_credentials(home: &Path) -> io::Result<()> {
    let entries = match std::fs::read_dir(home.join(".g5")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let entry = entry?;
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("codex-g5-tools-env-")
        {
            continue;
        }
        match entry.file_type() {
            Ok(kind) if !kind.is_dir() => remove_path(&entry.path())?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
            _ => {}
        }
    }
    Ok(())
}

/// A home that ran before the harness replaced the shipped profile still holds
/// that profile's G5 skills and bootstrap file.
fn remove_legacy_profile(codex_home: &Path) -> io::Result<()> {
    let marker = codex_home.join("profile_manifest.json");
    if !marker.is_file() {
        return Ok(());
    }
    if let Ok(entries) = std::fs::read_dir(codex_home.join("skills")) {
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("g5-") || name == "skill-builder" {
                remove_path(&entry.path())?;
            }
        }
    }
    remove_path(&codex_home.join("G5_SUBAGENT_BOOTSTRAP.md"))?;
    // Last, so an interrupted cleanup runs again on the next launch.
    remove_path(&marker)
}

/// Make `target` an exact copy of the directory `source`.
fn mirror(source: &Path, target: &Path, stage: &Path) -> io::Result<()> {
    if std::fs::symlink_metadata(target).is_ok_and(|metadata| !metadata.is_dir()) {
        remove_path(target)?;
    }
    std::fs::create_dir_all(target)?;
    let mut keep = BTreeSet::new();
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let from = entry.path();
        let to = target.join(&name);
        let metadata = std::fs::symlink_metadata(&from)?;
        if metadata.is_dir() {
            mirror(&from, &to, stage)?;
        } else if metadata.is_file() {
            if std::fs::symlink_metadata(&to).is_ok_and(|existing| existing.is_dir()) {
                remove_path(&to)?;
            }
            atomic_write(
                &to,
                &std::fs::read(&from)?,
                Some(metadata.permissions()),
                stage,
            )?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unsupported harness file",
            ));
        }
        keep.insert(name);
    }
    remove_others(target, &keep.iter().collect())
}

fn remove_others(directory: &Path, keep: &BTreeSet<&OsString>) -> io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if !keep.contains(&entry.file_name()) {
            remove_path(&entry.path())?;
        }
    }
    Ok(())
}

/// Remove a file or directory; a concurrent launch may already have done so.
fn remove_path(path: &Path) -> io::Result<()> {
    let result = match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(error) => Err(error),
    };
    match result {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Publish only complete files: a concurrent turn or native login may read the
/// target while another launch refreshes it. Temporary files stay in `stage`,
/// outside the mirrored directories, so a concurrent mirror cannot remove them.
fn atomic_write(
    path: &Path,
    bytes: &[u8],
    permissions: Option<std::fs::Permissions>,
    stage: &Path,
) -> io::Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(stage)?;
    file.write_all(bytes)?;
    if let Some(permissions) = permissions {
        file.as_file().set_permissions(permissions)?;
    }
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Package the same instruction and skill sources as the worker build.
    fn source() -> PathBuf {
        static BUNDLE: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
        BUNDLE
            .get_or_init(|| {
                let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
                let bundle = tempfile::tempdir().unwrap();
                std::fs::copy(
                    repo.join(".agents/AGENTS.md"),
                    bundle.path().join(INSTRUCTIONS),
                )
                .unwrap();
                for directory in [SKILLS, GUIDANCE] {
                    for file in files(&repo.join(directory)) {
                        let target = bundle.path().join(directory).join(&file);
                        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                        std::fs::copy(repo.join(directory).join(file), target).unwrap();
                    }
                }
                bundle
            })
            .path()
            .to_path_buf()
    }

    fn skill_names() -> Vec<OsString> {
        load(&source())
            .unwrap()
            .skills
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }

    fn files(root: &Path) -> BTreeSet<PathBuf> {
        let mut found = BTreeSet::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(&directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    found.insert(path.strip_prefix(root).unwrap().to_path_buf());
                }
            }
        }
        found
    }

    #[test]
    fn a_personal_launch_adds_the_personal_guidance_to_both_providers() {
        let home = tempfile::tempdir().unwrap();
        install_claude(&source(), home.path(), true).unwrap();
        install_codex(&source(), &home.path().join(".codex"), true).unwrap();
        let packaged = std::fs::read_to_string(source().join(INSTRUCTIONS)).unwrap();
        for path in [".claude/CLAUDE.md", ".codex/AGENTS.md"] {
            let written = std::fs::read_to_string(home.path().join(path)).unwrap();
            assert!(written.starts_with(&packaged), "{path}");
            assert!(written.ends_with(PERSONAL_GUIDANCE), "{path}");
        }
        let ordinary = tempfile::tempdir().unwrap();
        install_claude(&source(), ordinary.path(), false).unwrap();
        assert_eq!(
            std::fs::read_to_string(ordinary.path().join(".claude/CLAUDE.md")).unwrap(),
            packaged
        );
        // The packaged instructions describe the personal mount every other
        // workspace carries (g5.personal.mount).
        assert!(packaged.contains("`/work/personal`"));
        // Chat replies cite files as `g5://file` links (g5.chat.file-citations),
        // except in personal chats, which keep plain paths
        // (g5.personal.file-citations); the exception follows the rule.
        assert!(packaged.contains("In G5 chat replies, cite a workspace file"));
        assert!(packaged.contains("`?repo=<repository id>`"));
        assert!(PERSONAL_GUIDANCE.contains("not `g5://file` links"));
        assert!(!packaged.contains("not `g5://file` links"));
    }

    #[test]
    fn the_maintained_sources_are_a_complete_harness() {
        let names = skill_names();
        assert_eq!(
            names,
            vec![OsString::from("ontology-planning")],
            "ordinary sessions discover only the planning skill"
        );
        let harness = load(&source()).unwrap();
        assert!(harness
            .guidance
            .iter()
            .any(|(path, _)| path == Path::new("deep-review.md")));
        for name in GUIDANCE_DOCUMENTS.iter().chain(WORKFLOW_DOCUMENTS) {
            assert!(harness
                .guidance
                .iter()
                .any(|(path, _)| path == Path::new(name)));
        }
    }

    #[test]
    fn claude_home_gets_the_harness_without_replacing_owner_state() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join(".claude");
        for (file, contents) in [
            (".credentials.json", "keep credentials"),
            ("settings.json", "{}"),
            ("projects/session.jsonl", "keep session"),
            ("skills/owner-skill/SKILL.md", "keep skill"),
            ("rules/owner.md", "keep rule"),
        ] {
            let path = home.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        let legacy = temp.path().join(".g5/harness");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("CLAUDE.md"), "old instructions").unwrap();
        install_claude(&source(), temp.path(), false).unwrap();
        assert_eq!(
            std::fs::read(home.join("CLAUDE.md")).unwrap(),
            std::fs::read(source().join(INSTRUCTIONS)).unwrap()
        );
        assert!(!legacy.exists());

        // Repeated and concurrent starts refresh provided content, retire only
        // previously provided skills, and leave the owner's other files intact.
        std::fs::write(home.join("CLAUDE.md"), "changed").unwrap();
        std::fs::create_dir_all(home.join("skills/retired")).unwrap();
        std::fs::write(home.join("skills/retired/SKILL.md"), "retired").unwrap();
        std::fs::write(home.join("skills/ontology-planning/extra.md"), "stale").unwrap();
        let mut record = std::fs::read_to_string(home.join(PROVIDED_SKILLS)).unwrap();
        record.push_str("retired\n../projects\n");
        std::fs::write(home.join(PROVIDED_SKILLS), record).unwrap();
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| install_claude(&source(), temp.path(), false).unwrap());
            }
        });
        assert_eq!(
            std::fs::read(home.join("CLAUDE.md")).unwrap(),
            std::fs::read(source().join(INSTRUCTIONS)).unwrap()
        );
        for name in skill_names() {
            assert_eq!(
                files(&home.join("skills").join(&name)),
                files(&source().join(SKILLS).join(&name))
            );
        }
        assert!(!home.join("skills/retired").exists());
        for (file, contents) in [
            (".credentials.json", "keep credentials"),
            ("settings.json", "{}"),
            ("projects/session.jsonl", "keep session"),
            ("skills/owner-skill/SKILL.md", "keep skill"),
            ("rules/owner.md", "keep rule"),
        ] {
            assert_eq!(std::fs::read_to_string(home.join(file)).unwrap(), contents);
        }
    }

    #[test]
    fn claude_retries_a_partial_install_before_removing_the_legacy_harness() {
        let home = tempfile::tempdir().unwrap();
        let legacy = home.path().join(".g5/harness");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("CLAUDE.md"), "old instructions").unwrap();
        let claude = home.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(claude.join("skills"), "not a directory").unwrap();
        assert!(matches!(
            install_claude(&source(), home.path(), false),
            Err(HarnessError::Install(_))
        ));
        assert!(legacy.join("CLAUDE.md").is_file());
        std::fs::remove_file(claude.join("skills")).unwrap();
        install_claude(&source(), home.path(), false).unwrap();
        assert!(!legacy.exists());
        assert_eq!(
            std::fs::read(claude.join("CLAUDE.md")).unwrap(),
            std::fs::read(source().join(INSTRUCTIONS)).unwrap()
        );
        assert!(claude.join("skills/ontology-planning/SKILL.md").is_file());
    }

    #[test]
    fn both_providers_copy_updated_instructions_from_one_bundle() {
        let bundle = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let skill = bundle.path().join(SKILLS).join("test-skill");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("SKILL.md"), "test skill").unwrap();
        for instructions in [
            "Packaged workflow version one\n",
            "Packaged workflow version two\n",
        ] {
            std::fs::write(bundle.path().join(INSTRUCTIONS), instructions).unwrap();
            install_claude(bundle.path(), home.path(), false).unwrap();
            install_codex(bundle.path(), &home.path().join(".codex"), false).unwrap();
            for target in [".claude/CLAUDE.md", ".codex/AGENTS.md"] {
                assert_eq!(
                    std::fs::read(home.path().join(target)).unwrap(),
                    instructions.as_bytes()
                );
            }
        }
        // Missing instructions remain invalid.
        std::fs::remove_file(bundle.path().join(INSTRUCTIONS)).unwrap();
        assert!(matches!(
            install_claude(bundle.path(), home.path(), false),
            Err(HarnessError::Unavailable(_))
        ));
        assert!(matches!(
            install_codex(bundle.path(), &home.path().join(".codex"), false),
            Err(HarnessError::Unavailable(_))
        ));
    }

    #[test]
    fn codex_home_gets_the_harness_without_replacing_owner_state() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join(".codex");
        std::fs::create_dir_all(home.join("sessions")).unwrap();
        std::fs::create_dir_all(home.join("skills/owner-skill")).unwrap();
        std::fs::write(home.join("skills/owner-skill/SKILL.md"), "keep skill").unwrap();
        std::fs::write(home.join("auth.json"), "keep credentials").unwrap();
        std::fs::write(home.join("sessions/turn.jsonl"), "keep session").unwrap();
        let config = "model = 'owner-model'\n[mcp_servers.custom]\ncommand = 'custom'\n";
        std::fs::write(home.join("config.toml"), config).unwrap();

        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| install_codex(&source(), &home, false).unwrap());
            }
        });

        assert_eq!(
            std::fs::read(home.join("AGENTS.md")).unwrap(),
            std::fs::read(source().join("AGENTS.md")).unwrap()
        );
        for name in skill_names() {
            assert_eq!(
                files(&home.join("skills").join(&name)),
                files(&source().join(SKILLS).join(&name))
            );
        }
        // Ordinary launches add no MCP server; owner settings stay.
        let written = std::fs::read_to_string(home.join("config.toml"))
            .unwrap()
            .parse::<DocumentMut>()
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(home.join("config.toml")).unwrap(),
            config
        );
        assert_eq!(written["model"].as_str(), Some("owner-model"));
        assert_eq!(
            written["mcp_servers"]["custom"]["command"].as_str(),
            Some("custom")
        );
        for (file, content) in [
            ("skills/owner-skill/SKILL.md", "keep skill"),
            ("auth.json", "keep credentials"),
            ("sessions/turn.jsonl", "keep session"),
        ] {
            assert_eq!(std::fs::read_to_string(home.join(file)).unwrap(), content);
        }
    }

    #[test]
    fn a_codex_home_retires_skills_a_later_harness_drops() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join(".codex");
        install_codex(&source(), &home, false).unwrap();
        // An earlier harness provided `retired`; a malformed record entry
        // never reaches outside the skills directory.
        std::fs::create_dir_all(home.join("skills/retired")).unwrap();
        std::fs::write(home.join("skills/retired/SKILL.md"), "old").unwrap();
        std::fs::create_dir_all(home.join("skills/owner-skill")).unwrap();
        std::fs::write(home.join("skills/owner-skill/SKILL.md"), "keep").unwrap();
        std::fs::create_dir_all(home.join("sessions")).unwrap();
        let mut record = std::fs::read_to_string(home.join(PROVIDED_SKILLS)).unwrap();
        record.push_str("retired\n../sessions\n");
        std::fs::write(home.join(PROVIDED_SKILLS), record).unwrap();

        install_codex(&source(), &home, false).unwrap();

        assert!(!home.join("skills/retired").exists());
        assert!(home.join("skills/owner-skill/SKILL.md").is_file());
        assert!(home.join("sessions").is_dir());
        let listed: Vec<String> = std::fs::read_to_string(home.join(PROVIDED_SKILLS))
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        let names: Vec<String> = skill_names()
            .iter()
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        assert_eq!(listed, names);
    }

    #[test]
    fn retained_review_registration_and_credentials_are_removed() {
        for config in [
            "model = 'owner-model'\n[mcp_servers.g5-tools]\ncommand = 'g5d'\nargs = ['mcp']\n[mcp_servers.custom]\ncommand = 'custom'\n",
            "model = 'owner-model'\nmcp_servers = { g5-tools = { command = 'g5d' }, custom = { command = 'custom' } }\n",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let home = temp.path().join(".codex");
            std::fs::create_dir_all(&home).unwrap();
            std::fs::write(home.join("config.toml"), config).unwrap();
            let state = temp.path().join(".g5");
            std::fs::create_dir_all(&state).unwrap();
            let retired = state.join("codex-g5-tools-env-old.json");
            std::fs::write(&retired, "retired credentials").unwrap();
            std::fs::write(state.join("workspace-token"), "keep token").unwrap();
            install_codex(&source(), &home, false).unwrap();
            install_codex(&source(), &home, false).unwrap();
            let written = std::fs::read_to_string(home.join("config.toml")).unwrap()
                .parse::<DocumentMut>().unwrap();
            assert_eq!(written["model"].as_str(), Some("owner-model"));
            assert!(written["mcp_servers"].get("g5-tools").is_none());
            assert_eq!(written["mcp_servers"]["custom"]["command"].as_str(), Some("custom"));
            assert!(!retired.exists());
            assert_eq!(std::fs::read_to_string(state.join("workspace-token")).unwrap(), "keep token");
        }
        let fresh = tempfile::tempdir().unwrap();
        install_codex(&source(), &fresh.path().join(".codex"), false).unwrap();
        assert!(!fresh.path().join(".codex/config.toml").exists());
        // A fresh home gains only the managed guidance and its record.
        let created: BTreeSet<OsString> = std::fs::read_dir(fresh.path().join(".g5"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(created, BTreeSet::from(["provided-guidance".into()]));
        assert!(fresh.path().join(GUIDANCE).join("ontology.md").is_file());
    }

    #[test]
    fn a_retained_codex_home_loses_the_legacy_profile() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join(".codex");
        for skill in ["g5-rebase-workflow", "skill-builder", "owner-skill"] {
            std::fs::create_dir_all(home.join("skills").join(skill)).unwrap();
            std::fs::write(home.join("skills").join(skill).join("SKILL.md"), skill).unwrap();
        }
        std::fs::write(home.join("profile_manifest.json"), "{}").unwrap();
        std::fs::write(home.join("G5_SUBAGENT_BOOTSTRAP.md"), "old").unwrap();
        std::fs::write(
            home.join("config.toml"),
            "model = 'owner-model'\napproval_policy = 'never'\n[mcp_servers.custom]\ncommand = 'custom'\n[mcp_servers.g5-tools]\ncommand = '/opt/g5/bin/g5d'\nargs = ['mcp', 'intent-grounding']\nrequired = true\n[mcp_servers.g5-tools.tools.read_ontology]\napproval_mode = 'approve'\n",
        )
        .unwrap();

        install_codex(&source(), &home, false).unwrap();

        let config = std::fs::read_to_string(home.join("config.toml"))
            .unwrap()
            .parse::<DocumentMut>()
            .unwrap();
        // The retired required server is removed without replacing it.
        assert!(config["mcp_servers"].get("g5-tools").is_none());
        assert_eq!(
            config["mcp_servers"]["custom"]["command"].as_str(),
            Some("custom")
        );
        assert_eq!(config["model"].as_str(), Some("owner-model"));
        assert_eq!(config["approval_policy"].as_str(), Some("never"));
        for removed in [
            "skills/g5-rebase-workflow",
            "skills/skill-builder",
            "profile_manifest.json",
            "G5_SUBAGENT_BOOTSTRAP.md",
        ] {
            assert!(!home.join(removed).exists(), "{removed} remains");
        }
        assert!(home.join("skills/owner-skill/SKILL.md").is_file());
        assert!(home.join("skills/ontology-planning/SKILL.md").is_file());

        // An inline registration is removed the same way.
        std::fs::write(
            home.join("config.toml"),
            "mcp_servers = { g5-tools = { command = 'g5d', required = true } }\n",
        )
        .unwrap();
        install_codex(&source(), &home, false).unwrap();
        let config = std::fs::read_to_string(home.join("config.toml"))
            .unwrap()
            .parse::<DocumentMut>()
            .unwrap();
        assert!(config["mcp_servers"].get("g5-tools").is_none());
    }

    #[test]
    fn guidance_upgrade_preserves_owner_files_and_retires_only_provided_skills() {
        let old = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        std::fs::write(old.path().join(INSTRUCTIONS), "Legacy instructions").unwrap();
        for name in [
            "ontology-planning",
            "ontology-authoring",
            "ontology-uplift",
            "ontology-downlift",
        ] {
            let dir = old.path().join(SKILLS).join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("SKILL.md"), "Legacy skill").unwrap();
        }
        // Legacy bundles without guidance still launch both providers.
        install_claude(old.path(), home.path(), false).unwrap();
        install_codex(old.path(), &home.path().join(".codex"), false).unwrap();
        assert!(!home.path().join(GUIDANCE).exists());
        let guidance = home.path().join(GUIDANCE);
        std::fs::create_dir_all(&guidance).unwrap();
        std::fs::write(guidance.join("owner-notes.md"), "keep notes").unwrap();
        std::fs::write(guidance.join("obsolete.md"), "retire me").unwrap();
        std::fs::write(home.path().join("private.txt"), "keep private").unwrap();
        std::fs::create_dir_all(home.path().join(".g5")).unwrap();
        std::fs::write(
            home.path().join(PROVIDED_GUIDANCE),
            "obsolete.md\n../../private.txt\n",
        )
        .unwrap();
        for provider in [".claude", ".codex"] {
            let personal = home.path().join(provider).join("skills/owner-skill");
            std::fs::create_dir_all(&personal).unwrap();
            std::fs::write(personal.join("SKILL.md"), "keep skill").unwrap();
        }
        for _ in 0..2 {
            install_claude(&source(), home.path(), false).unwrap();
            install_codex(&source(), &home.path().join(".codex"), false).unwrap();
            // Refresh a modified managed document on the next launch.
            assert_eq!(
                std::fs::read(guidance.join("ontology.md")).unwrap(),
                std::fs::read(source().join(GUIDANCE).join("ontology.md")).unwrap()
            );
            std::fs::write(guidance.join("ontology.md"), "stale").unwrap();
        }
        for provider in [".claude", ".codex"] {
            let skills = home.path().join(provider).join("skills");
            for retired in ["ontology-authoring", "ontology-uplift", "ontology-downlift"] {
                assert!(!skills.join(retired).exists());
            }
            assert_eq!(
                std::fs::read_to_string(skills.join("owner-skill/SKILL.md")).unwrap(),
                "keep skill"
            );
            assert!(!skills.join("g5-agent-approval-review").exists());
            // Relative links from either native skill location reach ordinary guidance.
            assert!(skills
                .join("ontology-planning/../../../.agents/guidance/ontology-format.md")
                .is_file());
        }
        assert_eq!(
            std::fs::read_to_string(guidance.join("owner-notes.md")).unwrap(),
            "keep notes"
        );
        assert_eq!(
            std::fs::read_to_string(home.path().join("private.txt")).unwrap(),
            "keep private"
        );
        assert!(!guidance.join("obsolete.md").exists());
        assert!(guidance.join("scripts/check_ontology.py").is_file());
        for document in WORKFLOW_DOCUMENTS {
            assert_eq!(
                std::fs::read(guidance.join(document)).unwrap(),
                std::fs::read(source().join(GUIDANCE).join(document)).unwrap()
            );
        }
    }

    #[test]
    fn missing_or_partial_advertised_guidance_is_rejected_before_install() {
        let bundle = tempfile::tempdir().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("owner");
        let skill = bundle.path().join(SKILLS).join("ontology-planning");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("SKILL.md"), "Skill").unwrap();
        std::fs::write(
            bundle.path().join(INSTRUCTIONS),
            "Read $HOME/.agents/guidance/TLC/uplift.md",
        )
        .unwrap();
        for partial in [false, true] {
            if partial {
                std::fs::create_dir_all(bundle.path().join(GUIDANCE)).unwrap();
                std::fs::write(
                    bundle.path().join(GUIDANCE).join("ontology.md"),
                    "Only one document",
                )
                .unwrap();
            }
            assert!(matches!(
                install_claude(bundle.path(), &home, false),
                Err(HarnessError::Unavailable(_))
            ));
            assert!(matches!(
                install_codex(bundle.path(), &home.join(".codex"), false),
                Err(HarnessError::Unavailable(_))
            ));
            assert!(!home.exists());
        }
    }

    #[test]
    fn flat_bundles_still_load_and_split_bundles_require_both_workflows() {
        let bundle = tempfile::tempdir().unwrap();
        let root = bundle.path();
        let skill = root.join(SKILLS).join("ontology-planning");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("SKILL.md"), "Skill").unwrap();
        let guidance = root.join(GUIDANCE);
        std::fs::create_dir_all(&guidance).unwrap();
        for name in GUIDANCE_DOCUMENTS {
            std::fs::write(guidance.join(name), "Legacy guidance").unwrap();
        }
        std::fs::write(root.join(INSTRUCTIONS), "Read $HOME/.agents/guidance/uplift.md").unwrap();
        assert!(load(root).is_ok());

        std::fs::copy(source().join(INSTRUCTIONS), root.join(INSTRUCTIONS)).unwrap();
        assert!(load(root).is_err());
        for directory in ["TLC", "Verus"] {
            std::fs::create_dir(guidance.join(directory)).unwrap();
        }
        for name in WORKFLOW_DOCUMENTS {
            std::fs::copy(source().join(GUIDANCE).join(name), guidance.join(name)).unwrap();
        }
        assert!(load(root).is_ok());
        std::fs::remove_file(guidance.join("Verus/incremental-reverification.md")).unwrap();
        assert!(load(root).is_err());
    }

    #[test]
    fn an_incomplete_harness_installs_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let empty = temp.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let partial = temp.path().join("partial");
        std::fs::create_dir_all(partial.join(SKILLS).join("broken")).unwrap();
        std::fs::write(partial.join(INSTRUCTIONS), "instructions").unwrap();
        for source in [&empty, &partial] {
            assert!(matches!(
                install_claude(source, &home, false),
                Err(HarnessError::Unavailable(_))
            ));
            assert!(matches!(
                install_codex(source, &home.join(".codex"), false),
                Err(HarnessError::Unavailable(_))
            ));
        }
        assert!(!home.exists(), "an incomplete harness was partly installed");

        // A complete harness that cannot be written is an install failure.
        let file = temp.path().join("not-a-directory");
        std::fs::write(&file, "").unwrap();
        assert!(matches!(
            install_claude(&source(), &file, false),
            Err(HarnessError::Install(_))
        ));

        // A malformed owner configuration is left alone for Codex to report.
        let codex = home.join(".codex");
        std::fs::create_dir_all(&codex).unwrap();
        std::fs::write(codex.join("config.toml"), "[invalid").unwrap();
        install_codex(&source(), &codex, false).unwrap();
        assert_eq!(
            std::fs::read_to_string(codex.join("config.toml")).unwrap(),
            "[invalid"
        );
    }
}
