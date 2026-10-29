//! Encrypted key-value store backed by AES-256-GCM and a file lock.

use crate::core::security::files::{lock, open_private};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const HEADER: &[u8] = b"NIXARCHY-MEETINGAI-VAULT-v1\0";
const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;

const KEY_FILE: &str = "vault.key";
const BIN_FILE: &str = "vault.bin";
const LOCK_FILE: &str = "vault.lock";

type EntryMap = BTreeMap<String, Value>;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("vault I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("vault file format is unrecognized; existing file kept untouched")]
    Format,
    #[error("vault authentication failed; existing file kept untouched")]
    Auth,
    #[error("vault encryption key is missing; existing file kept untouched")]
    MissingKey,
    #[error("vault payload is not a valid JSON object (line {}, column {}); existing file kept untouched", .0.line(), .0.column())]
    Json(serde_json::Error),
}

pub struct Vault {
    dir: PathBuf,
}

impl Vault {
    pub fn open(directory: impl Into<PathBuf>) -> Result<Self, VaultError> {
        let dir = directory.into();
        fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self { dir })
    }

    pub fn read(&self) -> Result<BTreeMap<String, Value>, VaultError> {
        let _guard = self.acquire_lock()?;
        let (entries, _) = self.load_unlocked()?;
        Ok(entries)
    }

    pub fn get(&self, key: &str) -> Result<Option<Value>, VaultError> {
        let entries = self.read()?;
        Ok(entries.get(key).cloned())
    }

    pub fn set(&self, key: &str, value: Value) -> Result<(), VaultError> {
        let _guard = self.acquire_lock()?;
        let (mut entries, existing_key) = self.load_unlocked()?;
        entries.insert(key.to_owned(), value);
        self.save_unlocked(&entries, existing_key)
    }

    pub fn remove(&self, keys: &[&str]) -> Result<(), VaultError> {
        let _guard = self.acquire_lock()?;
        let (mut entries, existing_key) = self.load_unlocked()?;
        if existing_key.is_none() && !self.bin_path().exists() {
            return Ok(());
        }
        for name in keys {
            entries.remove(*name);
        }
        self.save_unlocked(&entries, existing_key)
    }

    fn key_path(&self) -> PathBuf {
        self.dir.join(KEY_FILE)
    }

    fn bin_path(&self) -> PathBuf {
        self.dir.join(BIN_FILE)
    }

    fn lock_path(&self) -> PathBuf {
        self.dir.join(LOCK_FILE)
    }

    fn acquire_lock(&self) -> Result<File, VaultError> {
        Ok(lock(&self.lock_path())?)
    }

    fn load_unlocked(&self) -> Result<(EntryMap, Option<[u8; KEY_LEN]>), VaultError> {
        let key_path = self.key_path();
        let key_opt = match fs::symlink_metadata(&key_path) {
            Ok(_) => Some(read_key_file(&key_path)?),
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => return Err(VaultError::Io(err)),
        };

        let bin_path = self.bin_path();
        match fs::symlink_metadata(&bin_path) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok((BTreeMap::new(), key_opt));
            }
            Err(err) => return Err(VaultError::Io(err)),
            Ok(_) => {}
        }

        let Some(raw_key) = key_opt else {
            return Err(VaultError::MissingKey);
        };

        let mut bin_file = open_private(&bin_path, false)?;
        let mut content = Vec::new();
        bin_file.read_to_end(&mut content)?;

        if content.len() < HEADER.len() + NONCE_LEN || !content.starts_with(HEADER) {
            return Err(VaultError::Format);
        }

        let (nonce_slice, body) = content[HEADER.len()..].split_at(NONCE_LEN);
        let nonce_arr: [u8; NONCE_LEN] = nonce_slice
            .try_into()
            .expect("slice has exact NONCE_LEN bytes");
        let cipher = Aes256Gcm::new(&raw_key.into());
        let plaintext = cipher
            .decrypt(&Nonce::from(nonce_arr), body)
            .map_err(|_| VaultError::Auth)?;

        let entries = decode_object(&plaintext)?;
        Ok((entries, Some(raw_key)))
    }

    fn save_unlocked(
        &self,
        entries: &BTreeMap<String, Value>,
        existing_key: Option<[u8; KEY_LEN]>,
    ) -> Result<(), VaultError> {
        let raw_key = match existing_key {
            Some(k) => k,
            None => self.create_key_file()?,
        };

        let plaintext = serde_json::to_vec(entries).map_err(VaultError::Json)?;
        let nonce_bytes: [u8; NONCE_LEN] = rand::random();
        let cipher = Aes256Gcm::new(&raw_key.into());
        let encrypted = cipher
            .encrypt(&Nonce::from(nonce_bytes), plaintext.as_slice())
            .map_err(|_| io::Error::other("vault payload encryption failed"))?;

        let mut blob = Vec::with_capacity(HEADER.len() + NONCE_LEN + encrypted.len());
        blob.extend_from_slice(HEADER);
        blob.extend_from_slice(&nonce_bytes);
        blob.extend_from_slice(&encrypted);

        let tmp_path = self
            .dir
            .join(format!("vault.stage-{}", uuid::Uuid::new_v4()));
        let write_result = (|| -> io::Result<()> {
            let mut tmp_file = open_private(&tmp_path, true)?;
            tmp_file.write_all(&blob)?;
            tmp_file.sync_all()?;
            drop(tmp_file);
            fs::rename(&tmp_path, self.bin_path())?;
            sync_directory(&self.dir)?;
            Ok(())
        })();

        if write_result.is_err() {
            let _ = fs::remove_file(&tmp_path);
        }
        write_result?;
        Ok(())
    }

    fn create_key_file(&self) -> Result<[u8; KEY_LEN], VaultError> {
        let generated: [u8; KEY_LEN] = rand::random();
        let key_path = self.key_path();
        let mut handle = open_private(&key_path, true)?;
        handle.write_all(&generated)?;
        handle.sync_all()?;
        drop(handle);
        sync_directory(&self.dir)?;
        Ok(generated)
    }
}

fn read_key_file(path: &Path) -> Result<[u8; KEY_LEN], VaultError> {
    let mut handle = open_private(path, false)?;
    let mut bytes = Vec::new();
    handle.read_to_end(&mut bytes)?;
    bytes.try_into().map_err(|_| {
        VaultError::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            "vault key length is invalid; existing files kept untouched",
        ))
    })
}

fn decode_object(bytes: &[u8]) -> Result<BTreeMap<String, Value>, VaultError> {
    let parsed: Value = serde_json::from_slice(bytes).map_err(|err| {
        let pos_err = serde_json::from_slice::<Value>(b"").unwrap_err();
        let _ = err;
        VaultError::Json(pos_err)
    })?;
    match parsed {
        Value::Object(map) => Ok(map.into_iter().collect()),
        _ => {
            let type_err = serde_json::from_slice::<BTreeMap<String, Value>>(b"null").unwrap_err();
            Err(VaultError::Json(type_err))
        }
    }
}

fn sync_directory(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(dir)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("nm-vault-{}", uuid::Uuid::new_v4()));
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn round_trip_and_fresh_directory_state() {
        let dir = TempDir::new();
        let vault = Vault::open(dir.path()).unwrap();

        assert_eq!(vault.get("never-set").unwrap(), None);
        assert!(vault.read().unwrap().is_empty());
        assert!(!dir.path().join("vault.key").exists());

        vault.set("panel.colour", json!("teal")).unwrap();
        assert_eq!(vault.get("panel.colour").unwrap(), Some(json!("teal")));

        let second = Vault::open(dir.path()).unwrap();
        assert_eq!(second.get("panel.colour").unwrap(), Some(json!("teal")));
    }

    #[test]
    fn binary_layout_and_unix_permissions() {
        let dir = TempDir::new();
        let vault = Vault::open(dir.path()).unwrap();
        vault.set("k", json!("v")).unwrap();

        let expected_plain = br#"{"k":"v"}"#;
        let bin_bytes = fs::read(dir.path().join("vault.bin")).unwrap();
        assert!(bin_bytes.starts_with(HEADER));
        assert_eq!(
            bin_bytes.len(),
            HEADER.len() + NONCE_LEN + expected_plain.len() + 16
        );

        let key_bytes = fs::read(dir.path().join("vault.key")).unwrap();
        assert_eq!(key_bytes.len(), 32);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir_mode = fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777;
            assert_eq!(dir_mode, 0o700);
            for name in ["vault.key", "vault.bin", "vault.lock"] {
                let mode = fs::metadata(dir.path().join(name))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777;
                assert_eq!(mode, 0o600, "{name}");
            }
        }
    }

    #[test]
    fn known_answer_vector_reads_back() {
        let dir = TempDir::new();
        let vault = Vault::open(dir.path()).unwrap();

        let raw_key = [0x5a_u8; 32];
        let raw_nonce = [0x0c_u8; 12];
        let plain = br#"{"zone":"west"}"#;
        let cipher = Aes256Gcm::new(&raw_key.into());
        let body = cipher
            .encrypt(&Nonce::from(raw_nonce), plain.as_slice())
            .unwrap();

        let mut blob = Vec::new();
        blob.extend_from_slice(HEADER);
        blob.extend_from_slice(&raw_nonce);
        blob.extend_from_slice(&body);

        fs::write(dir.path().join("vault.key"), raw_key).unwrap();
        fs::write(dir.path().join("vault.bin"), blob).unwrap();

        let map = vault.read().unwrap();
        assert_eq!(map.get("zone"), Some(&json!("west")));
    }

    #[test]
    fn consecutive_writes_pick_distinct_nonces_and_hide_plaintext() {
        let dir = TempDir::new();
        let vault = Vault::open(dir.path()).unwrap();
        let dummy = "dummy-token-for-test-7731";

        vault.set("token", json!(dummy)).unwrap();
        let first = fs::read(dir.path().join("vault.bin")).unwrap();
        vault.set("token", json!(dummy)).unwrap();
        let second = fs::read(dir.path().join("vault.bin")).unwrap();

        let nonce1 = &first[HEADER.len()..HEADER.len() + NONCE_LEN];
        let nonce2 = &second[HEADER.len()..HEADER.len() + NONCE_LEN];
        assert_ne!(nonce1, nonce2);
        assert!(!second.windows(dummy.len()).any(|w| w == dummy.as_bytes()));
    }

    #[test]
    fn tampering_and_truncation_report_format_or_auth_without_leaking() {
        let dir = TempDir::new();
        let vault = Vault::open(dir.path()).unwrap();
        let secret = "dummy-token-for-test-7731";
        vault.set("item", json!(secret)).unwrap();
        let valid = fs::read(dir.path().join("vault.bin")).unwrap();

        // Flipping one bit in nonce, ciphertext, or tag -> Auth
        let nonce_pos = HEADER.len();
        let cipher_pos = HEADER.len() + NONCE_LEN;
        let tag_pos = valid.len() - 1;
        for pos in [nonce_pos, cipher_pos, tag_pos] {
            let mut tampered = valid.clone();
            tampered[pos] ^= 0x01;
            fs::write(dir.path().join("vault.bin"), &tampered).unwrap();
            let err = vault.read().unwrap_err();
            assert!(matches!(err, VaultError::Auth));
            assert!(!err.to_string().contains(secret));
        }

        // Changing first header byte -> Format
        let mut bad_header = valid.clone();
        bad_header[0] ^= 0x01;
        fs::write(dir.path().join("vault.bin"), &bad_header).unwrap();
        let err = vault.read().unwrap_err();
        assert!(matches!(err, VaultError::Format));
        assert!(!err.to_string().contains(secret));

        // 30-byte file -> Format
        fs::write(dir.path().join("vault.bin"), vec![0_u8; 30]).unwrap();
        let err = vault.read().unwrap_err();
        assert!(matches!(err, VaultError::Format));
        assert!(!err.to_string().contains(secret));
    }

    #[test]
    fn missing_bad_or_symlinked_keys_preserve_files() {
        let dir = TempDir::new();
        let vault = Vault::open(dir.path()).unwrap();
        let secret = "dummy-token-for-test-7731";
        vault.set("k", json!(secret)).unwrap();

        let bin_path = dir.path().join("vault.bin");
        let key_path = dir.path().join("vault.key");
        let saved_bin = fs::read(&bin_path).unwrap();
        let saved_key = fs::read(&key_path).unwrap();

        // Delete vault.key
        fs::remove_file(&key_path).unwrap();
        let r_err = vault.read().unwrap_err();
        assert!(matches!(r_err, VaultError::MissingKey));
        assert!(!r_err.to_string().contains(secret));
        let s_err = vault.set("k2", json!("v2")).unwrap_err();
        assert!(matches!(s_err, VaultError::MissingKey));
        assert!(!key_path.exists());
        assert_eq!(fs::read(&bin_path).unwrap(), saved_bin);

        // 31-byte and 40-byte keys
        for bad_len in [31_usize, 40_usize] {
            let raw = vec![0x42_u8; bad_len];
            fs::write(&key_path, &raw).unwrap();
            let err1 = vault.read().unwrap_err();
            assert!(!err1.to_string().contains(secret));
            let err2 = vault.set("k2", json!("v2")).unwrap_err();
            assert!(!err2.to_string().contains(secret));
            assert_eq!(fs::read(&bin_path).unwrap(), saved_bin);
            assert_eq!(fs::read(&key_path).unwrap(), raw);
        }

        // Symlinked vault.key
        #[cfg(unix)]
        {
            fs::remove_file(&key_path).unwrap();
            let target = dir.path().join("target.txt");
            fs::write(&target, b"untouched-target").unwrap();
            std::os::unix::fs::symlink(&target, &key_path).unwrap();
            assert!(vault.read().is_err());
            assert!(vault.set("k2", json!("v2")).is_err());
            assert_eq!(fs::read(&target).unwrap(), b"untouched-target");
            assert_eq!(fs::read(&bin_path).unwrap(), saved_bin);
            fs::remove_file(&key_path).unwrap();
        }

        // Restore valid key and verify read works again
        fs::write(&key_path, saved_key).unwrap();
        assert_eq!(vault.get("k").unwrap(), Some(json!(secret)));
    }

    #[test]
    fn concurrent_writers_remove_and_cleanup() {
        let dir = TempDir::new();
        let base = dir.path().to_path_buf();

        let handles: Vec<_> = (0..9)
            .map(|idx| {
                let p = base.clone();
                std::thread::spawn(move || {
                    let v = Vault::open(p).unwrap();
                    v.set(&format!("entry-{idx}"), json!(idx)).unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let vault = Vault::open(&base).unwrap();
        let all = vault.read().unwrap();
        assert_eq!(all.len(), 9);
        for idx in 0..9 {
            assert_eq!(all.get(&format!("entry-{idx}")), Some(&json!(idx)));
        }

        let dir2 = TempDir::new();
        let vault2 = Vault::open(dir2.path()).unwrap();
        vault2.set("a", json!(1)).unwrap();
        vault2.set("b", json!(2)).unwrap();
        vault2.remove(&["b", "absent"]).unwrap();
        let remaining = vault2.read().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining.get("a"), Some(&json!(1)));

        vault2.remove(&[]).unwrap();
        assert_eq!(vault2.read().unwrap(), remaining);

        let names: Vec<String> = fs::read_dir(dir2.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        for name in &names {
            assert!(
                matches!(name.as_str(), "vault.key" | "vault.bin" | "vault.lock"),
                "unexpected file left behind: {name}"
            );
        }
    }
}
