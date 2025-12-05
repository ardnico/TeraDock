use std::fs;
use std::path::PathBuf;

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::Engine;
use rand::RngCore;

use crate::error::{Error, Result};

pub struct SecretStore {
    key_path: PathBuf,
}

impl SecretStore {
    pub fn new(key_path: impl Into<PathBuf>) -> Result<Self> {
        let key_path = key_path.into();
        if let Some(parent) = key_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if !key_path.exists() {
            let mut key = [0u8; 32];
            OsRng.fill_bytes(&mut key);
            let encoded = base64::engine::general_purpose::STANDARD.encode(key);
            fs::write(&key_path, encoded)?;
        }
        Ok(Self { key_path })
    }

    fn load_key(&self) -> Result<[u8; 32]> {
        let text = fs::read_to_string(&self.key_path)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(text.trim())
            .map_err(|e| Error::Crypto(format!("failed to decode key: {e}")))?;
        if bytes.len() != 32 {
            return Err(Error::Crypto("invalid key length".into()));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Ok(key)
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<String> {
        let key_bytes = self.load_key()?;
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| Error::Crypto(format!("cipher init: {e}")))?;
        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext)
            .map_err(|e| Error::Crypto(format!("encrypt: {e}")))?;
        let mut blob = Vec::with_capacity(nonce.len() + ciphertext.len());
        blob.extend_from_slice(&nonce);
        blob.extend_from_slice(&ciphertext);
        Ok(base64::engine::general_purpose::STANDARD.encode(blob))
    }

    pub fn decrypt(&self, ciphertext_b64: &str) -> Result<String> {
        let data = base64::engine::general_purpose::STANDARD
            .decode(ciphertext_b64)
            .map_err(|e| Error::Crypto(format!("decode: {e}")))?;
        if data.len() < 13 {
            return Err(Error::Crypto("ciphertext too short".into()));
        }
        let (nonce_bytes, cipher_bytes) = data.split_at(12);
        let key_bytes = self.load_key()?;
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| Error::Crypto(format!("cipher init: {e}")))?;
        let plaintext = cipher
            .decrypt(Nonce::from_slice(nonce_bytes), cipher_bytes)
            .map_err(|e| Error::Crypto(format!("decrypt: {e}")))?;
        String::from_utf8(plaintext).map_err(|e| Error::Crypto(format!("utf8: {e}")))
    }
}
