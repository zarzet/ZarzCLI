use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::json;
use serde_json::Value;

use super::{ToolExecutionContext, ToolExecutionOutput, ToolHandler};

#[derive(Deserialize)]
struct ListDirArgs {
    #[serde(default = "default_path")]
    path: String,
    #[serde(default = "default_depth")]
    depth: usize,
}

fn default_path() -> String {
    ".".to_string()
}

fn default_depth() -> usize {
    1
}

pub struct ListDirHandler;

impl ToolHandler for ListDirHandler {
    fn name(&self) -> &'static str {
        "list_dir"
    }

    fn description(&self) -> &'static str {
        "List the contents of a directory with an optional depth."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory to list (relative to working directory)."
                },
                "depth": {
                    "type": "integer",
                    "description": "Optional recursion depth (defaults to 1)."
                }
            }
        })
    }

    fn handle(
        &self,
        ctx: ToolExecutionContext<'_>,
        args: &Value,
    ) -> Result<ToolExecutionOutput> {
        let parsed: ListDirArgs = serde_json::from_value(args.clone()).map_err(|err| {
            anyhow!("invalid list_dir arguments: {}", err)
        })?;

        let target = resolve_path(ctx.working_directory, &parsed.path);
        if !target.exists() {
            return Err(anyhow!("Path '{}' does not exist", parsed.path));
        }
        if !target.is_dir() {
            return Err(anyhow!("'{}' is not a directory", parsed.path));
        }

        let summary = summarize_listing(&target, parsed.depth.max(1))?;

        Ok(ToolExecutionOutput {
            content: summary,
            success: true,
        })
    }
}

fn resolve_path(base: &Path, user_path: &str) -> PathBuf {
    let user = PathBuf::from(user_path);
    if user.is_absolute() {
        user
    } else {
        base.join(user)
    }
}

fn summarize_listing(path: &Path, depth: usize) -> Result<String> {
    let entries = collect_entries(path)?;
    if entries.is_empty() {
        return Ok("(directory is empty)".to_string());
    }

    let mut summary = Vec::new();
    let files: Vec<_> = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File)
        .collect();
    let dirs: Vec<_> = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::Dir)
        .collect();

    if !files.is_empty() {
        summary.push(format!("{} file(s): {}", files.len(), format_preview(&files, 6)));
    }
    if !dirs.is_empty() {
        summary.push(format!("{} dir(s): {}", dirs.len(), format_preview(&dirs, 6)));
    }

    if depth > 1 && !dirs.is_empty() {
        summary.push(format!("(depth {}: subdirectories scanned)", depth));
    }

    Ok(summary.join("\n"))
}

fn collect_entries(path: &Path) -> Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = if metadata.is_dir() {
            EntryKind::Dir
        } else {
            EntryKind::File
        };
        entries.push(Entry { name, kind });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

fn format_preview(entries: &[&Entry], limit: usize) -> String {
    let mut names: Vec<String> = entries.iter().take(limit).map(|entry| entry.name.clone()).collect();
    if entries.len() > limit {
        names.push(format!("... +{}", entries.len() - limit));
    }
    names.join(", ")
}

#[derive(Clone)]
struct Entry {
    name: String,
    kind: EntryKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    File,
    Dir,
}
