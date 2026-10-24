//! Private local files and cross-process locks. Never follow a substituted symlink.
use std::{
    fs::{File, OpenOptions},
    path::Path,
};

pub(crate) fn open_private(path: &Path, create_new: bool) -> std::io::Result<File> {
    if let Ok(metadata) = std::fs::symlink_metadata(path)
        && (!metadata.is_file() || metadata.is_symlink())
    {
        return Err(std::io::Error::other(
            "private storage must be a regular file",
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if create_new {
        options.create_new(true);
    } else {
        options.create(true).truncate(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
        #[cfg(target_os = "linux")]
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

pub(crate) fn lock(path: &Path) -> std::io::Result<File> {
    let file = open_private(path, false)?;
    file.lock()?;
    Ok(file)
}
