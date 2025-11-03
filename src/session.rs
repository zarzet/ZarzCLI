use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::intelligence::ProjectIntelligence;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool { server: String, tool: String },
}

#[derive(Debug)]
pub struct PendingChange {
    pub path: PathBuf,
    pub original_content: String,
    pub new_content: String,
}

#[derive(Debug)]
pub struct Session {
    pub conversation_history: Vec<Message>,
    pub current_files: HashMap<PathBuf, String>,
    pub pending_changes: Vec<PendingChange>,
    pub project_intelligence: ProjectIntelligence,
    pub working_directory: PathBuf,
    pub storage_id: Option<String>,
    pub title: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Session {
    pub fn new(working_directory: PathBuf) -> Self {
        let project_intelligence = ProjectIntelligence::new(working_directory.clone());

        Self {
            conversation_history: Vec::new(),
            current_files: HashMap::new(),
            pending_changes: Vec::new(),
            project_intelligence,
            working_directory,
            storage_id: None,
            title: None,
            created_at: None,
            updated_at: None,
        }
    }

    pub fn add_message(&mut self, role: MessageRole, content: String) {
        self.conversation_history.push(Message { role, content });
    }

    #[allow(dead_code)]
    pub fn add_pending_change(&mut self, path: PathBuf, original: String, new_content: String) {
        self.pending_changes.push(PendingChange {
            path,
            original_content: original,
            new_content,
        });
    }

    pub fn clear_pending_changes(&mut self) {
        self.pending_changes.clear();
    }

    pub fn reset_metadata(&mut self) {
        self.storage_id = None;
        self.title = None;
        self.created_at = None;
        self.updated_at = None;
    }

    pub fn load_file(&mut self, path: PathBuf, content: String) {
        self.current_files.insert(path, content);
    }

    #[allow(dead_code)]
    pub fn get_file(&self, path: &PathBuf) -> Option<&String> {
        self.current_files.get(path)
    }

    #[allow(dead_code)]
    pub fn get_conversation_context(&self, max_messages: usize) -> Vec<Message> {
        let start = if self.conversation_history.len() > max_messages {
            self.conversation_history.len() - max_messages
        } else {
            0
        };

        self.conversation_history[start..].to_vec()
    }

    pub fn build_prompt_with_context(&self, include_files: bool) -> String {
        let mut prompt = String::new();

        prompt.push_str("Conversation transcript (most recent last):\n\n");

        for message in &self.conversation_history {
            match &message.role {
                MessageRole::User => {
                    prompt.push_str("User: ");
                    prompt.push_str(&message.content);
                }
                MessageRole::Assistant => {
                    prompt.push_str("Assistant: ");
                    prompt.push_str(&message.content);
                }
                MessageRole::System => {
                    prompt.push_str("System: ");
                    prompt.push_str(&message.content);
                }
                MessageRole::Tool { server, tool } => {
                    prompt.push_str(&format!(
                        "Tool[{}.{tool}]: {}",
                        server,
                        truncate_for_prompt(&message.content, 4000)
                    ));
                }
            }

            prompt.push_str("\n\n");
        }

        if include_files && !self.current_files.is_empty() {
            prompt.push_str("## Current Files\n\n");

            for (path, content) in &self.current_files {
                prompt.push_str(&format!(
                    "<file path=\"{}\">\n{}\n</file>\n\n",
                    path.display(),
                    content
                ));
            }
        }

        prompt
    }

    pub fn get_relevant_context(&self, query: &str) -> Result<Vec<PathBuf>> {
        self.project_intelligence.get_relevant_context(query)
    }

    pub fn search_symbol(&self, name: &str) -> Result<Vec<crate::intelligence::Symbol>> {
        self.project_intelligence.find_symbol(name)
    }
}

fn truncate_for_prompt(text: &str, max_chars: usize) -> String {
    let mut result = String::new();
    let mut count = 0;

    for ch in text.chars() {
        if count >= max_chars {
            result.push_str("... (truncated)");
            return result;
        }
        result.push(ch);
        count += 1;
    }

    result
}
