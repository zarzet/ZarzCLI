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
    #[arg(long, visible_alias = "msg")]
    pub message: Option<String>,

    #[arg(short = 'f', long)]
    pub files: Vec<PathBuf>,

    #[command(flatten)]
    pub model_args: CommonModelArgs,

    #[arg(long)]
    pub directory: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Ask(AskArgs),
    Rewrite(RewriteArgs),
    Chat(ChatArgs),
    Config(ConfigArgs),
    Mcp(McpArgs),
}

#[derive(Debug, Args)]
pub struct CommonModelArgs {
    #[arg(short, long)]
    pub model: Option<String>,
    #[arg(long, value_enum)]
    pub provider: Option<Provider>,
    #[arg(long)]
    pub endpoint: Option<String>,
    #[arg(long)]
    pub system_prompt: Option<String>,
    #[arg(long)]
    pub timeout: Option<u64>,
}

#[derive(Debug, Args)]
pub struct AskArgs {
    #[command(flatten)]
    pub model_args: CommonModelArgs,
    #[arg(short, long)]
    pub prompt: Option<String>,
    #[arg(long)]
    pub prompt_file: Option<PathBuf>,
    #[arg(value_name = "FILE", num_args = 0..)]
    pub context_files: Vec<PathBuf>,
}

#[derive(Debug, Args)]
pub struct RewriteArgs {
    #[command(flatten)]
    pub model_args: CommonModelArgs,
    #[arg(short, long)]
    pub instructions: Option<String>,
    #[arg(long)]
    pub instructions_file: Option<PathBuf>,
    #[arg(long)]
    pub yes: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(value_name = "FILE", num_args = 1..)]
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ChatArgs {
    #[command(flatten)]
    pub model_args: CommonModelArgs,
    #[arg(long)]
    pub directory: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct ConfigArgs {
    #[arg(long)]
    pub reset: bool,
    #[arg(long)]
    pub show: bool,
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
    Add {
        name: String,
        #[arg(long)]
        command: Option<String>,
        #[arg(long, num_args = 0..)]
        args: Vec<String>,
        #[arg(long = "env")]
        env_vars: Vec<String>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long, default_value = "stdio")]
        transport: String,
    },
    List,
    Get {
        name: String,
    },
    Remove {
        name: String,
    },
}
