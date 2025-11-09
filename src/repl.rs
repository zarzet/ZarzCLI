use anyhow::{anyhow, Context, Result};
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor};
use crossterm::{cursor, terminal::{self, ClearType}, ExecutableCommand, QueueableCommand};
use dialoguer::{theme::ColorfulTheme, Select};
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::hint::{Hint as RtHint, Hinter};
use rustyline::highlight::Highlighter;
use rustyline::history::DefaultHistory;
use rustyline::validate::{ValidationContext, ValidationResult, Validator};
use rustyline::{Cmd as RlCmd, ConditionalEventHandler as RlConditionalEventHandler, Context as RtContext, Editor, Event as RlBindingEvent, EventContext as RlEventContext, EventHandler as RlEventHandler, Helper, KeyCode as RlKeyCode, KeyEvent as RlKeyEvent, Modifiers as RlModifiers, RepeatCount as RlRepeatCount};
use similar::{ChangeTag, TextDiff};
use std::collections::HashMap;
use std::io::{stdout, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
    Mutex,
};
use std::time::{Duration as StdDuration, Instant};

use crate::auth;
use crate::cli::Provider;
use crate::config::Config;
use crate::conversation_store::{ConversationStore, ConversationSummary};
use crate::fs_ops::FileSystemOps;
use crate::mcp::types::{CallToolResult, ToolContent};
use crate::mcp::{McpManager, McpTool};
use crate::providers::{CompletionProvider, CompletionRequest, ProviderClient, ReasoningEffort, ToolCall};
use crate::session::{MessageMetadata, MessageRole, Session};
use crate::tools::{ToolExecutionContext, ToolRegistry};
use serde_json::{self, json, Value};
use sha2::{Digest, Sha256};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};

struct CommandInfo {
    name: &'static str,
    description: &'static str,
}

const COMMANDS: &[CommandInfo] = &[
    CommandInfo { name: "help", description: "Show this help message" },
    CommandInfo { name: "apply", description: "Apply pending file changes" },
    CommandInfo { name: "diff", description: "Show pending changes" },
    CommandInfo { name: "undo", description: "Clear pending changes" },
    CommandInfo { name: "edit", description: "Load a file for editing" },
    CommandInfo { name: "search", description: "Search for a symbol" },
    CommandInfo { name: "context", description: "Find relevant files" },
    CommandInfo { name: "files", description: "List currently loaded files" },
    CommandInfo { name: "model", description: "Switch to a different AI model" },
    CommandInfo { name: "mcp", description: "Show MCP servers and available tools" },
    CommandInfo { name: "resume", description: "Resume a previous chat session" },
    CommandInfo { name: "clear", description: "Clear conversation history" },
    CommandInfo { name: "login", description: "Configure API keys or sign in" },
    CommandInfo { name: "logout", description: "Remove stored API keys and sign out" },
    CommandInfo { name: "exit", description: "Exit the session" },
];

const OPENAI_OAUTH_MODELS: &[(&str, &str)] = &[
    ("gpt-5-codex", "Optimized for coding (default)"),
    ("gpt-5-codex-low", "Lower reasoning effort; cheaper via OAuth"),
    ("gpt-5-codex-medium", "Balanced reasoning depth"),
    ("gpt-5-codex-high", "High reasoning effort with detailed summaries"),
    ("gpt-5-minimal", "Strictly minimal reasoning, low verbosity"),
    ("gpt-5-low", "Low effort general GPT-5 variant"),
    ("gpt-5-medium", "Balanced GPT-5 variant"),
    ("gpt-5-high", "High reasoning effort GPT-5"),
    ("gpt-5-mini", "Lightweight GPT-5 for quick tasks"),
    ("gpt-5-nano", "Fastest GPT-5 option with minimal reasoning"),
];

#[derive(Clone, Default)]
struct CommandHelper;

#[derive(Clone)]
struct CommandHint(String);

impl RtHint for CommandHint {
    fn display(&self) -> &str {
        &self.0
    }

    fn completion(&self) -> Option<&str> {
        None
    }
}

impl Helper for CommandHelper {}

impl Hinter for CommandHelper {
    type Hint = CommandHint;

    fn hint(&self, line: &str, pos: usize, _: &RtContext<'_>) -> Option<Self::Hint> {
        if !line.starts_with('/') || pos == 0 {
            return None;
        }

        let upto_cursor = &line[..pos];
        if upto_cursor.contains(' ') {
            return None;
        }

        let partial = upto_cursor.trim_start_matches('/');

        let matches: Vec<&CommandInfo> = COMMANDS
            .iter()
            .filter(|info| info.name.starts_with(partial))
            .collect();

        if matches.is_empty() {
            return None;
        }

        let mut hint_text = String::from("\n");

        if partial.is_empty() {
            hint_text.push_str("Available commands (press ↓ to browse):\n");
        } else {
            hint_text.push_str(&format!("Matches for '/{}' (press ↓ to browse):\n", partial));
        }

        let name_width = 10usize;
        for info in matches.iter().take(6) {
            hint_text.push_str("  /");
            hint_text.push_str(info.name);
            if info.name.len() < name_width {
                hint_text.push_str(&" ".repeat(name_width - info.name.len()));
            } else {
                hint_text.push(' ');
            }
            hint_text.push_str(info.description);
            hint_text.push('\n');
        }

        if matches.len() > 6 {
            hint_text.push_str("  ...\n");
        }

        Some(CommandHint(hint_text.trim_end().to_string()))
    }
}

impl Completer for CommandHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        _line: &str,
        pos: usize,
        _ctx: &RtContext<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        Ok((pos, Vec::new()))
    }
}

impl Highlighter for CommandHelper {}

impl Validator for CommandHelper {
    fn validate(
        &self,
        ctx: &mut ValidationContext<'_>,
    ) -> rustyline::Result<ValidationResult> {
        let input = ctx.input();
        if input.trim().is_empty() {
            Ok(ValidationResult::Invalid(Some(
                "Input cannot be empty".to_string(),
            )))
        } else {
            Ok(ValidationResult::Valid(None))
        }
    }
}

const REPL_SYSTEM_PROMPT: &str = r#"You are ZarzCLI, an AI coding assistant for the terminal.

You are an interactive CLI tool that helps users with software engineering tasks.

IMPORTANT: Assist with authorized security testing, defensive security, CTF challenges, and educational contexts. Refuse requests for destructive techniques, DoS attacks, mass targeting, supply chain compromise, or detection evasion for malicious purposes.

## Bash Tool for Context Understanding

You have access to a `bash` tool for executing shell commands when needed. Use it ONLY when necessary:
- Search for files: `find . -name "*.rs"` or `find . -type f -name "pattern"`
- Search code content: `grep -r "function_name" src/` or `rg "pattern" --type rust`
- Read file contents: `cat path/to/file.rs` or `head -n 20 file.py`
- List directory structure: `ls -la src/` or `tree -L 2`
- Check git status: `git log --oneline -10` or `git diff`

CRITICAL TOOL USAGE RULES:
- Use tools ONLY when you genuinely need more information to answer the user
- If you can answer based on existing context, DO NOT use tools
- NEVER run the same command twice - all previous outputs are in your context (duplicates will be blocked immediately)
- IMPORTANT: After using 1-3 tools, you MUST provide a text response explaining what you found
- Do NOT chain multiple tool calls without explaining your findings to the user
- If a command fails, explain the issue instead of retrying
- You will be forcibly stopped after 3 consecutive tool calls without text responses

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
- /search <symbol> - Search for a symbol in the codebase
- /context <query> - Find relevant files for a query
- /files - List currently loaded files
- /model <name> - Switch to a different AI model
- /mcp - Show MCP servers and available tools
- /resume - Resume a previous chat session
- /clear - Clear conversation history
- /exit - Exit the session

Response Priority and Thinking Pattern:
1. If you can answer the user's question with existing information, do so immediately WITHOUT using tools
2. Only use tools when you genuinely lack critical information needed to answer
3. After each tool call (or at most 2-3 tool calls), you MUST provide a text response explaining:
   - What information you found
   - How it relates to the user's question
   - What you plan to do next (if more investigation is needed)
4. This "think out loud" pattern helps the user understand your process and prevents endless tool loops

Tone and style:
- Only use emojis if the user explicitly requests it
- Responses should be short and concise
- Focus on facts and problem-solving
- Avoid over-the-top validation or excessive praise

Provide clear, concise responses. When suggesting changes, always use the file block format above.

Conversation format:
- The prompt includes the recent transcript using prefixes like "User:", "Assistant:", and "Tool[server.tool]:".
- Always respond in the voice of "Assistant" to the most recent user message.
- File changes are applied automatically; never instruct the user to run /apply or similar commands.

MCP tool usage:
- When the prompt lists available MCP tools, call them directly using the tool name shown to you (e.g., `mcp__server__tool`). If function calling becomes unavailable, you may fall back to emitting `CALL_MCP_TOOL server=<server_name> tool=<tool_name> args=<json_object>`.
- The JSON must be minified on a single line. Use {} when no arguments are required.
- You can provide explanation text before and after the tool call on separate lines
- Example (fallback):
  "I'll search for that information.

  CALL_MCP_TOOL server=firecrawl tool=firecrawl_search args={"query":"example"}

  This will help me find the answer."
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
    logout_requested: bool,
    pending_command: Arc<Mutex<Option<String>>>,
    last_interrupt: Option<std::time::Instant>,
    current_mode: String,
    status_message: Option<String>,
    tool_registry: ToolRegistry,
}

impl Repl {
    fn command_list() -> &'static [CommandInfo] {
        COMMANDS
    }

    fn print_command_suggestions(partial: &str) -> Result<bool> {
        let matches: Vec<&CommandInfo> = Self::command_list()
            .iter()
            .filter(|info| info.name.starts_with(partial))
            .collect();

        if matches.is_empty() {
            return Ok(false);
        }

        stdout().execute(SetForegroundColor(Color::Yellow)).ok();
        if partial.is_empty() {
            println!("Available commands (press Enter to choose):");
        } else {
            println!(
                "Commands matching '/{}' (press Enter to choose):",
                partial
            );
        }
        for info in matches {
            println!("  /{:<8} - {}", info.name, info.description);
        }
        stdout().execute(ResetColor).ok();
        println!();
        std::io::stdout().flush().ok();

        Ok(true)
    }

    fn take_pending_command(&self) -> Option<String> {
        self.pending_command
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
    }

    fn refresh_provider(&mut self) -> Result<()> {
        let api_key = match self.provider_kind {
            Provider::Anthropic => self.config.get_anthropic_key(),
            Provider::OpenAi => self.config.get_openai_key(),
            Provider::Glm => self.config.get_glm_key(),
        };
        self.provider = ProviderClient::new(
            self.provider_kind.clone(),
            api_key,
            self.endpoint.clone(),
            self.timeout,
        )?;
        Ok(())
    }

    fn current_reasoning_effort(&self) -> Option<ReasoningEffort> {
        if self.provider_kind == Provider::OpenAi {
            self.config.get_openai_reasoning_effort()
        } else {
            None
        }
    }

    fn reasoning_effort_label(effort: Option<ReasoningEffort>) -> &'static str {
        match effort {
            None => "Auto (model default: medium)",
            Some(ReasoningEffort::Minimal) => "Minimal",
            Some(ReasoningEffort::Low) => "Low",
            Some(ReasoningEffort::Medium) => "Medium",
            Some(ReasoningEffort::High) => "High",
        }
    }

    fn prompt_openai_reasoning_effort(&mut self) -> Result<()> {
        let current = self.config.get_openai_reasoning_effort();
        let options = vec![
            "Auto (model default: medium)",
            "Minimal",
            "Low",
            "Medium",
            "High",
        ];
        let default_index = match current {
            None => 0,
            Some(ReasoningEffort::Minimal) => 1,
            Some(ReasoningEffort::Low) => 2,
            Some(ReasoningEffort::Medium) => 3,
            Some(ReasoningEffort::High) => 4,
        };

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select reasoning effort for OpenAI models")
            .items(&options)
            .default(default_index)
            .interact()?;

        let new_setting = match selection {
            0 => None,
            1 => Some(ReasoningEffort::Minimal),
            2 => Some(ReasoningEffort::Low),
            3 => Some(ReasoningEffort::Medium),
            4 => Some(ReasoningEffort::High),
            _ => current,
        };

        if new_setting != current {
            self.config.openai_reasoning_effort = new_setting;
            self.config.save()?;
            println!(
                "OpenAI reasoning effort set to {}",
                Self::reasoning_effort_label(new_setting)
            );
        }

        Ok(())
    }

    async fn login_wizard(&mut self) -> Result<()> {
        println!("\nAuthentication options:");
        let options = vec![
            "Configure API keys manually",
            "Sign in with ChatGPT (OAuth for OpenAI)",
            "Cancel",
        ];

        let choice = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Choose how you want to authenticate ZarzCLI")
            .items(&options)
            .default(0)
            .interact()?;

        match choice {
            0 => {
                println!("Launching interactive setup...\n");
                let mut config = Config::interactive_setup()?;
                auth::prepare_openai_environment(&mut config).await?;
                self.config = config;
                if let Err(err) = self.refresh_provider() {
                    eprintln!("Warning: Could not refresh provider: {err}");
                }
                println!("API keys updated for this session.");
            }
            1 => {
                println!("Opening browser for ChatGPT login...\n");
                let auth::ChatGptLoginResult {
                    oauth_tokens,
                    api_key,
                    project_id,
                    organization_id,
                    account_id,
                } = auth::login_with_chatgpt().await?;

                // Store OAuth tokens
                self.config.openai_oauth_tokens = Some(oauth_tokens);
                self.config.openai_project_id = project_id;
                self.config.openai_organization_id = organization_id;
                self.config.openai_chatgpt_account_id = account_id;

                // Store API key if we got one
                if let Some(api_key) = api_key {
                    self.config.openai_api_key = Some(api_key);
                }

                self.config.save()?;
                auth::prepare_openai_environment(&mut self.config).await?;
                println!("OAuth credentials stored. Refreshing provider...");
                if let Err(err) = self.refresh_provider() {
                    eprintln!("Warning: Could not refresh provider: {err}");
                }
            }
            _ => {
                println!("Login cancelled.");
            }
        }

        Ok(())
    }

    fn record_message(&mut self, role: MessageRole, content: String) {
        self.record_message_with_metadata(role, content, None);
    }

    fn record_message_with_metadata(
        &mut self,
        role: MessageRole,
        content: String,
        metadata: Option<MessageMetadata>,
    ) {
        self.session
            .add_message_with_metadata(role, content, metadata);
        self.persist_session_if_needed();
    }

    fn has_executed_bash_command(&self, command: &str) -> bool {
        let needle = format!("Command: {}", command);
        let count = self
            .session
            .conversation_history
            .iter()
            .filter(|message| {
                matches!(
                    message.role,
                    MessageRole::Tool { ref server, ref tool }
                        if server == "system" && tool == "bash"
                ) && message.content == needle
            })
            .count();
        count >= 10
    }

    fn draw_prompt_frame(&self) {
        let mut out = stdout();
        let width = terminal::size().map(|(w, _)| w as usize).unwrap_or(120);
        let border = "─".repeat(width);

        out.queue(cursor::Hide).ok();
        out.queue(cursor::MoveToColumn(0)).ok();
        out.queue(SetForegroundColor(Color::DarkGrey)).ok();
        out.queue(Print(&border)).ok();
        out.queue(Print("\r\n")).ok();
        out.queue(Print("\r\n")).ok();
        out.queue(Print(&border)).ok();
        out.queue(ResetColor).ok();
        out.queue(Print("\r\n")).ok();

        if let Some(msg) = &self.status_message {
            out.execute(SetForegroundColor(Color::Yellow)).ok();
            out.queue(Print(msg)).ok();
            out.execute(ResetColor).ok();
        } else {
            out.execute(SetForegroundColor(Color::Green)).ok();
            out.queue(Print(format!("  ⏵⏵ Mode: {}", self.current_mode))).ok();
            out.execute(ResetColor).ok();
        }

        out.queue(cursor::MoveUp(2)).ok();
        out.queue(cursor::MoveToColumn(0)).ok();
        out.queue(cursor::Show).ok();
        out.flush().ok();
    }

    fn clear_prompt_frame() {
        let mut out = stdout();
        out.queue(cursor::Hide).ok();
        out.queue(cursor::MoveUp(1)).ok();
        out.queue(cursor::MoveToColumn(0)).ok();
        out.queue(terminal::Clear(ClearType::CurrentLine)).ok();
        out.queue(cursor::MoveDown(1)).ok();
        out.queue(cursor::MoveToColumn(0)).ok();
        out.queue(terminal::Clear(ClearType::CurrentLine)).ok();
        out.queue(cursor::MoveDown(1)).ok();
        out.queue(cursor::MoveToColumn(0)).ok();
        out.queue(terminal::Clear(ClearType::CurrentLine)).ok();
        out.queue(cursor::MoveDown(1)).ok();
        out.queue(cursor::MoveToColumn(0)).ok();
        out.queue(terminal::Clear(ClearType::CurrentLine)).ok();
        out.queue(cursor::MoveUp(3)).ok();
        out.queue(cursor::MoveToColumn(0)).ok();
        out.queue(cursor::Show).ok();
        out.flush().ok();
    }

    fn persist_session_if_needed(&mut self) {
        if self.session.conversation_history.is_empty() {
            return;
        }

        if let Err(err) = ConversationStore::save_session(
            &mut self.session,
            self.provider_kind.clone(),
            &self.model,
        ) {
            eprintln!("Warning: Failed to save session history: {:#}", err);
        }
    }

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
            logout_requested: false,
            pending_command: Arc::new(Mutex::new(None)),
            last_interrupt: None,
            current_mode: "Auto".to_string(),
            status_message: None,
            tool_registry: ToolRegistry::new(),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let mut editor: Editor<CommandHelper, DefaultHistory> = Editor::new()
            .context("Failed to initialize readline editor")?;
        editor.set_helper(Some(CommandHelper::default()));

        let handler_down = CommandMenuHandler::new(self.pending_command.clone());
        editor.bind_sequence(
            RlKeyEvent(RlKeyCode::Down, RlModifiers::NONE),
            RlEventHandler::Conditional(Box::new(handler_down)),
        );
        let handler_up = CommandMenuHandler::new(self.pending_command.clone());
        editor.bind_sequence(
            RlKeyEvent(RlKeyCode::Up, RlModifiers::NONE),
            RlEventHandler::Conditional(Box::new(handler_up)),
        );

        loop {
            self.draw_prompt_frame();
            let readline = editor.readline("> ");

            match readline {
                Ok(line) => {
                    self.last_interrupt = None;
                    self.status_message = None;

                    Self::clear_prompt_frame();

                    let line = line.trim();

                    if line.is_empty() {
                        continue;
                    }

                    let mut out = stdout();
                    out.execute(terminal::Clear(ClearType::CurrentLine)).ok();
                    out.execute(cursor::MoveToColumn(0)).ok();
                    println!("> {}", line);

                    editor.add_history_entry(line)
                        .context("Failed to add history entry")?;

                    if line.starts_with('/') {
                        if let Err(e) = self.handle_command(line).await {
                            eprintln!("Error: {:#}", e);
                        }

                        if self.logout_requested {
                            break;
                        }

                        if line == "/exit" {
                            break;
                        }
                    } else {
                        if self.logout_requested {
                            break;
                        }

                        if let Err(e) = self.handle_user_input(line).await {
                            eprintln!("Error: {:#}", e);
                        }

                        if self.logout_requested {
                            break;
                        }
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    if let Some(cmd) = self.take_pending_command() {
                        Self::clear_prompt_frame();
                        println!("> {}", cmd);
                        editor
                            .add_history_entry(cmd.as_str())
                            .context("Failed to add history entry")?;
                        if let Err(e) = self.handle_command(&cmd).await {
                            eprintln!("Error: {:#}", e);
                        }

                        if self.logout_requested {
                            break;
                        }

                        continue;
                    }

                    let now = std::time::Instant::now();
                    if let Some(last) = self.last_interrupt {
                        if now.duration_since(last).as_secs() < 2 {
                            Self::clear_prompt_frame();
                            println!();
                            println!("Exiting...");
                            break;
                        }
                    }

                    Self::clear_prompt_frame();
                    self.last_interrupt = Some(now);
                    self.status_message = Some("  Press Ctrl+C again to exit, or continue typing...".to_string());

                    continue;
                }
                Err(ReadlineError::Eof) => {
                    Self::clear_prompt_frame();
                    println!("Exiting");
                    break;
                }
                Err(err) => {
                    Self::clear_prompt_frame();
                    eprintln!("Error: {:#}", err);
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_command(&mut self, command: &str) -> Result<()> {
        let parts: Vec<&str> = command.splitn(2, ' ').collect();
        let cmd = parts[0];
        let args = parts.get(1).copied().unwrap_or("");

        if cmd == "/" {
            let matches: Vec<&CommandInfo> = Self::command_list().iter().collect();
            if let Some(choice) = pick_command_menu("", &matches, 0)? {
                let mut selected_command = format!("/{}", choice.name);
                if !args.is_empty() {
                    selected_command.push(' ');
                    selected_command.push_str(args);
                }
                return Self::execute_command(self, &selected_command).await;
            }
            return Ok(());
        }

        if let Some(partial) = cmd.strip_prefix('/') {
            if !partial.is_empty() && !Self::command_list().iter().any(|info| info.name == partial) {
                let matches: Vec<&CommandInfo> = Self::command_list()
                    .iter()
                    .filter(|info| info.name.starts_with(partial))
                    .collect();

                if matches.len() == 1 {
                    let mut selected_command = format!("/{}", matches[0].name);
                    if !args.is_empty() {
                        selected_command.push(' ');
                        selected_command.push_str(args);
                    }
                    return Self::execute_command(self, &selected_command).await;
                } else if matches.len() > 1 {
                    if let Some(choice) = pick_command_menu(partial, &matches, 0)? {
                        let mut selected_command = format!("/{}", choice.name);
                        if !args.is_empty() {
                            selected_command.push(' ');
                            selected_command.push_str(args);
                        }
                        return Self::execute_command(self, &selected_command).await;
                    } else {
                        return Ok(());
                    }
                } else if Self::print_command_suggestions(partial)? {
                    return Ok(());
                }
            }
        }

        Self::execute_command(self, command).await
    }

    async fn execute_command(&mut self, command: &str) -> Result<()> {
        let parts: Vec<&str> = command.splitn(2, ' ').collect();
        let cmd = parts[0];
        let args = parts.get(1).copied().unwrap_or("");

        match cmd {
            "/help" => self.show_help(),
            "/exit" => {
                println!("Goodbye!");
                Ok(())
            }
            "/apply" => self.apply_changes().await,
            "/diff" => self.show_diff(),
            "/undo" => self.undo_changes(),
            "/edit" => self.edit_file(args).await,
            "/search" => self.search_symbol(args).await,
            "/context" => self.find_context(args).await,
            "/files" => self.list_files(),
            "/model" => self.switch_model(args).await,
            "/mcp" => self.show_mcp_status().await,
            "/resume" => self.resume_session(args).await,
            "/clear" => self.clear_history(),
            "/login" => self.login_wizard().await,
            "/logout" => self.logout(),
            _ => {
                println!("Unknown command: {}", cmd);
                println!("Type /help for available commands");
                Ok(())
            }
        }
    }

    async fn handle_user_input(&mut self, input: &str) -> Result<()> {
        if self.logout_requested {
            return Err(anyhow!(
                "You have logged out. Restart ZarzCLI and run 'zarz config' to sign in again."
            ));
        }

        self.record_message(MessageRole::User, input.to_string());

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

        let builtin_specs = self.tool_registry.specs();
        let ToolRegistryConfig {
            specs: tool_specs,
            map: tool_name_map,
        } = build_tool_registry(&builtin_specs, tools_snapshot.as_ref());

        self.session.normalize_tool_history();

        let mut _tool_calls = 0usize;
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

            let structured_messages = if self.provider_kind == Provider::OpenAi {
                Some(self.session.build_openai_messages())
            } else {
                None
            };

            let request = CompletionRequest {
                model: self.model.clone(),
                system_prompt: Some(REPL_SYSTEM_PROMPT.to_string()),
                user_prompt: prompt.clone(),
                max_output_tokens: self.max_tokens,
                temperature: self.temperature,
                messages: structured_messages,
                tools: Some(tool_specs.clone()),
                reasoning_effort: self.current_reasoning_effort(),
            };

            let spinner = Spinner::start("Thinking...".to_string());
            let response_result = self.provider.complete(&request).await;
            spinner.stop().await;
            let mut response = response_result?;

            while !response.tool_calls.is_empty() {

                let is_anthropic = self.provider.name() == "anthropic";

                let mut messages = if is_anthropic {
                    vec![json!({
                        "role": "user",
                        "content": [{
                            "type": "text",
                            "text": prompt
                        }]
                    })]
                } else {
                    let mut msgs = Vec::new();
                    if let Some(system) = &request.system_prompt {
                        msgs.push(json!({
                            "role": "system",
                            "content": system
                        }));
                    }
                    msgs.push(json!({
                        "role": "user",
                        "content": prompt
                    }));
                    msgs
                };

                if is_anthropic {
                    let mut assistant_content = Vec::new();
                    if !response.text.is_empty() {
                        assistant_content.push(json!({
                            "type": "text",
                            "text": response.text
                        }));
                    }

                    for tool_call in response.tool_calls.clone() {
                        assistant_content.push(json!({
                            "type": "tool_use",
                            "id": tool_call.id,
                            "name": tool_call.name,
                            "input": tool_call.input
                        }));
                    }

                    messages.push(json!({
                        "role": "assistant",
                        "content": assistant_content
                    }));
                } else {
                    let mut openai_tool_calls = Vec::new();
                    for tool_call in response.tool_calls.clone() {
                        openai_tool_calls.push(json!({
                            "id": tool_call.id,
                            "type": "function",
                            "function": {
                                "name": tool_call.name,
                                "arguments": tool_call.input.to_string()
                            }
                        }));
                    }

                    messages.push(json!({
                        "role": "assistant",
                        "content": response.text,
                        "tool_calls": openai_tool_calls
                    }));
                }

                let mut executed_any = false;

                for tool_call in &response.tool_calls {

                    match tool_name_map.get(&tool_call.name) {
                        Some(tool_entry) => match tool_entry {
                            RegisteredTool::Bash => {
                                executed_any = true;
                                _tool_calls += 1;

                                let command = match extract_bash_command(&tool_call.input) {
                                    Ok(cmd) => cmd,
                                    Err(err_msg) => {
                                        append_tool_response_message(
                                            &mut messages,
                                            is_anthropic,
                                            &tool_call.id,
                                            &err_msg,
                                        );
                                        let metadata =
                                            Some(MessageMetadata::for_tool_output(tool_call.id.clone()));
                                        self.record_message_with_metadata(
                                            MessageRole::Tool {
                                                server: "system".to_string(),
                                                tool: "bash".to_string()
                                            },
                                            err_msg,
                                            metadata,
                                        );
                                        continue;
                                    }
                                };

                                println!();
                                stdout().execute(SetForegroundColor(Color::Cyan))?;
                                println!("  $ {}", command);
                                stdout().execute(ResetColor)?;

                                let command_repeated =
                                    self.has_executed_bash_command(command.as_str());

                                let command_metadata = Some(MessageMetadata::for_tool_command(
                                    tool_call.id.clone(),
                                    Some(tool_call.input.clone()),
                                ));

                                self.record_message_with_metadata(
                                    MessageRole::Tool {
                                        server: "system".to_string(),
                                        tool: "bash".to_string()
                                    },
                                    format!("Command: {}", command),
                                    command_metadata,
                                );

                                let command_output = if command_repeated {
                                    format!(
                                        "WARNING: Command '{}' has already been executed 10 times in this session.",
                                        command
                                    )
                                } else {
                                    execute_bash_command(&command, &self.session.working_directory)?.output
                                };

                                let output_metadata =
                                    Some(MessageMetadata::for_tool_output(tool_call.id.clone()));

                                self.record_message_with_metadata(
                                    MessageRole::Tool {
                                        server: "system".to_string(),
                                        tool: "bash".to_string()
                                    },
                                    format!("Output:\n{}", command_output),
                                    output_metadata,
                                );

                                let (preview, total_chars, was_truncated) =
                                    take_first_chars_with_total(&command_output, 4000);
                                let truncated = if was_truncated {
                                    format!(
                                        "{}... (truncated, {} total chars)",
                                        preview, total_chars
                                    )
                                } else {
                                    command_output.clone()
                                };

                                let mut out = stdout();
                                let color = if command_repeated {
                                    Color::Yellow
                                } else {
                                    Color::DarkGrey
                                };
                                out.execute(SetForegroundColor(color)).ok();
                                write!(out, "{}", truncated).ok();
                                if !truncated.ends_with('\n') {
                                    writeln!(out).ok();
                                }
                                out.execute(ResetColor).ok();
                                out.flush().ok();

                                append_tool_response_message(
                                    &mut messages,
                                    is_anthropic,
                                    &tool_call.id,
                                    &truncated,
                                );
                            }
                            RegisteredTool::Builtin(tool_name) => {
                                executed_any = true;
                                _tool_calls += 1;
                                self.handle_builtin_tool(tool_name, tool_call, &mut messages, is_anthropic);
                            }
                            RegisteredTool::Mcp { server, tool } => {
                                executed_any = true;
                                _tool_calls += 1;

                                let server_name = server.clone();
                                let tool_name = tool.clone();

                                println!();
                                stdout().execute(SetForegroundColor(Color::Cyan))?;
                                println!("  ⚙ MCP {}.{}", server_name, tool_name);
                                stdout().execute(ResetColor)?;

                                let args_display = if tool_call.input.is_null() {
                                    "Arguments: null".to_string()
                                } else {
                                    match serde_json::to_string_pretty(&tool_call.input) {
                                        Ok(pretty) => format!("Arguments:\n{}", pretty),
                                        Err(_) => format!("Arguments: {}", tool_call.input),
                                    }
                                };

                                let command_metadata = Some(MessageMetadata::for_tool_command(
                                    tool_call.id.clone(),
                                    Some(tool_call.input.clone()),
                                ));

                                self.record_message_with_metadata(
                                    MessageRole::Tool {
                                        server: server_name.clone(),
                                        tool: tool_name.clone(),
                                    },
                                    args_display,
                                    command_metadata,
                                );

                                let arguments = match extract_tool_arguments(&tool_call.input) {
                                    Ok(args) => args,
                                    Err(message) => {
                                        if is_anthropic {
                                            let tool_result_content = vec![json!({
                                                "type": "tool_result",
                                                "tool_use_id": tool_call.id,
                                                "content": message
                                            })];
                                            messages.push(json!({
                                                "role": "user",
                                                "content": tool_result_content
                                            }));
                                        } else {
                                            messages.push(json!({
                                                "role": "tool",
                                                "tool_call_id": tool_call.id,
                                                "content": message.clone()
                                            }));
                                        }
                                        let error_metadata =
                                            Some(MessageMetadata::for_tool_output(tool_call.id.clone()));
                                        self.record_message_with_metadata(
                                            MessageRole::Tool {
                                                server: server_name.clone(),
                                                tool: tool_name.clone(),
                                            },
                                            message,
                                            error_metadata,
                                        );
                                        continue;
                                    }
                                };

                                let manager = match &self.mcp_manager {
                                    Some(mgr) => mgr.clone(),
                                    None => {
                                        let warning = "ERROR: MCP tools are not available in this session.".to_string();
                                        if is_anthropic {
                                            let tool_result_content = vec![json!({
                                                "type": "tool_result",
                                                "tool_use_id": tool_call.id,
                                                "content": warning.clone()
                                            })];
                                            messages.push(json!({
                                                "role": "user",
                                                "content": tool_result_content
                                            }));
                                        } else {
                                            messages.push(json!({
                                                "role": "tool",
                                                "tool_call_id": tool_call.id,
                                                "content": warning.clone()
                                            }));
                                        }
                                        let warning_metadata =
                                            Some(MessageMetadata::for_tool_output(tool_call.id.clone()));
                                        self.record_message_with_metadata(
                                            MessageRole::Tool {
                                                server: server_name.clone(),
                                                tool: tool_name.clone(),
                                            },
                                            warning,
                                            warning_metadata,
                                        );
                                        continue;
                                    }
                                };

                                let spinner = Spinner::start(format!(
                                    "Running MCP {}.{}...",
                                    server_name, tool_name
                                ));
                                let tool_result = manager
                                    .call_tool(&server_name, tool_name.clone(), arguments.clone())
                                    .await;
                                spinner.stop().await;

                                let (mut tool_output, is_error) = match tool_result {
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

                                let output_metadata =
                                    Some(MessageMetadata::for_tool_output(tool_call.id.clone()));
                                self.record_message_with_metadata(
                                    MessageRole::Tool {
                                        server: server_name.clone(),
                                        tool: tool_name.clone(),
                                    },
                                    stored_output,
                                    output_metadata,
                                );

                                log_tool_execution(&server_name, &tool_name, &tool_output, is_error)?;

                                let (preview, total_chars, was_truncated) =
                                    take_first_chars_with_total(&tool_output, 4000);
                                let truncated = if was_truncated {
                                    format!(
                                        "{}... (truncated, {} total chars)",
                                        preview, total_chars
                                    )
                                } else {
                                    tool_output.clone()
                                };

                                if is_anthropic {
                                    let tool_result_content = vec![json!({
                                        "type": "tool_result",
                                        "tool_use_id": tool_call.id,
                                        "content": truncated
                                    })];
                                    messages.push(json!({
                                        "role": "user",
                                        "content": tool_result_content
                                    }));
                                } else {
                                    messages.push(json!({
                                        "role": "tool",
                                        "tool_call_id": tool_call.id,
                                        "content": truncated
                                    }));
                                }
                            }
                        },
                        None => {
                            executed_any = true;
                            _tool_calls += 1;

                            let warning = format!(
                                "ERROR: Tool '{}' is not registered in this session.",
                                tool_call.name
                            );
                            stdout().execute(SetForegroundColor(Color::Yellow)).ok();
                            println!("{}", warning);
                            stdout().execute(ResetColor).ok();

                            if is_anthropic {
                                let tool_result_content = vec![json!({
                                    "type": "tool_result",
                                    "tool_use_id": tool_call.id,
                                    "content": warning.clone()
                                })];
                                messages.push(json!({
                                    "role": "user",
                                    "content": tool_result_content
                                }));
                            } else {
                                messages.push(json!({
                                    "role": "tool",
                                    "tool_call_id": tool_call.id,
                                    "content": warning.clone()
                                }));
                            }

                            let missing_tool_metadata =
                                Some(MessageMetadata::for_tool_output(tool_call.id.clone()));
                            self.record_message_with_metadata(
                                MessageRole::Tool {
                                    server: "system".to_string(),
                                    tool: tool_call.name.clone(),
                                },
                                warning,
                                missing_tool_metadata,
                            );
                        }
                    }
                }

                if !executed_any {
                    break;
                }

                let follow_up_request = CompletionRequest {
                    model: self.model.clone(),
                    system_prompt: Some(REPL_SYSTEM_PROMPT.to_string()),
                    user_prompt: String::new(),
                    max_output_tokens: self.max_tokens,
                    temperature: self.temperature,
                    messages: Some(messages),
                    tools: Some(tool_specs.clone()),
                    reasoning_effort: self.current_reasoning_effort(),
                };

                let spinner = Spinner::start("Thinking...".to_string());
                let follow_up_result = self.provider.complete(&follow_up_request).await;
                spinner.stop().await;
                response = follow_up_result?;
            }

            let raw_text = response.text;

            match parse_mcp_tool_call(&raw_text) {
                Ok(Some(parsed)) => {
                    if let Some(prefix_text) = parsed.prefix.as_deref() {
                        let display = strip_file_blocks(prefix_text);
                        if !display.trim().is_empty() {
                            print_assistant_message(&display, &self.model)?;
                        }
                        self.record_message(
                            MessageRole::Assistant,
                            prefix_text.to_string(),
                        );
                    } else {
                        let note = format!(
                            "Calling MCP tool {}.{}...",
                            parsed.call.server, parsed.call.tool
                        );
                        print_assistant_message(&note, &self.model)?;
                        self.record_message(MessageRole::Assistant, note);
                    }

                    self.record_message(
                        MessageRole::Assistant,
                        parsed.command_text.clone(),
                    );
                    print_tool_command(&parsed.command_text)?;

                    if self.mcp_manager.is_none() {
                        stdout().execute(SetForegroundColor(Color::Yellow)).ok();
                        println!("MCP tool request ignored: no MCP manager configured.");
                        stdout().execute(ResetColor).ok();

                        self.record_message(
                            MessageRole::Tool {
                                server: parsed.call.server.clone(),
                                tool: parsed.call.tool.clone(),
                            },
                            "ERROR: MCP tools are not available in this session.".to_string(),
                        );

                        continue;
                    }

                    let manager = self.mcp_manager.as_ref().unwrap();

                    let spinner = Spinner::start(format!(
                        "Running MCP {}.{}...",
                        parsed.call.server, parsed.call.tool
                    ));
                    let tool_result = manager
                        .call_tool(
                            &parsed.call.server,
                            parsed.call.tool.clone(),
                            parsed.call.arguments.clone(),
                        )
                        .await;
                    spinner.stop().await;

                    let (mut tool_output, is_error) = match tool_result {
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

                    _tool_calls += 1;

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

                    self.record_message(
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

                    if let Some(suffix_text) = parsed.suffix.as_deref() {
                        let display = strip_file_blocks(suffix_text);
                        if !display.trim().is_empty() {
                            print_assistant_message(&display, &self.model)?;
                        }
                        self.record_message(
                            MessageRole::Assistant,
                            suffix_text.to_string(),
                        );
                    }

                    continue;
                }
                Ok(None) => {
                    let response_text = raw_text.clone();
                    final_response = Some(response_text.clone());
                    self.record_message(MessageRole::Assistant, response_text);
                    break;
                }
                Err(parse_error) => {
                    self.record_message(MessageRole::Assistant, raw_text.clone());
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
                print_assistant_message(&printable, &self.model)?;
            }

            let file_blocks = parse_file_blocks(&text);
            if !file_blocks.is_empty() {
                self.process_file_blocks(file_blocks).await?;
            }
        }

        Ok(())
    }

    fn handle_builtin_tool(
        &mut self,
        tool_name: &str,
        tool_call: &ToolCall,
        messages: &mut Vec<Value>,
        is_anthropic: bool,
    ) {
        let args_display = if tool_call.input.is_null() {
            "Arguments: null".to_string()
        } else {
            match serde_json::to_string_pretty(&tool_call.input) {
                Ok(pretty) => format!("Arguments:\n{}", pretty),
                Err(_) => format!("Arguments: {}", tool_call.input),
            }
        };

        if let Some(lines) = summarize_builtin_tool_action(tool_name, &tool_call.input) {
            for line in lines {
                println!("{}", line);
            }
        }

        let metadata = Some(MessageMetadata::for_tool_command(
            tool_call.id.clone(),
            Some(tool_call.input.clone()),
        ));
        self.record_message_with_metadata(
            MessageRole::Tool {
                server: "builtin".to_string(),
                tool: tool_name.to_string(),
            },
            args_display,
            metadata,
        );

        let ctx = ToolExecutionContext {
            working_directory: &self.session.working_directory,
        };

        let execution = self
            .tool_registry
            .execute(tool_name, ctx, &tool_call.input);

        let (content, success) = match execution {
            Ok(output) => (output.content, output.success),
            Err(err) => (format!("ERROR: {}", err), false),
        };

        let output_metadata = Some(MessageMetadata::for_tool_output(tool_call.id.clone()));
        self.record_message_with_metadata(
            MessageRole::Tool {
                server: "builtin".to_string(),
                tool: tool_name.to_string(),
            },
            content.clone(),
            output_metadata,
        );

        let (preview, total_chars, was_truncated) = take_first_chars_with_total(&content, 4000);
        let truncated = if was_truncated {
            format!("{}... (truncated, {} total chars)", preview, total_chars)
        } else {
            preview
        };

        let mut out = stdout();
        if tool_name == "read_file" {
            out.execute(SetForegroundColor(Color::DarkGrey)).ok();
            println!(
                "    (content captured; {} characters)",
                content.chars().count()
            );
            out.execute(ResetColor).ok();
        } else if !truncated.trim().is_empty() {
            let color = if success {
                Color::DarkGrey
            } else {
                Color::Yellow
            };
            out.execute(SetForegroundColor(color)).ok();
            write!(out, "{}", truncated).ok();
            if !truncated.ends_with('\n') {
                writeln!(out).ok();
            }
            out.execute(ResetColor).ok();
        }
        out.flush().ok();

        append_tool_response_message(messages, is_anthropic, &tool_call.id, &content);
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
        println!("  /search <name>  - Search for a symbol");
        println!("  /context <query>- Find relevant files");
        println!("  /files          - List loaded files");
        println!("  /model <name>   - Switch to a different AI model");
        println!("                    Examples: claude-sonnet-4-5-20250929, claude-haiku-4-5,");
        println!("                              gpt-5-codex-high, gpt-5-mini, glm-4.6");
        println!("  /mcp            - Show MCP servers and available tools");
        println!("  /resume         - Resume a previous chat session");
        println!("  /clear          - Clear conversation history");
        println!("  /logout         - Remove stored API keys and sign out");
        println!("  /exit           - Exit the session");
        println!();
        println!("Current model: {}", self.model);
        println!("Current provider: {}", self.provider.name());
        if self.provider_kind == Provider::OpenAi {
            println!(
                "OpenAI reasoning effort: {}",
                Self::reasoning_effort_label(self.current_reasoning_effort())
            );
        }
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
        self.session.reset_metadata();
        println!("Conversation history cleared");
        Ok(())
    }

    async fn resume_session(&mut self, args: &str) -> Result<()> {
        let summaries = ConversationStore::list_summaries()?;

        if summaries.is_empty() {
            println!("No saved sessions found.");
            return Ok(());
        }

        let trimmed = args.trim();

        let selected_summary = if trimmed.is_empty() {
            let items: Vec<String> = summaries
                .iter()
                .map(|summary| format_session_line(summary))
                .collect();

            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select a session to resume")
                .items(&items)
                .default(0)
                .interact_opt()?;

            match selection {
                Some(index) => summaries.get(index).cloned(),
                None => {
                    println!("Resume cancelled.");
                    return Ok(());
                }
            }
        } else {
            let needle = trimmed.to_ascii_lowercase();
            summaries
                .iter()
                .find(|summary| {
                    summary.id.to_ascii_lowercase().starts_with(&needle)
                        || summary
                            .title
                            .to_ascii_lowercase()
                            .contains(&needle)
                })
                .cloned()
        };

        let Some(summary) = selected_summary else {
            println!("No saved session matches '{}'.", trimmed);
            return Ok(());
        };

        let snapshot = ConversationStore::load_snapshot(&summary.id)?;

        let previous_provider = self.provider_kind.clone();
        let provider_kind = Provider::from_str(&snapshot.provider).ok_or_else(|| {
            anyhow!(
                "Unknown provider '{}' in saved session",
                snapshot.provider
            )
        })?;

        let switching_provider = provider_kind != previous_provider;

        if switching_provider {
            let api_key = match provider_kind {
                Provider::Anthropic => self.config.get_anthropic_key(),
                Provider::OpenAi => self.config.get_openai_key(),
                Provider::Glm => self.config.get_glm_key(),
            };

            let client = ProviderClient::new(
                provider_kind.clone(),
                api_key,
                self.endpoint.clone(),
                self.timeout,
            )?;

            self.provider = client;
            self.provider_kind = provider_kind;
        }

        let previous_model = self.model.clone();
        self.model = snapshot.model.clone();
        self.session.conversation_history = snapshot.messages.clone();
        self.session.storage_id = Some(snapshot.id.clone());
        self.session.title = Some(snapshot.title.clone());
        self.session.created_at = Some(snapshot.created_at);
        self.session.updated_at = Some(snapshot.updated_at);
        self.session.pending_changes.clear();
        self.session.current_files.clear();

        if !snapshot.working_directory.eq(&self.session.working_directory) {
            println!(
                "Note: saved session was created in {}",
                snapshot.working_directory.display()
            );
        }

        if switching_provider || self.model != previous_model {
            println!(
                "Active provider/model set to {} / {}",
                snapshot.provider, self.model
            );
        }

        let formatted_time = snapshot
            .updated_at
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string();

        println!(
            "Resumed session '{}' [{} • {}] ({} messages, updated {})",
            snapshot.title,
            snapshot.provider,
            snapshot.model,
            snapshot.message_count,
            formatted_time
        );

        if let Some(last_reply) = snapshot
            .messages
            .iter()
            .rev()
            .find(|message| matches!(message.role, MessageRole::Assistant))
        {
            let preview = truncate_for_display(&last_reply.content, 240);
            if !preview.trim().is_empty() {
                println!();
                print_assistant_message(&preview, &self.model)?;
            }
        }

        Ok(())
    }

    fn logout(&mut self) -> Result<()> {
        let config_path = Config::config_path()?;
        let had_keys = self.config.clear_api_keys()?;

        let mut env_removed = false;
        for var in ["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GLM_API_KEY"] {
            if std::env::var(var).is_ok() {
                env_removed = true;
            }
            unsafe {
                std::env::remove_var(var);
            }
        }

        if had_keys {
            println!(
                "Stored API keys removed from {}",
                config_path.display()
            );
        } else {
            println!(
                "No stored API keys found at {}",
                config_path.display()
            );
        }

        if env_removed {
            println!("Cleared API key environment variables for this session.");
        } else {
            println!("No API key environment variables were set for this session.");
        }

        println!("Restart ZarzCLI to complete logout. Run 'zarz config' to sign in again.");
        self.logout_requested = true;
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
            println!("  OpenAI (ChatGPT OAuth-ready):");
            for (model, blurb) in OPENAI_OAUTH_MODELS {
                println!("    {:<32} - {}", model, blurb);
            }
            println!();
            println!("  GLM (Z.AI - International):");
            println!("    glm-4.6                          - Best for coding (200K context)");
            println!("    glm-4.5                          - Previous generation");
            println!();
            if self.provider_kind == Provider::OpenAi {
                println!(
                    "OpenAI reasoning effort: {}",
                    Self::reasoning_effort_label(self.current_reasoning_effort())
                );
                println!("You will be prompted to adjust this when selecting an OpenAI model.");
                println!();
            }
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
        if self.provider_kind == Provider::OpenAi {
            self.prompt_openai_reasoning_effort()?;
        }

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

                let tools_by_server = match manager.get_all_tools().await {
                    Ok(map) => map,
                    Err(e) => {
                        eprintln!("Warning: Failed to fetch MCP tools: {}", e);
                        HashMap::new()
                    }
                };

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
                    if let Some(tools) = tools_by_server.get(server_name) {
                        if !tools.is_empty() {
                            println!("    Tools ({}):", tools.len());
                            for (i, tool) in tools.iter().enumerate() {
                                if i < 5 {
                                    let description = tool
                                        .description
                                        .as_deref()
                                        .map(|d| truncate_inline(d, 160))
                                        .unwrap_or_else(|| "No description".to_string());
                                    println!("      - {}: {}", tool.name, description);
                                }
                            }
                            if tools.len() > 5 {
                                println!("      ... and {} more", tools.len() - 5);
                            }
                        } else {
                            println!("    Tools: None available");
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

fn format_session_line(summary: &ConversationSummary) -> String {
    let time_str = summary
        .updated_at
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M")
        .to_string();

    let mut title = summary.title.clone();
    if title.len() > 60 {
        title.truncate(60);
        title.push('…');
    }

    let plural = if summary.message_count == 1 { "" } else { "s" };

    format!(
        "{} │ {} [{} • {}] • {} message{} (id: {})",
        time_str,
        title,
        summary.provider,
        summary.model,
        summary.message_count,
        plural,
        summary.id
    )
}

#[derive(Clone)]
struct CommandMenuHandler {
    pending_command: Arc<Mutex<Option<String>>>,
}

impl CommandMenuHandler {
    fn new(pending_command: Arc<Mutex<Option<String>>>) -> Self {
        Self { pending_command }
    }
}

impl RlConditionalEventHandler for CommandMenuHandler {
    fn handle(
        &self,
        evt: &RlBindingEvent,
        _n: RlRepeatCount,
        _positive: bool,
        ctx: &RlEventContext,
    ) -> Option<RlCmd> {
        let Some(key) = evt.get(0) else {
            return None;
        };

        let is_navigation = *key == RlKeyEvent(RlKeyCode::Down, RlModifiers::NONE)
            || *key == RlKeyEvent(RlKeyCode::Up, RlModifiers::NONE);

        if !is_navigation {
            return None;
        }

        let line = ctx.line();
        if !line.starts_with('/') {
            return None;
        }

        let pos = ctx.pos().min(line.len());
        let upto_cursor = &line[..pos];
        if upto_cursor.contains(' ') {
            return None;
        }

        let partial = if pos > 1 { &line[1..pos] } else { "" };
        let args_suffix = line
            .find(' ')
            .map(|idx| line[idx..].to_string())
            .unwrap_or_default();

        let matches: Vec<&CommandInfo> = COMMANDS
            .iter()
            .filter(|info| info.name.starts_with(partial))
            .collect();

        if matches.is_empty() {
            return Some(RlCmd::Noop);
        }

        let initial_index = match key.0 {
            RlKeyCode::Up => matches.len().saturating_sub(1),
            _ => 0,
        };

        match pick_command_menu(partial, &matches, initial_index) {
            Ok(Some(choice)) => {
                if let Ok(mut pending) = self.pending_command.lock() {
                    let mut command = format!("/{}", choice.name);
                    if !args_suffix.is_empty() {
                        command.push_str(&args_suffix);
                    }
                    *pending = Some(command);
                }
                Some(RlCmd::Interrupt)
            }
            Ok(None) => {
                if let Ok(mut pending) = self.pending_command.lock() {
                    if pending.is_some() {
                        *pending = None;
                    }
                }
                Some(RlCmd::Noop)
            }
            Err(err) => {
                eprintln!("Error: {:#}", err);
                Some(RlCmd::Noop)
            }
        }
    }
}

fn pick_command_menu<'a>(
    partial: &str,
    matches: &'a [&'a CommandInfo],
    initial_index: usize,
) -> Result<Option<&'a CommandInfo>> {
    if matches.is_empty() {
        return Ok(None);
    }

    print!("\n\n");

    let theme = ColorfulTheme::default();
    let items: Vec<String> = matches
        .iter()
        .map(|info| format!("/{:<16} {}", info.name, info.description))
        .collect();

    let prompt = if partial.is_empty() {
        "Select a command".to_string()
    } else {
        format!("Commands matching '/{}'", partial)
    };

    let default_index = initial_index.min(items.len() - 1);

    let selection = Select::with_theme(&theme)
        .with_prompt(prompt)
        .items(&items)
        .default(default_index)
        .clear(true)
        .report(false)
        .interact_opt()?;

    Ok(selection.map(|idx| matches[idx]))
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
    suffix: Option<String>,
}

#[derive(Debug, Clone)]
enum RegisteredTool {
    Bash,
    Builtin(String),
    Mcp { server: String, tool: String },
}

struct ToolRegistryConfig {
    specs: Vec<Value>,
    map: HashMap<String, RegisteredTool>,
}

fn build_tool_registry(
    builtin_specs: &[Value],
    tools_by_server: Option<&HashMap<String, Vec<McpTool>>>,
) -> ToolRegistryConfig {
    let mut specs = Vec::new();
    let mut map = HashMap::new();

    specs.push(build_bash_tool());
    map.insert("bash".to_string(), RegisteredTool::Bash);

    for spec in builtin_specs {
        if let Some(name) = spec.get("name").and_then(|v| v.as_str()) {
            map.insert(name.to_string(), RegisteredTool::Builtin(name.to_string()));
            specs.push(spec.clone());
        }
    }

    if let Some(snapshot) = tools_by_server {
        let mut servers: Vec<_> = snapshot.iter().collect();
        servers.sort_by(|a, b| a.0.cmp(b.0));

        for (server, tools) in servers {
            let mut sorted: Vec<&McpTool> = tools.iter().collect();
            sorted.sort_by(|a, b| a.name.cmp(&b.name));

            for tool in sorted {
                if let Some((qualified_name, spec)) = build_mcp_tool_definition(server, tool) {
                    if map.contains_key(&qualified_name) {
                        continue;
                    }

                    map.insert(
                        qualified_name.clone(),
                        RegisteredTool::Mcp {
                            server: (*server).clone(),
                            tool: tool.name.clone(),
                        },
                    );
                    specs.push(spec);
                }
            }
        }
    }

    ToolRegistryConfig { specs, map }
}

fn build_bash_tool() -> Value {
    json!({
        "name": "bash",
        "description": "Execute bash commands to search files, read file contents, or perform other system operations. Use this to understand the codebase context better.",
        "input_schema": {
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The bash command to execute (e.g., 'find . -name \"*.rs\"', 'grep -r \"function_name\" src/', 'cat src/main.rs')"
                }
            },
            "required": ["command"]
        }
    })
}


fn build_mcp_tool_definition(server: &str, tool: &McpTool) -> Option<(String, Value)> {
    let qualified_name = qualify_mcp_tool_name(server, &tool.name);
    let schema = sanitize_mcp_input_schema(&tool.input_schema);
    let description = tool
        .description
        .clone()
        .unwrap_or_else(|| format!("MCP tool {}.{}", server, tool.name));

    let spec = json!({
        "name": qualified_name.clone(),
        "description": description,
        "input_schema": schema,
    });

    Some((qualified_name, spec))
}

fn sanitize_mcp_input_schema(schema: &Value) -> Value {
    if let Value::Object(map) = schema {
        let mut sanitized = serde_json::Map::new();
        sanitized.insert("type".into(), Value::String("object".into()));

        if let Some(description) = map.get("description").cloned() {
            sanitized.insert("description".into(), description);
        }

        let properties = map
            .get("properties")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let mut sanitized_props = serde_json::Map::new();
        for (key, value) in properties {
            sanitized_props.insert(key, sanitize_schema_node(value));
        }
        sanitized.insert("properties".into(), Value::Object(sanitized_props));

        let required = map
            .get("required")
            .filter(|v| v.is_array())
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        sanitized.insert("required".into(), required);

        sanitized.insert(
            "additionalProperties".into(),
            map.get("additionalProperties")
                .cloned()
                .unwrap_or(Value::Bool(true)),
        );

        Value::Object(sanitized)
    } else {
        default_parameters_schema()
    }
}

fn sanitize_schema_node(value: Value) -> Value {
    match value {
        Value::Object(mut map) => {
            let mut inferred = infer_schema_type(&map);
            if inferred.eq_ignore_ascii_case("integer") {
                inferred = "number".to_string();
            }
            map.insert("type".into(), Value::String(inferred.clone()));

            if inferred == "object" {
                let nested = map
                    .remove("properties")
                    .and_then(|v| v.as_object().cloned())
                    .unwrap_or_default();
                let mut nested_props = serde_json::Map::new();
                for (key, value) in nested {
                    nested_props.insert(key, sanitize_schema_node(value));
                }
                map.insert("properties".into(), Value::Object(nested_props));
                if !map.contains_key("additionalProperties") {
                    map.insert("additionalProperties".into(), Value::Bool(true));
                }
            } else if inferred == "array" {
                let items = map
                    .remove("items")
                    .map(sanitize_schema_node)
                    .unwrap_or_else(|| json!({ "type": "string" }));
                map.insert("items".into(), items);
            }

            Value::Object(map)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sanitize_schema_node).collect()),
        Value::Bool(_) => json!({ "type": "boolean" }),
        Value::Number(_) => json!({ "type": "number" }),
        Value::String(_) => json!({ "type": "string" }),
        Value::Null => json!({ "type": "string" }),
    }
}

fn infer_schema_type(map: &serde_json::Map<String, Value>) -> String {
    if let Some(explicit) = map.get("type").and_then(|v| v.as_str()) {
        return explicit.to_string();
    }

    if map.contains_key("properties") {
        "object".to_string()
    } else if map.contains_key("items") {
        "array".to_string()
    } else if map.contains_key("enum") || map.contains_key("const") || map.contains_key("format") {
        "string".to_string()
    } else if map.contains_key("minimum") || map.contains_key("maximum") {
        "number".to_string()
    } else if map.contains_key("anyOf") || map.contains_key("oneOf") {
        "object".to_string()
    } else {
        "string".to_string()
    }
}

fn default_parameters_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "required": [],
        "additionalProperties": true
    })
}

fn sanitize_tool_component(input: &str) -> String {
    let mut cleaned = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            cleaned.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_') {
            cleaned.push(ch);
        } else {
            cleaned.push('_');
        }
    }

    if cleaned.is_empty() {
        "tool".to_string()
    } else {
        cleaned
    }
}

fn qualify_mcp_tool_name(server: &str, tool_name: &str) -> String {
    const MAX_TOOL_NAME: usize = 64;
    let server_component = sanitize_tool_component(server);
    let tool_component = sanitize_tool_component(tool_name);
    let mut qualified = format!("mcp__{}__{}", server_component, tool_component);

    if qualified.len() > MAX_TOOL_NAME {
        let mut hasher = Sha256::new();
        hasher.update(qualified.as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        let suffix_len = 8.min(hash.len());
        let prefix_len = MAX_TOOL_NAME.saturating_sub(suffix_len);
        let prefix = qualified.chars().take(prefix_len).collect::<String>();
        qualified = format!("{}{}", prefix, &hash[..suffix_len]);
    }

    qualified
}

fn extract_tool_arguments(value: &Value) -> Result<Option<HashMap<String, Value>>, String> {
    match value {
        Value::Null => Ok(None),
        Value::Object(map) => Ok(Some(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())),
        _ => Err("ERROR: Tool arguments must be provided as a JSON object or null.".to_string()),
    }
}

fn extract_bash_command(input: &Value) -> Result<String, String> {
    let command_value = input.get("command").ok_or_else(|| {
        "ERROR: Missing required field 'command' for bash tool input.".to_string()
    })?;
    let command_str = command_value.as_str().ok_or_else(|| {
        "ERROR: Field 'command' must be a string for bash tool input.".to_string()
    })?;
    let trimmed = command_str.trim();
    if trimmed.is_empty() {
        return Err("ERROR: Field 'command' cannot be empty for bash tool input.".to_string());
    }
    Ok(command_str.to_string())
}

fn append_tool_response_message(
    messages: &mut Vec<Value>,
    is_anthropic: bool,
    tool_call_id: &str,
    content: &str,
) {
    if is_anthropic {
        let tool_result_content = vec![json!({
            "type": "tool_result",
            "tool_use_id": tool_call_id,
            "content": content
        })];
        messages.push(json!({
            "role": "user",
            "content": tool_result_content
        }));
    } else {
        messages.push(json!({
            "role": "tool",
            "tool_call_id": tool_call_id,
            "content": content
        }));
    }
}

fn summarize_builtin_tool_action(tool_name: &str, input: &Value) -> Option<Vec<String>> {
    match tool_name {
        "read_file" => {
            let path = input.get("path").and_then(|v| v.as_str())?;
            let range = match (
                input.get("start_line").and_then(|v| v.as_i64()),
                input.get("end_line").and_then(|v| v.as_i64()),
            ) {
                (Some(start), Some(end)) => format!(" (lines {}-{})", start, end),
                (Some(start), None) => format!(" (from line {})", start),
                (None, Some(end)) => format!(" (through line {})", end),
                _ => String::new(),
            };
            Some(vec![
                "• Explored".to_string(),
                format!("  └ Read {}{}", path, range),
            ])
        }
        "list_dir" => {
            let path = input
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let depth = input
                .get("depth")
                .and_then(|v| v.as_i64())
                .unwrap_or(1);
            Some(vec![
                "• Explored".to_string(),
                format!("  └ List directory {} (depth {})", path, depth),
            ])
        }
        "grep_files" => {
            let path = input.get("path").and_then(|v| v.as_str())?;
            let pattern = input.get("pattern").and_then(|v| v.as_str())?;
            Some(vec![
                "• Explored".to_string(),
                format!("  └ Search '{}' in {}", pattern, path),
            ])
        }
        "apply_patch" => Some(vec!["• Explored".to_string(), "  └ Apply patch".to_string()]),
        _ => None,
    }
}

fn build_tool_prompt_section(tools_by_server: &HashMap<String, Vec<McpTool>>) -> String {
    let mut section = String::from(
        "Available MCP tools:\n\
Invoke these by calling the function tool using the `call name` shown below (e.g., mcp__server__tool).\n\
If tool calling is unavailable, you may fall back to CALL_MCP_TOOL server=<server_name> tool=<tool_name> args=<json_object>.\n\
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
                let call_name = qualify_mcp_tool_name(server, &tool.name);
                let description = tool
                    .description
                    .as_deref()
                    .unwrap_or("No description provided");
                section.push_str(&format!(
                    "  - {} (call name: `{}`): {}\n",
                    tool.name, call_name, description
                ));

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

    let (command_line, trailing_text) = if let Some(pos) = command_and_rest.find('\n') {
        let (line, rest) = command_and_rest.split_at(pos);
        (line.trim_end(), rest.trim())
    } else {
        (command_and_rest, "")
    };

    let suffix = if trailing_text.is_empty() {
        None
    } else {
        Some(trailing_text.to_string())
    };

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
        suffix,
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

fn take_first_chars_with_total(text: &str, max_chars: usize) -> (String, usize, bool) {
    let mut snippet = String::new();
    let mut total = 0usize;
    let mut truncated = false;

    for ch in text.chars() {
        if total < max_chars {
            snippet.push(ch);
        } else {
            truncated = true;
        }
        total += 1;
    }

    (snippet, total, truncated)
}

struct ToolExecutionLogger {
    tool_name: &'static str,
}

impl ToolExecutionLogger {
    fn start(tool_name: &'static str, command: &str) -> Self {
        let mut out = stdout();
        out.execute(SetForegroundColor(Color::Blue)).ok();
        println!("⏳ Running {tool_name} command…");
        out.execute(ResetColor).ok();
        out.execute(SetForegroundColor(Color::DarkGrey)).ok();
        println!("    {}", command);
        out.execute(ResetColor).ok();

        Self { tool_name }
    }

    fn finish(&self, exit_code: i32, duration: StdDuration) {
        let mut out = stdout();
        out.execute(SetForegroundColor(Color::Green)).ok();
        println!(
            "✔ {tool} completed in {} (exit code {exit})",
            format_duration(duration),
            tool = self.tool_name,
            exit = exit_code
        );
        out.execute(ResetColor).ok();
    }

    fn fail(&self, duration: StdDuration, message: &str) {
        let mut out = stdout();
        out.execute(SetForegroundColor(Color::Red)).ok();
        println!(
            "✖ {tool} failed after {}: {message}",
            format_duration(duration),
            tool = self.tool_name
        );
        out.execute(ResetColor).ok();
    }
}

fn format_duration(duration: StdDuration) -> String {
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();
    if secs > 0 {
        format!("{secs}.{millis:03}s")
    } else {
        format!("{millis}ms")
    }
}

fn get_model_display_name(model: &str) -> String {
    if model.contains("sonnet") {
        "Sonnet".to_string()
    } else if model.contains("opus") {
        "Opus".to_string()
    } else if model.contains("haiku") {
        "Haiku".to_string()
    } else if model.starts_with("gpt-5-codex") {
        "GPT-5 Codex".to_string()
    } else if model.starts_with("glm-4.6") {
        "GLM-4.6".to_string()
    } else if model.starts_with("glm-4.5") {
        "GLM-4.5".to_string()
    } else if model.starts_with("glm") {
        "GLM".to_string()
    } else {
        model.to_string()
    }
}

fn print_assistant_message(text: &str, model: &str) -> Result<()> {
    let mut out = stdout();
    let model_name = get_model_display_name(model);
    let trimmed_text = text.trim();

    println!();
    out.execute(SetForegroundColor(Color::Green))?;
    out.execute(Print("● "))?;
    out.execute(Print(format!("{}:", model_name)))?;
    out.execute(ResetColor)?;
    println!();

    print_formatted_text(trimmed_text, 2)?;
    println!();
    println!();
    Ok(())
}

fn print_formatted_text(text: &str, indent_spaces: usize) -> Result<()> {
    let mut out = stdout();
    let indent = " ".repeat(indent_spaces);
    let lines: Vec<&str> = text.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        print!("{}", indent);

        let mut chars = line.chars().peekable();
        let mut buffer = String::new();

        while let Some(ch) = chars.next() {
            if ch == '*' && chars.peek() == Some(&'*') {
                chars.next();

                if !buffer.is_empty() {
                    print!("{}", buffer);
                    buffer.clear();
                }

                let mut bold_text = String::new();
                let mut found_closing = false;

                while let Some(ch) = chars.next() {
                    if ch == '*' && chars.peek() == Some(&'*') {
                        chars.next();
                        found_closing = true;
                        break;
                    }
                    bold_text.push(ch);
                }

                if found_closing && !bold_text.is_empty() {
                    out.execute(SetAttribute(Attribute::Bold))?;
                    print!("{}", bold_text);
                    out.execute(SetAttribute(Attribute::Reset))?;
                } else {
                    print!("**{}", bold_text);
                }
            } else {
                buffer.push(ch);
            }
        }

        if !buffer.is_empty() {
            print!("{}", buffer);
        }

        if i < lines.len() - 1 {
            println!();
        }
    }

    Ok(())
}

fn print_tool_command(command: &str) -> Result<()> {
    let mut out = stdout();
    out.execute(SetForegroundColor(Color::DarkGrey))?;
    println!("{}", command);
    out.execute(ResetColor)?;
    Ok(())
}

struct Spinner {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

impl Spinner {
    fn start(message: String) -> Self {
        let stop = Arc::new(AtomicBool::new(true));
        let stop_clone = stop.clone();

        let display_text = if message.trim().is_empty() {
            "Thinking...".to_string()
        } else {
            message
        };

        let handle = tokio::spawn(async move {
            let symbols = ['|', '/', '-', '\\'];
            let chars: Vec<char> = display_text.chars().collect();
            let message_len = chars.len();
            let mut frame = 0usize;

            while stop_clone.load(Ordering::Relaxed) {
                let symbol = symbols[frame % symbols.len()];

                let rendered = if message_len == 0 {
                    String::new()
                } else {
                    let shine_center = frame % message_len;
                    let prev_index = if message_len > 0 {
                        (shine_center + message_len - 1) % message_len
                    } else {
                        0
                    };
                    let next_index = if message_len > 0 {
                        (shine_center + 1) % message_len
                    } else {
                        0
                    };

                    let mut highlighted = String::new();
                    for (i, ch) in chars.iter().enumerate() {
                        let style = if i == shine_center {
                            "\x1b[1;97m"
                        } else if message_len > 1 && (i == prev_index || i == next_index) {
                            "\x1b[37m"
                        } else {
                            "\x1b[90m"
                        };
                        highlighted.push_str(style);
                        highlighted.push(*ch);
                    }
                    highlighted.push_str("\x1b[0m");
                    highlighted
                };

                let mut out = stdout();
                let _ = write!(out, "\r{} {}\x1b[K", symbol, rendered);
                let _ = out.flush();
                frame = frame.wrapping_add(1);
                sleep(Duration::from_millis(120)).await;
            }

            let mut out = stdout();
            let _ = write!(out, "\r\x1B[K");
            let _ = out.flush();
        });

        Self { stop, handle }
    }

    async fn stop(self) {
        self.stop.store(false, Ordering::Relaxed);
        let _ = self.handle.await;
    }
}

fn print_file_change_summary(path: &Path, before: &str, after: &str) -> Result<()> {
    let mut out = stdout();

    let diff = TextDiff::from_lines(before, after);
    let mut additions = 0;
    let mut removals = 0;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Delete => removals += 1,
            ChangeTag::Insert => additions += 1,
            _ => {}
        }
    }

    if before.is_empty() {
        out.execute(SetForegroundColor(Color::Green)).ok();
        println!("● Create({})", path.display());
        out.execute(ResetColor).ok();
        println!("  ⎿ Created {} with {} lines", path.display(), additions);
    } else {
        out.execute(SetForegroundColor(Color::Green)).ok();
        println!("● Update({})", path.display());
        out.execute(ResetColor).ok();
        println!("  ⎿ Updated {} with {} addition{} and {} removal{}",
            path.display(),
            additions, if additions == 1 { "" } else { "s" },
            removals, if removals == 1 { "" } else { "s" }
        );
    }

    let mut old_line = 1usize;
    let mut new_line = 1usize;
    let mut context_before: Vec<(usize, String)> = Vec::new();
    let max_context = 3;

    for change in diff.iter_all_changes() {
        let value = change.value().trim_end_matches('\n');
        match change.tag() {
            ChangeTag::Equal => {
                context_before.push((old_line, value.to_string()));
                if context_before.len() > max_context {
                    context_before.remove(0);
                }
                old_line += 1;
                new_line += 1;
            }
            ChangeTag::Delete => {
                for (line_num, text) in &context_before {
                    print_context_line(*line_num, text);
                }
                context_before.clear();

                print_diff_line_with_bg('-', old_line, value, Color::Rgb { r: 60, g: 20, b: 20 })?;
                old_line += 1;
            }
            ChangeTag::Insert => {
                for (line_num, text) in &context_before {
                    print_context_line(*line_num, text);
                }
                context_before.clear();

                print_diff_line_with_bg('+', new_line, value, Color::Rgb { r: 20, g: 60, b: 20 })?;
                new_line += 1;
            }
        }
    }

    println!();
    Ok(())
}

fn print_context_line(line_number: usize, text: &str) {
    println!("       {:>5}    {}", line_number, text);
}

fn print_diff_line_with_bg(prefix: char, line_number: usize, text: &str, bg_color: Color) -> Result<()> {
    let mut out = stdout();

    out.execute(Print(format!("       {:>5} ", line_number)))?;

    let prefix_color = if prefix == '-' { Color::Red } else { Color::Green };
    out.execute(SetBackgroundColor(bg_color))?;
    out.execute(SetForegroundColor(prefix_color))?;
    out.execute(Print(prefix))?;

    if !text.is_empty() {
        out.execute(SetForegroundColor(Color::White))?;
        out.execute(Print(format!("  {}", text)))?;
    }

    out.execute(ResetColor)?;
    println!();
    Ok(())
}


struct BashCommandResult {
    output: String,
    #[allow(dead_code)]
    exit_code: i32,
    #[allow(dead_code)]
    duration: StdDuration,
}

fn execute_bash_command(command: &str, working_dir: &Path) -> Result<BashCommandResult> {
    use std::process::Command;

    let logger = ToolExecutionLogger::start("bash", command);
    let start = Instant::now();

    let output_result: Result<std::process::Output> = (|| {
        if cfg!(target_os = "windows") {
            if let Some(wsl_dir) = windows_to_wsl_path(working_dir) {
                let cd_command = format!("cd '{}' && {}", escape_single_quotes(&wsl_dir), command);

                match Command::new("wsl")
                    .args(["bash", "-lc", &cd_command])
                    .output()
                {
                    Ok(output) => Ok(output),
                    Err(_) => run_windows_shell(command, working_dir),
                }
            } else {
                run_windows_shell(command, working_dir)
            }
        } else {
            Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(working_dir)
                .output()
                .context("Failed to execute bash command")
        }
    })();

    let output = match output_result {
        Ok(out) => out,
        Err(err) => {
            logger.fail(start.elapsed(), &format!("{err:#}"));
            return Err(err);
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr_raw = String::from_utf8_lossy(&output.stderr);
    let stderr = sanitize_bash_stderr(&stderr_raw);

    let mut result = String::new();
    if !stdout.is_empty() {
        result.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !result.is_empty() {
            result.push_str("\n");
        }
        result.push_str("STDERR:\n");
        result.push_str(&stderr);
    }

    if result.is_empty() {
        result = "(command produced no output)".to_string();
    }

    let exit_code = output.status.code().unwrap_or_default();
    let duration = start.elapsed();
    logger.finish(exit_code, duration);

    Ok(BashCommandResult {
        output: result,
        exit_code,
        duration,
    })
}

#[cfg(target_os = "windows")]
fn run_windows_shell(command: &str, working_dir: &Path) -> Result<std::process::Output> {
    use std::process::Command;

    let bash_path = windows_path_to_bash_path(working_dir);
    let cd_command = format!(
        "cd '{}' && {}",
        escape_single_quotes(&bash_path),
        command
    );

    match Command::new("bash")
        .args(["-c", &cd_command])
        .output()
    {
        Ok(output) => Ok(output),
        Err(_) => Command::new("cmd")
            .args(&["/C", command])
            .current_dir(working_dir)
            .output()
            .context("Failed to execute bash command"),
    }
}

#[cfg(not(target_os = "windows"))]
fn run_windows_shell(command: &str, working_dir: &Path) -> Result<std::process::Output> {
    let _ = (command, working_dir);
    unreachable!("run_windows_shell should not be called on non-Windows platforms")
}

#[cfg(target_os = "windows")]
fn windows_path_to_bash_path(path: &Path) -> String {
    let path_str = path.to_string_lossy();

    if let Some(rest) = path_str.strip_prefix(r"\\?\") {
        return rest.replace('\\', "/");
    }

    if path_str.len() >= 2 && path_str.chars().nth(1) == Some(':') {
        let drive = path_str.chars().next().unwrap().to_ascii_lowercase();
        let rest = path_str.get(2..).unwrap_or("");
        format!("/mnt/{}{}", drive, rest.replace('\\', "/"))
    } else {
        path_str.replace('\\', "/")
    }
}

#[cfg(not(target_os = "windows"))]
fn windows_path_to_bash_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[cfg(target_os = "windows")]
fn windows_to_wsl_path(path: &Path) -> Option<String> {
    use std::process::Command;

    let path_str = path.to_string_lossy().into_owned();

    let output = Command::new("wsl")
        .arg("wslpath")
        .arg(&path_str)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let converted = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string();

    if converted.is_empty() {
        None
    } else {
        Some(converted)
    }
}

#[cfg(not(target_os = "windows"))]
fn windows_to_wsl_path(_: &Path) -> Option<String> {
    None
}

fn escape_single_quotes(input: &str) -> String {
    input.replace('\'', "'\"'\"'")
}

fn sanitize_bash_stderr(stderr: &str) -> String {
    if stderr.is_empty() {
        return String::new();
    }

    let mut filtered_lines = Vec::new();
    for line in stderr.lines() {
        if line.trim()
            == "your 131072x1 screen size is bogus. expect trouble"
        {
            continue;
        }
        filtered_lines.push(line);
    }

    if filtered_lines.is_empty() {
        return String::new();
    }

    let mut result = filtered_lines.join("\n");
    if stderr.ends_with('\n') {
        result.push('\n');
    }
    result
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
