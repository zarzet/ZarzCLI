use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::intelligence::ProjectIntelligence;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    #[serde(default)]
    pub metadata: Option<MessageMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool { server: String, tool: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ToolMessageKind {
    Command,
    Output,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageMetadata {
    pub tool_call_id: Option<String>,
    pub tool_message_kind: Option<ToolMessageKind>,
    #[serde(default)]
    pub tool_arguments: Option<Value>,
}

impl MessageMetadata {
    pub fn for_tool_command(
        call_id: impl Into<String>,
        arguments: Option<Value>,
    ) -> Self {
        Self {
            tool_call_id: Some(call_id.into()),
            tool_message_kind: Some(ToolMessageKind::Command),
            tool_arguments: arguments,
        }
    }

    pub fn for_tool_output(call_id: impl Into<String>) -> Self {
        Self {
            tool_call_id: Some(call_id.into()),
            tool_message_kind: Some(ToolMessageKind::Output),
            tool_arguments: None,
        }
    }
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

    pub fn add_message_with_metadata(
        &mut self,
        role: MessageRole,
        content: String,
        metadata: Option<MessageMetadata>,
    ) {
        self.conversation_history.push(Message {
            role,
            content,
            metadata,
        });
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

        prompt.push_str(&format!(
            "Working Directory: {}\n\n",
            self.working_directory.display()
        ));

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

    pub fn normalize_tool_history(&mut self) {
        #[derive(Clone)]
        struct PendingToolState {
            server: String,
            tool: String,
            insert_after: usize,
            has_output: bool,
        }

        let mut pending: HashMap<String, PendingToolState> = HashMap::new();

        for (idx, message) in self.conversation_history.iter().enumerate() {
            let Some(metadata) = message.metadata.as_ref() else {
                continue;
            };
            let Some(call_id) = metadata.tool_call_id.as_ref() else {
                continue;
            };
            let Some(kind) = metadata.tool_message_kind.as_ref() else {
                continue;
            };

            match kind {
                ToolMessageKind::Command => {
                    if let MessageRole::Tool { server, tool } = &message.role {
                        pending.insert(
                            call_id.clone(),
                            PendingToolState {
                                server: server.clone(),
                                tool: tool.clone(),
                                insert_after: idx + 1,
                                has_output: false,
                            },
                        );
                    }
                }
                ToolMessageKind::Output => {
                    if let Some(state) = pending.get_mut(call_id) {
                        state.has_output = true;
                    }
                }
            }
        }

        let mut inserts: Vec<(usize, Message)> = Vec::new();

        for (call_id, state) in pending.into_iter() {
            if state.has_output {
                continue;
            }

            let message = Message {
                role: MessageRole::Tool {
                    server: state.server,
                    tool: state.tool,
                },
                content: "Output:\nERROR: Tool call ended without returning output.".to_string(),
                metadata: Some(MessageMetadata::for_tool_output(call_id)),
            };

            inserts.push((state.insert_after, message));
        }

        if inserts.is_empty() {
            return;
        }

        inserts.sort_by(|a, b| b.0.cmp(&a.0));

        for (pos, message) in inserts {
            let insert_pos = pos.min(self.conversation_history.len());
            self.conversation_history.insert(insert_pos, message);
        }
    }

    pub fn build_openai_messages(&self) -> Vec<Value> {
        let mut items = Vec::new();

        for message in &self.conversation_history {
            match &message.role {
                MessageRole::User => {
                    items.push(json!({
                        "role": "user",
                        "content": message.content
                    }));
                }
                MessageRole::Assistant => {
                    items.push(json!({
                        "role": "assistant",
                        "content": message.content
                    }));
                }
                MessageRole::System => {
                    items.push(json!({
                        "role": "system",
                        "content": message.content
                    }));
                }
                MessageRole::Tool { tool, .. } => {
                    let metadata = message.metadata.as_ref();
                    match metadata.and_then(|meta| meta.tool_message_kind.as_ref()) {
                        Some(ToolMessageKind::Command) => {
                            if let Some(call_id) =
                                metadata.and_then(|meta| meta.tool_call_id.as_deref())
                            {
                                let arguments = metadata
                                    .and_then(|meta| meta.tool_arguments.clone())
                                    .unwrap_or_else(|| json!({}));
                                items.push(json!({
                                    "role": "assistant",
                                    "content": message.content,
                                    "tool_calls": [{
                                        "id": call_id,
                                        "type": "function",
                                        "function": {
                                            "name": tool,
                                            "arguments": arguments.to_string()
                                        }
                                    }]
                                }));
                            } else {
                                items.push(json!({
                                    "role": "assistant",
                                    "content": message.content
                                }));
                            }
                        }
                        Some(ToolMessageKind::Output) => {
                            if let Some(call_id) =
                                metadata.and_then(|meta| meta.tool_call_id.as_deref())
                            {
                                items.push(json!({
                                    "role": "tool",
                                    "tool_call_id": call_id,
                                    "content": message.content
                                }));
                            } else {
                                items.push(json!({
                                    "role": "assistant",
                                    "content": message.content
                                }));
                            }
                        }
                        None => {
                            items.push(json!({
                                "role": "assistant",
                                "content": message.content
                            }));
                        }
                    }
                }
            }
        }

        items
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
