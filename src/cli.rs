use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, PartialEq, ValueEnum)]
pub enum Provider {
    Anthropic,
    OpenAi,
    Glm,
}

impl Provider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
            Provider::OpenAi => "openai",
            Provider::Glm => "glm",
        }
    }

    pub fn from_str(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "anthropic" => Some(Provider::Anthropic),
            "openai" => Some(Provider::OpenAi),
            "glm" => Some(Provider::Glm),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn from_env_or_default() -> Self {
        match std::env::var("ZARZ_PROVIDER")
            .ok()
            .as_deref()
            .map(|v| v.to_ascii_lowercase())
        {
            Some(ref v) if v == "openai" => Provider::OpenAi,
            Some(ref v) if v == "anthropic" => Provider::Anthropic,
            Some(ref v) if v == "glm" => Provider::Glm,
            _ => Provider::Anthropic,
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "zarz",
    version,
    about = "ZarzCLI · AI-assisted code refactoring and rewrites",
    author = "zarzet",
    long_about = "ZarzCLI - Interactive AI coding assistant\n\nUsage:\n  zarz                      Start interactive chat\n  zarz --message \"prompt\"   Send a single prompt and exit\n  zarz ask \"question\"       Ask mode (legacy)\n  zarz chat                 Chat mode (legacy)"
)]
pub struct Cli {
    /// Send a message and exit (like Claude Code)
    #[arg(long, visible_alias = "msg")]
    pub message: Option<String>,

    /// Additional files to include as context
    #[arg(short = 'f', long)]
    pub files: Vec<PathBuf>,

    #[command(flatten)]
    pub model_args: CommonModelArgs,

    /// Working directory for the session
    #[arg(long)]
    pub directory: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Ask the model a question with optional code context.
    Ask(AskArgs),
    /// Rewrite one or more files using the model's response.
    Rewrite(RewriteArgs),
    /// Start an interactive chat session with AI code assistance.
    Chat(ChatArgs),
    /// Configure API keys and settings.
    Config(ConfigArgs),
    /// Manage MCP (Model Context Protocol) servers.
    Mcp(McpArgs),
}

#[derive(Debug, Args)]
pub struct CommonModelArgs {
    /// Target model identifier (e.g. claude-3-5-sonnet-20241022).
    #[arg(short, long)]
    pub model: Option<String>,
    /// Override the default provider.
    #[arg(long, value_enum)]
    pub provider: Option<Provider>,
    /// Override the default API endpoint.
    #[arg(long)]
    pub endpoint: Option<String>,
    /// Optional system prompt override.
    #[arg(long)]
    pub system_prompt: Option<String>,
    /// Timeout in seconds for the request.
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Debug, Args)]
pub struct AskArgs {
    #[command(flatten)]
    pub model_args: CommonModelArgs,
    /// Inline prompt text. If omitted, reads from STDIN.
    #[arg(short, long)]
    pub prompt: Option<String>,
    /// Optional file containing additional instructions.
    #[arg(long)]
    pub prompt_file: Option<PathBuf>,
    /// Additional context files to include in the request.
    #[arg(value_name = "FILE", num_args = 0..)]
    pub context_files: Vec<PathBuf>,
}

#[derive(Debug, Args)]
pub struct RewriteArgs {
    #[command(flatten)]
    pub model_args: CommonModelArgs,
    /// High-level instructions for the rewrite.
    #[arg(short, long)]
    pub instructions: Option<String>,
    /// File containing rewrite instructions.
    #[arg(long)]
    pub instructions_file: Option<PathBuf>,
    /// Apply the changes without confirmation.
    #[arg(long)]
    pub yes: bool,
    /// Preview diff without writing files.
    #[arg(long)]
    pub dry_run: bool,
    /// Target files that will be rewritten.
    #[arg(value_name = "FILE", num_args = 1..)]
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ChatArgs {
    #[command(flatten)]
    pub model_args: CommonModelArgs,
    /// Working directory for the session (defaults to current directory).
    #[arg(long)]
    pub directory: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct ConfigArgs {
    /// Reset configuration and run interactive setup.
    #[arg(long)]
    pub reset: bool,
    /// Show current configuration.
    #[arg(long)]
    pub show: bool,
    /// Authenticate with ChatGPT OAuth to fetch an OpenAI API key.
    #[arg(long)]
    pub login_chatgpt: bool,
}

#[derive(Debug, Clone, Args)]
pub struct McpArgs {
    #[command(subcommand)]
    pub command: McpCommands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum McpCommands {
    /// Add a new MCP server
    Add {
        /// Server name
        name: String,
        /// Server command (for stdio servers)
        #[arg(long)]
        command: Option<String>,
        /// Server arguments
        #[arg(long, num_args = 0..)]
        args: Vec<String>,
        /// Environment variables (KEY=VALUE)
        #[arg(long = "env")]
        env_vars: Vec<String>,
        /// Server URL (for http/sse servers)
        #[arg(long)]
        url: Option<String>,
        /// Transport type: stdio, http, sse
        #[arg(long, default_value = "stdio")]
        transport: String,
    },
    /// List all configured MCP servers
    List,
    /// Get details of a specific MCP server
    Get {
        /// Server name
        name: String,
    },
    /// Remove an MCP server
    Remove {
        /// Server name
        name: String,
    },
}
