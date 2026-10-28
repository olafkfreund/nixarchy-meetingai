//! Private SQLCipher storage and one-time plaintext conversion.
use super::{DbError, Result};
use crate::core::security::files::{lock, open_private};
use rusqlite::{Connection, params};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

fn side(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn sync_parent(path: &Path) -> Result<()> {
    #[cfg(unix)]
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::File::open(parent)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn key_text(key: &[u8; 32]) -> String {
    let mut text = String::from("x'");
    for byte in key {
        use std::fmt::Write;
        write!(text, "{byte:02x}").expect("String write");
    }
    text.push('\'');
    text
}

fn unlock(path: &Path, key: &[u8; 32]) -> Result<Connection> {
    let connection = Connection::open(path)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    connection.pragma_update(None, "key", key_text(key))?;
    connection.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    })?;
    Ok(connection)
}

fn migrate(path: &Path, key: &[u8; 32]) -> Result<()> {
    let original = Connection::open(path)?;
    original.busy_timeout(Duration::from_secs(1))?;
    original.pragma_update(None, "temp_store", "MEMORY")?;
    let mode: String = original.query_row("PRAGMA journal_mode=DELETE", [], |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("delete") {
        return Err(DbError::Storage(
            "close other database readers before encryption",
        ));
    }
    let temp = side(path, &format!(".tmp{}", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        open_private(&temp, true)?;
        let name = temp
            .to_str()
            .ok_or(DbError::Storage("temporary database path is not UTF-8"))?;
        original.execute(
            "ATTACH DATABASE ?1 AS encrypted KEY ?2",
            params![name, key_text(key)],
        )?;
        let exported = (|| -> Result<()> {
            original.query_row("SELECT sqlcipher_export('encrypted')", [], |_| Ok(()))?;
            let version: i64 = original.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            original.pragma_update(Some("encrypted"), "user_version", version)?;
            Ok(())
        })();
        let detached = original.execute_batch("DETACH DATABASE encrypted");
        exported?;
        detached?;
        let checked = unlock(&temp, key)?;
        let integrity: String = checked.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err(DbError::Storage(
                "encrypted copy failed verification; original kept",
            ));
        }
        drop(checked);
        fs::File::open(&temp)?.sync_all()?;
        drop(original);
        fs::rename(&temp, path)?;
        sync_parent(path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub(super) fn open(path: &Path) -> Result<Connection> {
    let _guard = lock(&side(path, ".open.lock"))?;
    let existed = path.exists();
    let mut file = open_private(path, false)?;
    let size = file.metadata()?.len();
    let mut header = [0; 16];
    let plaintext =
        size >= 16 && file.read_exact(&mut header).is_ok() && &header == b"SQLite format 3\0";
    drop(file);
    let key_path = side(path, ".key");
    let key = if key_path.exists() {
        let mut key_file = open_private(&key_path, false)?;
        if key_file.metadata()?.len() != 32 {
            return Err(DbError::Storage(
                "database key has the wrong length; data kept",
            ));
        }
        let mut key = [0; 32];
        key_file.read_exact(&mut key)?;
        key
    } else if existed && size > 0 && !plaintext {
        return Err(DbError::Storage(
            "database key is missing; encrypted data kept",
        ));
    } else {
        let key: [u8; 32] = rand::random();
        let temp = side(&key_path, &format!(".tmp{}", uuid::Uuid::new_v4()));
        let written = (|| -> Result<()> {
            let mut key_file = open_private(&temp, true)?;
            key_file.write_all(&key)?;
            key_file.sync_all()?;
            fs::hard_link(&temp, &key_path)?;
            fs::remove_file(&temp)?;
            Ok(())
        })();
        if written.is_err() {
            let _ = fs::remove_file(&temp);
        }
        written?;
        sync_parent(&key_path)?;
        key
    };
    if plaintext {
        migrate(path, &key)?;
    }
    unlock(path, &key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, DbError, conversations, meetings, profiles, schema};

    fn path() -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "nixarchy-meetingai-encryption-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&directory).unwrap();
        directory.join("history.db")
    }

    #[test]
    fn new_database_encrypts_content_and_reopens() {
        let path = path();
        let secret = "private-orchid-73920";
        let db = Database::open(&path).unwrap();
        let conversation = conversations::create(db.connection(), "chat", 1).unwrap();
        conversations::append_message(db.connection(), &conversation, "user", secret, 2).unwrap();
        let meeting = meetings::start(
            db.connection(),
            secret,
            "auto",
            "en",
            None,
            Default::default(),
        )
        .unwrap();
        meetings::add_line(db.connection(), &meeting, "me", Some(2), secret, 0.0, 1.0).unwrap();
        for file in [&path, &side(&path, "-wal")] {
            let bytes = fs::read(file).unwrap();
            assert!(!bytes.starts_with(b"SQLite format 3\0"));
            assert!(
                !bytes
                    .windows(secret.len())
                    .any(|part| part == secret.as_bytes())
            );
        }
        drop(db);
        let plain = Connection::open(&path).unwrap();
        assert!(
            plain
                .query_row("SELECT count(*) FROM sqlite_master", [], |r| r
                    .get::<_, i64>(0))
                .is_err()
        );
        let db = Database::open(&path).unwrap();
        assert_eq!(
            conversations::messages(db.connection(), &conversation).unwrap()[0].content,
            secret
        );
        let line = &meetings::lines(db.connection(), &meeting).unwrap()[0];
        assert_eq!(line.who, "me");
        assert_eq!(line.speaker, Some(2));
    }

    #[test]
    fn plaintext_migration_preserves_rows_and_version() {
        let path = path();
        let original = Connection::open(&path).unwrap();
        original.execute_batch(schema::SCHEMA).unwrap();
        original.execute_batch("PRAGMA user_version=7").unwrap();
        original
            .execute("INSERT INTO conversations VALUES ('c','title',1,1)", [])
            .unwrap();
        original.execute("INSERT INTO messages (id,conversation_id,role,content,timestamp) VALUES ('msg','c','user','café',2)", []).unwrap();
        // A legacy database has no series_id column, so insert without it.
        let meeting = "legacy-meeting".to_owned();
        original
            .execute(
                "INSERT INTO meetings (id, title, context, language) VALUES (?1, 'Legacy meeting', 'context', 'en')",
                [&meeting],
            )
            .unwrap();
        original.execute("INSERT INTO meeting_lines (id,meeting_id,who,text) VALUES ('line',?1,'them','hello')", [&meeting]).unwrap();
        original
            .execute("INSERT INTO files (id,name) VALUES ('file','CV')", [])
            .unwrap();
        original
            .execute(
                "INSERT INTO app_settings (key,value) VALUES ('personal_context','me')",
                [],
            )
            .unwrap();
        drop(original);
        let db = Database::open(&path).unwrap();
        assert_eq!(
            conversations::messages(db.connection(), "c").unwrap()[0].content,
            "café"
        );
        assert_eq!(
            db.connection()
                .query_row("SELECT count(*) FROM meeting_lines", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.connection()
                .query_row("SELECT name FROM files", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "CV"
        );
        assert!(!profiles::list(db.connection()).unwrap().is_empty());
        assert_eq!(
            db.connection()
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            7
        );
        drop(db);
        Database::open(&path).unwrap();
        assert!(!fs::read(&path).unwrap().starts_with(b"SQLite format 3\0"));
        assert!(!fs::read_dir(path.parent().unwrap()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp")
        }));
    }

    #[test]
    fn missing_and_wrong_keys_keep_database_bytes() {
        let path = path();
        drop(Database::open(&path).unwrap());
        let bytes = fs::read(&path).unwrap();
        let key_path = side(&path, ".key");
        let key = fs::read(&key_path).unwrap();
        fs::remove_file(&key_path).unwrap();
        assert!(matches!(Database::open(&path), Err(DbError::Storage(_))));
        assert!(!key_path.exists());
        fs::write(&key_path, [17; 32]).unwrap();
        assert!(Database::open(&path).is_err());
        fs::write(&key_path, [17; 33]).unwrap();
        assert!(matches!(Database::open(&path), Err(DbError::Storage(_))));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::write(&key_path, key).unwrap();
        Database::open(&path).unwrap();
    }

    #[test]
    fn concurrent_first_opens_share_one_key() {
        let path = path();
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let db = Database::open(path).unwrap();
                    conversations::create(db.connection(), &format!("chat {i}"), i).unwrap();
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let db = Database::open(path).unwrap();
        assert_eq!(conversations::list(db.connection(), 20).unwrap().len(), 8);
    }

    #[test]
    fn plaintext_reader_blocks_migration_until_closed() {
        let path = path();
        let original = Connection::open(&path).unwrap();
        original.execute_batch("CREATE TABLE legacy (value TEXT); INSERT INTO legacy VALUES ('kept'); PRAGMA journal_mode=WAL").unwrap();
        let reader = Connection::open(&path).unwrap();
        reader.execute_batch("BEGIN; SELECT * FROM legacy").unwrap();
        assert!(Database::open(&path).is_err());
        assert_eq!(
            original
                .query_row("SELECT value FROM legacy", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "kept"
        );
        reader.execute_batch("ROLLBACK").unwrap();
        drop(reader);
        drop(original);
        let db = Database::open(&path).unwrap();
        assert_eq!(
            db.connection()
                .query_row("SELECT value FROM legacy", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "kept"
        );
    }

    #[cfg(unix)]
    #[test]
    fn private_modes_and_symlinks() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let path = path();
        drop(Database::open(&path).unwrap());
        for file in [&path, &side(&path, ".key"), &side(&path, ".open.lock")] {
            assert_eq!(
                fs::metadata(file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let real = side(&path, ".real");
        fs::rename(&path, &real).unwrap();
        symlink(&real, &path).unwrap();
        assert!(Database::open(&path).is_err());
    }
}
