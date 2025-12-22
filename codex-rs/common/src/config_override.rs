//! Support for `-c key=value` overrides shared across Codex CLI tools.
//!
//! This module provides a [`CliConfigOverrides`] struct that can be embedded
//! into a `clap`-derived CLI struct using `#[clap(flatten)]`. Each occurrence
//! of `-c key=value` (or `--config key=value`) will be collected as a raw
//! string. Helper methods are provided to convert the raw strings into
//! key/value pairs as well as to apply them onto a mutable
//! `serde_json::Value` representing the configuration tree.

use clap::ArgAction;
use clap::Parser;
use clap::ValueHint;
use serde::de::Error as SerdeError;
use std::path::Path;
use std::path::PathBuf;
use toml::Value;

/// CLI option that captures arbitrary configuration overrides specified as
/// `-c key=value`. It intentionally keeps both halves **unparsed** so that the
/// calling code can decide how to interpret the right-hand side.
#[derive(Parser, Debug, Default, Clone)]
pub struct CliConfigOverrides {
    /// Override a configuration value that would otherwise be loaded from
    /// `~/.codex/config.toml`. Use a dotted path (`foo.bar.baz`) to override
    /// nested values. The `value` portion is parsed as TOML. If it fails to
    /// parse as TOML, the raw string is used as a literal.
    ///
    /// Examples:
    ///   - `-c model="o3"`
    ///   - `-c 'sandbox_permissions=["disk-full-read-access"]'`
    ///   - `-c shell_environment_policy.inherit=all`
    #[arg(
        short = 'c',
        long = "config",
        value_name = "key=value",
        action = ArgAction::Append,
        global = true,
    )]
    pub raw_overrides: Vec<String>,

    /// Path to a file whose contents are appended to the system prompt for this run.
    #[arg(
        long = "system-prompt-file",
        value_name = "FILE",
        value_hint = ValueHint::FilePath,
        global = true,
    )]
    pub system_prompt_file: Option<PathBuf>,

    /// API token to use for this run (sets CODEX_API_KEY for the process).
    #[arg(long = "api-token", value_name = "TOKEN", global = true)]
    pub api_token: Option<String>,
}

impl CliConfigOverrides {
    /// Parse the raw strings captured from the CLI into a list of `(path,
    /// value)` tuples where `value` is a `serde_json::Value`.
    pub fn parse_overrides(&self) -> Result<Vec<(String, Value)>, String> {
        let mut overrides: Vec<(String, Value)> = self
            .raw_overrides
            .iter()
            .map(|s| {
                // Only split on the *first* '=' so values are free to contain
                // the character.
                let mut parts = s.splitn(2, '=');
                let key = match parts.next() {
                    Some(k) => k.trim(),
                    None => return Err("Override missing key".to_string()),
                };
                let value_str = parts
                    .next()
                    .ok_or_else(|| format!("Invalid override (missing '='): {s}"))?
                    .trim();

                if key.is_empty() {
                    return Err(format!("Empty key in override: {s}"));
                }

                // Attempt to parse as TOML. If that fails, treat it as a raw
                // string. This allows convenient usage such as
                // `-c model=o3` without the quotes.
                let value: Value = match parse_toml_value(value_str) {
                    Ok(v) => v,
                    Err(_) => {
                        // Strip leading/trailing quotes if present
                        let trimmed = value_str.trim().trim_matches(|c| c == '"' || c == '\'');
                        Value::String(trimmed.to_string())
                    }
                };

                Ok((key.to_string(), value))
            })
            .collect::<Result<Vec<_>, _>>()?;

        if let Some(path) = &self.system_prompt_file {
            let contents = read_system_prompt_file(path)?;
            overrides.push(("instructions".to_string(), Value::String(contents)));
        }

        Ok(overrides)
    }

    /// Apply all parsed overrides onto `target`. Intermediate objects will be
    /// created as necessary. Values located at the destination path will be
    /// replaced.
    pub fn apply_on_value(&self, target: &mut Value) -> Result<(), String> {
        let overrides = self.parse_overrides()?;
        for (path, value) in overrides {
            apply_single_override(target, &path, value);
        }
        Ok(())
    }
}

/// Apply a single override onto `root`, creating intermediate objects as
/// necessary.
fn apply_single_override(root: &mut Value, path: &str, value: Value) {
    use toml::value::Table;

    let parts: Vec<&str> = path.split('.').collect();
    let mut current = root;

    for (i, part) in parts.iter().enumerate() {
        let is_last = i == parts.len() - 1;

        if is_last {
            match current {
                Value::Table(tbl) => {
                    tbl.insert((*part).to_string(), value);
                }
                _ => {
                    let mut tbl = Table::new();
                    tbl.insert((*part).to_string(), value);
                    *current = Value::Table(tbl);
                }
            }
            return;
        }

        // Traverse or create intermediate table.
        match current {
            Value::Table(tbl) => {
                current = tbl
                    .entry((*part).to_string())
                    .or_insert_with(|| Value::Table(Table::new()));
            }
            _ => {
                *current = Value::Table(Table::new());
                if let Value::Table(tbl) = current {
                    current = tbl
                        .entry((*part).to_string())
                        .or_insert_with(|| Value::Table(Table::new()));
                }
            }
        }
    }
}

fn read_system_prompt_file(path: &Path) -> Result<String, String> {
    let resolved_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let contents = std::fs::read_to_string(&resolved_path).map_err(|e| {
        format!(
            "failed to read system prompt file {}: {e}",
            resolved_path.display()
        )
    })?;
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        return Err(format!(
            "system prompt file is empty: {}",
            resolved_path.display()
        ));
    }
    Ok(trimmed.to_string())
}

fn parse_toml_value(raw: &str) -> Result<Value, toml::de::Error> {
    let wrapped = format!("_x_ = {raw}");
    let table: toml::Table = toml::from_str(&wrapped)?;
    table
        .get("_x_")
        .cloned()
        .ok_or_else(|| SerdeError::custom("missing sentinel key"))
}

#[cfg(all(test, feature = "cli"))]
mod tests {
    use super::*;
    use std::fs;
    use std::time::SystemTime;

    #[test]
    fn parses_basic_scalar() {
        let v = parse_toml_value("42").expect("parse");
        assert_eq!(v.as_integer(), Some(42));
    }

    #[test]
    fn parses_bool() {
        let true_literal = parse_toml_value("true").expect("parse");
        assert_eq!(true_literal.as_bool(), Some(true));

        let false_literal = parse_toml_value("false").expect("parse");
        assert_eq!(false_literal.as_bool(), Some(false));
    }

    #[test]
    fn fails_on_unquoted_string() {
        assert!(parse_toml_value("hello").is_err());
    }

    #[test]
    fn parses_array() {
        let v = parse_toml_value("[1, 2, 3]").expect("parse");
        let arr = v.as_array().expect("array");
        assert_eq!(arr.len(), 3);
    }

    #[test]
    fn parses_inline_table() {
        let v = parse_toml_value("{a = 1, b = 2}").expect("parse");
        let tbl = v.as_table().expect("table");
        assert_eq!(tbl.get("a").unwrap().as_integer(), Some(1));
        assert_eq!(tbl.get("b").unwrap().as_integer(), Some(2));
    }

    #[test]
    fn parse_overrides_includes_system_prompt_file() {
        let mut path = std::env::temp_dir();
        let unique = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        path.push(format!("codex-system-prompt-{unique}.txt"));
        fs::write(&path, "bible").expect("write");

        let overrides = CliConfigOverrides {
            raw_overrides: vec![],
            system_prompt_file: Some(path.clone()),
            api_token: None,
        }
        .parse_overrides()
        .expect("overrides");

        fs::remove_file(path).ok();

        assert_eq!(
            overrides,
            vec![(
                "instructions".to_string(),
                Value::String("bible".to_string())
            )]
        );
    }
}
