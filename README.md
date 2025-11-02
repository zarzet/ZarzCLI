# ZarzCLI

Fast AI coding assistant for terminal built with Rust.

## Installation

```bash
npm install -g zarz
```

## First Run Setup

On first run, you'll be prompted to enter your API keys interactively:

```bash
zarz

# Or set manually via environment variable
export ANTHROPIC_API_KEY=sk-ant-...
zarz
```

Your API keys are securely stored in `~/.zarz/config.toml`

## Usage

```bash
# Start interactive chat (default)
zarz

# Quick one-shot question
zarz --message "fix this bug"

# Manage configuration
zarz config --show     # Show current config
zarz config --reset    # Reconfigure API keys
```

## Features

- Interactive chat with AI (Claude, GPT & GLM models)
- Real-time streaming responses
- Automatic API key management
- File operations & code editing
- Symbol search & context detection
- MCP (Model Context Protocol) support
- Cross-platform (Windows, Linux, macOS)

## Supported AI Providers

- **Anthropic Claude** - Best for coding and agents
- **OpenAI GPT** - Multimodal capabilities
- **GLM (Z.AI)** - Cost-effective coding with 200K context ($3/month)

See [GLM-PROVIDER.md](GLM-PROVIDER.md) for detailed GLM setup and usage.

## Requirements

- Node.js 14.0.0 or higher
- Rust toolchain (auto-installed if missing)
- API key: Anthropic Claude, OpenAI, or GLM (Z.AI)

## License

Proprietary - All rights reserved

© 2025 zarzet. This software is licensed for personal use only.
