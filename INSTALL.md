# Installation Guide

ZarzCLI can be installed via npm or built from source. Choose the method that works best for you.

## Quick Install (Recommended)

### Via npm

The easiest way to install ZarzCLI is through npm:

```bash
npm install -g zarz-cli
```

This will:
1. Download the package
2. Automatically build the native Rust binary
3. Make the `zarz` command available globally

**Requirements:**
- Node.js 14.0.0 or higher
- Rust and Cargo (will be installed if needed)

**First-time installation:**

If you don't have Rust installed, the installer will prompt you:

```bash
npm install -g zarz-cli

# If Rust is not found:
# WARNING: Rust/Cargo not found!
# Please install Rust from: https://rustup.rs/
# After installing Rust, run: npm install
```

Install Rust from [rustup.rs](https://rustup.rs/), then run the install command again.

## Build from Source

If you prefer to build directly from source:

### Prerequisites

- **Rust toolchain** (rustc, cargo) - Install from [rustup.rs](https://rustup.rs/)
- **Git** (optional, for cloning)

### Steps

1. Clone the repository:
```bash
git clone https://github.com/fapzarz/zarzcli.git
cd zarzcli
```

2. Build the release binary:
```bash
cargo build --release
```

3. The binary will be at `target/release/zarzcli` (or `zarzcli.exe` on Windows)

### Running from Source

**On Unix/Linux/macOS:**
```bash
./target/release/zarzcli
# Or use the wrapper:
./zarz.sh
```

**On Windows:**
```batch
target\release\zarzcli.exe
# Or use the wrapper:
zarz.bat
```

### Adding to PATH

To run `zarz` from anywhere, add it to your PATH:

**Unix/Linux/macOS:**
```bash
# Add to ~/.bashrc or ~/.zshrc
export PATH="$PATH:/path/to/zarzcli/target/release"
```

**Windows:**
```batch
# Add to System Environment Variables:
# C:\path\to\zarzcli\target\release
```

## Verifying Installation

After installation, verify it works:

```bash
zarz --version
zarz --help
```

You should see the help text showing available commands.

## Setting Up API Keys

Before using ZarzCLI, set up your API key:

### For Claude (Anthropic)

```bash
# Unix/Linux/macOS
export ANTHROPIC_API_KEY=sk-ant-your-key-here

# Windows (Command Prompt)
set ANTHROPIC_API_KEY=sk-ant-your-key-here

# Windows (PowerShell)
$env:ANTHROPIC_API_KEY="sk-ant-your-key-here"
```

To make it permanent:

**Unix/Linux/macOS:**
```bash
# Add to ~/.bashrc or ~/.zshrc
echo 'export ANTHROPIC_API_KEY=sk-ant-your-key-here' >> ~/.bashrc
source ~/.bashrc
```

**Windows:**
Add it to System Environment Variables through System Properties.

### For OpenAI

```bash
# Unix/Linux/macOS
export OPENAI_API_KEY=sk-your-key-here

# Windows (Command Prompt)
set OPENAI_API_KEY=sk-your-key-here

# Windows (PowerShell)
$env:OPENAI_API_KEY="sk-your-key-here"
```

## First Run

Start an interactive session:

```bash
zarz
```

You should see:
```
ZarzCLI v0.1.0
Model: claude-sonnet-4-5-20250929
Type /help for available commands, /quit to exit

>
```

Try asking a question:
```
> What is Rust?
```

Or use a quick one-shot command:
```bash
zarz --message "Explain async/await in Rust"
```

## Configuration

ZarzCLI can be configured through environment variables:

```bash
# Model selection
export ZARZ_MODEL=claude-sonnet-4-5-20250929
export ZARZ_PROVIDER=anthropic

# Generation parameters
export ZARZ_MAX_OUTPUT_TOKENS=4096
export ZARZ_TEMPERATURE=0.7

# API endpoints (optional)
export ANTHROPIC_API_URL=https://api.anthropic.com/v1/messages
export OPENAI_API_URL=https://api.openai.com/v1/chat/completions

# Timeouts
export ANTHROPIC_TIMEOUT_SECS=300
export OPENAI_TIMEOUT_SECS=300
```

See [QUICKSTART.md](QUICKSTART.md) for detailed usage guide and [MODELS.md](MODELS.md) for model selection.

## Updating

### npm Installation

```bash
npm update -g zarz-cli
```

### Source Build

```bash
cd zarzcli
git pull
cargo build --release
```

## Uninstalling

### npm Installation

```bash
npm uninstall -g zarz-cli
```

### Source Build

Simply delete the directory:
```bash
rm -rf zarzcli
```

## Troubleshooting

### "command not found: zarz"

**npm installation:**
- Check if npm global bin is in PATH: `npm config get prefix`
- Add to PATH if needed: `export PATH="$PATH:$(npm config get prefix)/bin"`

**Source build:**
- Ensure the binary is in your PATH
- Or run with full path: `./target/release/zarzcli`

### "Build failed" during npm install

1. Make sure Rust is installed: `cargo --version`
2. Update Rust: `rustup update`
3. Check internet connection (downloads dependencies)
4. Try building manually: `cargo build --release`

### "API key not found"

Make sure you've set the appropriate environment variable:
```bash
echo $ANTHROPIC_API_KEY  # Should show your key
```

If empty, set it again (see "Setting Up API Keys" above).

### Binary not found on Windows

If you get "Binary not found" error on Windows:

1. Make sure you're running from Command Prompt or PowerShell (not Git Bash)
2. Try the batch wrapper: `zarz.bat`
3. Check if binary exists: `dir target\release\zarzcli.exe`

### Permission denied (Unix/Linux/macOS)

Make the binary executable:
```bash
chmod +x target/release/zarzcli
```

## Platform-Specific Notes

### Windows

- Use Command Prompt or PowerShell
- Git Bash works but may have path issues
- The `zarz.bat` wrapper handles building automatically

### macOS

- You may need to allow the binary in Security & Privacy settings on first run
- If you get "cannot be opened because the developer cannot be verified", run:
  ```bash
  xattr -d com.apple.quarantine target/release/zarzcli
  ```

### Linux

- Most distributions work out of the box
- On some systems you may need to install `pkg-config` and `libssl-dev`:
  ```bash
  # Debian/Ubuntu
  sudo apt install pkg-config libssl-dev

  # Fedora
  sudo dnf install pkg-config openssl-devel
  ```

## Getting Help

- Run `zarz --help` for command-line options
- Run `zarz` then type `/help` for chat commands
- See [README.md](README.md) for overview
- See [QUICKSTART.md](QUICKSTART.md) for detailed guide
- See [MODELS.md](MODELS.md) for model documentation
- Report issues: [GitHub Issues](https://github.com/fapzarz/zarzcli/issues)

## What's Next?

After installation, check out:

1. [QUICKSTART.md](QUICKSTART.md) - Learn the basics and common workflows
2. [MODELS.md](MODELS.md) - Understand which model to use for your task
3. [README.md](README.md) - Full feature documentation

Happy coding with ZarzCLI!
