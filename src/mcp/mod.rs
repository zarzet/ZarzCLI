// MCP (Model Context Protocol) support for ZarzCLI
pub mod config;
pub mod client;
pub mod types;
pub mod manager;

pub use config::{McpConfig, McpServerConfig};
#[allow(unused_imports)]
pub use client::McpClient;
#[allow(unused_imports)]
pub use types::{McpTool, McpResource, McpPrompt};
pub use manager::McpManager;
