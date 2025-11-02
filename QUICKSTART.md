# ZarzCLI Quick Start Guide

ZarzCLI is an AI-powered coding assistant that works directly from your command line, similar to Claude Code and Codex CLI.

## Installation

1. Make sure you have Rust installed:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

2. Build ZarzCLI:
```bash
cd ZarzCLI
cargo build --release
```

3. Set up your API key:
```bash
# For Claude (Anthropic)
export ANTHROPIC_API_KEY=sk-ant-your-key-here

# Or for OpenAI
export OPENAI_API_KEY=sk-your-key-here
```

## Basic Usage

### 1. Interactive Chat (Default)

Just run `zarz` to start an interactive session:

```bash
./zarz.bat  # Windows
./zarz.sh   # Linux/Mac
```

You'll enter a chat session where you can ask questions and edit code:

```
Zarz Interactive Session
Type /help for available commands, /quit to exit

> What does this codebase do?
Assistant: [AI analyzes your project...]

> /edit src/main.rs
Loaded src/main.rs for editing

> Add better error handling
Assistant: [AI suggests changes with file blocks]
File changes detected. Use /diff to review, /apply to apply

> /diff
[shows unified diff]

> /apply
Applied changes to src/main.rs

> /quit
Goodbye!
```

### 2. One-Shot Questions

Send a single question and get an answer:

```bash
zarz --message "Explain what this function does" -f src/auth.rs
```

Or shorter with alias:

```bash
zarz --msg "Fix the bug in this file" -f src/main.rs
```

### 3. Switch Models On-the-Fly

Start with one model, switch to another mid-conversation:

```bash
zarz

> /model claude-haiku-4-5
Switched to model: claude-haiku-4-5
Provider: anthropic

> Add docstrings
[Uses faster, cheaper Haiku model]

> /model gpt-5-codex
Switched to model: gpt-5-codex
Provider: openai

> Optimize this algorithm
[Uses GPT-5 Codex]
```

## Available Models

### Anthropic Claude

- **claude-sonnet-4-5-20250929** (default) - Best for complex coding ($3/$15 per 1M tokens)
- **claude-haiku-4-5** - Fast and cheap ($1/$5 per 1M tokens)
- **claude-opus-4-1** - Most powerful for complex tasks

### OpenAI

- **gpt-5-codex** - Optimized for coding ($1.25/$10 per 1M tokens)
- **gpt-4o** - Multimodal
- **gpt-4-turbo** - Fast

## In-Chat Commands

- `/help` - Show all commands and current model
- `/model <name>` - Switch AI model
- `/edit <file>` - Load a file for editing
- `/diff` - Preview pending changes
- `/apply` - Apply pending changes
- `/undo` - Discard pending changes
- `/run <command>` - Execute shell command
- `/search <symbol>` - Find symbol in codebase
- `/context <query>` - Find relevant files
- `/files` - List loaded files
- `/clear` - Clear conversation history
- `/quit` - Exit

## Common Workflows

### Workflow 1: Code Review and Fix

```bash
zarz

> /edit src/server.rs
> Review this code for security issues
[AI finds issues]

> Fix the SQL injection vulnerability
[AI provides fix with file blocks]

> /diff
[Review changes]

> /apply
[Changes applied]

> /run cargo test
[Verify tests pass]
```

### Workflow 2: Add Feature with Cost Optimization

```bash
zarz

# Use Sonnet for planning
> Design an authentication system with JWT

# Switch to Haiku for implementation (cheaper)
> /model claude-haiku-4-5
> Implement the JWT authentication based on your design

# Back to Sonnet for complex parts
> /model claude-sonnet-4-5-20250929
> Add rate limiting and refresh token logic
```

### Workflow 3: Multi-File Refactoring

```bash
zarz

> /edit src/main.rs
> /edit src/config.rs
> /edit src/handlers.rs

> Refactor these files to use async/await consistently

> /diff
[Shows changes across all files]

> /apply
[Applies all changes]

> /run cargo check
[Verify code compiles]
```

## Legacy Commands (Still Supported)

### Ask Mode

```bash
zarz ask --prompt "What does this do?" src/main.rs
```

### Rewrite Mode

```bash
zarz rewrite --instructions "Add error handling" src/lib.rs
```

### Explicit Chat Mode

```bash
zarz chat
```

## Configuration

### Environment Variables

```bash
# Model selection
export ZARZ_MODEL=claude-sonnet-4-5-20250929

# Provider selection
export ZARZ_PROVIDER=anthropic  # or openai

# Anthropic settings
export ANTHROPIC_API_KEY=sk-ant-...
export ANTHROPIC_API_URL=https://api.anthropic.com/v1/messages
export ANTHROPIC_TIMEOUT_SECS=120

# OpenAI settings
export OPENAI_API_KEY=sk-...
export OPENAI_API_URL=https://api.openai.com/v1/chat/completions
export OPENAI_TIMEOUT_SECS=120

# Advanced settings
export ZARZ_MAX_OUTPUT_TOKENS=4096
export ZARZ_TEMPERATURE=0.3
```

### Per-Command Overrides

```bash
# Use specific model
zarz --model claude-haiku-4-5

# Use OpenAI instead of default
zarz --provider openai --model gpt-5-codex

# Custom endpoint
zarz --endpoint https://custom-api.example.com
```

## Tips for Best Results

1. **Be Specific**: "Add error handling to the login function" is better than "improve the code"

2. **Use Context**: Load relevant files with `/edit` before asking questions

3. **Iterate**: Don't try to do everything at once. Make small, focused changes

4. **Review Before Applying**: Always `/diff` before `/apply`

5. **Choose the Right Model**:
   - Sonnet 4.5: Complex refactoring, architecture decisions
   - Haiku 4.5: Quick fixes, adding docstrings, simple tasks
   - GPT-5 Codex: Agentic workflows, algorithm optimization

6. **Cost Optimization**: Start with Sonnet for planning, switch to Haiku for implementation

## Troubleshooting

### API Key Not Found

```bash
Error: Environment variable ANTHROPIC_API_KEY is required

# Fix:
export ANTHROPIC_API_KEY=sk-ant-your-key
```

### Model Not Found

```bash
Error: Unknown model provider for 'typo-model'

# Fix: Use /model without arguments to see available models
> /model
```

### Build Errors

```bash
# Clean and rebuild
cargo clean
cargo build --release
```

## Next Steps

- Read [MODELS.md](MODELS.md) for detailed model information
- Check [README.md](README.md) for full documentation
- Report issues: https://github.com/fapzarz/zarzcli/issues

## Quick Reference Card

```
┌─────────────────────────────────────────────────┐
│ ZarzCLI Quick Reference                         │
├─────────────────────────────────────────────────┤
│ Start Chat:       zarz                          │
│ One-Shot:         zarz --message "prompt"       │
│ With Files:       zarz -f file.rs               │
│                                                 │
│ In Chat:                                        │
│   /help          Show all commands              │
│   /model <name>  Switch AI model                │
│   /edit <file>   Load file                      │
│   /diff          Preview changes                │
│   /apply         Apply changes                  │
│   /undo          Discard changes                │
│   /run <cmd>     Execute command                │
│   /quit          Exit                           │
│                                                 │
│ Models:                                         │
│   claude-sonnet-4-5-20250929 (default)         │
│   claude-haiku-4-5 (fast/cheap)                 │
│   gpt-5-codex (OpenAI)                          │
└─────────────────────────────────────────────────┘
```
