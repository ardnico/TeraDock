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
