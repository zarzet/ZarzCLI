use anyhow::{anyhow, Result};
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::unified_exec::UnifiedExecManager;

mod read_file;
mod list_dir;
mod grep_files;
mod apply_patch;
mod unified_exec;

pub use apply_patch::ApplyPatchHandler;
pub use grep_files::GrepFilesHandler;
pub use list_dir::ListDirHandler;
pub use read_file::ReadFileHandler;
pub use unified_exec::{ExecCommandHandler, WriteStdinHandler};

pub struct ToolExecutionContext<'a> {
    pub working_directory: &'a Path,
    pub unified_exec: Option<&'a UnifiedExecManager>,
}

pub struct ToolExecutionOutput {
    pub content: String,
    pub success: bool,
}

pub trait ToolHandler: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn input_schema(&self) -> Value;
    fn handle(
        &self,
        ctx: ToolExecutionContext<'_>,
        args: &Value,
    ) -> Result<ToolExecutionOutput>;
}

pub struct ToolRegistry {
    handlers: HashMap<&'static str, Arc<dyn ToolHandler>>,
}

impl ToolRegistry {
    pub fn new(unified_exec: Arc<UnifiedExecManager>) -> Self {
        let mut registry = Self {
            handlers: HashMap::new(),
        };
        registry.register(ReadFileHandler);
        registry.register(ListDirHandler);
        registry.register(GrepFilesHandler);
        registry.register(ApplyPatchHandler);
        registry.register(ExecCommandHandler::new(unified_exec.clone()));
        registry.register(WriteStdinHandler::new(unified_exec));
        registry
    }

    fn register<H: ToolHandler + 'static>(&mut self, handler: H) {
        let name = handler.name();
        if self.handlers.insert(name, Arc::new(handler)).is_some() {
            eprintln!("Warning: overwriting handler for tool {name}");
        }
    }

    pub fn specs(&self) -> Vec<Value> {
        self.handlers
            .values()
            .map(|handler| {
                json!({
                    "name": handler.name(),
                    "description": handler.description(),
                    "input_schema": handler.input_schema(),
                })
            })
            .collect()
    }

    pub fn execute(
        &self,
        tool_name: &str,
        ctx: ToolExecutionContext<'_>,
        args: &Value,
    ) -> Result<ToolExecutionOutput> {
        let handler = self
            .handlers
            .get(tool_name)
            .ok_or_else(|| anyhow!("Unknown tool: {}", tool_name))?;
        handler.handle(ctx, args)
    }
}
