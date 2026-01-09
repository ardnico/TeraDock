use std::fmt;

use common::id::{generate_id, normalize_id, validate_id};
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

use crate::doctor::ClientOverrides;
use crate::error::{CoreError, Result};
use crate::util::now_ms;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileType {
    Ssh,
    Telnet,
    Serial,
}

impl ProfileType {
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "ssh" => Ok(Self::Ssh),
            "telnet" => Ok(Self::Telnet),
            "serial" => Ok(Self::Serial),
            _ => Err(CoreError::NotFound(value.to_string())),
        }
    }
}

impl fmt::Display for ProfileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileType::Ssh => write!(f, "ssh"),
            ProfileType::Telnet => write!(f, "telnet"),
            ProfileType::Serial => write!(f, "serial"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DangerLevel {
    Normal,
    High,
    Critical,
}

impl Default for DangerLevel {
    fn default() -> Self {
        DangerLevel::Normal
    }
}

impl fmt::Display for DangerLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DangerLevel::Normal => write!(f, "normal"),
            DangerLevel::High => write!(f, "high"),
            DangerLevel::Critical => write!(f, "critical"),
        }
    }
}

impl DangerLevel {
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "normal" => Ok(DangerLevel::Normal),
            "high" => Ok(DangerLevel::High),
            "critical" => Ok(DangerLevel::Critical),
            _ => Err(CoreError::NotFound(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub profile_id: String,
    pub name: String,
    pub profile_type: ProfileType,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub danger_level: DangerLevel,
    pub group: Option<String>,
    pub tags: Vec<String>,
    pub note: Option<String>,
    pub client_overrides: Option<ClientOverrides>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_used_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewProfile {
    pub profile_id: Option<String>,
    pub name: String,
    pub profile_type: ProfileType,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub danger_level: DangerLevel,
    pub group: Option<String>,
    pub tags: Vec<String>,
    pub note: Option<String>,
    pub client_overrides: Option<ClientOverrides>,
}

impl NewProfile {
    pub fn normalize_id(&self) -> Result<String> {
        let id = match &self.profile_id {
            Some(explicit) => normalize_id(explicit),
            None => generate_id("p_"),
        };
        validate_id(&id).map_err(CoreError::InvalidId)?;
        Ok(id)
    }
}

#[derive(Default, Debug, Clone)]
pub struct ProfileFilters {
    pub group: Option<String>,
    pub tags: Vec<String>,
    pub profile_type: Option<ProfileType>,
    pub danger: Option<DangerLevel>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateProfile {
    pub name: Option<String>,
    pub profile_type: Option<ProfileType>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub danger_level: Option<DangerLevel>,
    pub group: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
    pub note: Option<Option<String>>,
    pub client_overrides: Option<Option<ClientOverrides>>,
}

pub struct ProfileStore {
    conn: Connection,
}

impl ProfileStore {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn insert(&self, input: NewProfile) -> Result<Profile> {
        let profile_id = input.normalize_id()?;
        let now = now_ms();
        let tags_json = serde_json::to_string(&input.tags)?;
        let overrides_json = input
            .client_overrides
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()?;

        self.conn.execute(
            r#"
            INSERT INTO profiles (
                profile_id, name, type, host, port, user, danger_level, "group",
                tags_json, note, client_overrides_json, created_at, updated_at, last_used_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, NULL)
            "#,
            params![
                profile_id,
                input.name,
                input.profile_type.to_string(),
                input.host,
                input.port as i64,
                input.user,
                input.danger_level.to_string(),
                input.group,
                tags_json,
                input.note,
                overrides_json,
                now,
                now
            ],
        )?;

        self.get(&profile_id)?
            .ok_or_else(|| CoreError::NotFound(profile_id))
    }

    pub fn get(&self, profile_id: &str) -> Result<Option<Profile>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT profile_id, name, type, host, port, user, danger_level, "group",
                   tags_json, note, client_overrides_json, created_at, updated_at, last_used_at
            FROM profiles
            WHERE profile_id = ?1
            "#,
        )?;
        let mut rows = stmt.query([profile_id])?;
        let result = match rows.next()? {
            Some(row) => Some(deserialize_profile(row)?),
            None => None,
        };
        Ok(result)
    }

    pub fn list(&self) -> Result<Vec<Profile>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT profile_id, name, type, host, port, user, danger_level, "group",
                   tags_json, note, client_overrides_json, created_at, updated_at, last_used_at
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

    pub fn list_filtered(&self, filters: &ProfileFilters) -> Result<Vec<Profile>> {
        let mut profiles = self.list()?;
        if let Some(group) = &filters.group {
            profiles.retain(|p| match &p.group {
                Some(g) => g.eq_ignore_ascii_case(group),
                None => false,
            });
        }
        if let Some(ptype) = filters.profile_type {
            profiles.retain(|p| p.profile_type == ptype);
        }
        if let Some(danger) = filters.danger {
            profiles.retain(|p| p.danger_level == danger);
        }
        if !filters.tags.is_empty() {
            profiles.retain(|p| {
                filters.tags.iter().all(|tag| {
                    p.tags
                        .iter()
                        .any(|t| t.eq_ignore_ascii_case(tag.as_str()))
                })
            });
        }
        if let Some(query) = &filters.query {
            let q = query.to_lowercase();
            profiles.retain(|p| {
                p.name.to_lowercase().contains(&q)
                    || p.host.to_lowercase().contains(&q)
                    || p.user.to_lowercase().contains(&q)
                    || p.profile_id.to_lowercase().contains(&q)
            });
        }
        Ok(profiles)
    }

    pub fn update(&self, profile_id: &str, changes: UpdateProfile) -> Result<Profile> {
        let mut profile = self
            .get(profile_id)?
            .ok_or_else(|| CoreError::NotFound(profile_id.to_string()))?;

        if let Some(name) = changes.name {
            profile.name = name;
        }
        if let Some(ptype) = changes.profile_type {
            profile.profile_type = ptype;
        }
        if let Some(host) = changes.host {
            profile.host = host;
        }
        if let Some(port) = changes.port {
            profile.port = port;
        }
        if let Some(user) = changes.user {
            profile.user = user;
        }
        if let Some(danger) = changes.danger_level {
            profile.danger_level = danger;
        }
        if let Some(group) = changes.group {
            profile.group = group;
        }
        if let Some(tags) = changes.tags {
            profile.tags = tags;
        }
        if let Some(note) = changes.note {
            profile.note = note;
        }
        if let Some(overrides) = changes.client_overrides {
            profile.client_overrides = overrides;
        }

        profile.updated_at = now_ms();
        let tags_json = serde_json::to_string(&profile.tags)?;
        let overrides_json = profile
            .client_overrides
            .as_ref()
            .map(|v| serde_json::to_string(v))
            .transpose()?;

        self.conn.execute(
            r#"
            UPDATE profiles
            SET name = ?1,
                type = ?2,
                host = ?3,
                port = ?4,
                user = ?5,
                danger_level = ?6,
                "group" = ?7,
                tags_json = ?8,
                note = ?9,
                client_overrides_json = ?10,
                updated_at = ?11
            WHERE profile_id = ?12
            "#,
            params![
                profile.name,
                profile.profile_type.to_string(),
                profile.host,
                profile.port as i64,
                profile.user,
                profile.danger_level.to_string(),
                profile.group,
                tags_json,
                profile.note,
                overrides_json,
                profile.updated_at,
                profile.profile_id,
            ],
        )?;

        self.get(profile_id)?
            .ok_or_else(|| CoreError::NotFound(profile_id.to_string()))
    }

    pub fn delete(&self, profile_id: &str) -> Result<bool> {
        let count = self
            .conn
            .execute("DELETE FROM profiles WHERE profile_id = ?1", [profile_id])?;
        Ok(count > 0)
    }

    pub fn touch_last_used(&self, profile_id: &str) -> Result<()> {
        let now = now_ms();
        self.conn.execute(
            "UPDATE profiles SET last_used_at = ?1 WHERE profile_id = ?2",
            params![now, profile_id],
        )?;
        Ok(())
    }
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
        client_overrides: match overrides {
            Some(raw) => Some(serde_json::from_str(&raw)?),
            None => None,
        },
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        last_used_at: row.get("last_used_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_in_memory;

    fn base_profile() -> NewProfile {
        NewProfile {
            profile_id: Some("p_test123".to_string()),
            name: "Test Profile".to_string(),
            profile_type: ProfileType::Ssh,
            host: "example.com".to_string(),
            port: 22,
            user: "alice".to_string(),
            danger_level: DangerLevel::Normal,
            group: None,
            tags: vec!["default".into()],
            note: Some("note".into()),
            client_overrides: None,
        }
    }

    #[test]
    fn rejects_invalid_id() {
        let conn = init_in_memory().unwrap();
        let store = ProfileStore::new(conn);
        let mut profile = base_profile();
        profile.profile_id = Some("Bad*Id".into());
        let err = store.insert(profile).unwrap_err();
        assert!(matches!(err, CoreError::InvalidId(_)));
    }

    #[test]
    fn inserts_and_reads_back() {
        let conn = init_in_memory().unwrap();
        let store = ProfileStore::new(conn);
        let created = store.insert(base_profile()).unwrap();
        assert_eq!(created.profile_id, "p_test123");
        let fetched = store.get("p_test123").unwrap().expect("profile exists");
        assert_eq!(fetched.name, "Test Profile");
        assert_eq!(fetched.profile_type.to_string(), "ssh");
        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].profile_id, "p_test123");
    }

    #[test]
    fn delete_profile() {
        let conn = init_in_memory().unwrap();
        let store = ProfileStore::new(conn);
        store.insert(base_profile()).unwrap();
        assert!(store.delete("p_test123").unwrap());
        assert!(!store.delete("p_test123").unwrap());
        assert!(store.get("p_test123").unwrap().is_none());
    }

    #[test]
    fn touch_last_used_sets_timestamp() {
        let conn = init_in_memory().unwrap();
        let store = ProfileStore::new(conn);
        store.insert(base_profile()).unwrap();
        store.touch_last_used("p_test123").unwrap();
        let fetched = store.get("p_test123").unwrap().expect("profile exists");
        assert!(fetched.last_used_at.is_some());
    }

    #[test]
    fn filters_by_group_tag_type_danger_and_query() {
        let conn = init_in_memory().unwrap();
        let store = ProfileStore::new(conn);
        let mut p1 = base_profile();
        p1.group = Some("grp".into());
        p1.tags = vec!["alpha".into(), "beta".into()];
        p1.danger_level = DangerLevel::High;
        store.insert(p1).unwrap();

        let mut p2 = base_profile();
        p2.profile_id = Some("p_other".into());
        p2.name = "Second".into();
        p2.profile_type = ProfileType::Telnet;
        p2.host = "other.example.com".into();
        p2.group = Some("other".into());
        p2.tags = vec!["gamma".into()];
        store.insert(p2).unwrap();

        let filters = ProfileFilters {
            group: Some("grp".into()),
            tags: vec!["alpha".into()],
            profile_type: Some(ProfileType::Ssh),
            danger: Some(DangerLevel::High),
            query: Some("test".into()),
        };
        let filtered = store.list_filtered(&filters).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].profile_id, "p_test123");
    }

    #[test]
    fn updates_profile_fields() {
        let conn = init_in_memory().unwrap();
        let store = ProfileStore::new(conn);
        store.insert(base_profile()).unwrap();
        let updated = store
            .update(
                "p_test123",
                UpdateProfile {
                    name: Some("Renamed".into()),
                    host: Some("new.example.com".into()),
                    port: Some(2022),
                    danger_level: Some(DangerLevel::High),
                    group: Some(Some("newgroup".into())),
                    tags: Some(vec!["x".into(), "y".into()]),
                    note: Some(None),
                    client_overrides: Some(None),
                    ..Default::default()
                },
            )
            .unwrap();

        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.host, "new.example.com");
        assert_eq!(updated.port, 2022);
        assert_eq!(updated.danger_level, DangerLevel::High);
        assert_eq!(updated.group.as_deref(), Some("newgroup"));
        assert_eq!(updated.tags, vec!["x", "y"]);
        assert!(updated.note.is_none());
        assert!(updated.client_overrides.is_none());
        assert!(updated.updated_at >= updated.created_at);
    }
}
