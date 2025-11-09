# Changelog

## 0.5.0-ALPHA

### New Native tooling
- Added native `read_file`, `list_dir`, `grep_files`, and `apply_patch` handlers; the AI no longer needs to shell out to `bash` for routine context gathering.
- `apply_patch` now applies Zarz-style diffs directly (Add/Update/Delete blocks) with safe path resolution.
- Tool invocations log like “• Explored…” summaries so the terminal stays concise while OpenAI still receives the full output.

### Prompt and UX cleanup
- Updated README.md to describe the new tool stack and exploration logs.

### Codex authentication
- Added full ChatGPT OAuth wizard, including PKCE, `originator=zarz_cli`, and first-party success page branded “Signed in to ZarzCLI”.
- Stored access/refresh/id tokens plus `project_id`, `organization_id`, and `chatgpt_account_id` in `~/.zarz/config.toml` with automatic refresh before every run.
- Exported Codex environment variables (`OPENAI_API_URL`, `OpenAI-Beta`, `originator`, etc.) so GPT‑5 presets transparently call the ChatGPT backend without requiring an API key.

### Responses API compatibility
- OpenAI provider now detects when Codex headers are required, converts system prompts into Codex “instructions”, filters unsupported roles, and parses SSE responses with encrypted reasoning content.
- Removed legacy tool-call throttling and forced filler replies—models can keep invoking tools with clean, autonomous logs.

### User-facing polish
- login-success HTML (with ZarzCLI branding) for a consistent OAuth experience.
- Documented the new flow, GPT‑5 presets, and References workspace inside `README.md`.
- Ignored `References/` in git so upstream mirrors don’t pollute commits.
