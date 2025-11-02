# GLM Provider Support (Z.AI International)

ZarzCLI now supports **GLM-4.6** via Z.AI's international platform - a world-class coding model with 200K context window!

## About GLM-4.6

GLM-4.6 is the latest flagship model optimized for:
- **AI-Powered Coding** - Superior performance in real-world coding tasks
- **Long Context** - 200K token context window for complex projects
- **Advanced Reasoning** - Improved reasoning and tool use capabilities
- **Cost-Effective** - Starting at just $3/month via GLM Coding Plan

### Model Specifications

- **Context Window**: 200K tokens
- **Max Output**: 128K tokens
- **Input**: Text
- **Output**: Text
- **Pricing**: $3/month (GLM Coding Plan)
- **API**: OpenAI-compatible

## Getting Started

### 1. Get Your GLM API Key

1. Visit [Z.AI Open Platform](https://z.ai/model-api)
2. Register or login to your account
3. Top up your balance if needed at [Billing Page](https://z.ai/manage-apikey/billing)
4. Create an API Key at [API Keys Management](https://z.ai/manage-apikey/apikey-list)
5. Copy your API key for use

### 2. Configure ZarzCLI

#### Option A: Interactive Setup

Run ZarzCLI without configuration:

```bash
zarz
```

Follow the prompts and enter your GLM API key when asked.

#### Option B: Manual Configuration

Edit `~/.zarz/config.toml` and add:

```toml
[glm]
api_key = "your-glm-api-key-here"
```

#### Option C: Environment Variable

Set the environment variable:

```bash
# Linux/macOS
export GLM_API_KEY="your-glm-api-key-here"

# Windows PowerShell
$env:GLM_API_KEY="your-glm-api-key-here"

# Windows CMD
set GLM_API_KEY=your-glm-api-key-here
```

### 3. Use GLM Models

#### One-Shot Mode

```bash
# Use GLM-4.6 with --provider flag
zarz --message "Write a function to sort an array" --provider glm --model glm-4.6

# Or set as default provider
export ZARZ_PROVIDER=glm
zarz --message "Write a function to sort an array"
```

#### Interactive Chat Mode

```bash
# Start with GLM provider
zarz --provider glm --model glm-4.6

# Or switch models during chat
zarz
> /model glm-4.6
```

## Available GLM Models

| Model | Description | Context | Best For |
|-------|-------------|---------|----------|
| **glm-4.6** | Latest flagship model | 200K | Coding, long context tasks |
| glm-4.5 | Previous generation | 128K | General tasks |

## Features

### ✅ Full Feature Support

- ✅ **Streaming responses** - Real-time output
- ✅ **System prompts** - Custom instructions
- ✅ **Long context** - Up to 200K tokens
- ✅ **Tool use** - Function calling support
- ✅ **Interactive REPL** - Chat mode
- ✅ **One-shot mode** - Quick commands

### 🔧 Configuration Options

You can customize GLM behavior with environment variables:

```bash
# Custom API endpoint (if using proxy)
export GLM_API_URL="https://your-proxy.com/api"

# Timeout (default: 120 seconds)
export GLM_TIMEOUT_SECS=180

# Default model
export ZARZ_MODEL=glm-4.6

# Provider
export ZARZ_PROVIDER=glm
```

## Examples

### Example 1: Quick Code Generation

```bash
zarz --provider glm --model glm-4.6 --message "Create a REST API for user management in Python using FastAPI"
```

### Example 2: Interactive Refactoring

```bash
zarz --provider glm --model glm-4.6
> I have a large codebase with 50+ files. Can you help me refactor the authentication module?
> /edit src/auth.py
> Please add JWT token support and refresh tokens
```

### Example 3: Long Context Analysis

```bash
zarz --provider glm --model glm-4.6
> /context "analyze entire authentication flow"
> Explain how the authentication system works across all files
```

## Performance Comparison

From Z.AI's official benchmarks:

- **Real-world Coding Tests**: GLM-4.6 surpasses Claude Sonnet 4 in 74 practical coding tasks
- **Token Efficiency**: 30% more efficient than GLM-4.5
- **Context Handling**: Best-in-class with 200K tokens

## Pricing

### GLM Coding Plan

- **$3/month** - Basic plan with generous usage
- **3× more usage** than standard plans
- **1/7 the cost** of competing services
- Compatible with Claude Code, Cline, and other coding tools

Visit [Z.AI Pricing](https://z.ai/subscribe) for current pricing.

## Troubleshooting

### Error: Environment variable GLM_API_KEY is required

**Solution**: Make sure you've configured your API key using one of the methods above.

### Error: GLM returned an error status

**Possible causes:**
1. Invalid API key
2. Insufficient credits
3. Rate limiting

**Solutions:**
1. Check your API key at [Z.AI API Keys](https://z.ai/manage-apikey/apikey-list)
2. Top up credits at [Billing Page](https://z.ai/manage-apikey/billing)
3. Wait a few moments and try again

### Slow Responses

**Tips:**
- GLM-4.6 with 200K context may take longer for very large inputs
- Use smaller context when possible
- Consider using glm-4.5 for simpler tasks

## API Documentation

For advanced use cases, refer to:

- [Z.AI Developer Documentation](https://docs.z.ai/guides/overview/quick-start)
- [GLM-4.6 API Reference](https://docs.z.ai/api-reference/llm/chat-completion)
- [GLM-4.6 Model Details](https://docs.z.ai/guides/llm/glm-4.6)

## Switching Between Providers

You can easily switch between Anthropic, OpenAI, and GLM:

```bash
# Use Claude for general tasks
zarz --provider anthropic --model claude-sonnet-4-5-20250929

# Use GLM for cost-effective coding
zarz --provider glm --model glm-4.6

# Use OpenAI when needed
zarz --provider openai --model gpt-4o
```

Or switch in interactive mode:

```bash
zarz
> /model glm-4.6        # Switch to GLM
> /model claude-sonnet-4-5-20250929  # Switch to Claude
> /model gpt-4o         # Switch to OpenAI
```

## Why Use GLM?

### Advantages

1. **Cost-Effective** - $3/month vs $20/month for competitors
2. **Long Context** - 200K tokens for large codebases
3. **Coding-Optimized** - Specifically tuned for development tasks
4. **International Access** - Z.AI provides global availability
5. **OpenAI-Compatible** - Easy to integrate and use

### Use Cases

- Large codebase refactoring
- Complex multi-file projects
- Extended coding sessions
- Cost-sensitive development
- International developers needing reliable access

## Support & Resources

- **Documentation**: https://docs.z.ai
- **Platform**: https://z.ai/model-api
- **Issues**: https://github.com/zarzet/zarzcli/issues

---

**Note**: GLM-4.6 is provided by Z.AI (Zhipu AI) via their international platform. ZarzCLI acts as a client interface to their API.
