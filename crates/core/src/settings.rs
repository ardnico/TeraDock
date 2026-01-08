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
