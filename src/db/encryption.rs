//! SQLCipher page encryption, including WAL pages. Keys never enter app settings.
//!
//! A private adjacent random key protects copies of the database, not a compromised
//! user account. Existing plaintext is exported and verified before atomic replacement.
use super::{DbError, Result};
use crate::core::security::files;
use rand::Rng;
use rusqlite::Connection;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn key(path: &Path, encrypted_exists: bool) -> Result<String> {
    let path = sidecar(path, ".key");
    let mut bytes = [0u8; 32];
    if path.try_exists()? {
        let mut file = files::open_private(&path, false)?;
        if file.metadata()?.len() != 32 {
            return Err(DbError::Storage(
                "history key has the wrong length; data was preserved",
            ));
        }
        file.read_exact(&mut bytes)?;
    } else {
        if encrypted_exists {
            return Err(DbError::Storage(
                "history key is missing; encrypted data was preserved",
            ));
        }
        rand::rng().fill_bytes(&mut bytes);
        let mut file = files::open_private(&path, true)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        #[cfg(unix)]
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::File::open(parent)?.sync_all()?;
        }
    }
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!("x'{hex}'"))
}

fn unlock(path: &Path, key: &str) -> Result<Connection> {
    let connection = Connection::open(path)?;
    connection.pragma_update(None, "key", key)?;
    // Force authentication now; wrong keys/corruption must never trigger reinitialization.
    connection.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))?;
    Ok(connection)
}

pub(super) fn open(path: &Path) -> Result<Connection> {
    let _lock = files::lock(&sidecar(path, ".open.lock"))?;
    let exists = path.try_exists()?;
    let mut header = [0u8; 16];
    let file = files::open_private(path, false)?;
    let length = file.metadata()?.len();
    if length >= 16 {
        (&file).read_exact(&mut header)?;
    }
    let plaintext = length >= 16 && &header == b"SQLite format 3\0";
    let key = key(path, exists && length > 0 && !plaintext)?;
    drop(file);
    if plaintext {
        migrate(path, &key)?;
    }
    unlock(path, &key)
}

fn migrate(path: &Path, key: &str) -> Result<()> {
    let original = Connection::open(path)?;
    original.execute_batch("PRAGMA temp_store=MEMORY")?;
    original.busy_timeout(std::time::Duration::from_secs(1))?;
    // Switching out of WAL checkpoints all committed pages and fails if another
    // process still holds a reader/writer. Do not replace a live WAL database.
    let mode: String = original.query_row("PRAGMA journal_mode=DELETE", [], |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("delete") {
        return Err(DbError::Storage(
            "close other history readers before encrypting history",
        ));
    }
    let temporary = sidecar(path, &format!(".encrypt-{}.tmp", uuid::Uuid::new_v4()));
    files::open_private(&temporary, true)?;
    let result = (|| -> Result<()> {
        original.execute(
            "ATTACH DATABASE ?1 AS encrypted KEY ?2",
            rusqlite::params![
                temporary
                    .to_str()
                    .ok_or(DbError::Storage("history path is not UTF-8"))?,
                key
            ],
        )?;
        original.query_row("SELECT sqlcipher_export('encrypted')", [], |_| Ok(()))?;
        let version: i64 = original.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        original.pragma_update(Some("encrypted"), "user_version", version)?;
        original.execute_batch("DETACH DATABASE encrypted")?;
        let verified = unlock(&temporary, key)?;
        let integrity: String = verified.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err(DbError::Storage(
                "encrypted history verification failed; original data was preserved",
            ));
        }
        drop(verified);
        files::open_private(&temporary, false)?.sync_all()?;
        drop(original);
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, conversations, library, meetings, profiles};

    fn path() -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "nixarchy-meetingai-encryption-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&directory).unwrap();
        directory.join("history.db")
    }

    #[test]
    fn history_and_wal_are_encrypted_and_reopen_with_the_same_key() {
        let path = path();
        let db = Database::open(&path).unwrap();
        let secret = "private-history-unique-sentinel";
        let chat = conversations::create(db.connection(), secret, 1).unwrap();
        conversations::append_message(db.connection(), &chat, "user", secret, 2).unwrap();
        let meeting = meetings::start(db.connection(), secret, "auto", "en").unwrap();
        meetings::add_line(db.connection(), &meeting, "them", Some(2), secret, 0.0, 1.0).unwrap();
        for file in [&path, &sidecar(&path, "-wal")] {
            let bytes = fs::read(file).unwrap();
            assert!(!bytes.starts_with(b"SQLite format 3"));
            assert!(!bytes.windows(secret.len()).any(|p| p == secret.as_bytes()));
        }
        let raw = Connection::open(&path).unwrap();
        assert!(
            raw.query_row("SELECT count(*) FROM sqlite_master", [], |r| r
                .get::<_, i64>(0))
                .is_err()
        );
        drop(raw);
        drop(db);
        let reopened = Database::open(&path).unwrap();
        assert_eq!(
            conversations::messages(reopened.connection(), &chat).unwrap()[0].content,
            secret
        );
        assert_eq!(
            meetings::lines(reopened.connection(), &meeting).unwrap()[0].speaker,
            Some(2)
        );
    }

    #[test]
    fn plaintext_migration_preserves_content_relationships_and_is_idempotent() {
        let path = path();
        let original = Connection::open(&path).unwrap();
        original.execute_batch(crate::db::schema::SCHEMA).unwrap();
        original.execute_batch("PRAGMA user_version=7").unwrap();
        let chat = conversations::create(&original, "Legacy", 1).unwrap();
        conversations::append_message(&original, &chat, "user", "Original question é", 2).unwrap();
        let meeting = meetings::start(&original, "Legacy meeting", "context", "en").unwrap();
        meetings::add_line(
            &original,
            &meeting,
            "me",
            Some(4),
            "The deadline is Friday",
            3.0,
            4.0,
        )
        .unwrap();
        library::insert(&original, "Notes", "text/plain", 9, 0, "Private document").unwrap();
        crate::db::settings::set(&original, "personal_context", "Legacy profile context").unwrap();
        drop(original);
        let db = Database::open(&path).unwrap();
        assert_eq!(
            conversations::messages(db.connection(), &chat).unwrap()[0].content,
            "Original question é"
        );
        assert_eq!(
            meetings::lines(db.connection(), &meeting).unwrap()[0].text,
            "The deadline is Friday"
        );
        assert_eq!(
            library::list(db.connection()).unwrap()[0].ocr_text,
            "Private document"
        );
        assert!(!profiles::list(db.connection()).unwrap().is_empty());
        let version: i64 = db
            .connection()
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 7);
        drop(db);
        assert!(Database::open(&path).is_ok());
        assert!(!fs::read(&path).unwrap().starts_with(b"SQLite format 3"));
        assert!(
            !fs::read_dir(path.parent().unwrap()).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp"))
        );
    }

    #[test]
    fn missing_wrong_and_malformed_keys_preserve_ciphertext() {
        let path = path();
        drop(Database::open(&path).unwrap());
        let before = fs::read(&path).unwrap();
        let key_path = sidecar(&path, ".key");
        let original_key = fs::read(&key_path).unwrap();
        fs::remove_file(&key_path).unwrap();
        assert!(matches!(Database::open(&path), Err(DbError::Storage(_))));
        assert!(!key_path.exists());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::write(&key_path, [17u8; 32]).unwrap();
        assert!(Database::open(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::write(&key_path, [17u8; 33]).unwrap();
        assert!(Database::open(&path).is_err());
        fs::write(&key_path, original_key).unwrap();
        assert!(Database::open(&path).is_ok());
    }

    #[test]
    fn concurrent_first_open_uses_one_key_without_losing_writes() {
        let path = path();
        let workers: Vec<_> = (0..8)
            .map(|i| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let db = Database::open(path).unwrap();
                    conversations::create(db.connection(), &format!("Chat {i}"), i).unwrap();
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let db = Database::open(&path).unwrap();
        assert_eq!(conversations::list(db.connection(), 20).unwrap().len(), 8);
    }

    #[test]
    fn migration_refuses_a_busy_legacy_wal_and_preserves_original() {
        let path = path();
        let original = Connection::open(&path).unwrap();
        original.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE legacy(value TEXT); INSERT INTO legacy VALUES('keep me'); BEGIN; SELECT * FROM legacy;").unwrap();
        assert!(Database::open(&path).is_err());
        original.execute_batch("COMMIT").unwrap();
        let value: String = original
            .query_row("SELECT value FROM legacy", [], |r| r.get(0))
            .unwrap();
        assert_eq!(value, "keep me");
        drop(original);
        let db = Database::open(&path).unwrap();
        assert_eq!(
            db.connection()
                .query_row("SELECT value FROM legacy", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "keep me"
        );
    }

    #[cfg(unix)]
    #[test]
    fn private_history_files_reject_symlinks_and_tighten_permissions() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let path = path();
        drop(Database::open(&path).unwrap());
        for file in [
            &path,
            &sidecar(&path, ".key"),
            &sidecar(&path, ".open.lock"),
        ] {
            assert_eq!(
                fs::metadata(file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let target = path.with_file_name("elsewhere");
        fs::rename(&path, &target).unwrap();
        symlink(&target, &path).unwrap();
        assert!(Database::open(&path).is_err());
    }
}
