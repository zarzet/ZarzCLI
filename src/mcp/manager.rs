use anyhow::{anyhow, Result};
use std::collections::HashMap;
use tokio::sync::RwLock;

use super::client::McpClient;
use super::config::{McpConfig, McpServerConfig};
use super::types::{McpTool, McpResource, McpPrompt};

/// Manages multiple MCP clients
pub struct McpManager {
    clients: RwLock<HashMap<String, McpClient>>,
}

impl McpManager {
    /// Create a new MCP manager
    pub fn new() -> Self {
        Self {
            clients: RwLock::new(HashMap::new()),
        }
    }

    /// Load and start all configured MCP servers
    pub async fn load_from_config(&self) -> Result<()> {
        let config = McpConfig::load()?;

        for (name, server_config) in config.mcp_servers {
            if let Err(e) = self.start_server(name.clone(), server_config).await {
                eprintln!("Warning: Failed to start MCP server '{}': {}", name, e);
            }
        }

        Ok(())
    }

    /// Start a specific MCP server
    pub async fn start_server(&self, name: String, config: McpServerConfig) -> Result<()> {
        let mut client = McpClient::new(name.clone(), config);
        client.start().await?;

        let mut clients = self.clients.write().await;
        clients.insert(name, client);

        Ok(())
    }

    /// Stop a specific MCP server
    #[allow(dead_code)]
    pub async fn stop_server(&self, name: &str) -> Result<()> {
        let mut clients = self.clients.write().await;

        if let Some(mut client) = clients.remove(name) {
            client.stop().await?;
            Ok(())
        } else {
            Err(anyhow!("Server '{}' not found", name))
        }
    }

    /// List all running servers
    pub async fn list_servers(&self) -> Vec<String> {
        let clients = self.clients.read().await;
        clients.keys().cloned().collect()
    }

    /// Get all available tools from all servers
    pub async fn get_all_tools(&self) -> Result<HashMap<String, Vec<McpTool>>> {
        let clients = self.clients.read().await;
        let mut all_tools = HashMap::new();

        for (name, client) in clients.iter() {
            match client.list_tools().await {
                Ok(tools) => {
                    all_tools.insert(name.clone(), tools);
                }
                Err(e) => {
                    eprintln!("Warning: Failed to get tools from '{}': {}", name, e);
                }
            }
        }

        Ok(all_tools)
    }

    /// Get all available resources from all servers
    #[allow(dead_code)]
    pub async fn get_all_resources(&self) -> Result<HashMap<String, Vec<McpResource>>> {
        let clients = self.clients.read().await;
        let mut all_resources = HashMap::new();

        for (name, client) in clients.iter() {
            match client.list_resources().await {
                Ok(resources) => {
                    all_resources.insert(name.clone(), resources);
                }
                Err(e) => {
                    eprintln!("Warning: Failed to get resources from '{}': {}", name, e);
                }
            }
        }

        Ok(all_resources)
    }

    /// Get all available prompts from all servers
    #[allow(dead_code)]
    pub async fn get_all_prompts(&self) -> Result<HashMap<String, Vec<McpPrompt>>> {
        let clients = self.clients.read().await;
        let mut all_prompts = HashMap::new();

        for (name, client) in clients.iter() {
            match client.list_prompts().await {
                Ok(prompts) => {
                    all_prompts.insert(name.clone(), prompts);
                }
                Err(e) => {
                    eprintln!("Warning: Failed to get prompts from '{}': {}", name, e);
                }
            }
        }

        Ok(all_prompts)
    }

    /// Call a tool on a specific server
    #[allow(dead_code)]
    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: String,
        arguments: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<super::types::CallToolResult> {
        let clients = self.clients.read().await;

        let client = clients.get(server_name)
            .ok_or_else(|| anyhow!("Server '{}' not found", server_name))?;

        client.call_tool(tool_name, arguments).await
    }

    /// Get server info for a specific server
    pub async fn get_server_info(&self, name: &str) -> Option<String> {
        let clients = self.clients.read().await;
        clients.get(name).and_then(|c| {
            c.server_info().map(|info| {
                format!("{} v{}", info.name, info.version)
            })
        })
    }

    /// Check if any servers are running
    pub async fn has_servers(&self) -> bool {
        let clients = self.clients.read().await;
        !clients.is_empty()
    }

    /// Stop all servers
    pub async fn stop_all(&self) -> Result<()> {
        let mut clients = self.clients.write().await;

        for (name, mut client) in clients.drain() {
            if let Err(e) = client.stop().await {
                eprintln!("Warning: Failed to stop server '{}': {}", name, e);
            }
        }

        Ok(())
    }
}

impl Default for McpManager {
    fn default() -> Self {
        Self::new()
    }
}
