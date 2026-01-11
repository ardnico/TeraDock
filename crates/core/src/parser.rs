use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParserSpec {
    Raw,
    Json,
    Regex(String),
}

impl ParserSpec {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "raw" => Ok(Self::Raw),
            "json" => Ok(Self::Json),
            _ => {
                if let Some(rest) = value.strip_prefix("regex:") {
                    if rest.is_empty() {
                        return Err(CoreError::InvalidCommandSpec(
                            "regex parser spec missing id".into(),
                        ));
                    }
                    Ok(Self::Regex(rest.to_string()))
                } else {
                    Err(CoreError::InvalidCommandSpec(format!(
                        "unknown parser spec: {value}"
                    )))
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParserType {
    Regex,
}

impl ParserType {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "regex" => Ok(ParserType::Regex),
            _ => Err(CoreError::InvalidCommandSpec(format!(
                "unknown parser type: {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParserDefinition {
    pub parser_id: String,
    pub parser_type: ParserType,
    pub definition: String,
}

pub fn parse_output(
    spec: &ParserSpec,
    stdout: &str,
    parser: Option<&ParserDefinition>,
) -> Result<serde_json::Value> {
    match spec {
        ParserSpec::Raw => Ok(serde_json::json!({})),
        ParserSpec::Json => match serde_json::from_str(stdout) {
            Ok(value) => Ok(value),
            Err(_) => Ok(serde_json::json!({})),
        },
        ParserSpec::Regex(parser_id) => {
            let definition = parser
                .filter(|p| p.parser_id == *parser_id)
                .ok_or_else(|| CoreError::ParserNotFound(parser_id.clone()))?;
            if definition.parser_type != ParserType::Regex {
                return Err(CoreError::InvalidCommandSpec(format!(
                    "parser {parser_id} is not regex"
                )));
            }
            parse_regex_output(&definition.definition, stdout)
        }
    }
}

fn parse_regex_output(pattern: &str, stdout: &str) -> Result<serde_json::Value> {
    let regex = Regex::new(pattern).map_err(|err| CoreError::Regex(err.to_string()))?;
    let mut matches = Vec::new();
    for caps in regex.captures_iter(stdout) {
        let mut entry = serde_json::Map::new();
        for (idx, name) in regex.capture_names().enumerate() {
            if idx == 0 {
                continue;
            }
            if let Some(value) = caps.get(idx) {
                let key = name
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| idx.to_string());
                entry.insert(key, serde_json::Value::String(value.as_str().to_string()));
            }
        }
        matches.push(serde_json::Value::Object(entry));
    }
    Ok(serde_json::Value::Array(matches))
}
