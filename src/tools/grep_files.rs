use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::json;
use serde_json::Value;

use super::{ToolExecutionContext, ToolExecutionOutput, ToolHandler};

#[derive(Deserialize)]
struct GrepArgs {
    path: String,
    pattern: String,
}

pub struct GrepFilesHandler;

impl ToolHandler for GrepFilesHandler {
    fn name(&self) -> &'static str {
        "grep_files"
    }

    fn description(&self) -> &'static str {
        "Search for a text pattern inside a single file (simple substring match)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File to search (relative to working directory)."
                },
                "pattern": {
                    "type": "string",
                    "description": "Substring to search for (case-sensitive)."
                }
            },
            "required": ["path", "pattern"]
        })
    }

    fn handle(
        &self,
        ctx: ToolExecutionContext<'_>,
        args: &Value,
    ) -> Result<ToolExecutionOutput> {
        let parsed: GrepArgs = serde_json::from_value(args.clone()).map_err(|err| {
            anyhow!("invalid grep_files arguments: {}", err)
        })?;

        let full_path = resolve_path(ctx.working_directory, &parsed.path);
        if !full_path.exists() {
            return Err(anyhow!("File '{}' does not exist", parsed.path));
        }
        if full_path.is_dir() {
            return Err(anyhow!("'{}' is a directory; grep_files expects a file", parsed.path));
        }

        let content = fs::read_to_string(&full_path)
            .map_err(|err| anyhow!("Failed to read '{}': {}", parsed.path, err))?;

        let mut matches = String::new();
        for (idx, line) in content.lines().enumerate() {
            if line.contains(&parsed.pattern) {
                matches.push_str(&format!("{:>6} | {}\n", idx + 1, line));
            }
        }

        let output = if matches.is_empty() {
            format!("No matches for '{}' in {}", parsed.pattern, parsed.path)
        } else {
            matches
        };

        Ok(ToolExecutionOutput {
            content: output,
            success: true,
        })
    }
}

fn resolve_path(base: &std::path::Path, user_path: &str) -> PathBuf {
    let user = PathBuf::from(user_path);
    if user.is_absolute() {
        user
    } else {
        base.join(user)
    }
}
