use crate::error::Result;
use crate::util::now_ms;
use rusqlite::{params, Connection};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct OpLogEntry {
    pub op: String,
    pub profile_id: Option<String>,
    pub client_used: Option<String>,
    pub ok: bool,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
    pub meta_json: Option<Value>,
}

pub fn log_operation(conn: &Connection, entry: OpLogEntry) -> Result<()> {
    let meta = entry
        .meta_json
        .as_ref()
        .map(|v| serde_json::to_string(v))
        .transpose()?;
    conn.execute(
        r#"
        INSERT INTO op_logs (ts, op, profile_id, client_used, ok, exit_code, duration_ms, meta_json)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            now_ms(),
            entry.op,
            entry.profile_id,
            entry.client_used,
            entry.ok as i32,
            entry.exit_code,
            entry.duration_ms,
            meta
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_in_memory;

    #[test]
    fn logs_operation_row() {
        let conn = init_in_memory().unwrap();
        let entry = OpLogEntry {
            op: "connect".into(),
            profile_id: Some("p1".into()),
            client_used: Some("ssh".into()),
            ok: true,
            exit_code: Some(0),
            duration_ms: Some(100),
            meta_json: None,
        };
        log_operation(&conn, entry).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM op_logs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
