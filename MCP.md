# Model Context Protocol (MCP) Support

ZarzCLI now supports the Model Context Protocol (MCP), allowing you to connect to external tools and services just like Claude Code and Codex CLI.

## What is MCP?

The Model Context Protocol (MCP) is an open-source standard that enables AI applications to connect to external tools, databases, and services. With MCP, ZarzCLI can:

- Access documentation from Context7
- Control browsers with Playwright
- Query databases
- Interact with APIs (GitHub, Figma, Sentry, etc.)
- And much more!

## Quick Start

### 1. Add an MCP Server

```bash
# Add Firecrawl MCP server
zarz mcp add firecrawl \
  --command npx \
  --args -y firecrawl-mcp \
  --env FIRECRAWL_API_KEY=your-api-key

# Add Context7 for documentation
zarz mcp add context7 \
  --command npx \
  --args -y @upstash/context7-mcp

# Add a custom server
zarz mcp add myserver \
  --command /path/to/server \
  --args --port 8080 \
  --env API_KEY=secret
```

### 2. List Configured Servers

```bash
zarz mcp list
```

Output:
```
Configured MCP servers:

  firecrawl
    Type: stdio
    Command: npx
    Args: -y firecrawl-mcp
    Environment:
      FIRECRAWL_API_KEY=your-api-key

  context7
    Type: stdio
    Command: npx
    Args: -y @upstash/context7-mcp
```

### 3. Get Server Details

```bash
zarz mcp get firecrawl
```

### 4. Remove a Server

```bash
zarz mcp remove firecrawl
```

## MCP CLI Commands

### `zarz mcp add`

Add a new MCP server to your configuration.

**STDIO servers** (local processes):
```bash
zarz mcp add <name> \
  --transport stdio \
  --command <command> \
  [--args <arg1> <arg2> ...] \
  [--env KEY1=VALUE1 --env KEY2=VALUE2]
```

**HTTP servers** (remote):
```bash
zarz mcp add <name> \
  --transport http \
  --url <https://api.example.com/mcp>
```

**SSE servers** (Server-Sent Events):
```bash
zarz mcp add <name> \
  --transport sse \
  --url <https://api.example.com/sse>
```

### `zarz mcp list`

List all configured MCP servers.

### `zarz mcp get <name>`

Get detailed information about a specific MCP server.

### `zarz mcp remove <name>`

Remove an MCP server from your configuration.

## Configuration File

MCP servers are stored in `~/.zarz/mcp.json`:

```json
{
  "mcpServers": {
    "firecrawl": {
      "command": "npx",
      "args": ["-y", "firecrawl-mcp"],
      "env": {
        "FIRECRAWL_API_KEY": "your-api-key"
      }
    },
    "context7": {
      "command": "npx",
      "args": ["-y", "@upstash/context7-mcp"]
    },
    "github": {
      "url": "https://api.githubcopilot.com/mcp/"
    }
  }
}
```

You can also edit this file directly if you prefer.

## Popular MCP Servers

### Documentation & Learning

**Context7** - Access up-to-date developer documentation
```bash
zarz mcp add context7 --command npx --args -y @upstash/context7-mcp
```

### Web Scraping & Data

**Firecrawl** - Web scraping and data extraction
```bash
zarz mcp add firecrawl \
  --command npx --args -y firecrawl-mcp \
  --env FIRECRAWL_API_KEY=your-key
```

### Browser Automation

**Playwright** - Control and inspect browsers
```bash
zarz mcp add playwright --command npx --args -y @playwright/mcp
```

**Chrome DevTools** - Control Chrome browser
```bash
zarz mcp add chrome --command npx --args -y chrome-devtools-mcp
```

### Development Tools

**GitHub** - Manage repos, PRs, issues
```bash
zarz mcp add github --transport http --url https://api.githubcopilot.com/mcp/
```

**Sentry** - Error monitoring
```bash
zarz mcp add sentry --transport http --url https://mcp.sentry.dev/mcp
```

**Figma** - Design access
```bash
zarz mcp add figma --transport http --url https://mcp.figma.com/mcp
```

### Databases

**PostgreSQL** - Database access
```bash
zarz mcp add postgres \
  --command npx --args -y @bytebase/dbhub \
  --args --dsn "postgresql://user:pass@localhost:5432/db"
```

## Using MCP in ZarzCLI

### In Interactive Chat Mode

Once MCP servers are configured, they will automatically connect when you start ZarzCLI:

```bash
zarz
```

The MCP servers will start in the background and their tools will be available to the AI.

### Access MCP Tools

```
> /mcp
```

This will show all connected MCP servers and their available tools.

### Example Workflow

```bash
# 1. Add Firecrawl MCP server
zarz mcp add firecrawl \
  --command npx --args -y firecrawl-mcp \
  --env FIRECRAWL_API_KEY=fc-xxxxx

# 2. Start ZarzCLI
zarz

# 3. In chat, ask AI to use Firecrawl
> "Scrape the homepage of example.com and summarize the content"

# The AI will automatically use Firecrawl MCP tools to fetch the content!
```

## Supported Transports

### 1. STDIO (Standard Input/Output)

Most common for local MCP servers. The server runs as a child process and communicates via stdin/stdout using JSON-RPC.

**Pros:**
- Fastest communication
- No network overhead
- Easy to debug

**Cons:**
- Only works locally
- Server must support stdio transport

### 2. HTTP

For remote MCP servers over HTTP.

**Pros:**
- Works remotely
- Widely supported
- Can use authentication headers

**Cons:**
- Slower than stdio
- Network latency

### 3. SSE (Server-Sent Events)

Streaming protocol for remote servers.

**Pros:**
- Supports streaming
- Real-time updates

**Cons:**
- Less widely supported
- Being deprecated in favor of HTTP

## Troubleshooting

### Server won't start

Check that the command is correct and the server is installed:
```bash
# Test the command directly
npx -y firecrawl-mcp

# Check if it's in PATH
which npx
```

### Environment variables not working

Make sure you're using the `--env` flag correctly:
```bash
zarz mcp add myserver \
  --command mycommand \
  --env API_KEY=value \
  --env ANOTHER_VAR=value2
```

### Configuration file location

The MCP config is stored at:
- **Linux/macOS**: `~/.zarz/mcp.json`
- **Windows**: `C:\Users\<username>\.zarz\mcp.json`

## Example: Complete Setup

Here's a complete example setting up ZarzCLI with multiple MCP servers:

```bash
# 1. Install ZarzCLI
npm install -g zarz

# 2. Add MCP servers
zarz mcp add context7 --command npx --args -y @upstash/context7-mcp
zarz mcp add firecrawl --command npx --args -y firecrawl-mcp --env FIRECRAWL_API_KEY=your-key
zarz mcp add playwright --command npx --args -y @playwright/mcp

# 3. List configured servers
zarz mcp list

# 4. Start ZarzCLI
zarz

# Now you can use all MCP tools in your chat!
```

## Future Enhancements

The current MCP implementation supports:
- ✅ STDIO transport
- ✅ Config management
- ✅ CLI commands
- ⏳ HTTP transport (structure ready, needs implementation)
- ⏳ Tool execution in REPL
- ⏳ Resource and prompt support
- ⏳ OAuth authentication

## Learn More

- [MCP Official Documentation](https://modelcontextprotocol.io/)
- [MCP Server Registry](https://github.com/modelcontextprotocol/servers)
- [Building MCP Servers](https://modelcontextprotocol.io/quickstart/server)

---

**Note**: MCP integration is currently in beta. Tool execution from REPL will be available in the next update!
