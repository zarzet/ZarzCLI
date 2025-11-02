# Supported AI Models

ZarzCLI supports the latest AI models from Anthropic and OpenAI, optimized for coding tasks.

## Quick Reference

| Model | ID | Provider | Input/Output | Best For |
|-------|-----|----------|--------------|----------|
| Claude Sonnet 4.5 | `claude-sonnet-4-5-20250929` | Anthropic | $3/$15 | Complex coding, agents |
| Claude Haiku 4.5 | `claude-haiku-4-5` | Anthropic | $1/$5 | Fast iterations |
| Claude Opus 4.1 | `claude-opus-4-1` | Anthropic | TBD | Maximum intelligence |
| GPT-5 Codex | `gpt-5-codex` | OpenAI | $1.25/$10 | Agentic coding |

## Anthropic Claude Models

### Claude Sonnet 4.5 (Default)

**Model ID:** `claude-sonnet-4-5-20250929`

**Released:** September 29, 2025

**Pricing:** $3 per 1M input tokens, $15 per 1M output tokens

**Context Window:** 1M tokens (750,000 words)

**Best For:**
- Complex code refactoring
- Building AI agents
- Multi-file codebase changes
- Architectural decisions

**Key Features:**
- Best coding model in the world
- Strongest model for complex agents
- Excellent instruction following
- Superior code generation quality

**Usage:**
```bash
./zarz.sh chat
# Uses Claude Sonnet 4.5 by default

./zarz.sh ask --model claude-sonnet-4-5-20250929 --prompt "Refactor this module" src/main.rs
```

### Claude Sonnet 4.5 Thinking

**Model ID:** `claude-sonnet-4-5-20250929-thinking`

**Released:** September 29, 2025

**Pricing:** Same as Claude Sonnet 4.5

**Best For:**
- Problems requiring deep reasoning
- Complex algorithmic challenges
- Architectural planning
- Code optimization strategies

**Key Features:**
- Extended thinking mode
- Dynamic reasoning time (seconds to hours)
- Better for problems that benefit from "thinking time"
- Shows reasoning process

**Usage:**
```bash
./zarz.sh ask --model claude-sonnet-4-5-20250929-thinking \
  --prompt "Design a scalable architecture for this system" \
  docs/requirements.md
```

### Claude Haiku 4.5

**Model ID:** `claude-haiku-4-5`

**Released:** October 15, 2025

**Pricing:** $1 per 1M input tokens, $5 per 1M output tokens

**Best For:**
- Fast iterations
- Simple refactoring tasks
- Code reviews
- Quick questions
- Cost-sensitive applications

**Key Features:**
- Similar coding performance to Sonnet 4
- 1/3 the cost of Sonnet 4
- More than 2x faster than Sonnet 4
- Great cost/performance ratio

**Usage:**
```bash
./zarz.sh chat --model claude-haiku-4-5

./zarz.sh rewrite --model claude-haiku-4-5 \
  --instructions "Add error handling" \
  src/lib.rs
```

### Claude Opus 4.1

**Model ID:** `claude-opus-4-1`

**Released:** August 5, 2025

**Pricing:** To be announced

**Best For:**
- Extremely complex tasks
- Maximum quality requirements
- Novel problem solving
- Critical code generation

**Key Features:**
- Most powerful Claude model
- Superior reasoning capabilities
- Best for tasks where quality matters most
- Improved code generation and search reasoning

**Usage:**
```bash
./zarz.sh ask --model claude-opus-4-1 \
  --prompt "Implement a complex algorithm with optimal performance" \
  src/core.rs
```

### Claude Sonnet 4

**Model ID:** `claude-sonnet-4`

**Released:** May 22, 2025

**Context Window:** 1M tokens

**Best For:**
- General coding tasks
- Large codebase analysis
- Legacy support

**Usage:**
```bash
./zarz.sh chat --model claude-sonnet-4
```

## OpenAI Models

### GPT-5 Codex

**Model ID:** `gpt-5-codex`

**Released:** September 15, 2025

**Pricing:** $1.25 per 1M input tokens, $10 per 1M output tokens

**Best For:**
- Agentic coding workflows
- Interactive development sessions
- Persistent, independent execution
- Long-running coding tasks

**Key Features:**
- Optimized for agentic coding
- Dynamic thinking time (seconds to 7 hours)
- Adapts complexity handling based on task
- Excellent for pairing with developers
- Integrated with GitHub Copilot

**Usage:**
```bash
./zarz.sh chat --model gpt-5-codex --provider openai

./zarz.sh rewrite --model gpt-5-codex --provider openai \
  --instructions "Refactor to async/await" \
  src/server.rs
```

**Note:** Requires `OPENAI_API_KEY` environment variable.

### GPT-4o

**Model ID:** `gpt-4o`

**Best For:**
- Multimodal tasks
- Image analysis
- General coding

**Usage:**
```bash
./zarz.sh ask --model gpt-4o --provider openai \
  --prompt "What does this code do?" \
  src/main.rs
```

### GPT-4 Turbo

**Model ID:** `gpt-4-turbo`

**Best For:**
- Fast responses
- Cost-effective OpenAI option
- General purpose

**Usage:**
```bash
./zarz.sh ask --model gpt-4-turbo --provider openai \
  --prompt "Explain this function"
```

## Model Selection Decision Tree

```
Need maximum quality?
├─ Yes → Claude Opus 4.1
└─ No → Continue

Need extended thinking?
├─ Yes → Claude Sonnet 4.5 Thinking
└─ No → Continue

Budget sensitive?
├─ Yes → Claude Haiku 4.5
└─ No → Continue

Prefer OpenAI?
├─ Yes → GPT-5 Codex
└─ No → Claude Sonnet 4.5 (Default)
```

## Cost Optimization Tips

### Use Haiku for Simple Tasks
```bash
# Good for Haiku (1/3 cost)
./zarz.sh ask --model claude-haiku-4-5 --prompt "Add docstrings" src/utils.rs

# Better for Sonnet (higher quality)
./zarz.sh ask --model claude-sonnet-4-5-20250929 --prompt "Refactor architecture" src/
```

### Cache Input Tokens
GPT-5 Codex offers cached input discounts at $0.125 per 1M tokens. Reuse context when possible.

### Batch Operations
```bash
# Instead of multiple single-file operations
./zarz.sh rewrite --model claude-haiku-4-5 --instructions "Fix linting" src/*.rs
```

## Environment Configuration

### Set Default Model
```bash
# Use Haiku by default to save costs
export ZARZ_MODEL=claude-haiku-4-5

# Use Sonnet for quality
export ZARZ_MODEL=claude-sonnet-4-5-20250929

# Use GPT-5 Codex
export ZARZ_MODEL=gpt-5-codex
export ZARZ_PROVIDER=openai
```

### Per-Session Overrides
```bash
# Override for specific task
./zarz.sh chat --model claude-opus-4-1
```

## API Keys Setup

### Anthropic
```bash
export ANTHROPIC_API_KEY=sk-ant-...
export ANTHROPIC_API_URL=https://api.anthropic.com/v1/messages  # optional
export ANTHROPIC_TIMEOUT_SECS=120  # optional
```

### OpenAI
```bash
export OPENAI_API_KEY=sk-...
export OPENAI_API_URL=https://api.openai.com/v1/chat/completions  # optional
export OPENAI_TIMEOUT_SECS=120  # optional
```

## Legacy Models

For backward compatibility, older model IDs are still supported:

- `claude-3-5-sonnet-20241022` - Legacy Sonnet 3.5

## Frequently Asked Questions

### Which model should I use for chat mode?

For interactive sessions, **Claude Sonnet 4.5** (default) provides the best balance of quality, speed, and cost. If budget is a concern, **Claude Haiku 4.5** offers similar quality at 1/3 the cost.

### When should I use Thinking mode?

Use **Claude Sonnet 4.5 Thinking** when:
- The problem is algorithmically complex
- You need to see the reasoning process
- Quality is more important than speed
- The task benefits from extended analysis

### Is GPT-5 Codex better than Claude Sonnet 4.5?

Both are excellent for coding:
- **GPT-5 Codex** excels at agentic workflows and long-running tasks
- **Claude Sonnet 4.5** is better for complex refactoring and agent building
- Try both to see which fits your workflow

### Can I mix models in a chat session?

Currently, no. Each chat session uses a single model. However, you can:
- Exit and start a new session with a different model
- Use different models for `ask` and `rewrite` commands

### How do I reduce costs?

1. Use **Claude Haiku 4.5** for simple tasks
2. Use specific prompts to get concise responses
3. Avoid loading unnecessary context files
4. Set `ZARZ_MAX_OUTPUT_TOKENS` to limit response length
5. Use batch operations instead of multiple individual calls

## Release Notes

### 2025 Models
- **September 2025:** Claude Sonnet 4.5, GPT-5 Codex released
- **October 2025:** Claude Haiku 4.5 released
- **August 2025:** Claude Opus 4.1 released
- **May 2025:** Claude Sonnet 4, Claude Opus 4 released

## References

- [Anthropic Claude Models](https://docs.anthropic.com/en/docs/about-claude/models/overview)
- [OpenAI Models](https://platform.openai.com/docs/models)
- [Claude Sonnet 4.5 Announcement](https://www.anthropic.com/news/claude-sonnet-4-5)
- [GPT-5 Codex System Card](https://openai.com/index/gpt-5-system-card-addendum-gpt-5-codex/)
