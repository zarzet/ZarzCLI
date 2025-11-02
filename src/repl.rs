use anyhow::{anyhow, Context, Result};
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::ExecutableCommand;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use similar::{ChangeTag, TextDiff};
use std::collections::HashMap;
use std::io::stdout;
use std::path::{Path, PathBuf};

use crate::cli::Provider;
use crate::config::Config;
use crate::executor::CommandExecutor;
use crate::fs_ops::FileSystemOps;
use crate::mcp::{McpManager, McpTool};
use crate::mcp::types::{CallToolResult, ToolContent};
use crate::providers::{CompletionProvider, CompletionRequest, ProviderClient};
use crate::session::{MessageRole, Session};
use serde_json::{self, Value};

const REPL_SYSTEM_PROMPT: &str = r#"You are ZarzCLI, Fapzarz's official CLI for Claude and Codex.

You are an interactive CLI tool that helps users with software engineering tasks.

IMPORTANT: Assist with authorized security testing, defensive security, CTF challenges, and educational contexts. Refuse requests for destructive techniques, DoS attacks, mass targeting, supply chain compromise, or detection evasion for malicious purposes.

When making file changes, use code fences in this exact format:
```file:relative/path.rs
<entire file content>
```

Available commands the user can use:
- /help - Show help
- /apply - Apply pending changes
- /diff - Show pending changes
- /undo - Clear pending changes
- /edit <file> - Load a file for editing
- /run <command> - Execute a shell command
- /search <symbol> - Search for a symbol in the codebase
- /context <query> - Find relevant files for a query
- /files - List currently loaded files
- /model <name> - Switch to a different AI model
- /mcp - Show MCP servers and available tools
- /clear - Clear conversation history
- /quit or /exit - Exit the session

Tone and style:
- Only use emojis if the user explicitly requests it
- Responses should be short and concise
- Focus on facts and problem-solving
- Avoid over-the-top validation or excessive praise

Provide clear, concise responses. When suggesting changes, always use the file block format above.

Conversation format:
- The prompt includes the recent transcript using prefixes like "User:", "Assistant:", and "Tool[server.tool]:".
- Always respond in the voice of "Assistant" to the most recent user message.

MCP tool usage:
- When the prompt lists available MCP tools, you may request one by replying exactly: CALL_MCP_TOOL server=<server_name> tool=<tool_name> args=<json_object>
- The JSON must be minified on a single line. Use {} when no arguments are required.
- Do not include any additional text when making a tool request. Wait for Tool[...] messages that show the results, then continue the conversation.
"#;

pub struct Repl {
    session: Session,
    provider: ProviderClient,
    provider_kind: Provider,
    endpoint: Option<String>,
    timeout: Option<u64>,
    model: String,
    max_tokens: u32,
    temperature: f32,
    mcp_manager: Option<std::sync::Arc<McpManager>>,
    config: Config,
}

impl Repl {
    pub fn new(
        working_dir: PathBuf,
        provider: ProviderClient,
        provider_kind: Provider,
        endpoint: Option<String>,
        timeout: Option<u64>,
        model: String,
        max_tokens: u32,
        temperature: f32,
        mcp_manager: Option<std::sync::Arc<McpManager>>,
        config: Config,
    ) -> Self {
        Self {
            session: Session::new(working_dir),
            provider,
            provider_kind,
            endpoint,
            timeout,
            model,
            max_tokens,
            temperature,
            mcp_manager,
            config,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        println!("Type /help for available commands, /quit to exit\n");

        let mut editor = DefaultEditor::new()
            .context("Failed to initialize readline editor")?;

        loop {
            // Print separator and get input
            Self::print_input_box_start();

            // Simple prompt
            let readline = editor.readline("> ");

            match readline {
                Ok(line) => {
                    // Print bottom border after input
                    Self::print_input_box_end();

                    let line = line.trim();

                    if line.is_empty() {
                        continue;
                    }

                    editor.add_history_entry(line)
                        .context("Failed to add history entry")?;

                    if line.starts_with('/') {
                        if let Err(e) = self.handle_command(line).await {
                            eprintln!("Error: {:#}", e);
                        }

                        if line == "/quit" || line == "/exit" {
                            break;
                        }
                    } else {
                        if let Err(e) = self.handle_user_input(line).await {
                            eprintln!("Error: {:#}", e);
                        }
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    println!("Interrupted");
                    break;
                }
                Err(ReadlineError::Eof) => {
                    println!("Exiting");
                    break;
                }
                Err(err) => {
                    eprintln!("Error: {:#}", err);
                    break;
                }
            }
        }

        Ok(())
    }

    fn print_input_box_start() {
        use crossterm::terminal;

        // Get terminal width, fallback to 120 if unable to get
        let terminal_width = terminal::size().map(|(w, _)| w as usize).unwrap_or(120);
        let border_line = "─".repeat(terminal_width);

        // Print top border
        println!("{}", border_line);

        // Reserve a blank line for the prompt so the border wraps the user input
        println!();

        // Print bottom border immediately so it's visible while typing
        println!("{}", border_line);

        // Move cursor up 2 lines so the prompt sits between the borders
        print!("\x1B[2A");
        std::io::Write::flush(&mut std::io::stdout()).ok();
    }

    fn print_input_box_end() {
        // Bottom border already printed, just add newline to move past it
        println!();
    }

    async fn handle_command(&mut self, command: &str) -> Result<()> {
        let parts: Vec<&str> = command.splitn(2, ' ').collect();
        let cmd = parts[0];
        let args = parts.get(1).copied().unwrap_or("");

        match cmd {
            "/help" => self.show_help(),
            "/quit" | "/exit" => {
                println!("Goodbye!");
                Ok(())
            }
            "/apply" => self.apply_changes().await,
            "/diff" => self.show_diff(),
            "/undo" => self.undo_changes(),
            "/edit" => self.edit_file(args).await,
            "/run" => self.run_command(args).await,
            "/search" => self.search_symbol(args).await,
            "/context" => self.find_context(args).await,
            "/files" => self.list_files(),
            "/model" => self.switch_model(args).await,
            "/mcp" => self.show_mcp_status().await,
            "/clear" => self.clear_history(),
            _ => {
                println!("Unknown command: {}", cmd);
                println!("Type /help for available commands");
                Ok(())
            }
        }
    }

    async fn handle_user_input(&mut self, input: &str) -> Result<()> {
        self.session.add_message(MessageRole::User, input.to_string());

        let tools_snapshot = if let Some(manager) = &self.mcp_manager {
            match manager.get_all_tools().await {
                Ok(map) if !map.is_empty() => Some(map),
                Ok(_) => None,
                Err(e) => {
                    eprintln!("Warning: Failed to fetch MCP tools: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let tool_prompt_section = tools_snapshot
            .as_ref()
            .map(|tools| build_tool_prompt_section(tools));

        let mut tool_calls = 0usize;
        let max_tool_calls = 5usize;
        #[allow(unused_assignments)]
        let mut final_response: Option<String> = None;

        loop {
            let mut prompt = String::new();

            if let Some(section) = &tool_prompt_section {
                prompt.push_str(section);
                prompt.push_str("\n\n");
            } else if self.mcp_manager.is_some() {
                prompt.push_str("No MCP tools are currently available.\n\n");
            }

            prompt.push_str(&self.session.build_prompt_with_context(true));
            prompt.push_str("Respond as the assistant to the latest user message.");

            let request = CompletionRequest {
                model: self.model.clone(),
                system_prompt: Some(REPL_SYSTEM_PROMPT.to_string()),
                user_prompt: prompt,
                max_output_tokens: self.max_tokens,
                temperature: self.temperature,
            };

            let response = self.provider.complete(&request).await?;
            let raw_text = response.text;

            match parse_mcp_tool_call(&raw_text) {
                Ok(Some(parsed)) => {
                    if let Some(prefix_text) = parsed.prefix.as_deref() {
                        let display = strip_file_blocks(prefix_text);
                        if !display.trim().is_empty() {
                            print_assistant_message(&display)?;
                        }
                        self.session.add_message(
                            MessageRole::Assistant,
                            prefix_text.to_string(),
                        );
                    } else {
                        let note = format!(
                            "Calling MCP tool {}.{}...",
                            parsed.call.server, parsed.call.tool
                        );
                        print_assistant_message(&note)?;
                        self.session.add_message(MessageRole::Assistant, note);
                    }

                    self.session
                        .add_message(MessageRole::Assistant, parsed.command_text.clone());
                    print_tool_command(&parsed.command_text)?;

                    if self.mcp_manager.is_none() {
                        stdout().execute(SetForegroundColor(Color::Yellow)).ok();
                        println!("MCP tool request ignored: no MCP manager configured.");
                        stdout().execute(ResetColor).ok();

                        self.session.add_message(
                            MessageRole::Tool {
                                server: parsed.call.server.clone(),
                                tool: parsed.call.tool.clone(),
                            },
                            "ERROR: MCP tools are not available in this session.".to_string(),
                        );

                        continue;
                    }

                    if tool_calls >= max_tool_calls {
                        stdout().execute(SetForegroundColor(Color::Yellow)).ok();
                        println!("Skipping MCP tool call (limit of {} reached).", max_tool_calls);
                        stdout().execute(ResetColor).ok();

                        self.session.add_message(
                            MessageRole::Tool {
                                server: parsed.call.server.clone(),
                                tool: parsed.call.tool.clone(),
                            },
                            "ERROR: MCP tool call limit reached for this request.".to_string(),
                        );

                        continue;
                    }

                    let manager = self.mcp_manager.as_ref().unwrap();

                    let (mut tool_output, is_error) = match manager
                        .call_tool(
                            &parsed.call.server,
                            parsed.call.tool.clone(),
                            parsed.call.arguments.clone(),
                        )
                        .await
                    {
                        Ok(result) => {
                            let is_error = result.is_error.unwrap_or(false);
                            let mut text = format_tool_result(&result);
                            if text.trim().is_empty() {
                                if is_error {
                                    text = "ERROR: MCP tool returned no content.".to_string();
                                } else {
                                    text = "MCP tool returned no content.".to_string();
                                }
                            }
                            (text, is_error)
                        }
                        Err(err) => (format!("ERROR: {}", err), true),
                    };

                    tool_calls += 1;

                    if is_error && !tool_output.starts_with("ERROR") {
                        tool_output = format!("ERROR: {}", tool_output);
                    }

                    let stored_output = if tool_output.chars().count() > 8000 {
                        let mut truncated = truncate_for_display(&tool_output, 8000);
                        truncated.push_str("\n... (truncated for conversation history)");
                        truncated
                    } else {
                        tool_output.clone()
                    };

                    self.session.add_message(
                        MessageRole::Tool {
                            server: parsed.call.server.clone(),
                            tool: parsed.call.tool.clone(),
                        },
                        stored_output,
                    );

                    log_tool_execution(
                        &parsed.call.server,
                        &parsed.call.tool,
                        &tool_output,
                        is_error,
                    )?;

                    continue;
                }
                Ok(None) => {
                    final_response = Some(raw_text.clone());
                    self.session.add_message(MessageRole::Assistant, raw_text.clone());
                    break;
                }
                Err(parse_error) => {
                    self.session.add_message(MessageRole::Assistant, raw_text.clone());
                    stdout().execute(SetForegroundColor(Color::Yellow)).ok();
                    println!("Warning: {}", parse_error);
                    stdout().execute(ResetColor).ok();
                    final_response = Some(raw_text.clone());
                    break;
                }
            }
        }

        if let Some(text) = final_response {
            let printable = strip_file_blocks(&text);
            if !printable.trim().is_empty() {
                print_assistant_message(&printable)?;
            }

            let file_blocks = parse_file_blocks(&text);
            if !file_blocks.is_empty() {
                self.process_file_blocks(file_blocks).await?;
            }
        }

        Ok(())
    }

    async fn process_file_blocks(&mut self, blocks: HashMap<PathBuf, String>) -> Result<()> {
        if blocks.is_empty() {
            return Ok(());
        }

        for (path, new_content) in blocks {
            let full_path = self.session.working_directory.join(&path);
            let existed = FileSystemOps::file_exists(&full_path).await;
            let original = if existed {
                FileSystemOps::read_file(&full_path).await?
            } else {
                String::new()
            };

            if original == new_content {
                stdout().execute(SetForegroundColor(Color::DarkGrey)).ok();
                println!("No changes for {}", path.display());
                stdout().execute(ResetColor).ok();
                continue;
            }

            print_file_change_summary(&path, &original, &new_content)?;

            FileSystemOps::create_file(&full_path, &new_content).await?;

            let mut out = stdout();
            let message = if existed {
                format!("Updated {}", path.display())
            } else {
                format!("Created {}", path.display())
            };
            out.execute(SetForegroundColor(Color::Green)).ok();
            println!("{}", message);
            out.execute(ResetColor).ok();
            println!();
        }

        // Since changes are applied immediately, clear any stale pending state
        self.session.clear_pending_changes();

        Ok(())
    }

    fn show_help(&self) -> Result<()> {
        println!("Available commands:");
        println!("  /help           - Show this help message");
        println!("  /apply          - Apply pending file changes");
        println!("  /diff           - Show pending changes");
        println!("  /undo           - Clear pending changes");
        println!("  /edit <file>    - Load a file for editing");
        println!("  /run <command>  - Execute a shell command");
        println!("  /search <name>  - Search for a symbol");
        println!("  /context <query>- Find relevant files");
        println!("  /files          - List loaded files");
        println!("  /model <name>   - Switch to a different AI model");
        println!("                    Examples: claude-sonnet-4-5-20250929, claude-haiku-4-5,");
        println!("                              gpt-5-codex, gpt-4o");
        println!("  /mcp            - Show MCP servers and available tools");
        println!("  /clear          - Clear conversation history");
        println!("  /quit, /exit    - Exit the session");
        println!();
        println!("Current model: {}", self.model);
        println!("Current provider: {}", self.provider.name());
        Ok(())
    }

    async fn apply_changes(&mut self) -> Result<()> {
        if self.session.pending_changes.is_empty() {
            println!("No pending changes to apply");
            return Ok(());
        }

        for change in &self.session.pending_changes {
            let full_path = self.session.working_directory.join(&change.path);
            FileSystemOps::create_file(&full_path, &change.new_content).await?;
            println!("Applied changes to {}", change.path.display());
        }

        self.session.clear_pending_changes();
        println!("All changes applied successfully");

        Ok(())
    }

    fn show_diff(&self) -> Result<()> {
        if self.session.pending_changes.is_empty() {
            println!("No pending changes");
            return Ok(());
        }

        for change in &self.session.pending_changes {
            println!("--- {}", change.path.display());
            println!("+++ {}", change.path.display());
            print_diff(&change.original_content, &change.new_content);
            println!();
        }

        Ok(())
    }

    fn undo_changes(&mut self) -> Result<()> {
        let count = self.session.pending_changes.len();
        self.session.clear_pending_changes();
        println!("Cleared {} pending change(s)", count);
        Ok(())
    }

    async fn edit_file(&mut self, path: &str) -> Result<()> {
        if path.is_empty() {
            return Err(anyhow!("Usage: /edit <file>"));
        }

        let file_path = PathBuf::from(path);
        let full_path = self.session.working_directory.join(&file_path);

        if !FileSystemOps::file_exists(&full_path).await {
            return Err(anyhow!("File not found: {}", path));
        }

        let content = FileSystemOps::read_file(&full_path).await?;
        self.session.load_file(file_path.clone(), content);

        println!("Loaded {} for editing", path);

        Ok(())
    }

    async fn run_command(&self, command: &str) -> Result<()> {
        if command.is_empty() {
            return Err(anyhow!("Usage: /run <command>"));
        }

        println!("Running: {}", command);

        let result = CommandExecutor::execute(command).await?;

        if !result.stdout.is_empty() {
            println!("{}", result.stdout);
        }

        if !result.stderr.is_empty() {
            eprintln!("{}", result.stderr);
        }

        if result.success {
            println!("Command completed successfully (exit code: {})", result.exit_code);
        } else {
            println!("Command failed (exit code: {})", result.exit_code);
        }

        Ok(())
    }

    async fn search_symbol(&self, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(anyhow!("Usage: /search <symbol>"));
        }

        println!("Searching for symbol: {}", name);

        let symbols = self.session.search_symbol(name)?;

        if symbols.is_empty() {
            println!("No symbols found matching '{}'", name);
        } else {
            println!("Found {} symbol(s):", symbols.len());
            for symbol in symbols {
                println!("  {:?} {} in {}", symbol.kind, symbol.name, symbol.file.display());
            }
        }

        Ok(())
    }

    async fn find_context(&self, query: &str) -> Result<()> {
        if query.is_empty() {
            return Err(anyhow!("Usage: /context <query>"));
        }

        println!("Finding relevant context for: {}", query);

        let files = self.session.get_relevant_context(query)?;

        if files.is_empty() {
            println!("No relevant files found");
        } else {
            println!("Relevant files:");
            for file in files {
                println!("  {}", file.display());
            }
        }

        Ok(())
    }

    fn list_files(&self) -> Result<()> {
        if self.session.current_files.is_empty() {
            println!("No files currently loaded");
        } else {
            println!("Currently loaded files:");
            for path in self.session.current_files.keys() {
                println!("  {}", path.display());
            }
        }

        Ok(())
    }

    fn clear_history(&mut self) -> Result<()> {
        self.session.conversation_history.clear();
        println!("Conversation history cleared");
        Ok(())
    }

    async fn switch_model(&mut self, model_name: &str) -> Result<()> {
        if model_name.is_empty() {
            println!("Usage: /model <name>");
            println!();
            println!("Available models:");
            println!("  Anthropic Claude:");
            println!("    claude-sonnet-4-5-20250929       - Best for coding and agents");
            println!("    claude-sonnet-4-5-20250929-thinking - Extended thinking mode");
            println!("    claude-haiku-4-5                 - Fast and cost-effective");
            println!("    claude-opus-4-1                  - Most powerful");
            println!("    claude-sonnet-4                  - General purpose");
            println!();
            println!("  OpenAI:");
            println!("    gpt-5-codex                      - Optimized for coding");
            println!("    gpt-4o                           - Multimodal");
            println!("    gpt-4-turbo                      - Fast and efficient");
            println!();
            println!("  GLM (Z.AI - International):");
            println!("    glm-4.6                          - Best for coding (200K context)");
            println!("    glm-4.5                          - Previous generation");
            println!();
            println!("Current model: {}", self.model);
            return Ok(());
        }

        let new_model = model_name.to_string();

        let new_provider_kind = if new_model.starts_with("claude") {
            Provider::Anthropic
        } else if new_model.starts_with("gpt") {
            Provider::OpenAi
        } else if new_model.starts_with("glm") {
            Provider::Glm
        } else {
            return Err(anyhow!("Unknown model provider for '{}'", new_model));
        };

        if new_provider_kind != self.provider_kind {
            // Get API key from config based on provider
            let api_key = match new_provider_kind {
                Provider::Anthropic => self.config.get_anthropic_key(),
                Provider::OpenAi => self.config.get_openai_key(),
                Provider::Glm => self.config.get_glm_key(),
            };

            let new_provider = ProviderClient::new(
                new_provider_kind.clone(),
                api_key,
                self.endpoint.clone(),
                self.timeout,
            )?;

            self.provider = new_provider;
            self.provider_kind = new_provider_kind;
        }

        self.model = new_model.clone();

        println!("Switched to model: {}", new_model);
        println!("Provider: {}", self.provider.name());

        Ok(())
    }

    async fn show_mcp_status(&self) -> Result<()> {
        match &self.mcp_manager {
            None => {
                println!("MCP support is not enabled.");
                println!();
                println!("To use MCP servers, add them with:");
                println!("  zarz mcp add <name> --command <cmd> --args <arg1> <arg2>");
                println!();
                println!("Example:");
                println!("  zarz mcp add firecrawl --command npx --args -y firecrawl-mcp \\");
                println!("    --env FIRECRAWL_API_KEY=your-key");
                Ok(())
            }
            Some(manager) => {
                let servers = manager.list_servers().await;

                if servers.is_empty() {
                    println!("No MCP servers are currently running.");
                    println!();
                    println!("To add MCP servers, use:");
                    println!("  zarz mcp add <name> --command <cmd> --args <arg1> <arg2>");
                    return Ok(());
                }

                println!("Connected MCP Servers:");
                println!();

                for server_name in &servers {
                    // Get server info
                    if let Some(info) = manager.get_server_info(server_name).await {
                        stdout().execute(SetForegroundColor(Color::Green))?;
                        println!("  ● {}", server_name);
                        stdout().execute(ResetColor)?;
                        println!("    Server: {}", info);
                    } else {
                        stdout().execute(SetForegroundColor(Color::Yellow))?;
                        println!("  ◐ {}", server_name);
                        stdout().execute(ResetColor)?;
                        println!("    Status: Initializing...");
                    }

                    // Get tools for this server
                    match manager.get_all_tools().await {
                        Ok(all_tools) => {
                            if let Some(tools) = all_tools.get(server_name) {
                                if !tools.is_empty() {
                                    println!("    Tools ({}):", tools.len());
                                    for (i, tool) in tools.iter().enumerate() {
                                        if i < 5 {
                                            println!("      - {}: {}", tool.name, tool.description.as_deref().unwrap_or("No description"));
                                        }
                                    }
                                    if tools.len() > 5 {
                                        println!("      ... and {} more", tools.len() - 5);
                                    }
                                } else {
                                    println!("    Tools: None available");
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("    Error fetching tools: {}", e);
                        }
                    }
                    println!();
                }

                println!("Total servers: {}", servers.len());
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone)]
struct McpToolCall {
    server: String,
    tool: String,
    arguments: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone)]
struct ParsedToolCall {
    prefix: Option<String>,
    command_text: String,
    call: McpToolCall,
}

fn build_tool_prompt_section(tools_by_server: &HashMap<String, Vec<McpTool>>) -> String {
    let mut section = String::from(
        "Available MCP tools:\n\
Use CALL_MCP_TOOL server=<server_name> tool=<tool_name> args=<json_object> to request a tool.\n\
Only request a tool when it will help solve the task.\n",
    );

    let mut server_names: Vec<&String> = tools_by_server.keys().collect();
    server_names.sort();

    for server in server_names {
        section.push_str(&format!("\nServer {}:\n", server));
        if let Some(tools) = tools_by_server.get(server) {
            let mut ordered: Vec<&McpTool> = tools.iter().collect();
            ordered.sort_by(|a, b| a.name.cmp(&b.name));

            for tool in ordered.iter().take(8) {
                let description = tool
                    .description
                    .as_deref()
                    .unwrap_or("No description provided");
                section.push_str(&format!("  - {}: {}\n", tool.name, description));

                if let Ok(schema_str) = serde_json::to_string(&tool.input_schema) {
                    let snippet = truncate_inline(&schema_str, 200);
                    section.push_str(&format!("      schema: {}\n", snippet));
                }
            }

            if ordered.len() > 8 {
                section.push_str(&format!("  - ... ({} more)\n", ordered.len() - 8));
            }
        }
    }

    section
}

fn parse_mcp_tool_call(text: &str) -> Result<Option<ParsedToolCall>> {
    let Some(command_index) = text.find("CALL_MCP_TOOL") else {
        return Ok(None);
    };

    let prefix_text = text[..command_index].trim();
    let prefix = if prefix_text.is_empty() {
        None
    } else {
        Some(prefix_text.to_string())
    };

    let command_and_rest = text[command_index..].trim();

    // Ensure the tool call is the only content after the prefix (allow trailing whitespace)
    let (command_line, trailing_text) = if let Some(pos) = command_and_rest.find('\n') {
        let (line, rest) = command_and_rest.split_at(pos);
        (line.trim_end(), rest[pos + 1..].trim())
    } else {
        (command_and_rest, "")
    };

    if !trailing_text.is_empty() {
        anyhow::bail!("Additional text found after MCP tool call. Tool calls must be on a single line.");
    }

    let command_line = command_line.trim();
    if !command_line.starts_with("CALL_MCP_TOOL") {
        return Ok(None);
    }

    let remainder = command_line["CALL_MCP_TOOL".len()..].trim();
    let mut parts = remainder.splitn(3, ' ');

    let server_part = parts
        .next()
        .ok_or_else(|| anyhow!("Missing server component in MCP tool call"))?;
    let tool_part = parts
        .next()
        .ok_or_else(|| anyhow!("Missing tool component in MCP tool call"))?;
    let args_part = parts
        .next()
        .ok_or_else(|| anyhow!("Missing args component in MCP tool call"))?;

    if !server_part.starts_with("server=") {
        anyhow::bail!("Expected server=<server_name> in MCP tool call");
    }
    if !tool_part.starts_with("tool=") {
        anyhow::bail!("Expected tool=<tool_name> in MCP tool call");
    }
    if !args_part.starts_with("args=") {
        anyhow::bail!("Expected args=<json> in MCP tool call");
    }

    let server = server_part["server=".len()..].to_string();
    let tool = tool_part["tool=".len()..].to_string();
    let args_raw = args_part["args=".len()..].trim();

    if server.is_empty() {
        anyhow::bail!("Server name cannot be empty in MCP tool call");
    }

    if tool.is_empty() {
        anyhow::bail!("Tool name cannot be empty in MCP tool call");
    }

    let arguments = if args_raw.eq_ignore_ascii_case("null") {
        None
    } else {
        let value: Value = serde_json::from_str(args_raw)
            .with_context(|| "Failed to parse MCP tool call arguments as JSON")?;

        match value {
            Value::Null => None,
            Value::Object(map) => Some(map.into_iter().collect()),
            _ => {
                anyhow::bail!("Tool arguments must be a JSON object or null");
            }
        }
    };

    Ok(Some(ParsedToolCall {
        prefix,
        command_text: command_line.to_string(),
        call: McpToolCall {
            server,
            tool,
            arguments,
        },
    }))
}

fn format_tool_result(result: &CallToolResult) -> String {
    if result.content.is_empty() {
        return String::new();
    }

    let mut parts = Vec::new();

    for item in &result.content {
        match item {
            ToolContent::Text { text } => parts.push(text.clone()),
            ToolContent::Image { mime_type, .. } => {
                parts.push(format!("Image content returned (mime type: {})", mime_type));
            }
            ToolContent::Resource { resource } => {
                let name = if resource.name.is_empty() {
                    resource.uri.clone()
                } else {
                    format!("{} ({})", resource.name, resource.uri)
                };
                parts.push(format!("Resource: {}", name));
            }
        }
    }

    parts.join("\n")
}

fn log_tool_execution(server: &str, tool: &str, output: &str, is_error: bool) -> Result<()> {
    let mut out = stdout();
    let color = if is_error { Color::Yellow } else { Color::DarkGrey };

    out.execute(SetForegroundColor(color))?;

    if is_error {
        println!("MCP tool {}.{} returned an error.", server, tool);
    } else {
        println!("MCP tool {}.{} executed.", server, tool);
    }

    out.execute(ResetColor)?;

    let trimmed = output.trim();
    if !trimmed.is_empty() {
        println!("{}", truncate_for_display(trimmed, 600));
    }

    println!();
    Ok(())
}

fn truncate_for_display(text: &str, max_chars: usize) -> String {
    let mut result = String::new();
    let mut count = 0;

    for ch in text.chars() {
        if count >= max_chars {
            result.push_str("\n... (truncated)");
            break;
        }
        result.push(ch);
        count += 1;
    }

    result
}

fn truncate_inline(text: &str, max_chars: usize) -> String {
    let mut result = String::new();
    let mut count = 0;

    for ch in text.chars() {
        if count >= max_chars {
            result.push_str("... (truncated)");
            break;
        }
        if ch.is_control() && ch != '\n' && ch != '\t' {
            continue;
        }
        result.push(ch);
        count += 1;
    }

    result.replace('\n', " ")
}

fn strip_file_blocks(text: &str) -> String {
    let mut output = String::new();
    let mut lines = text.lines();

    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("```file:") {
            while let Some(next) = lines.next() {
                if next.trim() == "```" {
                    break;
                }
            }
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }

    output.trim_end_matches('\n').to_string()
}

fn print_assistant_message(text: &str) -> Result<()> {
    let mut out = stdout();
    out.execute(SetForegroundColor(Color::Green))?;
    out.execute(Print("Assistant: "))?;
    out.execute(ResetColor)?;
    println!("{}", text);
    println!();
    Ok(())
}

fn print_tool_command(command: &str) -> Result<()> {
    let mut out = stdout();
    out.execute(SetForegroundColor(Color::DarkGrey))?;
    println!("{}", command);
    out.execute(ResetColor)?;
    Ok(())
}

fn print_file_change_summary(path: &Path, before: &str, after: &str) -> Result<()> {
    let mut out = stdout();
    out.execute(SetForegroundColor(Color::Cyan)).ok();
    if before.is_empty() {
        println!("Creating {}", path.display());
    } else {
        println!("Changes in {}", path.display());
    }
    out.execute(ResetColor).ok();

    let diff = TextDiff::from_lines(before, after);
    let mut old_line = 1usize;
    let mut new_line = 1usize;
    let mut wrote_any = false;

    for change in diff.iter_all_changes() {
        let value = change.value().trim_end_matches('\n');
        match change.tag() {
            ChangeTag::Delete => {
                wrote_any = true;
                print_colored_diff_line('-', old_line, value, Color::Red);
                old_line += 1;
            }
            ChangeTag::Insert => {
                wrote_any = true;
                print_colored_diff_line('+', new_line, value, Color::Green);
                new_line += 1;
            }
            ChangeTag::Equal => {
                old_line += 1;
                new_line += 1;
            }
        }
    }

    if !wrote_any {
        println!("(No textual changes)");
    }

    Ok(())
}

fn print_colored_diff_line(prefix: char, line_number: usize, text: &str, color: Color) {
    let mut out = stdout();
    out.execute(SetForegroundColor(color)).ok();
    if text.is_empty() {
        println!("{} {:>5} |", prefix, line_number);
    } else {
        println!("{} {:>5} | {}", prefix, line_number, text);
    }
    out.execute(ResetColor).ok();
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
