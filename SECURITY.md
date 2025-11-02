# Security Guidelines for ZarzCLI

## API Key Management

### ⚠️ CRITICAL: Never commit API keys to version control!

### Protected Files (in .gitignore)

The following files/patterns are automatically excluded from git:

```
# Configuration files
.env
.env.*
*.env
config.toml
.zarz/
**/.zarz/

# API keys and secrets
**/apikey*
**/api-key*
**/*secret*
**/*token*
```

### Safe Ways to Store API Keys

#### Option 1: Config File (Recommended)

API keys stored in `~/.zarz/config.toml` are automatically protected by `.gitignore`:

```toml
# ~/.zarz/config.toml
anthropic_api_key = "sk-ant-..."
openai_api_key = "sk-..."
glm_api_key = "your-glm-key"
```

**Location:**
- Linux/macOS: `~/.zarz/config.toml`
- Windows: `C:\Users\<username>\.zarz\config.toml`

#### Option 2: Environment Variables (Most Secure)

Set API keys as environment variables (session-only):

```bash
# Linux/macOS
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."
export GLM_API_KEY="your-glm-key"

# Windows PowerShell
$env:ANTHROPIC_API_KEY="sk-ant-..."
$env:OPENAI_API_KEY="sk-..."
$env:GLM_API_KEY="your-glm-key"

# Windows CMD
set ANTHROPIC_API_KEY=sk-ant-...
set OPENAI_API_KEY=sk-...
set GLM_API_KEY=your-glm-key
```

#### Option 3: .env File (Development Only)

Create a `.env` file in your project directory (automatically ignored by git):

```bash
# .env
ANTHROPIC_API_KEY=sk-ant-...
OPENAI_API_KEY=sk-...
GLM_API_KEY=your-glm-key
```

**Then load it before running:**
```bash
# Linux/macOS
source .env
zarz

# Windows PowerShell
Get-Content .env | ForEach-Object {
    $name, $value = $_.split('=')
    Set-Item -Path "env:$name" -Value $value
}
```

### ❌ NEVER Do This:

1. **Don't hardcode API keys in source code:**
   ```rust
   // ❌ BAD - NEVER DO THIS
   let api_key = "sk-ant-api03-xxxx";
   ```

2. **Don't commit `.env` or config files:**
   ```bash
   # ❌ BAD
   git add .env
   git add ~/.zarz/config.toml
   ```

3. **Don't share API keys in:**
   - Public repositories
   - Screenshots
   - Documentation examples
   - Chat messages
   - Code reviews

4. **Don't store API keys in:**
   - README files
   - Example files
   - Test files that will be committed
   - CI/CD configs (use secrets instead)

## Verifying Protection

### Check if files are protected:

```bash
# Check git status (should not show config files)
git status

# Verify .gitignore is working
git check-ignore ~/.zarz/config.toml
# Should output: /Users/you/.zarz/config.toml (if protected)
```

### Before committing:

```bash
# Always verify before pushing
git diff --cached
git status
```

### If you accidentally committed an API key:

1. **Immediately revoke the API key** at the provider's dashboard
2. Remove from git history:
   ```bash
   # Remove file from git history
   git rm --cached path/to/file
   git commit -m "Remove sensitive file"

   # Or use git filter-branch (advanced)
   git filter-branch --force --index-filter \
     "git rm --cached --ignore-unmatch path/to/file" \
     --prune-empty --tag-name-filter cat -- --all
   ```
3. Force push (if already pushed to remote)
4. Generate new API keys

## API Key Best Practices

### 1. Use Separate Keys for Different Environments

```toml
# Development
glm_api_key = "dev-key-xxxx"

# Production (different config file)
glm_api_key = "prod-key-yyyy"
```

### 2. Rotate Keys Regularly

- Change API keys every 90 days
- Change immediately if compromised
- Keep a record of key rotation dates

### 3. Use Limited-Scope Keys When Possible

Some providers allow you to create keys with limited permissions:
- Read-only keys for monitoring
- Write keys for production
- Testing keys with rate limits

### 4. Monitor API Usage

Check your provider dashboards regularly for:
- Unexpected usage spikes
- Unusual request patterns
- Access from unknown IPs

## ZarzCLI Security Features

### Built-in Protection

1. **Config File Encryption** (planned)
   - Future: Encrypt `~/.zarz/config.toml` at rest

2. **Secure Memory Handling**
   - API keys only kept in memory during requests
   - Cleared after use

3. **No Logging of Secrets**
   - API keys never written to logs
   - Only error messages logged (without credentials)

### What ZarzCLI Does NOT Do

- ❌ Send your API keys to any server except the provider's API
- ❌ Store API keys in plain text in version control
- ❌ Share your API keys with third parties
- ❌ Log or track your API usage beyond error reporting

## If Your API Key is Compromised

### Immediate Actions:

1. **Revoke the key immediately:**
   - Anthropic: https://console.anthropic.com/settings/keys
   - OpenAI: https://platform.openai.com/api-keys
   - GLM: https://z.ai/manage-apikey/apikey-list

2. **Generate a new key**

3. **Update your configuration:**
   ```bash
   zarz config --reset
   # Or manually edit ~/.zarz/config.toml
   ```

4. **Check for unauthorized usage** in your provider dashboard

5. **Update any automated scripts** or CI/CD pipelines

### Prevention Checklist

- [ ] `.gitignore` includes sensitive file patterns
- [ ] Config files are not in git tracking
- [ ] Environment variables used for CI/CD
- [ ] No API keys in code comments
- [ ] No API keys in documentation
- [ ] Regular key rotation schedule set
- [ ] Usage monitoring enabled

## Reporting Security Issues

If you find a security vulnerability in ZarzCLI:

1. **Do NOT open a public issue**
2. Email: [security contact]
3. Include:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if any)

## Additional Resources

- [OWASP API Security Top 10](https://owasp.org/www-project-api-security/)
- [Anthropic API Best Practices](https://docs.anthropic.com/claude/reference/api-keys)
- [OpenAI API Security](https://platform.openai.com/docs/guides/safety-best-practices)
- [Z.AI Security Guidelines](https://docs.z.ai/)

---

**Remember**: Your API keys are like passwords. Treat them with the same level of security! 🔒
