use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

use super::config::McpServerConfig;
use super::types::*;

/// MCP Client for communicating with MCP servers
pub struct McpClient {
    #[allow(dead_code)]
    name: String,
    config: McpServerConfig,
    process: Option<Mutex<Child>>,
    stdin: Option<Mutex<ChildStdin>>,
    stdout: Option<Mutex<BufReader<ChildStdout>>>,
    request_id: AtomicU64,
    initialized: bool,
    server_info: Option<ServerInfo>,
    capabilities: Option<ServerCapabilities>,
}

impl McpClient {
    /// Create a new MCP client
    pub fn new(name: String, config: McpServerConfig) -> Self {
        Self {
            name,
            config,
            process: None,
            stdin: None,
            stdout: None,
            request_id: AtomicU64::new(1),
            initialized: false,
            server_info: None,
            capabilities: None,
        }
    }

    /// Start the MCP server process (for STDIO servers)
    pub async fn start(&mut self) -> Result<()> {
        match &self.config {
            McpServerConfig::Stdio { command, args, env } => {
                // On Windows, wrap in cmd /c for proper PATH resolution
                let mut cmd = if cfg!(target_os = "windows") {
                    let mut win_cmd = Command::new("cmd");
                    win_cmd.arg("/c");
                    win_cmd.arg(command);
                    if let Some(args) = args {
                        win_cmd.args(args);
                    }
                    win_cmd
                } else {
                    let mut unix_cmd = Command::new(command);
                    if let Some(args) = args {
                        unix_cmd.args(args);
                    }
                    unix_cmd
                };

                if let Some(env_vars) = env {
                    cmd.envs(env_vars);
                }

                cmd.stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::inherit());

                let mut child = cmd.spawn()
                    .with_context(|| format!("Failed to start MCP server: {}", command))?;

                let stdin = child.stdin.take()
                    .context("Failed to open stdin")?;
                let stdout = child.stdout.take()
                    .context("Failed to open stdout")?;

                self.stdin = Some(Mutex::new(stdin));
                self.stdout = Some(Mutex::new(BufReader::new(stdout)));
                self.process = Some(Mutex::new(child));

                self.initialize().await?;

                Ok(())
            }
            _ => Err(anyhow!("Only STDIO servers are currently supported")),
        }
    }

    /// Initialize MCP connection
    async fn initialize(&mut self) -> Result<()> {
        let params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {},
                "resources": {},
                "prompts": {}
            },
            "clientInfo": {
                "name": "ZarzCLI",
                "version": "0.1.0"
            }
        });

        let response = self.send_request("initialize", Some(params)).await?;

        let result: InitializeResult = serde_json::from_value(response)
            .context("Failed to parse initialize response")?;

        self.server_info = Some(result.server_info);
        self.capabilities = Some(result.capabilities);
        self.initialized = true;

        self.send_notification("notifications/initialized", None).await?;

        Ok(())
    }

    /// List available tools from the MCP server
    pub async fn list_tools(&self) -> Result<Vec<McpTool>> {
        if !self.initialized {
            return Err(anyhow!("MCP client not initialized"));
        }

        let response = self.send_request("tools/list", None).await?;
        let result: ToolsListResult = serde_json::from_value(response)
            .context("Failed to parse tools/list response")?;

        Ok(result.tools)
    }

    /// Call a tool on the MCP server
    #[allow(dead_code)]
    pub async fn call_tool(&self, name: String, arguments: Option<HashMap<String, Value>>) -> Result<CallToolResult> {
        if !self.initialized {
            return Err(anyhow!("MCP client not initialized"));
        }

        let params = CallToolParams { name, arguments };
        let params_value = serde_json::to_value(params)?;

        let response = self.send_request("tools/call", Some(params_value)).await?;
        let result: CallToolResult = serde_json::from_value(response)
            .context("Failed to parse tools/call response")?;

        Ok(result)
    }

    /// List available resources from the MCP server
    #[allow(dead_code)]
    pub async fn list_resources(&self) -> Result<Vec<McpResource>> {
        if !self.initialized {
            return Err(anyhow!("MCP client not initialized"));
        }

        let response = self.send_request("resources/list", None).await?;
        let result: ResourcesListResult = serde_json::from_value(response)
            .context("Failed to parse resources/list response")?;

        Ok(result.resources)
    }

    /// List available prompts from the MCP server
    #[allow(dead_code)]
    pub async fn list_prompts(&self) -> Result<Vec<McpPrompt>> {
        if !self.initialized {
            return Err(anyhow!("MCP client not initialized"));
        }

        let response = self.send_request("prompts/list", None).await?;
        let result: PromptsListResult = serde_json::from_value(response)
            .context("Failed to parse prompts/list response")?;

        Ok(result.prompts)
    }

    /// Send a JSON-RPC request and wait for response
    async fn send_request(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.to_string(),
            params,
        };

        let request_json = serde_json::to_string(&request)?;

        if let Some(stdin) = &self.stdin {
            let mut stdin = stdin.lock().await;
            stdin.write_all(request_json.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
        } else {
            return Err(anyhow!("STDIN not available"));
        }

        if let Some(stdout) = &self.stdout {
            let mut stdout = stdout.lock().await;

            loop {
                let mut line = String::new();
                let bytes_read = stdout.read_line(&mut line).await?;

                if bytes_read == 0 {
                    return Err(anyhow!("MCP server closed the connection unexpectedly"));
                }

                if line.trim().is_empty() {
                    continue;
                }

                let value: Value = serde_json::from_str(&line)
                    .with_context(|| format!("Failed to parse JSON-RPC message: {}", line.trim()))?;

                // Notifications do not include an `id`, so we skip them (surface useful info when present)
                if value.get("id").is_none() {
                    if let Some(method) = value.get("method").and_then(|m| m.as_str()) {
                        if method == "notifications/message" {
                            if let Some(msg) = value
                                .get("params")
                                .and_then(|p| p.get("data"))
                                .and_then(|d| d.get("message"))
                                .and_then(|m| m.as_str())
                            {
                                eprintln!("MCP notification: {}", msg);
                            }
                        }
                    }
                    continue;
                }

                let response: JsonRpcResponse = serde_json::from_value(value)
                    .with_context(|| format!("Failed to parse JSON-RPC response: {}", line.trim()))?;

                if let Some(error) = response.error {
                    return Err(anyhow!("MCP error: {} (code: {})", error.message, error.code));
                }

                if let Some(result) = response.result {
                    return Ok(result);
                } else {
                    return Err(anyhow!("No result in response"));
                }
            }
        } else {
            Err(anyhow!("STDOUT not available"))
        }
    }

    /// Send a JSON-RPC notification (no response expected)
    async fn send_notification(&self, method: &str, params: Option<Value>) -> Result<()> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let notification_json = serde_json::to_string(&notification)?;

        if let Some(stdin) = &self.stdin {
            let mut stdin = stdin.lock().await;
            stdin.write_all(notification_json.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
        }

        Ok(())
    }

    /// Get server info
    pub fn server_info(&self) -> Option<&ServerInfo> {
        self.server_info.as_ref()
    }

    /// Get server capabilities
    #[allow(dead_code)]
    pub fn capabilities(&self) -> Option<&ServerCapabilities> {
        self.capabilities.as_ref()
    }

    /// Check if client is initialized
    #[allow(dead_code)]
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get server name
    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Stop the MCP server process
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(process) = &self.process {
            let mut process = process.lock().await;
            process.kill().await?;
        }
        Ok(())
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        // Note: We can't await in drop, so we just kill the process synchronously
        if let Some(process) = &self.process {
            if let Ok(mut process) = process.try_lock() {
                let _ = process.start_kill();
            }
        }
    }
}
