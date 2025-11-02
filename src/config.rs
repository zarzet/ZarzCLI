use anyhow::{Context, Result};
use dialoguer::Input;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anthropic_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub glm_api_key: Option<String>,
}

impl Config {
    /// Get the path to the config file (~/.zarz/config.toml)
    pub fn config_path() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not determine home directory")?;
        Ok(home.join(".zarz").join("config.toml"))
    }

    /// Load config from file, or return default if file doesn't exist
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

    /// Save config to file
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

    /// Check if at least one API key is configured
    pub fn has_api_key(&self) -> bool {
        self.anthropic_api_key.is_some() || self.openai_api_key.is_some() || self.glm_api_key.is_some()
    }

    /// Interactive setup to get API keys from user
    pub fn interactive_setup() -> Result<Self> {
        println!("\n🔧 Welcome to ZarzCLI! Let's set up your API keys.\n");
        println!("You can configure one or more providers:");
        println!("  • Anthropic Claude (recommended for coding)");
        println!("  • OpenAI GPT");
        println!("  • GLM (Z.AI - International GLM-4.6)\n");

        let mut config = Self::default();

        // Ask for Anthropic API key
        let anthropic_key: String = Input::new()
            .with_prompt("Enter your Anthropic API key (or press Enter to skip)")
            .allow_empty(true)
            .interact_text()?;

        if !anthropic_key.trim().is_empty() {
            config.anthropic_api_key = Some(anthropic_key.trim().to_string());
        }

        // Ask for OpenAI API key
        let openai_key: String = Input::new()
            .with_prompt("Enter your OpenAI API key (or press Enter to skip)")
            .allow_empty(true)
            .interact_text()?;

        if !openai_key.trim().is_empty() {
            config.openai_api_key = Some(openai_key.trim().to_string());
        }

        // Ask for GLM API key
        let glm_key: String = Input::new()
            .with_prompt("Enter your GLM API key (or press Enter to skip)")
            .allow_empty(true)
            .interact_text()?;

        if !glm_key.trim().is_empty() {
            config.glm_api_key = Some(glm_key.trim().to_string());
        }

        if !config.has_api_key() {
            anyhow::bail!("At least one API key is required to use ZarzCLI");
        }

        config.save()?;
        println!("\n✅ Configuration saved to {}\n", Self::config_path()?.display());

        Ok(config)
    }

    /// Get Anthropic API key from config or environment
    pub fn get_anthropic_key(&self) -> Option<String> {
        std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .or_else(|| self.anthropic_api_key.clone())
    }

    /// Get OpenAI API key from config or environment
    pub fn get_openai_key(&self) -> Option<String> {
        std::env::var("OPENAI_API_KEY")
            .ok()
            .or_else(|| self.openai_api_key.clone())
    }

    /// Get GLM API key from config or environment
    pub fn get_glm_key(&self) -> Option<String> {
        std::env::var("GLM_API_KEY")
            .ok()
            .or_else(|| self.glm_api_key.clone())
    }

    /// Get default provider based on available API keys
    /// Priority: Anthropic > OpenAI > GLM
    pub fn get_default_provider(&self) -> Option<crate::cli::Provider> {
        if self.get_anthropic_key().is_some() {
            Some(crate::cli::Provider::Anthropic)
        } else if self.get_openai_key().is_some() {
            Some(crate::cli::Provider::OpenAi)
        } else if self.get_glm_key().is_some() {
            Some(crate::cli::Provider::Glm)
        } else {
            None
        }
    }

    /// Apply config to environment variables
    pub fn apply_to_env(&self) {
        if let Some(key) = &self.anthropic_api_key {
            if std::env::var("ANTHROPIC_API_KEY").is_err() {
                unsafe { std::env::set_var("ANTHROPIC_API_KEY", key); }
            }
        }
        if let Some(key) = &self.openai_api_key {
            if std::env::var("OPENAI_API_KEY").is_err() {
                unsafe { std::env::set_var("OPENAI_API_KEY", key); }
            }
        }
        if let Some(key) = &self.glm_api_key {
            if std::env::var("GLM_API_KEY").is_err() {
                unsafe { std::env::set_var("GLM_API_KEY", key); }
            }
        }
    }
}
