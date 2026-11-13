//! File ownership for the host-provisioned module identity.
//!
//! Repository-facing code in a workspace VM runs under an unprivileged uid/gid
//! the host provisions (`G5D_MODULE_UID`/`G5D_MODULE_GID`, dispatch runtime
//! reference). Everything that identity must write is created by root: the
//! host materializes `/work` from its bare cache, and the dispatcher creates
//! the provider home at boot. OCI extraction deliberately drops image
//! ownership, so nothing else hands those trees to the module uid. These
//! helpers do, without following symlinks, so a link planted by a previous
//! guest can never redirect the ownership change outside the tree.

use std::io;
use std::os::unix::fs::lchown;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// Change the owner of `path` and, when it is a directory, everything below
/// it. Every entry is re-owned with `lchown(2)`, so a symlink changes its own
/// link ownership and its target is never touched, even if the entry is
/// swapped for a link between the type check and the ownership change. The
/// walk assumes a quiescent tree that no other writer touches meanwhile (the
/// host runs it before boot on a tree it just materialized; the dispatcher
/// runs it at startup before any module exists): a directory replaced by a
/// link during the walk is not descended, but ancestor resolution is not
/// descriptor-relative. `None` leaves that id unchanged, as for `chown(2)`.
pub fn chown_tree(path: &Path, uid: Option<u32>, gid: Option<u32>) -> io::Result<()> {
    if uid.is_none() && gid.is_none() {
        return Ok(());
    }
    let metadata = std::fs::symlink_metadata(path)?;
    lchown(path, uid, gid)?;
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path)? {
            chown_tree(&entry?.path(), uid, gid)?;
        }
    }
    Ok(())
}

/// Repair the private workspace before the guest admits any repository writers.
/// Always descend already-owned directories: an interrupted repair can leave
/// children behind. Do not cache success across dispatcher restarts.
/// Ownership failures on the private filesystem fail the handoff, including
/// immutable Git objects and symlinks; targets are never followed.
pub fn chown_guest_tree(path: &Path, uid: u32, gid: Option<u32>) -> io::Result<()> {
    chown_guest_tree_with(path, uid, gid, &|path, uid, gid| {
        lchown(path, Some(uid), gid)
    })
}

fn chown_guest_tree_with(
    path: &Path,
    uid: u32,
    gid: Option<u32>,
    chown: &impl Fn(&Path, u32, Option<u32>) -> io::Result<()>,
) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.uid() != uid || gid.is_some_and(|gid| metadata.gid() != gid) {
        chown(path, uid, gid).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("workspace ownership repair at {}: {error}", path.display()),
            )
        })?;
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path)? {
            chown_guest_tree_with(&entry?.path(), uid, gid, chown)?;
        }
    }
    Ok(())
}

/// Whether this process can hand a tree to another identity: only root may
/// `chown` to a uid it does not own. Local process mode without root keeps the
/// caller's identity rather than attempting a handoff.
pub fn can_hand_over() -> bool {
    // SAFETY: geteuid has no failure modes and touches no memory.
    unsafe { libc::geteuid() == 0 }
}

/// The module identity carried by a VM spec environment, or `None` when the
/// host provisioned none. Uid `0` is refused by the dispatcher at startup
/// (root is never handed to repository-facing children), so it is not an
/// identity to hand a tree to either.
pub fn module_identity_from_env<'a, I>(env: I) -> Option<(u32, Option<u32>)>
where
    I: IntoIterator<Item = (&'a String, &'a String)>,
{
    let mut uid = None;
    let mut gid = None;
    for (key, value) in env {
        match key.as_str() {
            "G5D_MODULE_UID" => uid = value.trim().parse::<u32>().ok(),
            "G5D_MODULE_GID" => gid = value.trim().parse::<u32>().ok(),
            _ => {}
        }
    }
    match uid {
        Some(0) | None => None,
        Some(uid) => Some((uid, gid.filter(|gid| *gid != 0))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::os::unix::fs::MetadataExt;

    fn current_ids() -> (u32, u32) {
        // SAFETY: geteuid/getegid have no failure modes and touch no memory.
        unsafe { (libc::geteuid(), libc::getegid()) }
    }

    #[test]
    fn chown_tree_visits_every_entry_and_never_follows_symlinks() {
        let temp = tempfile::tempdir().unwrap();
        let outside = temp.path().join("outside.txt");
        std::fs::write(&outside, b"outside").unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir_all(root.join("nested/deeper")).unwrap();
        std::fs::write(root.join("nested/deeper/file.txt"), b"x").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        let (uid, gid) = current_ids();

        chown_tree(&root, Some(uid), Some(gid)).unwrap();

        for path in [
            root.clone(),
            root.join("nested"),
            root.join("nested/deeper"),
            root.join("nested/deeper/file.txt"),
        ] {
            let metadata = std::fs::metadata(&path).unwrap();
            assert_eq!((metadata.uid(), metadata.gid()), (uid, gid), "{path:?}");
        }
        let link = std::fs::symlink_metadata(root.join("link")).unwrap();
        assert!(link.file_type().is_symlink());
        assert_eq!(std::fs::read(&outside).unwrap(), b"outside");
    }

    /// Only root can hand a tree to another identity; the CI Docker lanes run
    /// as root, a laptop run skips this half.
    #[test]
    fn root_hands_a_root_owned_tree_to_uid_1000_without_touching_link_targets() {
        if current_ids().0 != 0 {
            eprintln!("skipped: needs root");
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let outside = temp.path().join("outside.txt");
        std::fs::write(&outside, b"outside").unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("nested/file.txt"), b"x").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();

        chown_tree(&root, Some(1000), Some(1000)).unwrap();

        for path in [
            root.clone(),
            root.join("nested"),
            root.join("nested/file.txt"),
        ] {
            let metadata = std::fs::metadata(&path).unwrap();
            assert_eq!((metadata.uid(), metadata.gid()), (1000, 1000), "{path:?}");
        }
        let link = std::fs::symlink_metadata(root.join("link")).unwrap();
        assert_eq!((link.uid(), link.gid()), (1000, 1000));
        let target = std::fs::metadata(&outside).unwrap();
        assert_eq!(
            (target.uid(), target.gid()),
            (0, 0),
            "link target untouched"
        );
    }

    #[test]
    fn chown_tree_without_ids_is_a_no_op_even_for_a_missing_path() {
        chown_tree(Path::new("/definitely/not/here"), None, None).unwrap();
    }

    #[test]
    fn guest_repair_refuses_ownership_failures_for_files_and_symlinks() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("object");
        std::fs::write(&file, b"immutable git object").unwrap();
        let uid = current_ids().0 + 1;
        let denied =
            |_: &Path, _: u32, _: Option<u32>| Err(io::Error::from_raw_os_error(libc::EPERM));
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o444)).unwrap();
        assert!(chown_guest_tree_with(&file, uid, None, &denied).is_err());
        for mode in [0o644, 0o400] {
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
            assert!(chown_guest_tree_with(&file, uid, None, &denied).is_err());
        }
        assert!(chown_guest_tree_with(temp.path(), uid, None, &denied).is_err());
        let link = temp.path().join("dangling");
        std::os::unix::fs::symlink("missing", &link).unwrap();
        assert!(chown_guest_tree_with(&link, uid, None, &denied).is_err());
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o444)).unwrap();
        assert!(chown_guest_tree_with(&file, uid, None, &|_, _, _| {
            Err(io::Error::from_raw_os_error(libc::EROFS))
        })
        .is_err());
    }

    #[test]
    fn guest_repair_retries_children_of_an_already_owned_root() {
        if !can_hand_over() {
            eprintln!("skipped: needs root");
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("work");
        let git = root.join("docs/.git");
        std::fs::create_dir_all(&git).unwrap();
        let outside = temp.path().join("outside");
        std::fs::write(&outside, b"untouched").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        let repair = |path: &Path, uid, gid| {
            if path == git {
                return Err(io::Error::from_raw_os_error(libc::EACCES));
            }
            lchown(path, Some(uid), gid)
        };
        let failure = chown_guest_tree_with(&root, 1000, Some(1000), &repair).unwrap_err();
        assert!(failure.to_string().contains("docs/.git"));
        assert_eq!(std::fs::metadata(&root).unwrap().uid(), 1000);
        assert_eq!(std::fs::metadata(&git).unwrap().uid(), 0);
        chown_guest_tree(&root, 1000, Some(1000)).unwrap();
        assert_eq!(std::fs::metadata(&git).unwrap().uid(), 1000);
        assert_eq!(std::fs::metadata(&outside).unwrap().uid(), 0);
        // A successful repeat performs no chown at all, but still walks.
        chown_guest_tree_with(&root, 1000, Some(1000), &|_, _, _| {
            panic!("already-owned entry must not be chowned")
        })
        .unwrap();
    }

    #[test]
    fn module_identity_requires_a_non_root_uid() {
        let env = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<BTreeMap<_, _>>()
        };
        assert_eq!(
            module_identity_from_env(&env(&[
                ("G5D_MODULE_UID", "1000"),
                ("G5D_MODULE_GID", "1000")
            ])),
            Some((1000, Some(1000)))
        );
        assert_eq!(
            module_identity_from_env(&env(&[("G5D_MODULE_UID", " 1000 ")])),
            Some((1000, None))
        );
        assert_eq!(
            module_identity_from_env(&env(&[("G5D_MODULE_UID", "1000"), ("G5D_MODULE_GID", "0")])),
            Some((1000, None))
        );
        assert_eq!(
            module_identity_from_env(&env(&[("G5D_MODULE_UID", "0")])),
            None
        );
        assert_eq!(
            module_identity_from_env(&env(&[("G5D_MODULE_GID", "1000")])),
            None
        );
        assert_eq!(
            module_identity_from_env(&env(&[("G5D_MODULE_UID", "abc")])),
            None
        );
        assert_eq!(module_identity_from_env(&env(&[])), None);
    }
}
