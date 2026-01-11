use std::collections::HashSet;

use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::settings::SettingScopeKind;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingValueType {
    Boolean,
    String,
    Json,
    CsvList,
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingSchema {
    pub key: &'static str,
    pub description: &'static str,
    pub value_type: SettingValueType,
    #[serde(skip_serializing_if = "slice_is_empty")]
    pub allowed_values: &'static [&'static str],
    #[serde(skip_serializing_if = "slice_is_empty")]
    pub examples: &'static [&'static str],
    pub dangerous: bool,
    pub scopes: &'static [SettingScopeKind],
}

struct SettingDefinition {
    schema: SettingSchema,
    validator: fn(&str) -> Result<String>,
}

impl SettingDefinition {
    fn validate(&self, raw: &str) -> Result<String> {
        (self.validator)(raw)
    }

    fn supports_scope(&self, scope: SettingScopeKind) -> bool {
        self.schema.scopes.contains(&scope)
    }
}

const SSH_AUTH_ALLOWED: [&str; 3] = ["agent", "keys", "password"];
const ALLOW_INSECURE_EXAMPLES: [&str; 2] = ["true", "false"];
const SSH_AUTH_EXAMPLES: [&str; 2] = ["agent,keys,password", "keys,password"];
const CLIENT_OVERRIDE_EXAMPLES: [&str; 1] = [r#"{"ssh":"/usr/bin/ssh","scp":"/usr/bin/scp"}"#];

static SETTINGS: &[SettingDefinition] = &[
    SettingDefinition {
        schema: SettingSchema {
            key: "allow_insecure_transfers",
            description: "Allow insecure transfers when using FTP clients.",
            value_type: SettingValueType::Boolean,
            allowed_values: &ALLOW_INSECURE_EXAMPLES,
            examples: &ALLOW_INSECURE_EXAMPLES,
            dangerous: true,
            scopes: &[
                SettingScopeKind::Global,
                SettingScopeKind::Env,
                SettingScopeKind::Profile,
            ],
        },
        validator: validate_bool,
    },
    SettingDefinition {
        schema: SettingSchema {
            key: "ssh_auth_order",
            description: "Preferred SSH auth order (comma-delimited: agent,keys,password).",
            value_type: SettingValueType::CsvList,
            allowed_values: &SSH_AUTH_ALLOWED,
            examples: &SSH_AUTH_EXAMPLES,
            dangerous: false,
            scopes: &[
                SettingScopeKind::Global,
                SettingScopeKind::Env,
                SettingScopeKind::Profile,
            ],
        },
        validator: validate_ssh_auth_order,
    },
    SettingDefinition {
        schema: SettingSchema {
            key: "client_overrides",
            description: "JSON overrides for client paths (ssh/scp/sftp/ftp/telnet).",
            value_type: SettingValueType::Json,
            allowed_values: &[],
            examples: &CLIENT_OVERRIDE_EXAMPLES,
            dangerous: false,
            scopes: &[SettingScopeKind::Global],
        },
        validator: validate_json,
    },
];

pub fn list_keys() -> Vec<&'static str> {
    SETTINGS.iter().map(|def| def.schema.key).collect()
}

pub fn list_schemas() -> Vec<SettingSchema> {
    SETTINGS.iter().map(|def| def.schema.clone()).collect()
}

pub fn schema_for_key(key: &str) -> Option<&'static SettingSchema> {
    SETTINGS
        .iter()
        .find(|def| def.schema.key == key)
        .map(|def| &def.schema)
}

pub fn validate_setting_value(key: &str, raw: &str) -> Result<String> {
    let definition = SETTINGS
        .iter()
        .find(|def| def.schema.key == key)
        .ok_or_else(|| CoreError::InvalidSetting(format!("unknown setting '{key}'")))?;
    definition.validate(raw)
}

pub fn scope_supported(key: &str, scope: SettingScopeKind) -> Result<bool> {
    let definition = SETTINGS
        .iter()
        .find(|def| def.schema.key == key)
        .ok_or_else(|| CoreError::InvalidSetting(format!("unknown setting '{key}'")))?;
    Ok(definition.supports_scope(scope))
}

fn validate_bool(raw: &str) -> Result<String> {
    let normalized = match raw.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "y" => "true",
        "false" | "0" | "no" | "n" => "false",
        _ => {
            return Err(CoreError::InvalidSetting(format!(
                "invalid boolean value '{raw}'"
            )))
        }
    };
    Ok(normalized.to_string())
}

fn validate_ssh_auth_order(raw: &str) -> Result<String> {
    if raw.trim().is_empty() {
        return Err(CoreError::InvalidSetting(
            "auth order cannot be empty".to_string(),
        ));
    }
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for item in raw.split(',') {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !SSH_AUTH_ALLOWED.contains(&trimmed) {
            return Err(CoreError::InvalidSetting(format!(
                "unknown auth method '{trimmed}'"
            )));
        }
        if !seen.insert(trimmed) {
            return Err(CoreError::InvalidSetting(format!(
                "auth order contains duplicate '{trimmed}'"
            )));
        }
        normalized.push(trimmed);
    }
    if normalized.is_empty() {
        return Err(CoreError::InvalidSetting(
            "auth order cannot be empty".to_string(),
        ));
    }
    Ok(normalized.join(","))
}

fn validate_json(raw: &str) -> Result<String> {
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|err| CoreError::InvalidSetting(format!("invalid json: {err}")))?;
    Ok(serde_json::to_string(&value)?)
}

fn slice_is_empty<T>(slice: &[T]) -> bool {
    slice.is_empty()
}
