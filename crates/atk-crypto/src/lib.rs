use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;
use std::path::Path;

/// Handle to a resolved 32-byte encryption key used for encrypting secrets (PATs, webhook
/// secrets) at rest. Kept out of the database itself.
#[derive(Clone)]
pub struct EncryptionKey(Key<Aes256Gcm>);

impl EncryptionKey {
    /// Resolve the key from config/env, or generate and persist one under `secrets_dir`.
    pub fn load_or_generate(configured: Option<&str>, secrets_dir: &Path) -> Result<Self> {
        if let Some(b64) = configured {
            let bytes = B64.decode(b64).context("ENCRYPTION_KEY is not valid base64")?;
            anyhow::ensure!(bytes.len() == 32, "ENCRYPTION_KEY must decode to 32 bytes");
            return Ok(Self(*Key::<Aes256Gcm>::from_slice(&bytes)));
        }

        std::fs::create_dir_all(secrets_dir)?;
        let key_path = secrets_dir.join("encryption.key");
        if key_path.exists() {
            let b64 = std::fs::read_to_string(&key_path)?;
            let bytes = B64.decode(b64.trim())?;
            anyhow::ensure!(bytes.len() == 32, "persisted encryption key is corrupt");
            return Ok(Self(*Key::<Aes256Gcm>::from_slice(&bytes)));
        }

        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        std::fs::write(&key_path, B64.encode(bytes))
            .context("failed to persist generated encryption key")?;
        tracing::warn!(
            path = %key_path.display(),
            "generated a new encryption key; back this file up, losing it makes stored PATs/webhook secrets unrecoverable"
        );
        Ok(Self(*Key::<Aes256Gcm>::from_slice(&bytes)))
    }

    /// A fresh, in-memory-only key with no disk I/O: used for a bucket's ephemeral session key,
    /// which must never be persisted or exposed to a user, unlike the durable at-rest key above.
    /// Callers are responsible for dropping it once the bucket it belongs to is torn down.
    pub fn generate_ephemeral() -> Self {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        Self(*Key::<Aes256Gcm>::from_slice(&bytes))
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
        let cipher = Aes256Gcm::new(&self.0);
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;
        Ok((ciphertext, nonce_bytes.to_vec()))
    }

    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>> {
        let cipher = Aes256Gcm::new(&self.0);
        let nonce = Nonce::from_slice(nonce);
        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))
    }

    pub fn encrypt_str(&self, plaintext: &str) -> Result<(Vec<u8>, Vec<u8>)> {
        self.encrypt(plaintext.as_bytes())
    }

    pub fn decrypt_str(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<String> {
        let bytes = self.decrypt(ciphertext, nonce)?;
        Ok(String::from_utf8(bytes)?)
    }
}
