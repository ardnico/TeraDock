use crate::error::Result;
use crate::util::now_ms;
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::Value;

pub const SSH_SESSION_OP: &str = "ssh_session";

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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecentSshSession {
    pub profile_id: String,
    pub name: String,
    pub user: String,
    pub host: String,
    pub port: u16,
    pub profile_type: String,
    pub danger_level: String,
    pub last_connected_at: i64,
    pub last_ok: bool,
    pub last_exit_code: Option<i32>,
    pub client_used: Option<String>,
    pub duration_ms: Option<i64>,
}

pub fn log_operation(conn: &Connection, entry: OpLogEntry) -> Result<()> {
    let meta = entry
        .meta_json
        .as_ref()
        .map(serde_json::to_string)
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

pub fn recent_ssh_sessions(conn: &Connection, limit: usize) -> Result<Vec<RecentSshSession>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT p.profile_id,
               p.name,
               p.user,
               p.host,
               p.port,
               p.type,
               p.danger_level,
               l.ts,
               l.ok,
               l.exit_code,
               l.client_used,
               l.duration_ms
        FROM op_logs l
        JOIN profiles p ON p.profile_id = l.profile_id
        WHERE l.op = ?1
          AND l.id = (
              SELECT l2.id
              FROM op_logs l2
              WHERE l2.op = ?1
                AND l2.profile_id = l.profile_id
              ORDER BY l2.ts DESC, l2.id DESC
              LIMIT 1
          )
        ORDER BY l.ts DESC, l.id DESC
        LIMIT ?2
        "#,
    )?;
    let mut rows = stmt.query(params![SSH_SESSION_OP, limit as i64])?;
    let mut sessions = Vec::new();
    while let Some(row) = rows.next()? {
        sessions.push(RecentSshSession {
            profile_id: row.get("profile_id")?,
            name: row.get("name")?,
            user: row.get("user")?,
            host: row.get("host")?,
            port: row.get::<_, i64>("port")? as u16,
            profile_type: row.get("type")?,
            danger_level: row.get("danger_level")?,
            last_connected_at: row.get("ts")?,
            last_ok: row.get::<_, i64>("ok")? != 0,
            last_exit_code: row.get("exit_code")?,
            client_used: row.get("client_used")?,
            duration_ms: row.get("duration_ms")?,
        });
    }
    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_in_memory;
    use crate::profile::{DangerLevel, NewProfile, ProfileStore, ProfileType};

    #[test]
    fn logs_operation_row() {
        let conn = init_in_memory().unwrap();
        let store = ProfileStore::new(conn);
        let profile = store
            .insert(NewProfile {
                profile_id: Some("p_abc".into()),
                name: "sample".into(),
                profile_type: ProfileType::Ssh,
                host: "localhost".into(),
                port: 22,
                user: "root".into(),
                danger_level: DangerLevel::Normal,
                group: None,
                tags: vec![],
                note: None,
                initial_send: None,
                client_overrides: None,
            })
            .unwrap();
        let entry = OpLogEntry {
            op: "connect".into(),
            profile_id: Some(profile.profile_id),
            client_used: Some("ssh".into()),
            ok: true,
            exit_code: Some(0),
            duration_ms: Some(100),
            meta_json: None,
        };
        log_operation(store.conn(), entry).unwrap();
        let count: i64 = store
            .conn()
            .query_row("SELECT COUNT(*) FROM op_logs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn recent_ssh_sessions_returns_latest_per_profile() {
        let conn = init_in_memory().unwrap();
        let store = ProfileStore::new(conn);
        for (profile_id, name, danger) in [
            ("p_abc", "alpha", DangerLevel::Normal),
            ("p_def", "delta", DangerLevel::Critical),
        ] {
            store
                .insert(NewProfile {
                    profile_id: Some(profile_id.into()),
                    name: name.into(),
                    profile_type: ProfileType::Ssh,
                    host: format!("{name}.example.com"),
                    port: 22,
                    user: "root".into(),
                    danger_level: danger,
                    group: None,
                    tags: vec![],
                    note: None,
                    initial_send: None,
                    client_overrides: None,
                })
                .unwrap();
        }
        store
            .conn()
            .execute(
                "INSERT INTO op_logs (ts, op, profile_id, client_used, ok, exit_code, duration_ms, meta_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
                params![1000_i64, SSH_SESSION_OP, "p_abc", "ssh", 1_i64, 0_i64, 10_i64],
            )
            .unwrap();
        store
            .conn()
            .execute(
                "INSERT INTO op_logs (ts, op, profile_id, client_used, ok, exit_code, duration_ms, meta_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
                params![2000_i64, "run", "p_def", "ssh", 1_i64, 0_i64, 11_i64],
            )
            .unwrap();
        store
            .conn()
            .execute(
                "INSERT INTO op_logs (ts, op, profile_id, client_used, ok, exit_code, duration_ms, meta_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
                params![3000_i64, SSH_SESSION_OP, "p_def", "ssh", 0_i64, 255_i64, 12_i64],
            )
            .unwrap();
        store
            .conn()
            .execute(
                "INSERT INTO op_logs (ts, op, profile_id, client_used, ok, exit_code, duration_ms, meta_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
                params![4000_i64, SSH_SESSION_OP, "p_abc", "ssh", 0_i64, Option::<i32>::None, 13_i64],
            )
            .unwrap();

        let recent = recent_ssh_sessions(store.conn(), 10).unwrap();

        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].profile_id, "p_abc");
        assert_eq!(recent[0].last_connected_at, 4000);
        assert_eq!(recent[0].last_exit_code, None);
        assert_eq!(recent[1].profile_id, "p_def");
        assert_eq!(recent[1].last_exit_code, Some(255));
    }

    #[test]
    fn recent_ssh_sessions_honors_limit() {
        let conn = init_in_memory().unwrap();
        let store = ProfileStore::new(conn);
        for profile_id in ["p_one", "p_two"] {
            store
                .insert(NewProfile {
                    profile_id: Some(profile_id.into()),
                    name: profile_id.into(),
                    profile_type: ProfileType::Ssh,
                    host: "example.com".into(),
                    port: 22,
                    user: "root".into(),
                    danger_level: DangerLevel::Normal,
                    group: None,
                    tags: vec![],
                    note: None,
                    initial_send: None,
                    client_overrides: None,
                })
                .unwrap();
            store
                .conn()
                .execute(
                    "INSERT INTO op_logs (ts, op, profile_id, client_used, ok, exit_code, duration_ms, meta_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)",
                    params![1000_i64, SSH_SESSION_OP, profile_id, "ssh", 1_i64, 0_i64, 10_i64],
                )
                .unwrap();
        }

        let recent = recent_ssh_sessions(store.conn(), 1).unwrap();

        assert_eq!(recent.len(), 1);
    }
}
