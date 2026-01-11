use std::borrow::Cow;

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::doctor::ClientOverrides;
use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingScopeKind {
    Global,
    Env,
    Profile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingScope {
    Global,
    Env(String),
    Profile(String),
}

impl SettingScope {
    pub fn global() -> Self {
        Self::Global
    }

    pub fn profile(profile_id: impl Into<String>) -> Self {
        Self::Profile(profile_id.into())
    }

    pub fn kind(&self) -> SettingScopeKind {
        match self {
            SettingScope::Global => SettingScopeKind::Global,
            SettingScope::Env(_) => SettingScopeKind::Env,
            SettingScope::Profile(_) => SettingScopeKind::Profile,
        }
    }

    pub fn as_db(&self) -> Cow<'_, str> {
        match self {
            SettingScope::Global => Cow::Borrowed("global"),
            SettingScope::Env(name) => Cow::Owned(format!("env:{name}")),
            SettingScope::Profile(profile_id) => Cow::Owned(format!("profile:{profile_id}")),
        }
    }

    pub fn parse(raw: &str) -> Result<Self> {
        if raw.eq_ignore_ascii_case("global") {
            return Ok(Self::Global);
        }
        if let Some(name) = raw.strip_prefix("env:") {
            if name.trim().is_empty() {
                return Err(CoreError::InvalidSetting(
                    "env scope requires a name (env:NAME)".to_string(),
                ));
            }
            return Ok(Self::Env(name.trim().to_string()));
        }
        if let Some(profile_id) = raw.strip_prefix("profile:") {
            if profile_id.trim().is_empty() {
                return Err(CoreError::InvalidSetting(
                    "profile scope requires an id (profile:ID)".to_string(),
                ));
            }
            return Ok(Self::Profile(profile_id.trim().to_string()));
        }
        Err(CoreError::InvalidSetting(format!(
            "unknown scope '{raw}' (expected global, env:NAME, or profile:ID)"
        )))
    }
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    get_setting_scoped(conn, &SettingScope::Global, key)
}

pub fn get_setting_scoped(
    conn: &Connection,
    scope: &SettingScope,
    key: &str,
) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE scope = ?1 AND key = ?2")?;
    let mut rows = stmt.query(params![scope.as_db(), key])?;
    let value = match rows.next()? {
        Some(row) => Some(row.get(0)?),
        None => None,
    };
    Ok(value)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    set_setting_scoped(conn, &SettingScope::Global, key, value)
}

pub fn set_setting_scoped(
    conn: &Connection,
    scope: &SettingScope,
    key: &str,
    value: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (scope, key, value) VALUES (?1, ?2, ?3) ON CONFLICT(scope, key) DO UPDATE SET value = excluded.value",
        params![scope.as_db(), key, value],
    )?;
    Ok(())
}

pub fn clear_setting_scoped(conn: &Connection, scope: &SettingScope, key: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM settings WHERE scope = ?1 AND key = ?2",
        params![scope.as_db(), key],
    )?;
    Ok(())
}

pub fn get_setting_resolved(
    conn: &Connection,
    scope: &SettingScope,
    key: &str,
) -> Result<Option<String>> {
    get_setting_resolved_with_override(conn, scope, key, None)
}

pub fn get_setting_resolved_with_override(
    conn: &Connection,
    scope: &SettingScope,
    key: &str,
    command_override: Option<&str>,
) -> Result<Option<String>> {
    if let Some(value) = command_override {
        return Ok(Some(value.to_string()));
    }
    match scope {
        SettingScope::Global => get_setting_scoped(conn, scope, key),
        SettingScope::Env(_) => {
            let scoped = get_setting_scoped(conn, scope, key)?;
            if scoped.is_some() {
                Ok(scoped)
            } else {
                get_setting_scoped(conn, &SettingScope::Global, key)
            }
        }
        SettingScope::Profile(_) => {
            let scoped = get_setting_scoped(conn, scope, key)?;
            if scoped.is_some() {
                return Ok(scoped);
            }
            if let Some(env_name) = get_current_env(conn)? {
                let env_scope = SettingScope::Env(env_name);
                let env_value = get_setting_scoped(conn, &env_scope, key)?;
                if env_value.is_some() {
                    return Ok(env_value);
                }
            }
            get_setting_scoped(conn, &SettingScope::Global, key)
        }
    }
}

pub fn get_current_env(conn: &Connection) -> Result<Option<String>> {
    get_setting_scoped(conn, &SettingScope::Global, "env.current")
}

pub fn set_current_env(conn: &Connection, name: &str) -> Result<()> {
    set_setting_scoped(conn, &SettingScope::Global, "env.current", name)
}

pub fn clear_current_env(conn: &Connection) -> Result<()> {
    clear_setting_scoped(conn, &SettingScope::Global, "env.current")
}

pub fn list_env_names(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT scope FROM settings WHERE scope LIKE 'env:%' ORDER BY scope",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut envs = Vec::new();
    for row in rows {
        let scope = row?;
        if let Some(name) = scope.strip_prefix("env:") {
            envs.push(name.to_string());
        }
    }
    Ok(envs)
}

pub fn list_settings_scoped(
    conn: &Connection,
    scope: &SettingScope,
) -> Result<Vec<(String, String)>> {
    let mut stmt =
        conn.prepare("SELECT key, value FROM settings WHERE scope = ?1 ORDER BY key")?;
    let rows = stmt.query_map(params![scope.as_db()], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut settings = Vec::new();
    for row in rows {
        settings.push(row?);
    }
    Ok(settings)
}

pub fn get_client_overrides(conn: &Connection) -> Result<Option<ClientOverrides>> {
    let raw = get_setting(conn, "client_overrides")?;
    match raw {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

pub fn set_client_overrides(conn: &Connection, overrides: &ClientOverrides) -> Result<()> {
    let json = serde_json::to_string(overrides)?;
    set_setting(conn, "client_overrides", &json)
}

pub fn clear_client_overrides(conn: &Connection) -> Result<()> {
    clear_setting_scoped(conn, &SettingScope::Global, "client_overrides")
}

pub fn get_ssh_auth_order(conn: &Connection) -> Result<Option<String>> {
    get_setting(conn, "ssh_auth_order")
}

pub fn set_ssh_auth_order(conn: &Connection, order: &str) -> Result<()> {
    set_setting(conn, "ssh_auth_order", order)
}

pub fn clear_ssh_auth_order(conn: &Connection) -> Result<()> {
    clear_setting_scoped(conn, &SettingScope::Global, "ssh_auth_order")
}

pub fn get_allow_insecure_transfers(conn: &Connection) -> Result<bool> {
    let raw = get_setting(conn, "allow_insecure_transfers")?;
    match raw {
        Some(value) => Ok(value.trim().eq_ignore_ascii_case("true")),
        None => Ok(false),
    }
}

pub fn set_allow_insecure_transfers(conn: &Connection, allow: bool) -> Result<()> {
    set_setting(
        conn,
        "allow_insecure_transfers",
        if allow { "true" } else { "false" },
    )
}

pub fn clear_allow_insecure_transfers(conn: &Connection) -> Result<()> {
    clear_setting_scoped(conn, &SettingScope::Global, "allow_insecure_transfers")
}
