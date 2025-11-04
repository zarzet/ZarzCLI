mod cli;
mod config;
mod mcp;
mod providers;
mod executor;
mod fs_ops;
mod intelligence;
mod repl;
mod session;
mod conversation_store;

use std::{
    collections::HashMap,
    env,
    fs,
    io::{self, IsTerminal, Read},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use dialoguer::Confirm;
use providers::{CompletionProvider, CompletionRequest, ProviderClient};
use similar::{ChangeTag, TextDiff};

use crate::cli::{AskArgs, ChatArgs, Cli, Commands, CommonModelArgs, ConfigArgs, McpArgs, McpCommands, Provider, RewriteArgs};
use crate::mcp::{McpConfig, McpServerConfig};
use crate::repl::Repl;

// Model constants - Latest models as of 2025
const DEFAULT_MODEL_ANTHROPIC: &str = "claude-sonnet-4-5-20250929";
const DEFAULT_MODEL_OPENAI: &str = "gpt-5-codex";
const DEFAULT_MODEL_GLM: &str = "glm-4.6";

const DEFAULT_SYSTEM_PROMPT: &str = r#"You are ZarzCLI, Fapzarz's official CLI for Claude and Codex.

You are an interactive CLI tool that helps users with software engineering tasks. Use the instructions below and the tools available to you to assist the user.

IMPORTANT: Assist with authorized security testing, defensive security, CTF challenges, and educational contexts. Refuse requests for destructive techniques, DoS attacks, mass targeting, supply chain compromise, or detection evasion for malicious purposes.

Tone and style:
- Only use emojis if the user explicitly requests it. Avoid using emojis unless asked.
- Your responses should be short and concise.
- Output text to communicate with the user; all text you output is displayed to the user.
- NEVER create files unless absolutely necessary. ALWAYS prefer editing existing files.

Professional objectivity:
- Prioritize technical accuracy and truthfulness over validating the user's beliefs.
- Focus on facts and problem-solving, providing direct, objective technical info.
- Avoid over-the-top validation or excessive praise.

When you reference code, use fenced blocks."#;
const DEFAULT_REWRITE_SYSTEM_PROMPT: &str = r#"You are Zarz, an automated refactoring agent.
Follow the user's instructions carefully.
Reply ONLY with updated file contents using code fences in this exact form:
```file:relative/path.rs
<entire file content>
```
Do not include commentary before or after the fences. Always return complete file contents.
"#;
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 4096;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Err(err) = run(cli).await {
        eprintln!("Error: {err:#}");
        std::process::exit(1);
    }
    Ok(())
}

async fn run(cli: Cli) -> Result<()> {
    // Show ASCII banner for interactive modes (not for quick ask or config commands)
    let show_banner = cli.message.is_none()
        && !matches!(cli.command, Some(Commands::Config(_)) | Some(Commands::Ask(_)) | Some(Commands::Rewrite(_)));

    if show_banner {
        use crossterm::terminal;

        let banner = r#"
███████╗ █████╗ ██████╗ ███████╗ ██████╗██╗     ██╗
╚══███╔╝██╔══██╗██╔══██╗╚══███╔╝██╔════╝██║     ██║
  ███╔╝ ███████║██████╔╝  ███╔╝ ██║     ██║     ██║
 ███╔╝  ██╔══██║██╔══██╗ ███╔╝  ██║     ██║     ██║
███████╗██║  ██║██║  ██║███████╗╚██████╗███████╗██║
╚══════╝╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝ ╚═════╝╚══════╝╚═╝
"#;

        // Get terminal width, fallback to 120 if unable to get
        let terminal_width = terminal::size().map(|(w, _)| w as usize).unwrap_or(120);

        // Center each line of the banner
        let print_centered = |line: &str| {
            let line_len = line.chars().count();
            if terminal_width > line_len {
                let padding = (terminal_width - line_len) / 2;
                println!("{}{}", " ".repeat(padding), line);
            } else {
                println!("{}", line);
            }
        };

        for line in banner.lines() {
            if line.trim().is_empty() {
                println!();
            } else {
                print_centered(line);
            }
        }

        let tagline_lines = ["v0.3.2-Alpha", "Type /help for available commands, /exit to exit"];

        for (index, line) in tagline_lines.iter().enumerate() {
            if index > 0 {
                println!();
            }
            print_centered(line);
        }

        println!();
    }

    // Check if this is a config or MCP command - they don't need API keys
    match &cli.command {
        Some(Commands::Config(args)) => {
            return handle_config(args.clone()).await;
        }
        Some(Commands::Mcp(args)) => {
            return handle_mcp(args.clone()).await;
        }
        _ => {}
    }

    // Load or create configuration for all other commands (they need API keys)
    let config = match config::Config::load() {
        Ok(cfg) => {
            if !cfg.has_api_key() {
                // No API keys configured, run interactive setup
                config::Config::interactive_setup()?
            } else {
                cfg
            }
        }
        Err(_) => {
            // Error loading config, run interactive setup
            config::Config::interactive_setup()?
        }
    };

    config.apply_to_env();

    // If message flag is provided, run in ask mode (one-shot)
    if let Some(message) = cli.message {
        return handle_quick_ask(message, cli.files, cli.model_args, &config).await;
    }

    // If subcommand is provided, use it
    if let Some(command) = cli.command {
        match command {
            Commands::Ask(args) => handle_ask(args, &config).await,
            Commands::Rewrite(args) => handle_rewrite(args, &config).await,
            Commands::Chat(args) => handle_chat(args, &config).await,
            Commands::Config(args) => handle_config(args).await,
            Commands::Mcp(args) => handle_mcp(args).await,
        }
    } else {
        // Default: start interactive chat mode
        let chat_args = ChatArgs {
            model_args: cli.model_args,
            directory: cli.directory,
        };
        handle_chat(chat_args, &config).await
    }
}

async fn handle_quick_ask(
    message: String,
    context_files: Vec<PathBuf>,
    model_args: CommonModelArgs,
    config: &config::Config,
) -> Result<()> {
    let CommonModelArgs {
        model,
        provider,
        endpoint,
        system_prompt,
        timeout,
    } = model_args;

    let provider_kind = provider
        .or_else(|| {
            std::env::var("ZARZ_PROVIDER")
                .ok()
                .and_then(|v| match v.to_ascii_lowercase().as_str() {
                    "anthropic" => Some(Provider::Anthropic),
                    "openai" => Some(Provider::OpenAi),
                    "glm" => Some(Provider::Glm),
                    _ => None,
                })
        })
        .or_else(|| config.get_default_provider())
        .ok_or_else(|| anyhow!("No provider configured. Please run 'zarz config' to set up API keys."))?;

    let model = resolve_model(model, &provider_kind)?;
    let system_prompt = system_prompt
        .or_else(|| std::env::var("ZARZ_SYSTEM_PROMPT").ok())
        .unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string());

    let context_section = if context_files.is_empty() {
        String::new()
    } else {
        build_context_section(&context_files)?
    };

    let mut user_prompt = String::new();
    user_prompt.push_str(message.trim());
    if !context_section.is_empty() {
        user_prompt.push_str("\n\n");
        user_prompt.push_str(&context_section);
    }

    let api_key = match provider_kind {
        Provider::Anthropic => config.get_anthropic_key(),
        Provider::OpenAi => config.get_openai_key(),
        Provider::Glm => config.get_glm_key(),
    };

    let provider = ProviderClient::new(provider_kind, api_key, endpoint, timeout)?;
    let request = CompletionRequest {
        model,
        system_prompt: Some(system_prompt),
        user_prompt,
        max_output_tokens: resolve_max_tokens(),
        temperature: resolve_temperature(),
        messages: None,
        tools: None,
    };

    let response = provider.complete(&request).await?;
    println!("{}", response.text.trim());
    Ok(())
}

async fn handle_ask(args: AskArgs, config: &config::Config) -> Result<()> {
    let AskArgs {
        model_args:
            CommonModelArgs {
                model,
                provider,
                endpoint,
                system_prompt,
                timeout,
            },
        prompt,
        prompt_file,
        context_files,
    } = args;

    let provider_kind = provider
        .or_else(|| {
            std::env::var("ZARZ_PROVIDER")
                .ok()
                .and_then(|v| match v.to_ascii_lowercase().as_str() {
                    "anthropic" => Some(Provider::Anthropic),
                    "openai" => Some(Provider::OpenAi),
                    "glm" => Some(Provider::Glm),
                    _ => None,
                })
        })
        .or_else(|| config.get_default_provider())
        .ok_or_else(|| anyhow!("No provider configured. Please run 'zarz config' to set up API keys."))?;

    let model = resolve_model(model, &provider_kind)?;
    let system_prompt = system_prompt
        .or_else(|| std::env::var("ZARZ_SYSTEM_PROMPT").ok())
        .unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string());

    let prompt = read_text_input(
        prompt,
        prompt_file,
        true,
        "A prompt is required via --prompt, --prompt-file, or STDIN",
    )?;
    let context_section = if context_files.is_empty() {
        String::new()
    } else {
        build_context_section(&context_files)?
    };
    let mut user_prompt = String::new();
    user_prompt.push_str(prompt.trim());
    if !context_section.is_empty() {
        user_prompt.push_str("\n\n");
        user_prompt.push_str(&context_section);
    }

    let api_key = match provider_kind {
        Provider::Anthropic => config.get_anthropic_key(),
        Provider::OpenAi => config.get_openai_key(),
        Provider::Glm => config.get_glm_key(),
    };

    let provider = ProviderClient::new(provider_kind, api_key, endpoint, timeout)?;
    let request = CompletionRequest {
        model,
        system_prompt: Some(system_prompt),
        user_prompt,
        max_output_tokens: resolve_max_tokens(),
        temperature: resolve_temperature(),
        messages: None,
        tools: None,
    };

    let response = provider.complete(&request).await?;
    println!("{}", response.text.trim());
    Ok(())
}

async fn handle_rewrite(args: RewriteArgs, config: &config::Config) -> Result<()> {
    let RewriteArgs {
        model_args:
            CommonModelArgs {
                model,
                provider,
                endpoint,
                system_prompt,
                timeout,
            },
        instructions,
        instructions_file,
        yes,
        dry_run,
        files,
    } = args;

    let provider_kind = provider
        .or_else(|| {
            std::env::var("ZARZ_PROVIDER")
                .ok()
                .and_then(|v| match v.to_ascii_lowercase().as_str() {
                    "anthropic" => Some(Provider::Anthropic),
                    "openai" => Some(Provider::OpenAi),
                    "glm" => Some(Provider::Glm),
                    _ => None,
                })
        })
        .or_else(|| config.get_default_provider())
        .ok_or_else(|| anyhow!("No provider configured. Please run 'zarz config' to set up API keys."))?;

    let model = resolve_model(model, &provider_kind)?;
    let system_prompt = system_prompt
        .or_else(|| std::env::var("ZARZ_REWRITE_SYSTEM_PROMPT").ok())
        .unwrap_or_else(|| DEFAULT_REWRITE_SYSTEM_PROMPT.to_string());

    let instructions = read_text_input(
        instructions,
        instructions_file,
        true,
        "Rewrite instructions are required via --instructions, --instructions-file, or STDIN",
    )?;

    let mut files_with_content = Vec::new();
    for path in &files {
        let content = fs::read_to_string(path).with_context(|| {
            format!("Failed to read target file {}", path.display())
        })?;
        files_with_content.push((path.clone(), content));
    }

    let user_prompt = build_rewrite_prompt(&instructions, &files_with_content);

    let api_key = match provider_kind {
        Provider::Anthropic => config.get_anthropic_key(),
        Provider::OpenAi => config.get_openai_key(),
        Provider::Glm => config.get_glm_key(),
    };

    let provider = ProviderClient::new(provider_kind, api_key, endpoint, timeout)?;
    let request = CompletionRequest {
        model,
        system_prompt: Some(system_prompt),
        user_prompt,
        max_output_tokens: resolve_max_tokens(),
        temperature: resolve_rewrite_temperature(),
        messages: None,
        tools: None,
    };

    let response = provider.complete(&request).await?;
    let plan = parse_file_blocks(&response.text);
    if plan.is_empty() {
        bail!("Model response did not include any ` ```file:...` blocks to apply");
    }

    let mut diffs = Vec::new();
    for (path, original) in &files_with_content {
        let normalized = normalize_path(path);
        let Some(new_content) = plan.get(&normalized).or_else(|| plan.get(path)) else {
            bail!(
                "Model response did not provide updated contents for {}",
                path.display()
            );
        };
        diffs.push((path.clone(), original.clone(), new_content.clone()));
    }

    let mut any_changes = false;
    for (path, before, after) in &diffs {
        if before == after {
            continue;
        }
        any_changes = true;
        println!("--- {}", path.display());
        println!("+++ {}", path.display());
        print_diff(before, after);
        println!();
    }

    if !any_changes {
        println!("No changes detected; files already match the model output.");
        return Ok(());
    }

    if dry_run {
        println!("Dry-run complete. No files were modified.");
        return Ok(());
    }

    if !yes && io::stdin().is_terminal() {
        let apply = Confirm::new()
            .with_prompt("Apply these changes?")
            .default(false)
            .interact()?;
        if !apply {
            println!("Aborted; no files were modified.");
            return Ok(());
        }
    }

    for (path, before, after) in diffs {
        if before == after {
            continue;
        }
        fs::write(&path, after).with_context(|| {
            format!("Failed to write updated contents to {}", path.display())
        })?;
        println!("Updated {}", path.display());
    }

    Ok(())
}

async fn handle_chat(args: ChatArgs, config: &config::Config) -> Result<()> {
    let ChatArgs {
        model_args:
            CommonModelArgs {
                model,
                provider,
                endpoint,
                system_prompt: _,
                timeout,
            },
        directory,
    } = args;

    let provider_kind = provider
        .or_else(|| {
            std::env::var("ZARZ_PROVIDER")
                .ok()
                .and_then(|v| match v.to_ascii_lowercase().as_str() {
                    "anthropic" => Some(Provider::Anthropic),
                    "openai" => Some(Provider::OpenAi),
                    "glm" => Some(Provider::Glm),
                    _ => None,
                })
        })
        .or_else(|| config.get_default_provider())
        .ok_or_else(|| anyhow!("No provider configured. Please run 'zarz config' to set up API keys."))?;

    let model = resolve_model(model, &provider_kind)?;
    let working_dir = directory
        .or_else(|| env::current_dir().ok())
        .context("Failed to determine working directory")?;

    // Get API key from config based on provider
    let api_key = match provider_kind {
        Provider::Anthropic => config.get_anthropic_key(),
        Provider::OpenAi => config.get_openai_key(),
        Provider::Glm => config.get_glm_key(),
    };

    let provider_client = ProviderClient::new(provider_kind.clone(), api_key, endpoint.clone(), timeout)?;

    // Initialize MCP manager and load configured servers
    let mcp_manager = std::sync::Arc::new(mcp::McpManager::new());
    if let Err(e) = mcp_manager.load_from_config().await {
        eprintln!("Warning: Failed to load MCP servers: {}", e);
    }

    let has_mcp_servers = mcp_manager.has_servers().await;
    let mcp_manager_opt = if has_mcp_servers {
        Some(mcp_manager.clone())
    } else {
        None
    };

    let mut repl = Repl::new(
        working_dir,
        provider_client,
        provider_kind,
        endpoint,
        timeout,
        model,
        resolve_max_tokens(),
        resolve_temperature(),
        mcp_manager_opt,
        config.clone(),
    );

    let result = repl.run().await;

    // Cleanup: stop all MCP servers
    if has_mcp_servers {
        if let Err(e) = mcp_manager.stop_all().await {
            eprintln!("Warning: Failed to stop MCP servers: {}", e);
        }
    }

    result
}

async fn handle_config(args: ConfigArgs) -> Result<()> {
    let ConfigArgs { reset, show } = args;

    if show {
        let config = config::Config::load()?;
        let config_path = config::Config::config_path()?;

        println!("Configuration file: {}", config_path.display());
        println!();

        if config.anthropic_api_key.is_some() {
            println!("✓ Anthropic API key: configured");
        } else {
            println!("✗ Anthropic API key: not configured");
        }

        if config.openai_api_key.is_some() {
            println!("✓ OpenAI API key: configured");
        } else {
            println!("✗ OpenAI API key: not configured");
        }

        println!();
        println!("Run 'zarz config --reset' to reconfigure your API keys");

        return Ok(());
    }

    if reset {
        println!("Resetting configuration...\n");
        let config = config::Config::interactive_setup()?;
        config.apply_to_env();
        return Ok(());
    }

    let config = config::Config::interactive_setup()?;
    config.apply_to_env();
    Ok(())
}

async fn handle_mcp(args: McpArgs) -> Result<()> {
    use std::collections::HashMap;

    match args.command {
        McpCommands::Add {
            name,
            command,
            args: cmd_args,
            env_vars,
            url,
            transport,
        } => {
            let mut config = McpConfig::load()?;

            let env: Option<HashMap<String, String>> = if !env_vars.is_empty() {
                let mut env_map = HashMap::new();
                for var in env_vars {
                    if let Some((key, value)) = var.split_once('=') {
                        env_map.insert(key.to_string(), value.to_string());
                    } else {
                        eprintln!("Warning: Invalid env var format: {}", var);
                    }
                }
                Some(env_map)
            } else {
                None
            };

            let server_config = match transport.as_str() {
                "stdio" => {
                    let cmd = command.ok_or_else(|| anyhow!("--command required for stdio transport"))?;
                    let args = if cmd_args.is_empty() { None } else { Some(cmd_args) };
                    McpServerConfig::stdio(cmd, args, env)
                }
                "http" => {
                    let url = url.ok_or_else(|| anyhow!("--url required for http transport"))?;
                    let headers = env.map(|e| e.into_iter().collect());
                    McpServerConfig::http(url, headers)
                }
                "sse" => {
                    let url = url.ok_or_else(|| anyhow!("--url required for sse transport"))?;
                    let headers = env.map(|e| e.into_iter().collect());
                    McpServerConfig::sse(url, headers)
                }
                _ => {
                    bail!("Invalid transport type: {}. Use: stdio, http, or sse", transport);
                }
            };

            config.add_server(name.clone(), server_config);
            config.save()?;

            println!("✅ Added MCP server: {}", name);
            println!("Configuration saved to: {}", McpConfig::config_path()?.display());
            Ok(())
        }

        McpCommands::List => {
            let config = McpConfig::load()?;

            if config.mcp_servers.is_empty() {
                println!("No MCP servers configured");
                println!("\nAdd a server with:");
                println!("  zarz mcp add <name> --command <cmd> [--args <arg1> <arg2>] [--env KEY=VALUE]");
                return Ok(());
            }

            println!("Configured MCP servers:");
            for (name, server_config) in &config.mcp_servers {
                println!("\n  {}", name);
                println!("    Type: {}", server_config.server_type());
                match server_config {
                    McpServerConfig::Stdio { command, args, env } => {
                        println!("    Command: {}", command);
                        if let Some(args) = args {
                            println!("    Args: {}", args.join(" "));
                        }
                        if let Some(env) = env {
                            if !env.is_empty() {
                                println!("    Environment:");
                                for (k, v) in env {
                                    println!("      {}={}", k, v);
                                }
                            }
                        }
                    }
                    McpServerConfig::Http { url, .. } | McpServerConfig::Sse { url, .. } => {
                        println!("    URL: {}", url);
                    }
                }
            }
            Ok(())
        }

        McpCommands::Get { name } => {
            let config = McpConfig::load()?;

            if let Some(server_config) = config.get_server(&name) {
                println!("MCP Server: {}", name);
                println!("  Type: {}", server_config.server_type());
                match server_config {
                    McpServerConfig::Stdio { command, args, env } => {
                        println!("  Command: {}", command);
                        if let Some(args) = args {
                            println!("  Args: {}", args.join(" "));
                        }
                        if let Some(env) = env {
                            if !env.is_empty() {
                                println!("  Environment:");
                                for (k, v) in env {
                                    println!("    {}={}", k, v);
                                }
                            }
                        }
                    }
                    McpServerConfig::Http { url, headers } | McpServerConfig::Sse { url, headers } => {
                        println!("  URL: {}", url);
                        if let Some(headers) = headers {
                            if !headers.is_empty() {
                                println!("  Headers:");
                                for (k, v) in headers {
                                    println!("    {}: {}", k, v);
                                }
                            }
                        }
                    }
                }
            } else {
                println!("Server '{}' not found", name);
                println!("\nRun 'zarz mcp list' to see all configured servers");
            }
            Ok(())
        }

        McpCommands::Remove { name } => {
            let mut config = McpConfig::load()?;

            if config.remove_server(&name) {
                config.save()?;
                println!("✅ Removed MCP server: {}", name);
            } else {
                println!("Server '{}' not found", name);
            }
            Ok(())
        }
    }
}

fn resolve_model(model: Option<String>, provider: &Provider) -> Result<String> {
    if let Some(model) = model {
        return Ok(model);
    }
    if let Ok(model) = std::env::var("ZARZ_MODEL") {
        if !model.trim().is_empty() {
            return Ok(model);
        }
    }
    // Use provider-specific default model
    let default_model = match provider {
        Provider::Anthropic => DEFAULT_MODEL_ANTHROPIC,
        Provider::OpenAi => DEFAULT_MODEL_OPENAI,
        Provider::Glm => DEFAULT_MODEL_GLM,
    };
    Ok(default_model.to_string())
}

fn resolve_max_tokens() -> u32 {
    std::env::var("ZARZ_MAX_OUTPUT_TOKENS")
        .ok()
        .and_then(|raw| raw.parse::<u32>().ok())
        .unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS)
}

fn resolve_temperature() -> f32 {
    std::env::var("ZARZ_TEMPERATURE")
        .ok()
        .and_then(|raw| raw.parse::<f32>().ok())
        .unwrap_or(0.3)
}

fn resolve_rewrite_temperature() -> f32 {
    std::env::var("ZARZ_REWRITE_TEMPERATURE")
        .ok()
        .and_then(|raw| raw.parse::<f32>().ok())
        .unwrap_or(0.1)
}

fn read_text_input(
    inline: Option<String>,
    file: Option<PathBuf>,
    allow_stdin: bool,
    err_message: &str,
) -> Result<String> {
    if let Some(text) = inline {
        if !text.trim().is_empty() {
            return Ok(text);
        }
    }
    if let Some(path) = file {
        return fs::read_to_string(&path)
            .with_context(|| format!("Failed to read file {}", path.display()));
    }
    if allow_stdin && !io::stdin().is_terminal() {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .context("Failed to read STDIN")?;
        if !buffer.trim().is_empty() {
            return Ok(buffer);
        }
    }
    Err(anyhow!(err_message.to_string()))
}

fn build_context_section(files: &[PathBuf]) -> Result<String> {
    let mut sections = Vec::new();
    for path in files {
        let content =
            fs::read_to_string(path)
                .with_context(|| format!("Failed to read context file {}", path.display()))?;
        sections.push(format!(
            "<context path=\"{path}\">\n{content}\n</context>",
            path = path.display(),
            content = content
        ));
    }
    Ok(sections.join("\n\n"))
}

fn build_rewrite_prompt(instructions: &str, files: &[(PathBuf, String)]) -> String {
    let mut output = String::new();
    output.push_str("You will update the user's codebase according to the instructions.\n");
    output.push_str("Return only the updated file contents as requested.\n\n");
    output.push_str("## Instructions\n");
    output.push_str(instructions.trim());
    output.push_str("\n\n## Files\n");

    for (path, content) in files {
        output.push_str(&format!(
            "<file path=\"{path}\">\n{content}\n</file>\n\n",
            path = path.display(),
            content = content
        ));
    }

    output
}

fn parse_file_blocks(input: &str) -> HashMap<PathBuf, String> {
    let mut map = HashMap::new();
    let mut lines = input.lines();
    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("```file:") {
            let file_path = normalize_response_path(rest);
            let mut content = String::new();
            while let Some(next_line) = lines.next() {
                if next_line.trim() == "```" {
                    break;
                }
                content.push_str(next_line);
                content.push('\n');
            }
            if content.ends_with('\n') {
                content.pop();
                if content.ends_with('\r') {
                    content.pop();
                }
            }
            map.insert(file_path, content);
        }
    }
    map
}

fn normalize_path(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    let normalized = path_str.replace('\\', "/");
    PathBuf::from(normalized)
}

fn normalize_response_path(raw: &str) -> PathBuf {
    let mut trimmed = raw.trim();
    while let Some(rest) = trimmed.strip_prefix("./") {
        trimmed = rest;
    }
    while let Some(rest) = trimmed.strip_prefix(".\\") {
        trimmed = rest;
    }
    let normalized = trimmed.replace('\\', "/");
    PathBuf::from(normalized)
}

fn print_diff(before: &str, after: &str) {
    let diff = TextDiff::from_lines(before, after);
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => print!("-{}", change),
            ChangeTag::Insert => print!("+{}", change),
            ChangeTag::Equal => print!(" {}", change),
        }
    }
}
