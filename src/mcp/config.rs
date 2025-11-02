use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpServerConfig {
    Stdio {
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        args: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        env: Option<HashMap<String, String>>,
    },
    Http {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
    },
    Sse {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
    },
}

/// Root MCP configuration file structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

impl McpConfig {
    /// Get the path to the MCP config file (~/.zarz/mcp.json)
    pub fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not determine home directory")?;
        Ok(home.join(".zarz").join("mcp.json"))
    }

    /// Load MCP config from file, or return default if file doesn't exist
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)
            .context("Failed to read MCP config file")?;

        let config: McpConfig = serde_json::from_str(&content)
            .context("Failed to parse MCP config file")?;

        Ok(config)
    }

    /// Save MCP config to file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create MCP config directory")?;
        }

        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize MCP config")?;

        fs::write(&path, content)
            .context("Failed to write MCP config file")?;

        Ok(())
    }

    /// Add a new MCP server
    pub fn add_server(&mut self, name: String, config: McpServerConfig) {
        self.mcp_servers.insert(name, config);
    }

    /// Remove an MCP server
    pub fn remove_server(&mut self, name: &str) -> bool {
        self.mcp_servers.remove(name).is_some()
    }

    /// Get an MCP server config by name
    pub fn get_server(&self, name: &str) -> Option<&McpServerConfig> {
        self.mcp_servers.get(name)
    }

    /// List all configured servers
    #[allow(dead_code)]
    pub fn list_servers(&self) -> Vec<String> {
        self.mcp_servers.keys().cloned().collect()
    }

    /// Check if config has any servers
    #[allow(dead_code)]
    pub fn has_servers(&self) -> bool {
        !self.mcp_servers.is_empty()
    }
}

impl McpServerConfig {
    /// Create a new STDIO server config
    pub fn stdio(command: String, args: Option<Vec<String>>, env: Option<HashMap<String, String>>) -> Self {
        McpServerConfig::Stdio { command, args, env }
    }

    /// Create a new HTTP server config
    pub fn http(url: String, headers: Option<HashMap<String, String>>) -> Self {
        McpServerConfig::Http { url, headers }
    }

    /// Create a new SSE server config
    pub fn sse(url: String, headers: Option<HashMap<String, String>>) -> Self {
        McpServerConfig::Sse { url, headers }
    }

    /// Get server type as string
    pub fn server_type(&self) -> &'static str {
        match self {
            McpServerConfig::Stdio { .. } => "stdio",
            McpServerConfig::Http { .. } => "http",
            McpServerConfig::Sse { .. } => "sse",
        }
    }
}
