use rusqlite::{Connection, Row};
use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::parser::{ParserDefinition, ParserSpec, ParserType};

#[derive(Debug, Clone)]
pub struct CmdSet {
    pub cmdset_id: String,
    pub name: String,
    pub vars: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepOnError {
    Stop,
    Continue,
}

impl StepOnError {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "stop" => Ok(Self::Stop),
            "continue" => Ok(Self::Continue),
            _ => Err(CoreError::InvalidCommandSpec(format!(
                "unknown on_error: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CmdStep {
    pub id: i64,
    pub cmdset_id: String,
    pub ord: i64,
    pub cmd: String,
    pub timeout_ms: Option<u64>,
    pub on_error: StepOnError,
    pub parser_spec: ParserSpec,
}

pub struct CmdSetStore {
    conn: Connection,
}

impl CmdSetStore {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn get(&self, cmdset_id: &str) -> Result<Option<CmdSet>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT cmdset_id, name, vars_json
            FROM cmdsets
            WHERE cmdset_id = ?1
            "#,
        )?;
        let mut rows = stmt.query([cmdset_id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(deserialize_cmdset(row)?))
    }

    pub fn list(&self) -> Result<Vec<CmdSet>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT cmdset_id, name, vars_json
            FROM cmdsets
            ORDER BY cmdset_id ASC
            "#,
        )?;
        let mut rows = stmt.query([])?;
        let mut sets = Vec::new();
        while let Some(row) = rows.next()? {
            sets.push(deserialize_cmdset(row)?);
        }
        Ok(sets)
    }

    pub fn list_steps(&self, cmdset_id: &str) -> Result<Vec<CmdStep>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, cmdset_id, ord, cmd, timeout_ms, on_error, parser_spec
            FROM cmdsteps
            WHERE cmdset_id = ?1
            ORDER BY ord ASC
            "#,
        )?;
        let mut rows = stmt.query([cmdset_id])?;
        let mut steps = Vec::new();
        while let Some(row) = rows.next()? {
            steps.push(deserialize_cmdstep(row)?);
        }
        Ok(steps)
    }

    pub fn get_parser(&self, parser_id: &str) -> Result<Option<ParserDefinition>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT parser_id, type, definition
            FROM parsers
            WHERE parser_id = ?1
            "#,
        )?;
        let mut rows = stmt.query([parser_id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let parser_type: String = row.get("type")?;
        Ok(Some(ParserDefinition {
            parser_id: row.get("parser_id")?,
            parser_type: ParserType::parse(&parser_type)?,
            definition: row.get("definition")?,
        }))
    }
}

fn deserialize_cmdset(row: &Row<'_>) -> Result<CmdSet> {
    let vars_json: Option<String> = row.get("vars_json")?;
    Ok(CmdSet {
        cmdset_id: row.get("cmdset_id")?,
        name: row.get("name")?,
        vars: vars_json
            .map(|raw| serde_json::from_str(&raw))
            .transpose()?,
    })
}

fn deserialize_cmdstep(row: &Row<'_>) -> Result<CmdStep> {
    let on_error: String = row.get("on_error")?;
    let parser_spec: String = row.get("parser_spec")?;
    let timeout_ms: Option<i64> = row.get("timeout_ms")?;
    Ok(CmdStep {
        id: row.get("id")?,
        cmdset_id: row.get("cmdset_id")?,
        ord: row.get("ord")?,
        cmd: row.get("cmd")?,
        timeout_ms: timeout_ms.map(|value| value as u64),
        on_error: StepOnError::parse(&on_error)?,
        parser_spec: ParserSpec::parse(&parser_spec)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_in_memory;

    #[test]
    fn loads_cmdset_steps_and_parser() {
        let conn = init_in_memory().unwrap();
        conn.execute(
            "INSERT INTO cmdsets (cmdset_id, name, vars_json) VALUES (?1, ?2, ?3)",
            ["c_main", "Main", "{\"env\":\"prod\"}"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO parsers (parser_id, type, definition) VALUES (?1, ?2, ?3)",
            ["r_status", "regex", "(?P<code>\\d+)"],
        )
        .unwrap();
        conn.execute(
            r#"
            INSERT INTO cmdsteps (id, cmdset_id, ord, cmd, timeout_ms, on_error, parser_spec)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            [
                "1",
                "c_main",
                "1",
                "echo 200",
                "5000",
                "stop",
                "regex:r_status",
            ],
        )
        .unwrap();

        let store = CmdSetStore::new(conn);
        let cmdset = store.get("c_main").unwrap().expect("cmdset");
        assert_eq!(cmdset.name, "Main");
        assert_eq!(
            cmdset.vars,
            Some(serde_json::json!({ "env": "prod" }))
        );

        let steps = store.list_steps("c_main").unwrap();
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].cmd, "echo 200");
        assert_eq!(steps[0].timeout_ms, Some(5000));
        assert_eq!(steps[0].on_error, StepOnError::Stop);

        let parser = store.get_parser("r_status").unwrap().expect("parser");
        assert_eq!(parser.parser_id, "r_status");
        assert_eq!(parser.parser_type, ParserType::Regex);
    }
}
