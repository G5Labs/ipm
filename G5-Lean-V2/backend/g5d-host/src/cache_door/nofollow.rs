//! Descriptor-based, symlink-refusing path resolution for guest-authored
//! trees. A template rootfs is image content: a symlink planted anywhere on
//! the way to a seed object, before or during the walk, must never lead the
//! door to a host file. Every component is opened relative to the previous
//! descriptor with `O_NOFOLLOW`, so a check-then-open race has nothing to
//! race against.

use std::{
    ffi::CString,
    fs::{File, Metadata},
    io,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::ffi::OsStrExt,
    },
    path::Path,
};

fn open_relative(dir: Option<&File>, name: &[u8], directory: bool) -> io::Result<File> {
    let name = CString::new(name)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "nul byte in path"))?;
    let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    if directory {
        flags |= libc::O_DIRECTORY;
    }
    // SAFETY: `name` is a valid C string for the call's duration and the
    // directory descriptor, when given, is open; the returned descriptor is
    // owned by the `File` built from it exactly once.
    let fd = unsafe {
        match dir {
            Some(dir) => libc::openat(dir.as_raw_fd(), name.as_ptr(), flags),
            None => libc::open(name.as_ptr(), flags),
        }
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

/// Open `rel` beneath `root`, a regular file (or a directory when
/// `directory` is set), refusing a symlink at any component. `rel` is a
/// `/`-separated relative path without empty, `.` or `..` components.
pub(crate) fn open_nofollow(
    root: &Path,
    rel: &str,
    directory: bool,
) -> io::Result<(File, Metadata)> {
    let mut current = open_relative(None, root.as_os_str().as_bytes(), true)?;
    let parts: Vec<&str> = rel.split('/').collect();
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() || *part == "." || *part == ".." {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path component is empty or relative",
            ));
        }
        let last = index + 1 == parts.len();
        current = open_relative(Some(&current), part.as_bytes(), !last || directory)?;
    }
    let metadata = current.metadata()?;
    if (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path does not name the expected kind of entry",
        ));
    }
    Ok((current, metadata))
}

#[cfg(test)]
mod tests {
    use super::open_nofollow;

    #[test]
    fn symlinks_are_refused_at_every_component() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("rootfs");
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/b/key"), b"object").unwrap();
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret"), b"host").unwrap();
        std::os::unix::fs::symlink(outside.join("secret"), root.join("a/b/link")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("a/dir")).unwrap();

        let (_, meta) = open_nofollow(&root, "a/b/key", false).unwrap();
        assert_eq!(meta.len(), 6);
        assert!(open_nofollow(&root, "a/b", true).is_ok());
        assert!(open_nofollow(&root, "a/b/link", false).is_err());
        assert!(open_nofollow(&root, "a/dir/secret", false).is_err());
        assert!(open_nofollow(&root, "a/dir", true).is_err());
        assert!(open_nofollow(&root, "a/../a/b/key", false).is_err());
        assert!(
            open_nofollow(&root, "a/b", false).is_err(),
            "a directory is not a file"
        );
        assert!(open_nofollow(&dir.path().join("missing"), "a", false).is_err());
    }
}
