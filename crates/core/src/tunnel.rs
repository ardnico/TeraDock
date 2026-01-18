use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use std::process::Command;

use crate::error::{CoreError, Result};
use crate::util::now_ms;
use common::id::generate_id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForwardKind {
    Local,
    Remote,
    Dynamic,
}

impl ForwardKind {
    pub(crate) fn from_str(value: &str) -> Result<Self> {
        match value {
            "local" => Ok(Self::Local),
            "remote" => Ok(Self::Remote),
            "dynamic" => Ok(Self::Dynamic),
            _ => Err(CoreError::InvalidSetting(format!(
                "unknown forward kind: {value}"
            ))),
        }
    }

    pub fn as_flag(self) -> &'static str {
        match self {
            ForwardKind::Local => "-L",
            ForwardKind::Remote => "-R",
            ForwardKind::Dynamic => "-D",
        }
    }
}

impl std::fmt::Display for ForwardKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForwardKind::Local => write!(f, "local"),
            ForwardKind::Remote => write!(f, "remote"),
            ForwardKind::Dynamic => write!(f, "dynamic"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Forward {
    pub id: i64,
    pub profile_id: String,
    pub name: String,
    pub kind: ForwardKind,
    pub listen: String,
    pub dest: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewForward {
    pub profile_id: String,
    pub name: String,
    pub kind: ForwardKind,
    pub listen: String,
    pub dest: Option<String>,
}

pub struct ForwardStore {
    conn: Connection,
}

impl ForwardStore {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, input: NewForward) -> Result<Forward> {
        if input.name.trim().is_empty() {
            return Err(CoreError::InvalidSetting("forward name is required".into()));
        }
        if self.get_by_name(&input.profile_id, &input.name)?.is_some() {
            return Err(CoreError::Conflict(format!(
                "forward name already exists: {}",
                input.name
            )));
        }
        let listen = normalize_listen(&input.listen)?;
        let dest = normalize_dest(input.kind, input.dest)?;
        let dest_value = dest.clone().unwrap_or_default();
        self.conn.execute(
            r#"
            INSERT INTO ssh_forwards (profile_id, name, kind, listen, dest)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                input.profile_id,
                input.name,
                input.kind.to_string(),
                listen,
                dest_value
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.get_by_id(id)
            .and_then(|forward| forward.ok_or_else(|| CoreError::NotFound(id.to_string())))
    }

    pub fn list_for_profile(&self, profile_id: &str) -> Result<Vec<Forward>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, profile_id, name, kind, listen, dest
            FROM ssh_forwards
            WHERE profile_id = ?1
            ORDER BY name ASC
            "#,
        )?;
        let mut rows = stmt.query([profile_id])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(deserialize_forward(row)?);
        }
        Ok(out)
    }

    pub fn get_by_name(&self, profile_id: &str, name: &str) -> Result<Option<Forward>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, profile_id, name, kind, listen, dest
            FROM ssh_forwards
            WHERE profile_id = ?1 AND name = ?2
            "#,
        )?;
        let mut rows = stmt.query(params![profile_id, name])?;
        let result = match rows.next()? {
            Some(row) => Some(deserialize_forward(row)?),
            None => None,
        };
        Ok(result)
    }

    pub fn remove(&self, profile_id: &str, name: &str) -> Result<()> {
        let affected = self.conn.execute(
            "DELETE FROM ssh_forwards WHERE profile_id = ?1 AND name = ?2",
            params![profile_id, name],
        )?;
        if affected == 0 {
            return Err(CoreError::NotFound(format!("forward not found: {name}")));
        }
        Ok(())
    }

    fn get_by_id(&self, id: i64) -> Result<Option<Forward>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, profile_id, name, kind, listen, dest
            FROM ssh_forwards
            WHERE id = ?1
            "#,
        )?;
        let mut rows = stmt.query([id])?;
        let result = match rows.next()? {
            Some(row) => Some(deserialize_forward(row)?),
            None => None,
        };
        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    Tunnel,
}

impl SessionKind {
    pub(crate) fn from_str(value: &str) -> Result<Self> {
        match value {
            "tunnel" => Ok(Self::Tunnel),
            _ => Err(CoreError::InvalidSetting(format!(
                "unknown session kind: {value}"
            ))),
        }
    }
}

impl std::fmt::Display for SessionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionKind::Tunnel => write!(f, "tunnel"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    pub session_id: String,
    pub kind: SessionKind,
    pub profile_id: String,
    pub pid: Option<u32>,
    pub started_at: i64,
    pub forwards: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NewSession {
    pub kind: SessionKind,
    pub profile_id: String,
    pub pid: Option<u32>,
    pub forwards: Vec<String>,
}

pub struct SessionStore {
    conn: Connection,
}

impl SessionStore {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, input: NewSession) -> Result<Session> {
        let session_id = generate_id("s_");
        let now = now_ms();
        let forwards_json = serde_json::to_string(&input.forwards)?;
        self.conn.execute(
            r#"
            INSERT INTO sessions (session_id, kind, profile_id, pid, started_at, forwards_json)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                session_id,
                input.kind.to_string(),
                input.profile_id,
                input.pid.map(|pid| pid as i64),
                now,
                forwards_json
            ],
        )?;
        self.get(&session_id)?
            .ok_or_else(|| CoreError::NotFound(session_id))
    }

    pub fn list(&self) -> Result<Vec<Session>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT session_id, kind, profile_id, pid, started_at, forwards_json
            FROM sessions
            ORDER BY started_at DESC
            "#,
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(deserialize_session(row)?);
        }
        Ok(out)
    }

    pub fn get(&self, session_id: &str) -> Result<Option<Session>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT session_id, kind, profile_id, pid, started_at, forwards_json
            FROM sessions
            WHERE session_id = ?1
            "#,
        )?;
        let mut rows = stmt.query([session_id])?;
        let result = match rows.next()? {
            Some(row) => Some(deserialize_session(row)?),
            None => None,
        };
        Ok(result)
    }

    pub fn remove(&self, session_id: &str) -> Result<()> {
        let affected = self.conn.execute(
            "DELETE FROM sessions WHERE session_id = ?1",
            params![session_id],
        )?;
        if affected == 0 {
            return Err(CoreError::NotFound(format!(
                "session not found: {session_id}"
            )));
        }
        Ok(())
    }

    pub fn cleanup_dead(&self) -> Result<Vec<Session>> {
        let sessions = self.list()?;
        let mut removed = Vec::new();
        for session in sessions {
            let alive = session
                .pid
                .filter(|pid| *pid > 0)
                .map(is_pid_alive)
                .unwrap_or(false);
            if !alive {
                self.conn.execute(
                    "DELETE FROM sessions WHERE session_id = ?1",
                    params![session.session_id],
                )?;
                removed.push(session);
            }
        }
        Ok(removed)
    }
}

fn deserialize_forward(row: &Row<'_>) -> Result<Forward> {
    let dest_raw: String = row.get(5)?;
    let dest = if dest_raw.trim().is_empty() {
        None
    } else {
        Some(dest_raw)
    };
    Ok(Forward {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        name: row.get(2)?,
        kind: ForwardKind::from_str(&row.get::<_, String>(3)?)?,
        listen: row.get(4)?,
        dest,
    })
}

fn deserialize_session(row: &Row<'_>) -> Result<Session> {
    let forwards_json: String = row.get(5)?;
    let forwards = serde_json::from_str(&forwards_json)?;
    let pid: Option<i64> = row.get(3)?;
    Ok(Session {
        session_id: row.get(0)?,
        kind: SessionKind::from_str(&row.get::<_, String>(1)?)?,
        profile_id: row.get(2)?,
        pid: pid.map(|value| value as u32),
        started_at: row.get(4)?,
        forwards,
    })
}

fn normalize_listen(listen: &str) -> Result<String> {
    if listen.chars().all(|c| c.is_ascii_digit()) {
        let port = parse_port(listen)?;
        return Ok(format!("127.0.0.1:{port}"));
    }
    let (host, port) = split_host_port(listen)?;
    Ok(format!("{host}:{port}"))
}

fn normalize_dest(kind: ForwardKind, dest: Option<String>) -> Result<Option<String>> {
    match kind {
        ForwardKind::Dynamic => {
            if dest.as_ref().is_some_and(|value| !value.trim().is_empty()) {
                return Err(CoreError::InvalidSetting(
                    "dynamic forward cannot have a destination".into(),
                ));
            }
            Ok(None)
        }
        ForwardKind::Local | ForwardKind::Remote => {
            let raw = dest
                .as_ref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| CoreError::InvalidSetting("forward dest is required".into()))?;
            let (host, port) = split_host_port(raw)?;
            Ok(Some(format!("{host}:{port}")))
        }
    }
}

fn split_host_port(input: &str) -> Result<(String, u16)> {
    let mut parts = input.rsplitn(2, ':');
    let port = parts
        .next()
        .ok_or_else(|| CoreError::InvalidSetting(format!("invalid host:port: {input}")))?;
    let host = parts
        .next()
        .ok_or_else(|| CoreError::InvalidSetting(format!("invalid host:port: {input}")))?;
    if host.trim().is_empty() {
        return Err(CoreError::InvalidSetting(format!(
            "invalid host:port: {input}"
        )));
    }
    Ok((host.to_string(), parse_port(port)?))
}

fn parse_port(value: &str) -> Result<u16> {
    value
        .trim()
        .parse::<u16>()
        .map_err(|_| CoreError::InvalidSetting(format!("invalid port: {value}")))
}

fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .map(|status| status.success())
            .unwrap_or(true)
    }
    #[cfg(windows)]
    {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}")])
            .output();
        output
            .map(|data| String::from_utf8_lossy(&data.stdout).contains(&pid.to_string()))
            .unwrap_or(true)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_in_memory;
    use crate::profile::{DangerLevel, NewProfile, ProfileStore, ProfileType};

    fn sample_profile(store: &ProfileStore) -> String {
        let profile = store
            .insert(NewProfile {
                profile_id: Some("p_forward".into()),
                name: "forward".into(),
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
        profile.profile_id
    }

    #[test]
    fn normalizes_listen_port_only() {
        let conn = init_in_memory().unwrap();
        let store = ProfileStore::new(conn);
        let profile_id = sample_profile(&store);
        let forward_store = ForwardStore::new(store.conn().try_clone().unwrap());
        let forward = forward_store
            .insert(NewForward {
                profile_id,
                name: "web".into(),
                kind: ForwardKind::Local,
                listen: "8080".into(),
                dest: Some("example.com:80".into()),
            })
            .unwrap();
        assert_eq!(forward.listen, "127.0.0.1:8080");
    }

    #[test]
    fn rejects_dest_without_host() {
        let conn = init_in_memory().unwrap();
        let store = ProfileStore::new(conn);
        let profile_id = sample_profile(&store);
        let forward_store = ForwardStore::new(store.conn().try_clone().unwrap());
        let err = forward_store
            .insert(NewForward {
                profile_id,
                name: "bad".into(),
                kind: ForwardKind::Local,
                listen: "127.0.0.1:8080".into(),
                dest: Some(":80".into()),
            })
            .unwrap_err();
        assert!(matches!(err, CoreError::InvalidSetting(_)));
    }

    #[test]
    fn dynamic_forward_omits_dest() {
        let conn = init_in_memory().unwrap();
        let store = ProfileStore::new(conn);
        let profile_id = sample_profile(&store);
        let forward_store = ForwardStore::new(store.conn().try_clone().unwrap());
        let forward = forward_store
            .insert(NewForward {
                profile_id,
                name: "dyn".into(),
                kind: ForwardKind::Dynamic,
                listen: "127.0.0.1:1080".into(),
                dest: None,
            })
            .unwrap();
        assert!(forward.dest.is_none());
    }
}
