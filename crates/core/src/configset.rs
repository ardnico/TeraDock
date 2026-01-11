use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use common::id::{generate_id, normalize_id, validate_id};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSet {
    pub config_id: String,
    pub name: String,
    pub hooks_cmdset_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigFileWhen {
    Always,
    Missing,
    Changed,
}

impl ConfigFileWhen {
    pub fn parse(value: &str) -> Result<Self> {
        match value.to_lowercase().as_str() {
            "always" => Ok(Self::Always),
            "missing" => Ok(Self::Missing),
            "changed" => Ok(Self::Changed),
            _ => Err(CoreError::InvalidCommandSpec(format!(
                "invalid config when: {value}"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Missing => "missing",
            Self::Changed => "changed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    pub id: i64,
    pub config_id: String,
    pub src: String,
    pub dest: String,
    pub mode: Option<String>,
    pub when: ConfigFileWhen,
}

#[derive(Debug, Clone)]
pub struct NewConfigSet {
    pub config_id: Option<String>,
    pub name: String,
    pub hooks_cmdset_id: Option<String>,
    pub files: Vec<NewConfigFile>,
}

#[derive(Debug, Clone)]
pub struct NewConfigFile {
    pub src: String,
    pub dest: String,
    pub mode: Option<String>,
    pub when: ConfigFileWhen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSetDetails {
    pub config: ConfigSet,
    pub files: Vec<ConfigFile>,
}

pub struct ConfigSetStore {
    conn: Connection,
}

impl ConfigSetStore {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&mut self, input: NewConfigSet) -> Result<ConfigSetDetails> {
        let config_id = match &input.config_id {
            Some(id) => normalize_id(id),
            None => generate_id("cfg_"),
        };
        validate_id(&config_id).map_err(CoreError::InvalidId)?;
        let tx = self.conn.transaction()?;
        tx.execute(
            r#"
            INSERT INTO configsets (config_id, name, hooks_cmdset_id)
            VALUES (?1, ?2, ?3)
            "#,
            params![config_id, input.name, input.hooks_cmdset_id],
        )?;
        for file in input.files {
            tx.execute(
                r#"
                INSERT INTO configfiles (config_id, src, dest, mode, "when")
                VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
                params![
                    config_id,
                    file.src,
                    file.dest,
                    file.mode,
                    file.when.as_str()
                ],
            )?;
        }
        tx.commit()?;
        self.get(&config_id)?
            .ok_or_else(|| CoreError::NotFound(config_id))
    }

    pub fn list(&self) -> Result<Vec<ConfigSet>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT config_id, name, hooks_cmdset_id
            FROM configsets
            ORDER BY config_id ASC
            "#,
        )?;
        let mut rows = stmt.query([])?;
        let mut sets = Vec::new();
        while let Some(row) = rows.next()? {
            sets.push(deserialize_configset(row)?);
        }
        Ok(sets)
    }

    pub fn get(&self, config_id: &str) -> Result<Option<ConfigSetDetails>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT config_id, name, hooks_cmdset_id
            FROM configsets
            WHERE config_id = ?1
            "#,
        )?;
        let mut rows = stmt.query([config_id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let config = deserialize_configset(row)?;
        let mut file_stmt = self.conn.prepare(
            r#"
            SELECT id, config_id, src, dest, mode, "when"
            FROM configfiles
            WHERE config_id = ?1
            ORDER BY id ASC
            "#,
        )?;
        let mut file_rows = file_stmt.query([config_id])?;
        let mut files = Vec::new();
        while let Some(file_row) = file_rows.next()? {
            files.push(deserialize_configfile(file_row)?);
        }
        Ok(Some(ConfigSetDetails { config, files }))
    }

    pub fn delete(&self, config_id: &str) -> Result<bool> {
        let rows = self
            .conn
            .execute("DELETE FROM configsets WHERE config_id = ?1", [config_id])?;
        Ok(rows > 0)
    }
}

fn deserialize_configset(row: &Row<'_>) -> Result<ConfigSet> {
    Ok(ConfigSet {
        config_id: row.get("config_id")?,
        name: row.get("name")?,
        hooks_cmdset_id: row.get("hooks_cmdset_id")?,
    })
}

fn deserialize_configfile(row: &Row<'_>) -> Result<ConfigFile> {
    let when_raw: String = row.get("when")?;
    Ok(ConfigFile {
        id: row.get("id")?,
        config_id: row.get("config_id")?,
        src: row.get("src")?,
        dest: row.get("dest")?,
        mode: row.get("mode")?,
        when: ConfigFileWhen::parse(&when_raw)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_in_memory;

    #[test]
    fn stores_and_loads_configset() {
        let conn = init_in_memory().unwrap();
        let mut store = ConfigSetStore::new(conn);

        let created = store
            .insert(NewConfigSet {
                config_id: Some("cfg_dotfiles".into()),
                name: "Dotfiles".into(),
                hooks_cmdset_id: None,
                files: vec![NewConfigFile {
                    src: "./.bashrc".into(),
                    dest: "~/.bashrc".into(),
                    mode: Some("644".into()),
                    when: ConfigFileWhen::Changed,
                }],
            })
            .unwrap();

        assert_eq!(created.config.config_id, "cfg_dotfiles");
        assert_eq!(created.files.len(), 1);
        assert_eq!(created.files[0].dest, "~/.bashrc");
        assert_eq!(created.files[0].when, ConfigFileWhen::Changed);

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Dotfiles");

        let fetched = store.get("cfg_dotfiles").unwrap().expect("configset");
        assert_eq!(fetched.files.len(), 1);
    }
}
