use std::collections::HashSet;

use rusqlite::{params, Connection, Row, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::configset::ConfigFileWhen;
use crate::crypto::{decrypt, encrypt, random_bytes, MasterKey};
use crate::error::{CoreError, Result};
use crate::profile::{DangerLevel, Profile, ProfileType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictStrategy {
    Reject,
    Rename,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportDocument {
    pub version: u32,
    pub profiles: Vec<Profile>,
    pub cmdsets: Vec<ExportCmdSet>,
    pub parsers: Vec<ExportParser>,
    pub configs: Vec<ExportConfigSet>,
    pub secrets: Vec<ExportSecret>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportCmdSet {
    pub cmdset_id: String,
    pub name: String,
    pub vars: Option<Value>,
    pub steps: Vec<ExportCmdStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportCmdStep {
    pub ord: i64,
    pub cmd: String,
    pub timeout_ms: Option<u64>,
    pub on_error: String,
    pub parser_spec: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportParser {
    pub parser_id: String,
    pub parser_type: String,
    pub definition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfigSet {
    pub config_id: String,
    pub name: String,
    pub hooks_cmdset_id: Option<String>,
    pub files: Vec<ExportConfigFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfigFile {
    pub src: String,
    pub dest: String,
    pub mode: Option<String>,
    pub when: ConfigFileWhen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportSecret {
    pub secret_id: String,
    pub kind: String,
    pub label: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportReport {
    pub profiles: usize,
    pub cmdsets: usize,
    pub parsers: usize,
    pub configs: usize,
    pub secrets: usize,
    pub secrets_skipped: usize,
}

pub fn export_document(
    conn: &Connection,
    include_secrets: bool,
    master: Option<&MasterKey>,
) -> Result<ExportDocument> {
    if include_secrets && master.is_none() {
        return Err(CoreError::Import(
            "master key required to export secrets".into(),
        ));
    }

    let profiles = load_profiles(conn)?;
    let cmdsets = load_cmdsets(conn)?;
    let parsers = load_parsers(conn)?;
    let configs = load_configs(conn)?;
    let secrets = load_secrets(conn, include_secrets, master)?;

    Ok(ExportDocument {
        version: 1,
        profiles,
        cmdsets,
        parsers,
        configs,
        secrets,
    })
}

pub fn export_to_json(
    conn: &Connection,
    include_secrets: bool,
    master: Option<&MasterKey>,
) -> Result<String> {
    let document = export_document(conn, include_secrets, master)?;
    Ok(serde_json::to_string_pretty(&document)?)
}

pub fn import_document(
    conn: &mut Connection,
    document: ExportDocument,
    strategy: ConflictStrategy,
    master: Option<&MasterKey>,
) -> Result<ImportReport> {
    if document.version != 1 {
        return Err(CoreError::Import(format!(
            "unsupported export version {}",
            document.version
        )));
    }

    let secrets_with_values = document.secrets.iter().any(|s| s.value.is_some());
    if secrets_with_values && master.is_none() {
        return Err(CoreError::Import(
            "master key required to import secrets".into(),
        ));
    }

    let tx = conn.transaction()?;
    let mut report = ImportReport::default();

    let existing_profile_ids = load_id_set(&tx, "profiles", "profile_id")?;
    let existing_cmdset_ids = load_id_set(&tx, "cmdsets", "cmdset_id")?;
    let existing_config_ids = load_id_set(&tx, "configsets", "config_id")?;
    let existing_parser_ids = load_id_set(&tx, "parsers", "parser_id")?;
    let existing_secret_ids = load_id_set(&tx, "secrets", "secret_id")?;

    ensure_no_id_conflicts(
        &existing_profile_ids,
        document.profiles.iter().map(|p| &p.profile_id),
    )?;
    ensure_no_id_conflicts(
        &existing_cmdset_ids,
        document.cmdsets.iter().map(|c| &c.cmdset_id),
    )?;
    ensure_no_id_conflicts(
        &existing_config_ids,
        document.configs.iter().map(|c| &c.config_id),
    )?;
    ensure_no_id_conflicts(
        &existing_parser_ids,
        document.parsers.iter().map(|p| &p.parser_id),
    )?;
    ensure_no_id_conflicts(
        &existing_secret_ids,
        document
            .secrets
            .iter()
            .filter(|secret| secret.value.is_some())
            .map(|secret| &secret.secret_id),
    )?;

    let mut profile_names = load_name_set(&tx, "profiles")?;
    let mut cmdset_names = load_name_set(&tx, "cmdsets")?;
    let mut config_names = load_name_set(&tx, "configsets")?;

    let mut profiles = document.profiles;
    for profile in &mut profiles {
        profile.name = resolve_name(
            &mut profile_names,
            profile.name.clone(),
            strategy,
            "profile",
        )?;
        insert_profile(&tx, profile)?;
        report.profiles += 1;
    }

    for parser in &document.parsers {
        insert_parser(&tx, parser)?;
        report.parsers += 1;
    }

    let mut cmdsets = document.cmdsets;
    for cmdset in &mut cmdsets {
        cmdset.name = resolve_name(&mut cmdset_names, cmdset.name.clone(), strategy, "cmdset")?;
        insert_cmdset(&tx, cmdset)?;
        report.cmdsets += 1;
    }

    let mut available_cmdsets: HashSet<String> = existing_cmdset_ids;
    for cmdset in &cmdsets {
        available_cmdsets.insert(cmdset.cmdset_id.clone());
    }

    let mut configs = document.configs;
    for config in &mut configs {
        if let Some(hooks) = config.hooks_cmdset_id.as_deref() {
            if !available_cmdsets.contains(hooks) {
                return Err(CoreError::Import(format!(
                    "configset {} references missing cmdset {}",
                    config.config_id, hooks
                )));
            }
        }
        config.name = resolve_name(
            &mut config_names,
            config.name.clone(),
            strategy,
            "configset",
        )?;
        insert_configset(&tx, config)?;
        report.configs += 1;
    }

    let mut secrets_skipped = 0usize;
    for secret in &document.secrets {
        match &secret.value {
            Some(_value) => {
                let master = master.ok_or_else(|| {
                    CoreError::Import("master key required to import secrets".into())
                })?;
                insert_secret(&tx, secret, master)?;
                report.secrets += 1;
            }
            None => {
                secrets_skipped += 1;
            }
        }
    }
    report.secrets_skipped = secrets_skipped;

    tx.commit()?;
    Ok(report)
}

pub fn import_from_json(
    conn: &mut Connection,
    json: &str,
    strategy: ConflictStrategy,
    master: Option<&MasterKey>,
) -> Result<ImportReport> {
    let document: ExportDocument = serde_json::from_str(json)?;
    import_document(conn, document, strategy, master)
}

fn load_profiles(conn: &Connection) -> Result<Vec<Profile>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT profile_id, name, type, host, port, user, danger_level, "group",
               tags_json, note, initial_send, client_overrides_json, created_at, updated_at, last_used_at
        FROM profiles
        ORDER BY name ASC
        "#,
    )?;
    let mut rows = stmt.query([])?;
    let mut profiles = Vec::new();
    while let Some(row) = rows.next()? {
        profiles.push(deserialize_profile(row)?);
    }
    Ok(profiles)
}

fn load_cmdsets(conn: &Connection) -> Result<Vec<ExportCmdSet>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT cmdset_id, name, vars_json
        FROM cmdsets
        ORDER BY cmdset_id ASC
        "#,
    )?;
    let mut rows = stmt.query([])?;
    let mut cmdsets = Vec::new();
    while let Some(row) = rows.next()? {
        let cmdset_id: String = row.get("cmdset_id")?;
        let mut steps_stmt = conn.prepare(
            r#"
            SELECT ord, cmd, timeout_ms, on_error, parser_spec
            FROM cmdsteps
            WHERE cmdset_id = ?1
            ORDER BY ord ASC
            "#,
        )?;
        let mut step_rows = steps_stmt.query([cmdset_id.clone()])?;
        let mut steps = Vec::new();
        while let Some(step_row) = step_rows.next()? {
            let timeout_ms: Option<i64> = step_row.get("timeout_ms")?;
            steps.push(ExportCmdStep {
                ord: step_row.get("ord")?,
                cmd: step_row.get("cmd")?,
                timeout_ms: timeout_ms.map(|value| value as u64),
                on_error: step_row.get("on_error")?,
                parser_spec: step_row.get("parser_spec")?,
            });
        }

        let vars_json: Option<String> = row.get("vars_json")?;
        cmdsets.push(ExportCmdSet {
            cmdset_id,
            name: row.get("name")?,
            vars: vars_json
                .map(|raw| serde_json::from_str(&raw))
                .transpose()?,
            steps,
        });
    }
    Ok(cmdsets)
}

fn load_parsers(conn: &Connection) -> Result<Vec<ExportParser>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT parser_id, type, definition
        FROM parsers
        ORDER BY parser_id ASC
        "#,
    )?;
    let mut rows = stmt.query([])?;
    let mut parsers = Vec::new();
    while let Some(row) = rows.next()? {
        parsers.push(ExportParser {
            parser_id: row.get("parser_id")?,
            parser_type: row.get("type")?,
            definition: row.get("definition")?,
        });
    }
    Ok(parsers)
}

fn load_configs(conn: &Connection) -> Result<Vec<ExportConfigSet>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT config_id, name, hooks_cmdset_id
        FROM configsets
        ORDER BY config_id ASC
        "#,
    )?;
    let mut rows = stmt.query([])?;
    let mut configs = Vec::new();
    while let Some(row) = rows.next()? {
        let config_id: String = row.get("config_id")?;
        let mut file_stmt = conn.prepare(
            r#"
            SELECT src, dest, mode, "when"
            FROM configfiles
            WHERE config_id = ?1
            ORDER BY id ASC
            "#,
        )?;
        let mut file_rows = file_stmt.query([config_id.clone()])?;
        let mut files = Vec::new();
        while let Some(file_row) = file_rows.next()? {
            let when_raw: String = file_row.get("when")?;
            files.push(ExportConfigFile {
                src: file_row.get("src")?,
                dest: file_row.get("dest")?,
                mode: file_row.get("mode")?,
                when: ConfigFileWhen::parse(&when_raw)?,
            });
        }
        configs.push(ExportConfigSet {
            config_id,
            name: row.get("name")?,
            hooks_cmdset_id: row.get("hooks_cmdset_id")?,
            files,
        });
    }
    Ok(configs)
}

fn load_secrets(
    conn: &Connection,
    include_secrets: bool,
    master: Option<&MasterKey>,
) -> Result<Vec<ExportSecret>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT secret_id, kind, label, ciphertext, nonce, created_at, updated_at
        FROM secrets
        ORDER BY created_at ASC
        "#,
    )?;
    let mut rows = stmt.query([])?;
    let mut secrets = Vec::new();
    while let Some(row) = rows.next()? {
        let secret_id: String = row.get("secret_id")?;
        let kind: String = row.get("kind")?;
        let value = if include_secrets {
            let master = master
                .ok_or_else(|| CoreError::Import("master key required to export secrets".into()))?;
            let ciphertext: Vec<u8> = row.get("ciphertext")?;
            let nonce: Vec<u8> = row.get("nonce")?;
            let aad = secret_aad(&secret_id, &kind);
            let plaintext = decrypt(master.as_ref(), &nonce, aad.as_bytes(), &ciphertext)?;
            let value = String::from_utf8(plaintext).map_err(|_| CoreError::DecryptionFailed)?;
            Some(value)
        } else {
            None
        };

        secrets.push(ExportSecret {
            secret_id,
            kind,
            label: row.get("label")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
            value,
        });
    }
    Ok(secrets)
}

fn deserialize_profile(row: &Row<'_>) -> Result<Profile> {
    let profile_type: String = row.get("type")?;
    let danger: String = row.get("danger_level")?;
    let tags_json: String = row.get("tags_json")?;
    let overrides: Option<String> = row.get("client_overrides_json")?;

    Ok(Profile {
        profile_id: row.get("profile_id")?,
        name: row.get("name")?,
        profile_type: ProfileType::from_str(&profile_type)?,
        host: row.get("host")?,
        port: row.get::<_, i64>("port")? as u16,
        user: row.get("user")?,
        danger_level: DangerLevel::from_str(&danger)?,
        group: row.get("group")?,
        tags: serde_json::from_str(&tags_json)?,
        note: row.get("note")?,
        initial_send: row.get("initial_send")?,
        client_overrides: match overrides {
            Some(raw) => Some(serde_json::from_str(&raw)?),
            None => None,
        },
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        last_used_at: row.get("last_used_at")?,
    })
}

fn load_id_set(conn: &Connection, table: &str, col: &str) -> Result<HashSet<String>> {
    let sql = format!("SELECT {col} FROM {table}");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut ids = HashSet::new();
    while let Some(row) = rows.next()? {
        let value: String = row.get(0)?;
        ids.insert(value);
    }
    Ok(ids)
}

fn load_name_set(conn: &Connection, table: &str) -> Result<HashSet<String>> {
    let sql = format!("SELECT name FROM {table}");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut names = HashSet::new();
    while let Some(row) = rows.next()? {
        let value: String = row.get(0)?;
        names.insert(value);
    }
    Ok(names)
}

fn ensure_no_id_conflicts<'a, I>(existing: &HashSet<String>, incoming: I) -> Result<()>
where
    I: Iterator<Item = &'a String>,
{
    let mut seen = HashSet::new();
    for id in incoming {
        if !seen.insert(id) {
            return Err(CoreError::Conflict(format!(
                "import id appears multiple times: {id}"
            )));
        }
        if existing.contains(id) {
            return Err(CoreError::Conflict(format!(
                "import id already exists: {id}"
            )));
        }
    }
    Ok(())
}

fn resolve_name(
    names: &mut HashSet<String>,
    candidate: String,
    strategy: ConflictStrategy,
    kind: &str,
) -> Result<String> {
    if !names.contains(&candidate) {
        names.insert(candidate.clone());
        return Ok(candidate);
    }

    match strategy {
        ConflictStrategy::Reject => Err(CoreError::Conflict(format!(
            "{kind} name already exists: {candidate}"
        ))),
        ConflictStrategy::Rename => {
            let mut suffix = 1usize;
            loop {
                let next = if suffix == 1 {
                    format!("{candidate}-imported")
                } else {
                    format!("{candidate}-imported-{suffix}")
                };
                if !names.contains(&next) {
                    names.insert(next.clone());
                    return Ok(next);
                }
                suffix += 1;
            }
        }
    }
}

fn insert_profile(tx: &Transaction<'_>, profile: &Profile) -> Result<()> {
    let tags_json = serde_json::to_string(&profile.tags)?;
    let overrides_json = profile
        .client_overrides
        .as_ref()
        .map(|v| serde_json::to_string(v))
        .transpose()?;

    tx.execute(
        r#"
        INSERT INTO profiles (
            profile_id, name, type, host, port, user, danger_level, "group",
            tags_json, note, initial_send, client_overrides_json, created_at, updated_at, last_used_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        "#,
        params![
            profile.profile_id,
            profile.name,
            profile.profile_type.to_string(),
            profile.host,
            profile.port as i64,
            profile.user,
            profile.danger_level.to_string(),
            profile.group,
            tags_json,
            profile.note,
            profile.initial_send,
            overrides_json,
            profile.created_at,
            profile.updated_at,
            profile.last_used_at,
        ],
    )?;
    Ok(())
}

fn insert_parser(tx: &Transaction<'_>, parser: &ExportParser) -> Result<()> {
    tx.execute(
        r#"
        INSERT INTO parsers (parser_id, type, definition)
        VALUES (?1, ?2, ?3)
        "#,
        params![parser.parser_id, parser.parser_type, parser.definition],
    )?;
    Ok(())
}

fn insert_cmdset(tx: &Transaction<'_>, cmdset: &ExportCmdSet) -> Result<()> {
    let vars_json = cmdset
        .vars
        .as_ref()
        .map(|vars| serde_json::to_string(vars))
        .transpose()?;
    tx.execute(
        r#"
        INSERT INTO cmdsets (cmdset_id, name, vars_json)
        VALUES (?1, ?2, ?3)
        "#,
        params![cmdset.cmdset_id, cmdset.name, vars_json],
    )?;
    for step in &cmdset.steps {
        let timeout_ms = step.timeout_ms.map(|value| value as i64);
        tx.execute(
            r#"
            INSERT INTO cmdsteps (cmdset_id, ord, cmd, timeout_ms, on_error, parser_spec)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                cmdset.cmdset_id,
                step.ord,
                step.cmd,
                timeout_ms,
                step.on_error,
                step.parser_spec
            ],
        )?;
    }
    Ok(())
}

fn insert_configset(tx: &Transaction<'_>, config: &ExportConfigSet) -> Result<()> {
    tx.execute(
        r#"
        INSERT INTO configsets (config_id, name, hooks_cmdset_id)
        VALUES (?1, ?2, ?3)
        "#,
        params![config.config_id, config.name, config.hooks_cmdset_id],
    )?;
    for file in &config.files {
        tx.execute(
            r#"
            INSERT INTO configfiles (config_id, src, dest, mode, "when")
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                config.config_id,
                file.src,
                file.dest,
                file.mode,
                file.when.as_str()
            ],
        )?;
    }
    Ok(())
}

fn insert_secret(tx: &Transaction<'_>, secret: &ExportSecret, master: &MasterKey) -> Result<()> {
    let value = secret
        .value
        .as_ref()
        .ok_or_else(|| CoreError::Import("secret value missing".into()))?;
    let nonce = random_bytes::<24>();
    let aad = secret_aad(&secret.secret_id, &secret.kind);
    let ciphertext = encrypt(master.as_ref(), &nonce, aad.as_bytes(), value.as_bytes())?;

    tx.execute(
        r#"
        INSERT INTO secrets (secret_id, kind, label, ciphertext, nonce, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            secret.secret_id,
            secret.kind,
            secret.label,
            ciphertext,
            nonce.to_vec(),
            secret.created_at,
            secret.updated_at
        ],
    )?;
    Ok(())
}

fn secret_aad(secret_id: &str, kind: &str) -> String {
    format!("{secret_id}:{kind}")
}
