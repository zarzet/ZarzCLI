use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use serde_json::json;
use serde_json::Value;

use super::{ToolExecutionContext, ToolExecutionOutput, ToolHandler};

#[derive(Deserialize)]
struct ApplyPatchArgs {
    patch: String,
}

pub struct ApplyPatchHandler;

impl ToolHandler for ApplyPatchHandler {
    fn name(&self) -> &'static str {
        "apply_patch"
    }

    fn description(&self) -> &'static str {
        "Apply a Zarz-style multi-file patch (*** Begin Patch format)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "patch": {
                    "type": "string",
                    "description": "Patch in Zarz apply_patch format."
                }
            },
            "required": ["patch"]
        })
    }

    fn handle(
        &self,
        ctx: ToolExecutionContext<'_>,
        args: &Value,
    ) -> Result<ToolExecutionOutput> {
        let parsed: ApplyPatchArgs = serde_json::from_value(args.clone()).map_err(|err| {
            anyhow!("invalid apply_patch arguments: {}", err)
        })?;

        let blocks = parse_patch_blocks(&parsed.patch)?;
        if blocks.is_empty() {
            return Err(anyhow!("No patch blocks were provided"));
        }

        let mut summary = Vec::new();

        for block in blocks {
            match block {
                PatchBlock::Add { path, lines } => {
                    let resolved = resolve_safe_path(ctx.working_directory, &path)?;
                    ensure_parent_dir(&resolved)?;
                    let mut content = String::new();
                    for line in lines {
                        if let Some(rest) = line.strip_prefix('+') {
                            content.push_str(rest);
                        } else {
                            content.push_str(&line);
                        }
                        content.push('\n');
                    }
                    fs::write(&resolved, content)
                        .with_context(|| format!("Failed to write {}", path))?;
                    summary.push(format!("Added {}", path));
                }
                PatchBlock::Delete { path } => {
                    let resolved = resolve_safe_path(ctx.working_directory, &path)?;
                    if resolved.exists() {
                        fs::remove_file(&resolved)
                            .with_context(|| format!("Failed to delete {}", path))?;
                        summary.push(format!("Deleted {}", path));
                    } else {
                        summary.push(format!("Skipped deleting {} (file missing)", path));
                    }
                }
                PatchBlock::Update { path, hunks } => {
                    let resolved = resolve_safe_path(ctx.working_directory, &path)?;
                    if !resolved.exists() {
                        return Err(anyhow!("Cannot update '{}': file does not exist", path));
                    }
                    apply_update_patch(&resolved, &hunks)
                        .with_context(|| format!("Failed to apply patch to {}", path))?;
                    summary.push(format!("Updated {}", path));
                }
            }
        }

        Ok(ToolExecutionOutput {
            content: summary.join("\n"),
            success: true,
        })
    }
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directories for {}", path.display()))?;
    }
    Ok(())
}

fn resolve_safe_path(base: &Path, user_path: &str) -> Result<PathBuf> {
    let relative = Path::new(user_path);
    if relative.is_absolute() {
        return Err(anyhow!("Absolute paths are not allowed in apply_patch"));
    }

    for component in relative.components() {
        if matches!(component, Component::ParentDir | Component::Prefix(_)) {
            return Err(anyhow!("Parent directory components are not allowed in apply_patch paths"));
        }
    }

    Ok(base.join(relative))
}

fn apply_update_patch(path: &Path, hunks: &[Hunk]) -> Result<()> {
    let original = fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let original_lines: Vec<String> = if original.is_empty() {
        Vec::new()
    } else {
        let mut lines: Vec<String> = original.split('\n').map(|line| line.to_string()).collect();
        if original.ends_with('\n') {
            lines.pop();
        }
        lines
    };

    let mut result = Vec::new();
    let mut orig_index: usize = 1;

    for hunk in hunks {
        let target_start = hunk.start_old.max(1);
        while orig_index < target_start && orig_index <= original_lines.len() {
            result.push(original_lines[orig_index - 1].clone());
            orig_index += 1;
        }

        for line in &hunk.lines {
            match line.kind {
                LineKind::Context => {
                    let current = original_lines
                        .get(orig_index - 1)
                        .ok_or_else(|| anyhow!("Patch context exceeds file length"))?;
                    if current != &line.text {
                        return Err(anyhow!(
                            "Context mismatch while applying patch: expected '{}', found '{}'",
                            line.text,
                            current
                        ));
                    }
                    result.push(current.clone());
                    orig_index += 1;
                }
                LineKind::Removal => {
                    let current = original_lines
                        .get(orig_index - 1)
                        .ok_or_else(|| anyhow!("Patch removal exceeds file length"))?;
                    if current != &line.text {
                        return Err(anyhow!(
                            "Removal mismatch while applying patch: expected '{}', found '{}'",
                            line.text,
                            current
                        ));
                    }
                    orig_index += 1;
                }
                LineKind::Addition => {
                    result.push(line.text.clone());
                }
            }
        }
    }

    while orig_index <= original_lines.len() {
        result.push(original_lines[orig_index - 1].clone());
        orig_index += 1;
    }

    let mut new_text = result.join("\n");
    if !result.is_empty() {
        new_text.push('\n');
    }

    fs::write(path, new_text)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

fn parse_patch_blocks(input: &str) -> Result<Vec<PatchBlock>> {
    let mut blocks = Vec::new();
    let mut lines = input.lines();

    while let Some(line) = lines.next() {
        if line.trim() != "*** Begin Patch" {
            continue;
        }

        let header = lines
            .next()
            .ok_or_else(|| anyhow!("Patch block missing header line"))?;
        let (kind, path) = parse_patch_header(header)?;

        let mut block_lines = Vec::new();
        while let Some(next_line) = lines.next() {
            if next_line.trim() == "*** End Patch" {
                break;
            }
            block_lines.push(next_line.to_string());
        }

        match kind {
            PatchKind::Add => blocks.push(PatchBlock::Add { path, lines: block_lines }),
            PatchKind::Delete => blocks.push(PatchBlock::Delete { path }),
            PatchKind::Update => {
                let hunks = parse_hunks(&block_lines)?;
                blocks.push(PatchBlock::Update { path, hunks });
            }
        }
    }

    Ok(blocks)
}

fn parse_patch_header(header: &str) -> Result<(PatchKind, String)> {
    if let Some(rest) = header.trim().strip_prefix("*** Add File: ") {
        Ok((PatchKind::Add, rest.trim().to_string()))
    } else if let Some(rest) = header.trim().strip_prefix("*** Delete File: ") {
        Ok((PatchKind::Delete, rest.trim().to_string()))
    } else if let Some(rest) = header.trim().strip_prefix("*** Update File: ") {
        Ok((PatchKind::Update, rest.trim().to_string()))
    } else {
        Err(anyhow!("Unrecognized patch header: {}", header))
    }
}

fn parse_hunks(lines: &[String]) -> Result<Vec<Hunk>> {
    let mut hunks = Vec::new();
    let mut idx = 0;

    while idx < lines.len() {
        let line = &lines[idx];
        if !line.starts_with("@@") {
            idx += 1;
            continue;
        }

        let (start_old, _len_old) = parse_hunk_header(line)?;
        idx += 1;
        let mut hunk_lines = Vec::new();
        while idx < lines.len() {
            let current = &lines[idx];
            if current.starts_with("@@") {
                break;
            }
            if let Some(rest) = current.strip_prefix('+') {
                hunk_lines.push(HunkLine {
                    kind: LineKind::Addition,
                    text: rest.to_string(),
                });
            } else if let Some(rest) = current.strip_prefix('-') {
                hunk_lines.push(HunkLine {
                    kind: LineKind::Removal,
                    text: rest.to_string(),
                });
            } else if let Some(rest) = current.strip_prefix(' ') {
                hunk_lines.push(HunkLine {
                    kind: LineKind::Context,
                    text: rest.to_string(),
                });
            } else {
                hunk_lines.push(HunkLine {
                    kind: LineKind::Context,
                    text: current.to_string(),
                });
            }
            idx += 1;
        }

        hunks.push(Hunk {
            start_old,
            lines: hunk_lines,
        });
    }

    Ok(hunks)
}

fn parse_hunk_header(header: &str) -> Result<(usize, usize)> {
    let tokens: Vec<&str> = header.split_whitespace().collect();
    let old_spec = tokens
        .iter()
        .find(|token| token.starts_with('-'))
        .ok_or_else(|| anyhow!("Malformed hunk header: {}", header))?;
    let trimmed = old_spec.trim_start_matches('-');
    let mut parts = trimmed.split(',');
    let start: usize = parts
        .next()
        .ok_or_else(|| anyhow!("Malformed hunk header: {}", header))?
        .parse()
        .unwrap_or(1);
    let len: usize = parts
        .next()
        .unwrap_or("1")
        .parse()
        .unwrap_or(1);
    Ok((start, len))
}

enum PatchKind {
    Add,
    Delete,
    Update,
}

enum PatchBlock {
    Add { path: String, lines: Vec<String> },
    Delete { path: String },
    Update { path: String, hunks: Vec<Hunk> },
}

struct Hunk {
    start_old: usize,
    lines: Vec<HunkLine>,
}

struct HunkLine {
    kind: LineKind,
    text: String,
}

enum LineKind {
    Context,
    Removal,
    Addition,
}
