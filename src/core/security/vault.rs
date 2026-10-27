//! Encrypted credential vault.
//!
//! Credentials (API keys, licence data, BYOK settings) live in a single
//! AES-256-GCM encrypted JSON document. The key is a private file beside it with
//! `0600` permissions, and the vault directory is `0700`.
//!
//! This protects against accidental plaintext disclosure and casual inspection.
//! It does not protect against a process that can already read every file as the
//! same user, and that limitation is intentional to state rather than paper over.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::Rng;
use serde_json::Value;

/// File header, so format drift is detected instead of silently misread.
const HEADER: &[u8] = b"NIXARCHY-MEETINGAI-VAULT-v1\0";

/// Errors surfaced to callers.
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("vault I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("vault is not a recognised format; existing data was preserved")]
    Format,
    #[error("vault could not be authenticated; existing data was preserved")]
    Auth,
    #[error("vault key is missing but encrypted data exists; data was preserved")]
    MissingKey,
    #[error("vault contents are not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Values stored in the vault. Ordered so serialisation is stable.
pub type Store = BTreeMap<String, Value>;

/// A vault backed by a directory containing `vault.key` and `vault.bin`.
pub struct Vault {
    directory: PathBuf,
}

impl Vault {
    /// Open (or initialise) a vault in `directory`, creating it if needed.
    pub fn open(directory: impl Into<PathBuf>) -> Result<Self, VaultError> {
        let directory = directory.into();
        fs::create_dir_all(&directory)?;
        restrict_directory(&directory)?;
        Ok(Self { directory })
    }

    /// Open the vault in the platform's user data directory.
    pub fn open_default() -> Result<Self, VaultError> {
        let base = directories::ProjectDirs::from("dev", "nixarchy", "nixarchy-meetingai")
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no user data directory available",
                )
            })?
            .data_dir()
            .to_path_buf();
        Self::open(base)
    }

    /// Directory backing this vault.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    fn cipher(&self) -> Result<Aes256Gcm, VaultError> {
        let key_path = self.directory.join("vault.key");
        let mut key = [0u8; 32];
        match super::files::open_private(&key_path, true) {
            Ok(mut file) => {
                if self.directory.join("vault.bin").exists() {
                    drop(file);
                    fs::remove_file(&key_path)?;
                    return Err(VaultError::MissingKey);
                }
                rand::rng().fill_bytes(&mut key);
                file.write_all(&key)?;
                file.sync_all()?;
                #[cfg(unix)]
                File::open(&self.directory)?.sync_all()?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let mut file = super::files::open_private(&key_path, false)?;
                if file.metadata()?.len() != 32 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "vault key has the wrong length",
                    )
                    .into());
                }
                file.read_exact(&mut key).map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "vault key has the wrong length",
                    )
                })?;
            }
            Err(error) => return Err(VaultError::Io(error)),
        }
        Aes256Gcm::new_from_slice(&key).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid vault key").into()
        })
    }

    /// Read every value. A missing vault file is an empty store.
    pub fn read(&self) -> Result<Store, VaultError> {
        let _lock = super::files::lock(&self.directory.join("vault.lock"))?;
        self.read_unlocked()
    }

    fn read_unlocked(&self) -> Result<Store, VaultError> {
        let bytes = match fs::read(self.directory.join("vault.bin")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Store::new());
            }
            Err(error) => return Err(VaultError::Io(error)),
        };
        if bytes.len() < HEADER.len() + 12 || !bytes.starts_with(HEADER) {
            return Err(VaultError::Format);
        }
        let nonce_bytes = &bytes[HEADER.len()..HEADER.len() + 12];
        let nonce = Nonce::try_from(nonce_bytes)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "bad nonce"))?;
        let plaintext = self
            .cipher()?
            .decrypt(&nonce, &bytes[HEADER.len() + 12..])
            .map_err(|_| VaultError::Auth)?;
        Ok(serde_json::from_slice(&plaintext)?)
    }

    /// Replace all values atomically.
    pub fn write(&self, store: &Store) -> Result<(), VaultError> {
        let _lock = super::files::lock(&self.directory.join("vault.lock"))?;
        self.write_unlocked(store)
    }

    fn write_unlocked(&self, store: &Store) -> Result<(), VaultError> {
        let mut nonce = [0u8; 12];
        rand::rng().fill_bytes(&mut nonce);
        let plaintext = serde_json::to_vec(store)?;
        let nonce = Nonce::from(nonce);
        let encrypted = self
            .cipher()?
            .encrypt(&nonce, plaintext.as_ref())
            .map_err(|_| std::io::Error::other("vault encryption failed"))?;

        let temporary = self
            .directory
            .join(format!(".vault-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> std::io::Result<()> {
            let mut file = private_file(&temporary)?;
            file.write_all(HEADER)?;
            file.write_all(&nonce)?;
            file.write_all(&encrypted)?;
            file.sync_all()?;
            fs::rename(&temporary, self.directory.join("vault.bin"))?;
            #[cfg(unix)]
            File::open(&self.directory)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        Ok(result?)
    }

    /// Read-modify-write under a single lock, so concurrent writers cannot lose
    /// each other's updates.
    pub fn update(&self, change: impl FnOnce(&mut Store)) -> Result<(), VaultError> {
        let _lock = super::files::lock(&self.directory.join("vault.lock"))?;
        let mut store = self.read_unlocked()?;
        change(&mut store);
        self.write_unlocked(&store)
    }

    /// Fetch one value.
    pub fn get(&self, key: &str) -> Result<Option<Value>, VaultError> {
        Ok(self.read()?.get(key).cloned())
    }

    /// Set one value.
    pub fn set(&self, key: &str, value: Value) -> Result<(), VaultError> {
        self.update(|store| {
            store.insert(key.to_owned(), value);
        })
    }

    /// Remove values.
    pub fn remove(&self, keys: &[&str]) -> Result<(), VaultError> {
        self.update(|store| {
            for key in keys {
                store.remove(*key);
            }
        })
    }
}

fn restrict_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(not(unix))]
    let _ = path;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn private_file(path: &Path) -> std::io::Result<File> {
    super::files::open_private(path, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("nixarchy-meetingai-vault-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn round_trips_values() {
        let vault = Vault::open(temp_dir()).unwrap();
        vault
            .set("api_key", Value::String("secret".into()))
            .unwrap();
        assert_eq!(
            vault.get("api_key").unwrap(),
            Some(Value::String("secret".into()))
        );
    }

    #[test]
    fn ciphertext_does_not_contain_plaintext() {
        let directory = temp_dir();
        let vault = Vault::open(&directory).unwrap();
        vault
            .set("api_key", Value::String("super-secret-value".into()))
            .unwrap();
        let bytes = fs::read(directory.join("vault.bin")).unwrap();
        assert!(
            !bytes
                .windows(b"super-secret-value".len())
                .any(|w| w == b"super-secret-value"),
            "plaintext must not appear in the vault file"
        );
    }

    #[test]
    fn tampering_is_rejected() {
        let directory = temp_dir();
        let vault = Vault::open(&directory).unwrap();
        vault.set("k", Value::String("v".into())).unwrap();
        let mut bytes = fs::read(directory.join("vault.bin")).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        fs::write(directory.join("vault.bin"), bytes).unwrap();
        assert!(matches!(vault.read(), Err(VaultError::Auth)));
    }

    #[test]
    fn lost_key_preserves_data() {
        let directory = temp_dir();
        let vault = Vault::open(&directory).unwrap();
        vault.set("k", Value::String("v".into())).unwrap();
        fs::remove_file(directory.join("vault.key")).unwrap();
        assert!(matches!(vault.read(), Err(VaultError::MissingKey)));
        assert!(directory.join("vault.bin").exists());
    }

    #[test]
    fn writes_are_atomic_and_repeatable() {
        let vault = Vault::open(temp_dir()).unwrap();
        for i in 0..5 {
            vault.set("counter", Value::from(i)).unwrap();
        }
        assert_eq!(vault.get("counter").unwrap(), Some(Value::from(4)));
    }

    #[test]
    fn remove_clears_keys() {
        let vault = Vault::open(temp_dir()).unwrap();
        vault.set("a", Value::from(1)).unwrap();
        vault.remove(&["a"]).unwrap();
        assert_eq!(vault.get("a").unwrap(), None);
    }

    #[test]
    fn concurrent_vault_handles_do_not_lose_credentials() {
        let directory = temp_dir();
        let workers: Vec<_> = (0..12)
            .map(|i| {
                let directory = directory.clone();
                std::thread::spawn(move || {
                    Vault::open(directory)
                        .unwrap()
                        .set(&format!("key-{i}"), Value::from(i))
                        .unwrap()
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(Vault::open(directory).unwrap().read().unwrap().len(), 12);
    }

    #[test]
    fn overlong_key_is_rejected_without_replacing_data() {
        let vault = Vault::open(temp_dir()).unwrap();
        vault.set("key", Value::from("value")).unwrap();
        let before = fs::read(vault.directory.join("vault.bin")).unwrap();
        fs::write(vault.directory.join("vault.key"), [7u8; 33]).unwrap();
        assert!(vault.set("new", Value::from("value")).is_err());
        assert_eq!(fs::read(vault.directory.join("vault.bin")).unwrap(), before);
    }
}
