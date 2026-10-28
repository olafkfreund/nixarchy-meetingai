//! Opening secret-bearing files without following links and with owner-only permissions.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

/// Open `path` read+write, creating it when missing, and never truncating it.
///
/// Symlinks and non-regular files are refused. With `create_new` an existing
/// file is an error. On Unix the file ends up mode `0600`.
pub(crate) fn open_private(path: &Path, create_new: bool) -> io::Result<File> {
    let refuse = |what: &str| io::Error::other(format!("{}: {what}", path.display()));

    // Pre-check by name; the fstat after opening catches a swap in between.
    if let Ok(found) = std::fs::symlink_metadata(path) {
        let kind = found.file_type();
        if kind.is_symlink() {
            return Err(refuse("is a symbolic link"));
        }
        if !kind.is_file() {
            return Err(refuse("is not a regular file"));
        }
    }

    let mut opts = OpenOptions::new();
    opts.read(true).write(true).truncate(false);
    if create_new {
        opts.create_new(true);
    } else {
        opts.create(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
        #[cfg(target_os = "linux")]
        opts.custom_flags(libc::O_NOFOLLOW);
    }

    let handle = opts.open(path)?;
    if !handle.metadata()?.is_file() {
        return Err(refuse("is not a regular file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        handle.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(handle)
}

/// Open (creating if needed) and exclusively lock `path`; the lock lasts until the file drops.
pub(crate) fn lock(path: &Path) -> io::Result<File> {
    let file = open_private(path, false)?;
    file.lock()?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// A temp directory removed on drop, even when a test fails.
    struct Scratch(std::path::PathBuf);

    impl std::ops::Deref for Scratch {
        type Target = std::path::Path;
        fn deref(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch() -> Scratch {
        let dir = std::env::temp_dir().join(format!("nixarchy-files-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    #[test]
    fn symlink_is_rejected() {
        let dir = scratch();
        let real = dir.join("real");
        std::fs::write(&real, "x").unwrap();
        let link = dir.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(open_private(&link, false).is_err());
        assert!(open_private(&link, true).is_err());
    }

    #[test]
    fn directory_is_rejected() {
        let dir = scratch();
        assert!(open_private(&dir, false).is_err());
    }

    #[test]
    fn existing_content_survives_and_create_new_refuses() {
        let _dir = scratch();
        let path = _dir.join("f");
        std::fs::write(&path, "keep me").unwrap();
        let mut text = String::new();
        open_private(&path, false)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "keep me");
        let error = open_private(&path, true).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn mode_becomes_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let _dir = scratch();
        let path = _dir.join("f");
        std::fs::write(&path, "").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        open_private(&path, false).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn second_lock_waits_for_the_first() {
        let _dir = scratch();
        let path = _dir.join("lock");
        let first = lock(&path).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let other = path.clone();
        let waiter = std::thread::spawn(move || {
            let _second = lock(&other).unwrap();
            tx.send(()).unwrap();
        });
        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(300))
                .is_err()
        );
        drop(first);
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        waiter.join().unwrap();
        let _ = std::fs::File::create(&path).unwrap().write(b"");
    }
}
