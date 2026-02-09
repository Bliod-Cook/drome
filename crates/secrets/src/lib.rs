use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result, bail};
use argon2::Argon2;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::info;

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("encrypted secrets exist but no password provided")]
    MissingPassword,
}

#[derive(Debug, Clone)]
pub struct SecretStore {
    root: PathBuf,
    encryption_password: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PlainSecretsFile {
    schema_version: u32,
    values: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct EncryptedSecretsFile {
    schema_version: u32,
    salt_b64: String,
    nonce_b64: String,
    ciphertext_b64: String,
}

impl SecretStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            encryption_password: None,
        }
    }

    pub fn set_password(&mut self, password: Option<String>) {
        self.encryption_password = password;
    }

    pub fn put(
        &self,
        namespace: impl AsRef<str>,
        key: impl AsRef<str>,
        value: impl Into<String>,
    ) -> Result<()> {
        let mut values = self.load_values()?;
        values.insert(
            format!("{}:{}", namespace.as_ref(), key.as_ref()),
            value.into(),
        );
        self.save_values(&values)
    }

    pub fn get(&self, namespace: impl AsRef<str>, key: impl AsRef<str>) -> Result<Option<String>> {
        let values = self.load_values()?;
        Ok(values
            .get(&format!("{}:{}", namespace.as_ref(), key.as_ref()))
            .cloned())
    }

    pub fn remove(&self, namespace: impl AsRef<str>, key: impl AsRef<str>) -> Result<()> {
        let mut values = self.load_values()?;
        values.remove(&format!("{}:{}", namespace.as_ref(), key.as_ref()));
        self.save_values(&values)
    }

    pub fn is_encrypted_mode(&self) -> bool {
        self.encryption_password.is_some()
    }

    fn load_values(&self) -> Result<BTreeMap<String, String>> {
        let plain_path = self.plain_path();
        let enc_path = self.encrypted_path();

        match (
            plain_path.exists(),
            enc_path.exists(),
            self.encryption_password.as_ref(),
        ) {
            (false, false, _) => Ok(BTreeMap::new()),
            (true, false, _) => self.read_plain_file(),
            (false, true, None) => Err(SecretError::MissingPassword.into()),
            (false, true, Some(password)) => self.read_encrypted_file(password),
            (true, true, Some(password)) => {
                let values = self.read_encrypted_file(password)?;
                Ok(values)
            }
            (true, true, None) => self.read_plain_file(),
        }
    }

    fn save_values(&self, values: &BTreeMap<String, String>) -> Result<()> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("failed to create {}", self.root.display()))?;

        if let Some(password) = self.encryption_password.as_ref() {
            let encrypted = encrypt_values(password, values)?;
            let text = serde_json::to_string_pretty(&encrypted)?;
            fs::write(self.encrypted_path(), text)?;
            if self.plain_path().exists() {
                fs::remove_file(self.plain_path()).ok();
            }
            info!("secrets persisted in encrypted mode");
            return Ok(());
        }

        let plain = PlainSecretsFile {
            schema_version: SCHEMA_VERSION,
            values: values.clone(),
        };
        let text = serde_json::to_string_pretty(&plain)?;
        fs::write(self.plain_path(), text)?;
        if self.encrypted_path().exists() {
            fs::remove_file(self.encrypted_path()).ok();
        }
        info!("secrets persisted in plain mode");
        Ok(())
    }

    fn read_plain_file(&self) -> Result<BTreeMap<String, String>> {
        let text = fs::read_to_string(self.plain_path())?;
        let doc: PlainSecretsFile = serde_json::from_str(&text)?;
        Ok(doc.values)
    }

    fn read_encrypted_file(&self, password: &str) -> Result<BTreeMap<String, String>> {
        let text = fs::read_to_string(self.encrypted_path())?;
        let doc: EncryptedSecretsFile = serde_json::from_str(&text)?;
        decrypt_values(password, &doc)
    }

    fn plain_path(&self) -> PathBuf {
        self.root.join("secrets.json")
    }

    fn encrypted_path(&self) -> PathBuf {
        self.root.join("secrets.enc.json")
    }
}

fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow::anyhow!("failed to derive encryption key: {e}"))?;
    Ok(key)
}

fn encrypt_values(
    password: &str,
    values: &BTreeMap<String, String>,
) -> Result<EncryptedSecretsFile> {
    let plaintext = serde_json::to_vec(values)?;
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);

    let key = derive_key(password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key).context("failed to build cipher")?;

    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_ref())
        .map_err(|e| anyhow::anyhow!("failed to encrypt secrets: {e}"))?;

    Ok(EncryptedSecretsFile {
        schema_version: SCHEMA_VERSION,
        salt_b64: BASE64.encode(salt),
        nonce_b64: BASE64.encode(nonce_bytes),
        ciphertext_b64: BASE64.encode(ciphertext),
    })
}

fn decrypt_values(
    password: &str,
    encrypted: &EncryptedSecretsFile,
) -> Result<BTreeMap<String, String>> {
    let salt = BASE64.decode(&encrypted.salt_b64)?;
    let nonce_bytes = BASE64.decode(&encrypted.nonce_b64)?;
    let ciphertext = BASE64.decode(&encrypted.ciphertext_b64)?;

    if nonce_bytes.len() != 12 {
        bail!("invalid nonce length");
    }

    let key = derive_key(password, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key).context("failed to build cipher")?;

    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext.as_ref())
        .map_err(|e| anyhow::anyhow!("failed to decrypt secrets: {e}"))?;
    let values: BTreeMap<String, String> = serde_json::from_slice(&plaintext)?;
    Ok(values)
}

pub fn default_secret_dir_from(base_dir: &Path) -> PathBuf {
    base_dir.join("secrets")
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn roundtrip_plain() {
        let dir = tempdir().expect("tempdir");
        let store = SecretStore::new(dir.path());
        store.put("provider", "openai", "k1").expect("write");
        let value = store.get("provider", "openai").expect("read");
        assert_eq!(value.as_deref(), Some("k1"));
    }

    #[test]
    fn roundtrip_encrypted() {
        let dir = tempdir().expect("tempdir");
        let mut store = SecretStore::new(dir.path());
        store.set_password(Some("p@ss".to_string()));
        store.put("provider", "anthropic", "k2").expect("write");

        let mut store2 = SecretStore::new(dir.path());
        store2.set_password(Some("p@ss".to_string()));
        let value = store2.get("provider", "anthropic").expect("read");
        assert_eq!(value.as_deref(), Some("k2"));
    }

    #[test]
    fn wrong_password_fails() {
        let dir = tempdir().expect("tempdir");
        let mut store = SecretStore::new(dir.path());
        store.set_password(Some("good".to_string()));
        store.put("provider", "gemini", "k3").expect("write");

        let mut store_bad = SecretStore::new(dir.path());
        store_bad.set_password(Some("bad".to_string()));
        let err = store_bad.get("provider", "gemini").expect_err("must fail");
        assert!(
            err.to_string().contains("failed to decrypt"),
            "unexpected error: {err}"
        );
    }
}
