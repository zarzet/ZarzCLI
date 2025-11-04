# ZarzCLI

[![npm version](https://img.shields.io/npm/v/zarz.svg)](https://www.npmjs.com/package/zarz)
[![npm downloads](https://img.shields.io/npm/dd/zarz.svg)](https://www.npmjs.com/package/zarz)
[![GitHub release](https://img.shields.io/github/v/release/zarzet/ZarzCLI.svg)](https://github.com/zarzet/ZarzCLI/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**ZarzCLI** is a blazingly fast AI coding assistant for your terminal, built with Rust for maximum performance. It brings the power of Claude, GPT, and GLM models directly to your command line with intelligent context awareness and autonomous tool execution.

## Features

### Core Capabilities
- **Interactive Chat** - Real-time streaming responses with multiple AI models
- **Multi-Provider Support** - Claude (Anthropic), GPT (OpenAI), and GLM (Z.AI)
- **Bash Tool Calling** - AI can autonomously execute bash commands to understand your codebase
- **File Operations** - Direct file editing, creation, and management
- **Smart Context** - Automatic symbol search and relevant file detection
- **MCP Support** - Model Context Protocol integration for extended capabilities
- **Auto Update** - Automatic update checks and notifications for new versions
- **Cross-Platform** - Works seamlessly on Windows, Linux, and macOS

### Intelligent Context Understanding

ZarzCLI v0.3.4+ includes autonomous bash tool execution, allowing AI models to:
- **Search files**: `find . -name "*.rs"` or `rg "pattern"`
- **Read contents**: `cat src/main.rs` or `head -n 20 file.py`
- **Grep code**: `grep -r "function_name" src/`
- **Navigate structure**: `ls -la src/` or `tree -L 2`
- **Check git**: `git log --oneline -10` or `git diff`

The AI automatically decides when to execute commands for better context - no manual `/run` needed!

### User Experience
- **Status Line** - Shows current mode and notifications
- **Double Ctrl+C** - Confirmation before exit (prevents accidental exits)
- **Colored Diff Display** - Beautiful file change visualization with context
- **Persistent Sessions** - Resume previous conversations anytime

## Installation

### Via NPM (Recommended)
```bash
npm install -g zarz
```

### From Source
```bash
git clone https://github.com/zarzet/ZarzCLI.git
cd ZarzCLI
cargo build --release
```

### Updating

ZarzCLI will automatically check for updates and notify you when a new version is available. To update manually:

```bash
npm update -g zarz
```

## Quick Start

### First Run Setup

On first run, you'll be prompted to enter your API keys interactively:

```bash
zarz
```

Or set manually via environment variables:
```bash
# For Anthropic Claude
export ANTHROPIC_API_KEY=sk-ant-...

# For OpenAI GPT
export OPENAI_API_KEY=sk-...

# For GLM (Z.AI)
export GLM_API_KEY=...

zarz
```

Your API keys are securely stored in `~/.zarz/config.toml`

### Basic Usage

```bash
# Start interactive chat (default)
zarz

# Quick one-shot question
zarz --message "fix this bug"

# Use specific model
zarz --model claude-sonnet-4-5-20250929

# Manage configuration
zarz config --show     # Show current config
zarz config --reset    # Reconfigure API keys
```

## Available Commands

Once inside the interactive chat:

| Command | Description |
|---------|-------------|
| `/help` | Show all available commands |
| `/apply` | Apply pending file changes |
| `/diff` | Show pending changes with colored diff |
| `/undo` | Clear pending changes |
| `/edit <file>` | Load a file for editing |
| `/search <symbol>` | Search for a symbol in codebase |
| `/context <query>` | Find relevant files for a query |
| `/files` | List currently loaded files |
| `/model <name>` | Switch to a different AI model |
| `/mcp` | Show MCP servers and available tools |
| `/resume` | Resume a previous chat session |
| `/clear` | Clear conversation history |
| `/exit` | Exit the session |

## Supported AI Models

### Anthropic Claude
Best for coding tasks and autonomous agents:
- `claude-sonnet-4-5-20250929` (Latest, most capable)
- `claude-haiku-4-5` (Fast, cost-effective)
- `claude-opus-4-5` (Most powerful)

### OpenAI GPT
Multimodal capabilities with vision:
- `gpt-5-codex` (Best for coding)
- `gpt-4o` (Multimodal)
- `gpt-4-turbo`

### GLM (Z.AI)
Cost-effective coding with 200K context window:
- `glm-coder-4-lite` ($3/month subscription)
- 200,000 token context window
- Specialized for coding tasks

See [MODELS.md](MODELS.md) for full model list and [GLM-PROVIDER.md](GLM-PROVIDER.md) for GLM setup.

## Advanced Features

### MCP (Model Context Protocol)

ZarzCLI supports MCP servers for extended capabilities. Configure in `~/.zarz/config.toml`:

```toml
[[mcp_servers]]
name = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/project"]
```

### Bash Tool Integration

AI models can automatically execute bash commands when they need context:

```bash
> Tell me about the authentication implementation

# AI automatically executes:
$ find . -name "*auth*" -type f
$ grep -r "authenticate" src/
$ cat src/auth/login.rs

# Then provides informed response based on actual codebase
```

### Automatic Updates

ZarzCLI automatically checks for updates on startup and notifies you when a new version is available. Updates are downloaded from npm registry and can be installed with a single command.

## Requirements

- **Node.js** 14.0.0 or higher
- **API Key** from one of:
  - Anthropic Claude ([get key](https://console.anthropic.com/))
  - OpenAI ([get key](https://platform.openai.com/api-keys))
  - GLM Z.AI ([get key](https://z.ai/))

> **Note**: Rust is **NOT required** for installation. Pre-built binaries are automatically downloaded for your platform (Windows, macOS, Linux).

## Contributing

Contributions are welcome! ZarzCLI is now open source under MIT license.

### Development Setup

For contributors who want to modify the source code:

**Requirements:**
- Node.js 14.0.0 or higher
- Rust toolchain ([install from rustup.rs](https://rustup.rs/))

```bash
# Clone the repository
git clone https://github.com/zarzet/ZarzCLI.git
cd ZarzCLI

# Build the project
cargo build --release

# Run tests
cargo test

# Install locally for testing
npm install -g .
```

**Note**: Regular users don't need Rust installed. Pre-built binaries are automatically downloaded during `npm install`.

### Contribution Guidelines

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

Please ensure:
- Code compiles without warnings
- Tests pass
- Follow existing code style
- Update documentation as needed

## License

MIT License - see [LICENSE](LICENSE) file for details.

## Support

- **Issues**: [GitHub Issues](https://github.com/zarzet/ZarzCLI/issues)
- **Discussions**: [GitHub Discussions](https://github.com/zarzet/ZarzCLI/discussions)
- **Author**: [@zarzet](https://github.com/zarzet)

---

Made with love by zarzet
