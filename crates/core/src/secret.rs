use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::crypto::{decrypt, derive_key, encrypt, random_bytes, KdfParams, MasterKey};
use crate::error::{CoreError, Result};
use crate::settings::{get_setting, set_setting};
use common::id::{generate_id, normalize_id, validate_id};
use rusqlite::{params, Connection};
use time::OffsetDateTime;

const KEY_SALT: &str = "master_salt";
const KEY_KDF_PARAMS: &str = "master_kdf_params";
const KEY_CHECK: &str = "master_check";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CheckToken {
    nonce: String,
    ciphertext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterState {
    pub salt: Vec<u8>,
    pub params: KdfParams,
    pub check: CheckToken,
}

impl MasterState {
    pub fn create(password: &[u8]) -> Result<(Self, MasterKey)> {
        let salt = random_bytes::<16>().to_vec();
        let params = KdfParams::default();
        let key = derive_key(password, &salt, &params)?;
        let check_plain = random_bytes::<16>();
        let nonce = random_bytes::<24>();
        let ciphertext = encrypt(key.as_ref(), &nonce, b"master-check", &check_plain)?;
        Ok((
            Self {
                salt,
                params,
                check: CheckToken {
                    nonce: B64.encode(nonce),
                    ciphertext: B64.encode(ciphertext),
                },
            },
            key,
        ))
    }

    pub fn store(&self, conn: &Connection) -> Result<()> {
        set_setting(conn, KEY_SALT, &B64.encode(&self.salt))?;
        let params_json = serde_json::to_string(&self.params)?;
        set_setting(conn, KEY_KDF_PARAMS, &params_json)?;
        let check_json = serde_json::to_string(&self.check)?;
        set_setting(conn, KEY_CHECK, &check_json)?;
        Ok(())
    }

    pub fn load(conn: &Connection) -> Result<Option<Self>> {
        let salt = match get_setting(conn, KEY_SALT)? {
            Some(s) => B64
                .decode(s.as_bytes())
                .map_err(|e| CoreError::Crypto(e.to_string()))?,
            None => return Ok(None),
        };
        let params = match get_setting(conn, KEY_KDF_PARAMS)? {
            Some(raw) => serde_json::from_str(&raw)?,
            None => return Ok(None),
        };
        let check = match get_setting(conn, KEY_CHECK)? {
            Some(raw) => serde_json::from_str(&raw)?,
            None => return Ok(None),
        };
        Ok(Some(Self {
            salt,
            params,
            check,
        }))
    }

    pub fn load_and_verify(&self, password: &[u8]) -> Result<MasterKey> {
        let key = derive_key(password, &self.salt, &self.params)?;
        let nonce_bytes = B64
            .decode(self.check.nonce.as_bytes())
            .map_err(|e| CoreError::Crypto(e.to_string()))?;
        let cipher_bytes = B64
            .decode(self.check.ciphertext.as_bytes())
            .map_err(|e| CoreError::Crypto(e.to_string()))?;
        let decrypted = decrypt(key.as_ref(), &nonce_bytes, b"master-check", &cipher_bytes)?;
        if decrypted.is_empty() {
            return Err(CoreError::MasterVerificationFailed);
        }
        Ok(key)
    }
}

#[derive(Debug, Clone)]
pub struct NewSecret {
    pub secret_id: Option<String>,
    pub kind: String,
    pub label: String,
    pub value: Zeroizing<String>,
    pub meta: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecretMetadata {
    pub secret_id: String,
    pub kind: String,
    pub label: String,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct SecretStore {
    conn: Connection,
}

impl SecretStore {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn is_master_set(&self) -> Result<bool> {
        Ok(MasterState::load(&self.conn)?.is_some())
    }

    pub fn set_master(&self, password: &str) -> Result<()> {
        if self.is_master_set()? {
            return Err(CoreError::MasterAlreadySet);
        }
        let (state, _key) = MasterState::create(password.as_bytes())?;
        state.store(&self.conn)?;
        Ok(())
    }

    pub fn load_master(&self, password: &str) -> Result<MasterKey> {
        let state = MasterState::load(&self.conn)?.ok_or(CoreError::MasterNotSet)?;
        state
            .load_and_verify(password.as_bytes())
            .map_err(|_| CoreError::MasterVerificationFailed)
    }

    pub fn add(&self, master: &MasterKey, input: NewSecret) -> Result<SecretMetadata> {
        let secret_id = match &input.secret_id {
            Some(id) => normalize_id(id),
            None => generate_id("s_"),
        };
        validate_id(&secret_id).map_err(CoreError::InvalidId)?;
        let aad = Self::aad(&secret_id, &input.kind);
        let nonce = random_bytes::<24>();
        let ciphertext = encrypt(
            master.as_ref(),
            &nonce,
            aad.as_bytes(),
            input.value.as_bytes(),
        )?;
        let now = now_ms();
        let meta_json = input
            .meta
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()?;
        let _ = meta_json;

        self.conn.execute(
            r#"
            INSERT INTO secrets (secret_id, kind, label, ciphertext, nonce, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                secret_id,
                input.kind,
                input.label,
                ciphertext,
                nonce.to_vec(),
                now,
                now
            ],
        )?;
        // Store optional meta as a settings-style side table in the future; for now ignore meta_json.
        let meta = SecretMetadata {
            secret_id,
            kind: input.kind,
            label: input.label,
            created_at: now,
            updated_at: now,
        };
        Ok(meta)
    }

    pub fn list(&self) -> Result<Vec<SecretMetadata>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT secret_id, kind, label, created_at, updated_at
            FROM secrets
            ORDER BY created_at ASC
            "#,
        )?;
        let mut rows = stmt.query([])?;
        let mut secrets = Vec::new();
        while let Some(row) = rows.next()? {
            secrets.push(SecretMetadata {
                secret_id: row.get("secret_id")?,
                kind: row.get("kind")?,
                label: row.get("label")?,
                created_at: row.get("created_at")?,
                updated_at: row.get("updated_at")?,
            });
        }
        Ok(secrets)
    }

    pub fn reveal(&self, master: &MasterKey, secret_id: &str) -> Result<String> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT secret_id, kind, label, ciphertext, nonce
            FROM secrets
            WHERE secret_id = ?1
            "#,
        )?;
        let mut rows = stmt.query([secret_id])?;
        let row = match rows.next()? {
            Some(row) => row,
            None => return Err(CoreError::NotFound(secret_id.to_string())),
        };
        let kind: String = row.get("kind")?;
        let aad = Self::aad(secret_id, &kind);
        let ciphertext: Vec<u8> = row.get("ciphertext")?;
        let nonce: Vec<u8> = row.get("nonce")?;
        let plaintext = decrypt(master.as_ref(), &nonce, aad.as_bytes(), &ciphertext)?;
        let value = String::from_utf8(plaintext).map_err(|_| CoreError::DecryptionFailed)?;
        Ok(value)
    }

    pub fn delete(&self, secret_id: &str) -> Result<bool> {
        let count = self
            .conn
            .execute("DELETE FROM secrets WHERE secret_id = ?1", [secret_id])?;
        Ok(count > 0)
    }

    fn aad(secret_id: &str, kind: &str) -> String {
        format!("{secret_id}:{kind}")
    }
}

fn now_ms() -> i64 {
    let nanos = OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000;
    i64::try_from(nanos).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_in_memory;

    #[test]
    fn master_required() {
        let conn = init_in_memory().unwrap();
        let store = SecretStore::new(conn);
        assert!(!store.is_master_set().unwrap());
        let err = store.load_master("bad").unwrap_err();
        assert!(matches!(err, CoreError::MasterNotSet));
    }

    #[test]
    fn set_master_and_reveal() {
        let conn = init_in_memory().unwrap();
        let store = SecretStore::new(conn);
        store.set_master("topsecret").unwrap();
        assert!(store.is_master_set().unwrap());
        let master = store.load_master("topsecret").unwrap();
        let secret = store
            .add(
                &master,
                NewSecret {
                    secret_id: None,
                    kind: "password".into(),
                    label: "db".into(),
                    value: Zeroizing::new("hunter2".into()),
                    meta: None,
                },
            )
            .unwrap();
        let revealed = store.reveal(&master, &secret.secret_id).unwrap();
        assert_eq!(revealed, "hunter2");
    }

    #[test]
    fn wrong_master_fails() {
        let conn = init_in_memory().unwrap();
        let store = SecretStore::new(conn);
        store.set_master("right").unwrap();
        let master = store.load_master("right").unwrap();
        let secret = store
            .add(
                &master,
                NewSecret {
                    secret_id: Some("s_test".into()),
                    kind: "token".into(),
                    label: "api".into(),
                    value: Zeroizing::new("value".into()),
                    meta: None,
                },
            )
            .unwrap();
        let err = store.load_master("wrong").unwrap_err();
        assert!(matches!(err, CoreError::MasterVerificationFailed));
        let err = store
            .reveal(&Zeroizing::new([0u8; 32]), &secret.secret_id)
            .unwrap_err();
        assert!(matches!(err, CoreError::DecryptionFailed));
    }
}
