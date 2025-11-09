use anyhow::{Context, Result};
use crossterm::style::{Color, Stylize};
use dialoguer::{theme::ColorfulTheme, Select};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::providers::ReasoningEffort;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anthropic_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glm_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_reasoning_effort: Option<ReasoningEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_oauth_tokens: Option<OAuthTokens>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_organization_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_chatgpt_account_id: Option<String>,
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not determine home directory")?;
        Ok(home.join(".zarz").join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)
            .context("Failed to read config file")?;

        let config: Config = toml::from_str(&content)
            .context("Failed to parse config file")?;

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
        }

        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;

        fs::write(&path, content)
            .context("Failed to write config file")?;

        Ok(())
    }

    pub fn has_api_key(&self) -> bool {
        self.anthropic_api_key.is_some()
            || self.openai_api_key.is_some()
            || self.openai_oauth_tokens.is_some()
            || self.glm_api_key.is_some()
    }

    pub fn has_openai_auth(&self) -> bool {
        self.openai_api_key.is_some() || self.openai_oauth_tokens.is_some()
    }

    pub fn interactive_setup() -> Result<Self> {
        let theme = ColorfulTheme::default();

        println!(
            "\n{}\n",
            "ZarzCLI Setup".bold().with(Color::Cyan)
        );
        println!(
            "{}",
            "Choose a provider to configure. Use the arrow keys and press Enter.".with(Color::DarkGrey)
        );
        println!(
            "{}",
            "API keys are displayed while typing so you can verify them, then hidden before storing.".with(Color::DarkGrey)
        );
        println!(
            "{}\n",
            "You can configure additional providers later with `zarz config --reset`.".with(Color::DarkGrey)
        );
        println!(
            "{}\n",
            "Prefer logging in with ChatGPT instead? Use `zarz config --login-chatgpt` to run the OAuth flow.".with(Color::DarkGrey)
        );

        let options = vec![
            "Anthropic Claude (recommended for coding)".bold().with(Color::Yellow).to_string(),
            "OpenAI GPT".bold().with(Color::Yellow).to_string(),
            "GLM (Z.AI - International GLM-4.6)".bold().with(Color::Yellow).to_string(),
        ];

        let selection = Select::with_theme(&theme)
            .with_prompt("Select a provider to set up")
            .items(&options)
            .default(0)
            .interact()?;

        let mut config = Self::default();
        let mut enabled = Vec::new();

        match selection {
            0 => {
                let key = Self::prompt_for_key("Anthropic API key")?;
                config.anthropic_api_key = Some(key);
                enabled.push("Anthropic Claude");
                println!("{}\n", "✓ Anthropic ready".with(Color::Green));
            }
            1 => {
                let key = Self::prompt_for_key("OpenAI API key")?;
                config.openai_api_key = Some(key);
                enabled.push("OpenAI GPT");
                println!("{}\n", "✓ OpenAI ready".with(Color::Green));
            }
            _ => {
                let key = Self::prompt_for_key("GLM API key")?;
                config.glm_api_key = Some(key);
                enabled.push("GLM 4.6");
                println!("{}\n", "✓ GLM ready".with(Color::Green));
            }
        }

        if !config.has_api_key() {
            anyhow::bail!("At least one API key is required to use ZarzCLI");
        }

        config.save()?;
        println!(
            "{} {}\n",
            "[OK]".with(Color::Green),
            format!(
                "Configuration saved to {}",
                Self::config_path()?.display()
            )
            .bold()
        );
        println!(
            "{}",
            format!("Enabled providers: {}", enabled.join(", ")).with(Color::Green)
        );
        println!(
            "{}\n",
            "Run `zarz` any time to start chatting.".with(Color::DarkGrey)
        );

        Ok(config)
    }

    fn prompt_for_key(label: &str) -> Result<String> {
        loop {
            print!("Enter your {}: ", label);
            io::stdout().flush().ok();

            let mut key = String::new();
            io::stdin()
                .read_line(&mut key)
                .context("Failed to read API key from stdin")?;

            let trimmed = key.trim();
            if trimmed.is_empty() {
                println!("{}", "Key cannot be empty. Please try again.".with(Color::Red));
                continue;
            }

            println!("{}", "Key captured ✔".with(Color::Green));
            println!(
                "{}",
                "(The key is now stored securely and will no longer be displayed.)".with(Color::DarkGrey)
            );

            return Ok(trimmed.to_string());
        }
    }

    pub fn get_anthropic_key(&self) -> Option<String> {
        std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .or_else(|| self.anthropic_api_key.clone())
    }

    pub fn get_openai_key(&self) -> Option<String> {
        std::env::var("OPENAI_API_KEY")
            .ok()
            .or_else(|| self.openai_api_key.clone())
    }

    pub fn get_glm_key(&self) -> Option<String> {
        std::env::var("GLM_API_KEY")
            .ok()
            .or_else(|| self.glm_api_key.clone())
    }

    pub fn get_openai_reasoning_effort(&self) -> Option<ReasoningEffort> {
        self.openai_reasoning_effort
    }

    pub fn get_default_provider(&self) -> Option<crate::cli::Provider> {
        if self.get_anthropic_key().is_some() {
            Some(crate::cli::Provider::Anthropic)
        } else if self.has_openai_auth() {
            Some(crate::cli::Provider::OpenAi)
        } else if self.get_glm_key().is_some() {
            Some(crate::cli::Provider::Glm)
        } else {
            None
        }
    }

    pub fn apply_to_env(&self) {
        if let Some(key) = &self.anthropic_api_key {
            if std::env::var("ANTHROPIC_API_KEY").is_err() {
                unsafe { std::env::set_var("ANTHROPIC_API_KEY", key); }
            }
        }

        // For OpenAI: prefer explicit API key, otherwise use OAuth access token
        if let Some(key) = &self.openai_api_key {
            if std::env::var("OPENAI_API_KEY").is_err() {
                unsafe { std::env::set_var("OPENAI_API_KEY", key); }
            }
        } else if let Some(tokens) = &self.openai_oauth_tokens {
            // Always export the latest access token so refreshed values propagate
            unsafe { std::env::set_var("OPENAI_API_KEY", &tokens.access_token); }
            unsafe {
                std::env::set_var(
                    "OPENAI_API_URL",
                    "https://chatgpt.com/backend-api/codex/responses",
                );
                std::env::set_var(
                    "OPENAI_CHAT_API_URL",
                    "https://chatgpt.com/backend-api/chat/completions",
                );
            }

            if let Some(account) = &self.openai_chatgpt_account_id {
                unsafe { std::env::set_var("CHATGPT_ACCOUNT_ID", account); }
            }
        }

        if std::env::var("OPENAI_ORGANIZATION").is_err() {
            if let Some(org) = &self.openai_organization_id {
                unsafe { std::env::set_var("OPENAI_ORGANIZATION", org); }
            }
        }

        if std::env::var("OPENAI_PROJECT").is_err() {
            if let Some(project) = &self.openai_project_id {
                unsafe { std::env::set_var("OPENAI_PROJECT", project); }
            }
        }

        if let Some(key) = &self.glm_api_key {
            if std::env::var("GLM_API_KEY").is_err() {
                unsafe { std::env::set_var("GLM_API_KEY", key); }
            }
        }
    }

    pub fn clear_api_keys(&mut self) -> Result<bool> {
        let mut removed = false;

        if self.anthropic_api_key.take().is_some() {
            removed = true;
        }
        if self.openai_api_key.take().is_some() {
            removed = true;
        }
        if self.openai_oauth_tokens.take().is_some() {
            removed = true;
        }
        if self.openai_project_id.take().is_some() {
            removed = true;
        }
        if self.openai_organization_id.take().is_some() {
            removed = true;
        }
        if self.openai_chatgpt_account_id.take().is_some() {
            removed = true;
        }
        if self.glm_api_key.take().is_some() {
            removed = true;
        }

        self.save()?;

        Ok(removed)
    }
}
