use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::json;
use serde_json::Value;

use super::{ToolExecutionContext, ToolExecutionOutput, ToolHandler};
use crate::unified_exec::{ExecCommandRequest, UnifiedExecManager, WriteStdinRequest};
use tokio::runtime::Handle;

#[derive(Deserialize)]
struct ExecCommandArgs {
    cmd: String,
    #[serde(default = "default_shell")]
    shell: String,
    #[serde(default = "default_login")]
    login: bool,
    #[serde(default)]
    yield_time_ms: Option<u64>,
}

#[derive(Deserialize)]
struct WriteStdinArgs {
    session_id: i32,
    #[serde(default)]
    chars: String,
    #[serde(default)]
    yield_time_ms: Option<u64>,
}

fn default_shell() -> String {
    if cfg!(windows) {
        "cmd".to_string()
    } else {
        "/bin/bash".to_string()
    }
}

fn default_login() -> bool {
    true
}

pub struct ExecCommandHandler {
    manager: std::sync::Arc<UnifiedExecManager>,
}

impl ExecCommandHandler {
    pub fn new(manager: std::sync::Arc<UnifiedExecManager>) -> Self {
        Self { manager }
    }
}

impl ToolHandler for ExecCommandHandler {
    fn name(&self) -> &'static str {
        "exec_command"
    }

    fn description(&self) -> &'static str {
        "Run a shell command inside an interactive session and return recent output."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "cmd": {"type": "string", "description": "Command to run"},
                "shell": {"type": "string", "description": "Shell executable (default /bin/bash or cmd)"},
                "login": {"type": "boolean", "description": "Whether to run via shell -lc"},
                "yield_time_ms": {"type": "integer", "description": "Time in ms to wait for output before returning"}
            },
            "required": ["cmd"]
        })
    }

    fn handle(
        &self,
        _ctx: ToolExecutionContext<'_>,
        args: &Value,
    ) -> Result<ToolExecutionOutput> {
        let parsed: ExecCommandArgs = serde_json::from_value(args.clone()).map_err(|err| {
            anyhow!("invalid exec_command arguments: {}", err)
        })?;

        let response = Handle::current().block_on(self.manager.exec_command(ExecCommandRequest {
            command: parsed.cmd,
            shell: parsed.shell,
            login: parsed.login,
            yield_time_ms: parsed.yield_time_ms,
        }))?;

        Ok(ToolExecutionOutput {
            content: response.format_for_display(),
            success: true,
        })
    }
}

pub struct WriteStdinHandler {
    manager: std::sync::Arc<UnifiedExecManager>,
}

impl WriteStdinHandler {
    pub fn new(manager: std::sync::Arc<UnifiedExecManager>) -> Self {
        Self { manager }
    }
}

impl ToolHandler for WriteStdinHandler {
    fn name(&self) -> &'static str {
        "write_stdin"
    }

    fn description(&self) -> &'static str {
        "Send characters to a running exec_command session."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "session_id": {"type": "integer"},
                "chars": {"type": "string", "description": "Characters to send (\n supported)"},
                "yield_time_ms": {"type": "integer", "description": "Time in ms to wait for output"}
            },
            "required": ["session_id", "chars"]
        })
    }

    fn handle(
        &self,
        _ctx: ToolExecutionContext<'_>,
        args: &Value,
    ) -> Result<ToolExecutionOutput> {
        let parsed: WriteStdinArgs = serde_json::from_value(args.clone()).map_err(|err| {
            anyhow!("invalid write_stdin arguments: {}", err)
        })?;

        let response = Handle::current().block_on(self.manager.write_stdin(WriteStdinRequest {
            session_id: parsed.session_id,
            input: parsed.chars,
            yield_time_ms: parsed.yield_time_ms,
        }))?;

        Ok(ToolExecutionOutput {
            content: response.format_for_display(),
            success: true,
        })
    }
}
