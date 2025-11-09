use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

impl McpConfig {
    pub fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not determine home directory")?;
        Ok(home.join(".zarz").join("mcp.json"))
    }

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

    pub fn add_server(&mut self, name: String, config: McpServerConfig) {
        self.mcp_servers.insert(name, config);
    }

    pub fn remove_server(&mut self, name: &str) -> bool {
        self.mcp_servers.remove(name).is_some()
    }

    pub fn get_server(&self, name: &str) -> Option<&McpServerConfig> {
        self.mcp_servers.get(name)
    }

    #[allow(dead_code)]
    pub fn list_servers(&self) -> Vec<String> {
        self.mcp_servers.keys().cloned().collect()
    }

    #[allow(dead_code)]
    pub fn has_servers(&self) -> bool {
        !self.mcp_servers.is_empty()
    }
}

impl McpServerConfig {
    pub fn stdio(command: String, args: Option<Vec<String>>, env: Option<HashMap<String, String>>) -> Self {
        McpServerConfig::Stdio { command, args, env }
    }

    pub fn http(url: String, headers: Option<HashMap<String, String>>) -> Self {
        McpServerConfig::Http { url, headers }
    }

    pub fn sse(url: String, headers: Option<HashMap<String, String>>) -> Self {
        McpServerConfig::Sse { url, headers }
    }

    pub fn server_type(&self) -> &'static str {
        match self {
            McpServerConfig::Stdio { .. } => "stdio",
            McpServerConfig::Http { .. } => "http",
            McpServerConfig::Sse { .. } => "sse",
        }
    }
}
