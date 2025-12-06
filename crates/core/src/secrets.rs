use std::fs;
use std::path::PathBuf;

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::Engine;
use rand::RngCore;

use crate::config::{AppPaths, SecretBackend, SecretsConfig};
use crate::error::{Error, Result};

pub struct SecretStore {
    key_path: PathBuf,
    backend: SecretBackend,
    credential_target: String,
}

impl SecretStore {
    pub fn new(paths: &AppPaths, secrets: &SecretsConfig) -> Result<Self> {
        let key_path = paths.secret_key_path.clone();
        if secrets.backend == SecretBackend::FileKey {
            if let Some(parent) = key_path.parent() {
                fs::create_dir_all(parent)?;
            }
            if !key_path.exists() {
                let mut key = [0u8; 32];
                OsRng.fill_bytes(&mut key);
                let encoded = base64::engine::general_purpose::STANDARD.encode(key);
                fs::write(&key_path, encoded)?;
            }
        }
        Ok(Self {
            key_path,
            backend: secrets.backend.clone(),
            credential_target: secrets.credential_target.clone(),
        })
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
        match self.backend {
            SecretBackend::FileKey => self.encrypt_file_key(plaintext),
            SecretBackend::WindowsCredentialManager => self.save_credential(plaintext),
            SecretBackend::WindowsDpapi => self.encrypt_dpapi(plaintext),
        }
    }

    pub fn decrypt(&self, ciphertext_b64: &str) -> Result<String> {
        match self.backend {
            SecretBackend::FileKey => self.decrypt_file_key(ciphertext_b64),
            SecretBackend::WindowsCredentialManager => self.read_credential(),
            SecretBackend::WindowsDpapi => self.decrypt_dpapi(ciphertext_b64),
        }
    }

    fn encrypt_file_key(&self, plaintext: &[u8]) -> Result<String> {
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

    fn decrypt_file_key(&self, ciphertext_b64: &str) -> Result<String> {
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

    #[cfg(windows)]
    fn encrypt_dpapi(&self, plaintext: &[u8]) -> Result<String> {
        use windows::core::PCWSTR;
        use windows::Win32::Security::Cryptography::{
            CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };

        let mut data = CRYPT_INTEGER_BLOB {
            cbData: plaintext.len() as u32,
            pbData: plaintext.as_ptr() as *mut u8,
        };
        let mut out = CRYPT_INTEGER_BLOB::default();
        let ok = unsafe {
            CryptProtectData(
                &mut data,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok.is_err() {
            return Err(Error::Crypto("CryptProtectData failed".into()));
        }
        let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
        unsafe { windows::Win32::System::Memory::LocalFree(out.pbData as isize) };
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    #[cfg(not(windows))]
    fn encrypt_dpapi(&self, _plaintext: &[u8]) -> Result<String> {
        Err(Error::Crypto(
            "DPAPI backend is only available on Windows".into(),
        ))
    }

    #[cfg(windows)]
    fn decrypt_dpapi(&self, ciphertext_b64: &str) -> Result<String> {
        use windows::Win32::Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };

        let data = base64::engine::general_purpose::STANDARD
            .decode(ciphertext_b64)
            .map_err(|e| Error::Crypto(format!("decode: {e}")))?;
        let mut in_blob = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut out = CRYPT_INTEGER_BLOB::default();
        let ok = unsafe {
            CryptUnprotectData(
                &mut in_blob,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok.is_err() {
            return Err(Error::Crypto("CryptUnprotectData failed".into()));
        }
        let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
        unsafe { windows::Win32::System::Memory::LocalFree(out.pbData as isize) };
        String::from_utf8(bytes).map_err(|e| Error::Crypto(format!("utf8: {e}")))
    }

    #[cfg(not(windows))]
    fn decrypt_dpapi(&self, _ciphertext_b64: &str) -> Result<String> {
        Err(Error::Crypto(
            "DPAPI backend is only available on Windows".into(),
        ))
    }

    #[cfg(windows)]
    fn save_credential(&self, plaintext: &[u8]) -> Result<String> {
        use std::mem::size_of;
        use windows::core::{PCWSTR, PWSTR};
        use windows::Win32::Security::Credentials::{
            CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
        };

        let target: Vec<u16> = self
            .credential_target
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let mut credential = CREDENTIALW::default();
        credential.Type = CRED_TYPE_GENERIC;
        credential.Persist = CRED_PERSIST_LOCAL_MACHINE;
        credential.TargetName = PWSTR(target.as_ptr() as *mut _);
        credential.CredentialBlobSize = plaintext.len() as u32;
        credential.CredentialBlob = plaintext.as_ptr() as *mut u8;
        credential.UserName = PWSTR(std::ptr::null_mut());

        let ok = unsafe { CredWriteW(&credential, 0) };
        if ok.is_err() {
            return Err(Error::Crypto("Credential write failed".into()));
        }
        Ok(self.credential_target.clone())
    }

    #[cfg(not(windows))]
    fn save_credential(&self, _plaintext: &[u8]) -> Result<String> {
        Err(Error::Crypto(
            "Windows Credential Manager backend is only available on Windows".into(),
        ))
    }

    #[cfg(windows)]
    fn read_credential(&self) -> Result<String> {
        use windows::core::PWSTR;
        use windows::Win32::Security::Credentials::{
            CredFree, CredReadW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
        };

        let target: Vec<u16> = self
            .credential_target
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut cred_ptr: *mut CREDENTIALW = std::ptr::null_mut();
        let ok = unsafe { CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, 0, &mut cred_ptr) };
        if ok.is_err() {
            return Err(Error::Crypto("Credential read failed".into()));
        }
        let cred = unsafe { &*cred_ptr };
        let blob = unsafe {
            std::slice::from_raw_parts(cred.CredentialBlob, cred.CredentialBlobSize as usize)
        };
        let text =
            String::from_utf8(blob.to_vec()).map_err(|e| Error::Crypto(format!("utf8: {e}")))?;
        unsafe { CredFree(cred_ptr as *mut _) };
        Ok(text)
    }

    #[cfg(not(windows))]
    fn read_credential(&self) -> Result<String> {
        Err(Error::Crypto(
            "Windows Credential Manager backend is only available on Windows".into(),
        ))
    }
}
