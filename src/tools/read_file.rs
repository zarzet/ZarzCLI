use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::json;
use serde_json::Value;

use super::{ToolExecutionContext, ToolExecutionOutput, ToolHandler};

#[derive(Deserialize)]
struct ReadFileArgs {
    path: String,
    #[serde(default)]
    start_line: Option<usize>,
    #[serde(default)]
    end_line: Option<usize>,
}

pub struct ReadFileHandler;

impl ToolHandler for ReadFileHandler {
    fn name(&self) -> &'static str {
        "read_file"
    }

    fn description(&self) -> &'static str {
        "Read the contents of a file. Accepts optional start/end line numbers."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file (relative to the working directory)."
                },
                "start_line": {
                    "type": "integer",
                    "description": "Optional starting line number (1-based)."
                },
                "end_line": {
                    "type": "integer",
                    "description": "Optional ending line number (1-based, inclusive)."
                }
            },
            "required": ["path"]
        })
    }

    fn handle(
        &self,
        ctx: ToolExecutionContext<'_>,
        args: &Value,
    ) -> Result<ToolExecutionOutput> {
        let parsed: ReadFileArgs = serde_json::from_value(args.clone()).map_err(|err| {
            anyhow!("invalid read_file arguments: {}", err)
        })?;

        let ReadFileArgs {
            path,
            start_line,
            end_line,
        } = parsed;

        let full_path = resolve_path(ctx.working_directory, &path);
        if !full_path.exists() {
            return Err(anyhow!("File '{}' does not exist", path));
        }
        if full_path.is_dir() {
            return Err(anyhow!("'{}' is a directory", path));
        }

        let content = fs::read_to_string(&full_path)
            .map_err(|err| anyhow!("Failed to read '{}': {}", path, err))?;

        let filtered = slice_content(&content, start_line, end_line);
        Ok(ToolExecutionOutput {
            content: filtered,
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

fn slice_content(content: &str, start_line: Option<usize>, end_line: Option<usize>) -> String {
    let total_lines = content.lines().count();
    let start = start_line.unwrap_or(1).max(1).min(total_lines.max(1));
    let mut end = end_line.unwrap_or(total_lines).max(start);
    end = end.min(total_lines);

    if start_line.is_none() && end_line.is_none() {
        return truncate(content);
    }

    let mut buf = String::new();
    for (idx, line) in content.lines().enumerate() {
        let line_no = idx + 1;
        if (start..=end).contains(&line_no) {
            buf.push_str(&format!("{:>6} | {}\n", line_no, line));
        }
    }

    if buf.is_empty() {
        format!("No content in requested range ({}-{})", start, end)
    } else {
        truncate(&buf)
    }
}

fn truncate(text: &str) -> String {
    const MAX_CHARS: usize = 16_000;
    if text.len() <= MAX_CHARS {
        text.to_string()
    } else {
        format!("{}\n... (truncated, {} total chars)", &text[..MAX_CHARS], text.len())
    }
}
