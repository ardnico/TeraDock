use rusqlite::Connection;

use crate::doctor::ClientOverrides;
use crate::error::Result;

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query([key])?;
    let value = match rows.next()? {
        Some(row) => Some(row.get(0)?),
        None => None,
    };
    Ok(value)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
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
    conn.execute("DELETE FROM settings WHERE key = 'client_overrides'", [])?;
    Ok(())
}

pub fn get_ssh_auth_order(conn: &Connection) -> Result<Option<String>> {
    get_setting(conn, "ssh_auth_order")
}

pub fn set_ssh_auth_order(conn: &Connection, order: &str) -> Result<()> {
    set_setting(conn, "ssh_auth_order", order)
}

pub fn clear_ssh_auth_order(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM settings WHERE key = 'ssh_auth_order'", [])?;
    Ok(())
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
    conn.execute(
        "DELETE FROM settings WHERE key = 'allow_insecure_transfers'",
        [],
    )?;
    Ok(())
}
