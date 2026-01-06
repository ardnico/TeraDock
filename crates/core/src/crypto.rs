use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305, XNonce};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdfParams {
    pub mem_cost_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            mem_cost_kib: 19_456,
            iterations: 3,
            parallelism: 1,
        }
    }
}

pub type MasterKey = Zeroizing<[u8; 32]>;

pub fn derive_key(password: &[u8], salt: &[u8], params: &KdfParams) -> Result<MasterKey> {
    let argon_params = Params::new(
        params.mem_cost_kib,
        params.iterations,
        params.parallelism,
        None,
    )
    .map_err(|e| CoreError::Crypto(e.to_string()))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon_params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password, salt, key.as_mut())
        .map_err(|e| CoreError::Crypto(e.to_string()))?;
    Ok(key)
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    OsRng.fill_bytes(&mut buf);
    buf
}

pub fn encrypt(key: &[u8], nonce: &[u8], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = XNonce::from_slice(nonce);
    cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|e| CoreError::Crypto(e.to_string()))
}

pub fn decrypt(key: &[u8], nonce: &[u8], aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = XNonce::from_slice(nonce);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| CoreError::DecryptionFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_round_trip() {
        let params = KdfParams::default();
        let salt = random_bytes::<16>();
        let key = derive_key(b"password", &salt, &params).unwrap();
        let nonce = random_bytes::<24>();
        let aad = b"meta";
        let plaintext = b"super secret";
        let ct = encrypt(key.as_ref(), &nonce, aad, plaintext).unwrap();
        let decrypted = decrypt(key.as_ref(), &nonce, aad, &ct).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn aad_mismatch_fails() {
        let params = KdfParams::default();
        let salt = random_bytes::<16>();
        let key = derive_key(b"password", &salt, &params).unwrap();
        let nonce = random_bytes::<24>();
        let ct = encrypt(key.as_ref(), &nonce, b"aad1", b"plain").unwrap();
        let err = decrypt(key.as_ref(), &nonce, b"aad2", &ct).unwrap_err();
        assert!(matches!(err, CoreError::DecryptionFailed));
    }
}
