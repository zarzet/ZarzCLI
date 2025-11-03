use std::fs;
use std::path::{PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::cli::Provider;
use crate::session::{Message, MessageRole, Session};
use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSnapshot {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub provider: String,
    pub model: String,
    pub working_directory: PathBuf,
    pub message_count: usize,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub updated_at: DateTime<Utc>,
    pub provider: String,
    pub model: String,
    pub message_count: usize,
}

pub struct ConversationStore;

impl ConversationStore {
    fn storage_dir() -> Result<PathBuf> {
        let config_path = Config::config_path()?;
        let dir = config_path
            .parent()
            .map(|p| p.join("sessions"))
            .unwrap_or_else(|| PathBuf::from(".zarz/sessions"));
        fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create session storage at {}", dir.display()))?;
        Ok(dir)
    }

    fn generate_id() -> String {
        let now = Utc::now();
        let nanos = now.timestamp_subsec_nanos();
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{}-{:09}-{:x}", now.format("%Y%m%d-%H%M%S"), nanos, since_epoch)
    }

    fn derive_title(messages: &[Message]) -> String {
        const DEFAULT_TITLE: &str = "Untitled session";
        let candidate = messages
            .iter()
            .find_map(|msg| match msg.role {
                MessageRole::User => {
                    msg.content
                        .lines()
                        .find(|line| !line.trim().is_empty())
                        .map(|line| line.trim())
                        .map(str::to_string)
                }
                _ => None,
            })
            .unwrap_or_else(|| DEFAULT_TITLE.to_string());

        let trimmed = candidate.trim();
        let title = if trimmed.is_empty() {
            DEFAULT_TITLE.to_string()
        } else {
            trimmed.to_string()
        };

        if title.len() > 80 {
            format!("{}…", &title[..80])
        } else {
            title
        }
    }

    pub fn save_session(session: &mut Session, provider: Provider, model: &str) -> Result<()> {
        if session.conversation_history.is_empty() {
            return Ok(());
        }

        let now = Utc::now();
        let id = session
            .storage_id
            .clone()
            .unwrap_or_else(Self::generate_id);
        let created_at = session
            .created_at
            .unwrap_or_else(|| {
                let ts = now;
                session.created_at = Some(ts);
                ts
            });

        let title = session
            .title
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| Self::derive_title(&session.conversation_history));

        session.storage_id = Some(id.clone());
        session.title = Some(title.clone());
        session.updated_at = Some(now);

        let snapshot = ConversationSnapshot {
            id: id.clone(),
            title,
            created_at,
            updated_at: now,
            provider: provider.as_str().to_string(),
            model: model.to_string(),
            working_directory: session.working_directory.clone(),
            message_count: session.conversation_history.len(),
            messages: session.conversation_history.clone(),
        };

        let dir = Self::storage_dir()?;
        let path = dir.join(format!("{id}.json"));
        let data = serde_json::to_string_pretty(&snapshot)
            .context("Failed to serialize conversation snapshot")?;
        fs::write(&path, data)
            .with_context(|| format!("Failed to write conversation snapshot to {}", path.display()))?;

        Ok(())
    }

    pub fn list_summaries() -> Result<Vec<ConversationSummary>> {
        let dir = Self::storage_dir()?;
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut summaries = Vec::new();
        for entry in fs::read_dir(&dir).with_context(|| format!("Failed to read {}", dir.display()))? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let content = fs::read_to_string(entry.path());
            let Ok(content) = content else {
                continue;
            };
            let snapshot: Result<ConversationSnapshot, _> = serde_json::from_str(&content);
            let Ok(snapshot) = snapshot else {
                continue;
            };
            summaries.push(ConversationSummary {
                id: snapshot.id,
                title: snapshot.title,
                updated_at: snapshot.updated_at,
                provider: snapshot.provider,
                model: snapshot.model,
                message_count: snapshot.message_count,
            });
        }

        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(summaries)
    }

    pub fn load_snapshot(id: &str) -> Result<ConversationSnapshot> {
        let dir = Self::storage_dir()?;
        let path = dir.join(format!("{id}.json"));
        let data = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read session file {}", path.display()))?;
        let snapshot: ConversationSnapshot =
            serde_json::from_str(&data).context("Failed to parse stored session data")?;
        Ok(snapshot)
    }
}
