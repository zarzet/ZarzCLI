use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use futures::stream::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT};
use reqwest::{Client, StatusCode};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{CompletionRequest, CompletionResponse, CompletionStream, ReasoningEffort, ToolCall};

#[derive(Debug)]
enum ResponsesCallError {
    MissingScope(String),
    Other(anyhow::Error),
}

impl From<anyhow::Error> for ResponsesCallError {
    fn from(err: anyhow::Error) -> Self {
        Self::Other(err)
    }
}

fn generate_session_id() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect()
}

fn extract_sse_response(text: &str) -> Option<Value> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data: ") {
            if let Ok(json) = serde_json::from_str::<Value>(data) {
                if let Some(ty) = json.get("type").and_then(|v| v.as_str()) {
                    if (ty == "response.completed" || ty == "response.done")
                        && json.get("response").is_some()
                    {
                        return json.get("response").cloned();
                    }
                }
            }
        }
    }
    None
}

const DEFAULT_RESPONSES_ENDPOINT: &str = "https://api.openai.com/v1/responses";
const DEFAULT_CHAT_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";
const CHATGPT_RESPONSES_ENDPOINT: &str = "https://chatgpt.com/backend-api/codex/responses";
const CHATGPT_CHAT_ENDPOINT: &str = "https://chatgpt.com/backend-api/chat/completions";
const ORIGINATOR_HEADER: &str = "zarz_cli";
const CHATGPT_ORIGINATOR_HEADER: &str = "codex_cli_rs";
const CHATGPT_CODEX_INSTRUCTIONS: &str = include_str!("../prompts/codex_instructions.md");

pub struct OpenAiClient {
    http: Client,
    responses_endpoint: String,
    chat_endpoint: String,
    api_key: String,
    is_chatgpt_backend: bool,
    session_id: Option<String>,
}

impl OpenAiClient {
    pub fn from_env(
        api_key_override: Option<String>,
        endpoint_override: Option<String>,
        timeout_override: Option<u64>,
    ) -> Result<Self> {
        let api_key = api_key_override
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .ok_or_else(|| anyhow!("OPENAI_API_KEY is required. Please set it in ~/.zarz/config.toml or as an environment variable"))?;

        let mut responses_endpoint = endpoint_override
            .or_else(|| std::env::var("OPENAI_API_URL").ok())
            .unwrap_or_else(|| DEFAULT_RESPONSES_ENDPOINT.to_string());

        let mut chat_endpoint = std::env::var("OPENAI_CHAT_API_URL")
            .unwrap_or_else(|_| DEFAULT_CHAT_ENDPOINT.to_string());

        let chatgpt_account_id = std::env::var("CHATGPT_ACCOUNT_ID").ok();
        let is_chatgpt_backend = responses_endpoint.contains("chatgpt.com/backend-api/codex")
            || chatgpt_account_id.is_some();

        if is_chatgpt_backend {
            if !responses_endpoint.contains("chatgpt.com/backend-api/codex") {
                responses_endpoint = CHATGPT_RESPONSES_ENDPOINT.to_string();
            }
            if !chat_endpoint.contains("chatgpt.com/backend-api") {
                chat_endpoint = CHATGPT_CHAT_ENDPOINT.to_string();
            }
        }

        let timeout_secs = timeout_override
            .or_else(|| {
                std::env::var("OPENAI_TIMEOUT_SECS")
                    .ok()
                    .and_then(|raw| raw.parse::<u64>().ok())
            })
            .unwrap_or(120);

        let mut default_headers = HeaderMap::new();
        let originator = if is_chatgpt_backend {
            CHATGPT_ORIGINATOR_HEADER
        } else {
            ORIGINATOR_HEADER
        };
        default_headers.insert("originator", HeaderValue::from_static(originator));

        let session_id = if is_chatgpt_backend {
            if let Some(account_id) = chatgpt_account_id.as_deref() {
                if let Ok(value) = HeaderValue::from_str(account_id) {
                    default_headers.insert("chatgpt-account-id", value);
                }
            }

            default_headers.insert(
                "OpenAI-Beta",
                HeaderValue::from_static("responses=experimental"),
            );

            let sid = generate_session_id();
            if let Ok(value) = HeaderValue::from_str(&sid) {
                default_headers.insert("conversation_id", value.clone());
                default_headers.insert("session_id", value.clone());
            }
            default_headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
            Some(sid)
        } else {
            None
        };

        if let Ok(project) = std::env::var("OPENAI_PROJECT") {
            let project = project.trim();
            if !project.is_empty() {
                if let Ok(value) = HeaderValue::from_str(project) {
                    default_headers.insert("OpenAI-Project", value);
                }
            }
        }

        if let Ok(org) = std::env::var("OPENAI_ORGANIZATION") {
            let org = org.trim();
            if !org.is_empty() {
                if let Ok(value) = HeaderValue::from_str(org) {
                    default_headers.insert("OpenAI-Organization", value);
                }
            }
        }

        let http = Client::builder()
            .default_headers(default_headers)
            .user_agent("zarz-cli/0.1")
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .context("Failed to build HTTP client for OpenAI")?;

        Ok(Self {
            http,
            responses_endpoint,
            chat_endpoint,
            api_key,
            is_chatgpt_backend,
            session_id,
        })
    }

    pub async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if self.is_chatgpt_backend {
            return match self.complete_via_responses(request).await {
                Ok(result) => Ok(result),
                Err(ResponsesCallError::MissingScope(msg)) => Err(anyhow!(msg)),
                Err(ResponsesCallError::Other(err)) => Err(err),
            };
        }

        match self.complete_via_responses(request).await {
            Ok(result) => Ok(result),
            Err(ResponsesCallError::MissingScope(msg)) => {
                eprintln!(
                    "Warning: {} Falling back to Chat Completions.",
                    msg
                );
                self.complete_via_chat(request).await
            }
            Err(ResponsesCallError::Other(err)) => Err(err),
        }
    }

    async fn complete_via_responses(
        &self,
        request: &CompletionRequest,
    ) -> Result<CompletionResponse, ResponsesCallError> {
        let instructions = request.system_prompt.clone().unwrap_or_default();
        let mut input_items = build_responses_input(&request.messages, &request.user_prompt);
        let tools = build_responses_tools(request.tools.as_ref());

        let reasoning_effort = request
            .reasoning_effort
            .unwrap_or(ReasoningEffort::Medium);

        let mut payload = json!({
            "model": request.model,
            "instructions": instructions,
            "input": input_items,
            "tool_choice": "auto",
            "parallel_tool_calls": true,
            "store": false,
            "stream": false,
            "reasoning": {
                "summary": "auto",
                "effort": reasoning_effort.as_str(),
            },
            "include": ["reasoning.encrypted_content"],
            "text": { "verbosity": "medium" },
        });

        if !tools.is_empty() {
            payload["tools"] = json!(tools);
        }

        if self.is_chatgpt_backend {
            // Remove system/developer messages; instructions field handles system prompt.
            input_items = input_items
                .into_iter()
                .filter(|item| {
                    item.get("role")
                        .and_then(|v| v.as_str())
                        .map(|role| role != "system" && role != "developer")
                        .unwrap_or(true)
                })
                .collect();
            payload["instructions"] = json!(CHATGPT_CODEX_INSTRUCTIONS);
            if let Some(session) = &self.session_id {
                payload["prompt_cache_key"] = json!(session);
            }
            payload["stream"] = json!(true);
        } else {
            payload["temperature"] = json!(request.temperature);
        }

        payload["input"] = json!(input_items);

        let response = self
            .http
            .post(&self.responses_endpoint)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await
            .context("OpenAI Responses request failed")?;

        let status = response.status();

        if self.is_chatgpt_backend {
            let body_text = response
                .text()
                .await
                .context("Failed to read OpenAI Responses payload")?;

            if !status.is_success() {
                if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
                    && body_text.to_ascii_lowercase().contains("insufficient permissions")
                {
                    return Err(ResponsesCallError::MissingScope(format!(
                        "OpenAI Responses returned status {}: {}",
                        status,
                        body_text.trim()
                    )));
                }

                return Err(ResponsesCallError::Other(anyhow!(
                    "OpenAI Responses returned status {}: {}",
                    status,
                    body_text.trim()
                )));
            }

            let body = extract_sse_response(&body_text)
                .ok_or_else(|| anyhow!("Failed to decode OpenAI Responses payload"))?;
            return Ok(parse_responses_completion(body)?);
        } else {
            let body_bytes = response
                .bytes()
                .await
                .context("Failed to read OpenAI Responses payload")?;

            if !status.is_success() {
                let body_text = String::from_utf8_lossy(&body_bytes);
                if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN)
                    && body_text.to_ascii_lowercase().contains("insufficient permissions")
                {
                    return Err(ResponsesCallError::MissingScope(format!(
                        "OpenAI Responses returned status {}: {}",
                        status,
                        body_text.trim()
                    )));
                }

                return Err(ResponsesCallError::Other(anyhow!(
                    "OpenAI Responses returned status {}: {}",
                    status,
                    body_text.trim()
                )));
            }

            let body: Value = serde_json::from_slice(&body_bytes)
                .context("Failed to decode OpenAI Responses payload")?;

            return Ok(parse_responses_completion(body)?);
        }
    }

    #[allow(dead_code)]
    async fn complete_via_chat(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let messages = if let Some(msgs) = &request.messages {
            msgs.clone()
        } else {
            let mut messages = Vec::new();
            if let Some(system) = &request.system_prompt {
                messages.push(json!({
                    "role": "system",
                    "content": system,
                }));
            }
            messages.push(json!({
                "role": "user",
                "content": request.user_prompt,
            }));
            messages
        };

        let mut payload = json!({
            "model": request.model,
            "max_tokens": request.max_output_tokens,
            "temperature": request.temperature,
            "messages": messages,
        });

        if let Some(tools) = &request.tools {
            let openai_tools: Vec<_> = tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": tool["name"],
                            "description": tool["description"],
                            "parameters": tool["input_schema"]
                        }
                    })
                })
                .collect();
            payload["tools"] = json!(openai_tools);
        }

        let response = self
            .http
            .post(&self.chat_endpoint)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await
            .context("OpenAI Chat Completions request failed")?;

        let response = response
            .error_for_status()
            .context("OpenAI Chat Completions returned an error status")?;

        let parsed: OpenAiResponse = response
            .json()
            .await
            .context("Failed to decode OpenAI Chat Completions response")?;

        let first_choice = parsed
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("OpenAI response did not include any choices"))?;

        let text = first_choice.message.content.unwrap_or_default();
        let mut tool_calls = Vec::new();

        if let Some(calls) = first_choice.message.tool_calls {
            for call in calls {
                let OpenAiToolCall { id, function, .. } = call;
                let OpenAiFunction { name, arguments } = function;
                let normalized_arguments = normalize_tool_arguments(&arguments);

                tool_calls.push(ToolCall {
                    id,
                    name,
                    input: normalized_arguments,
                });
            }
        }

        Ok(CompletionResponse {
            text,
            tool_calls,
            stop_reason: first_choice.finish_reason,
        })
    }

    #[allow(dead_code)]
    pub async fn complete_stream(&self, request: &CompletionRequest) -> Result<CompletionStream> {
        self.complete_chat_stream(request).await
    }

    async fn complete_chat_stream(&self, request: &CompletionRequest) -> Result<CompletionStream> {
        let mut messages = Vec::new();
        if let Some(system) = &request.system_prompt {
            messages.push(json!({
                "role": "system",
                "content": system,
            }));
        }
        messages.push(json!({
            "role": "user",
            "content": request.user_prompt,
        }));

        let payload = json!({
            "model": request.model,
            "max_tokens": request.max_output_tokens,
            "temperature": request.temperature,
            "messages": messages,
            "stream": true,
        });

        let response = self
            .http
            .post(&self.chat_endpoint)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await
            .context("OpenAI streaming request failed")?;

        let response = response
            .error_for_status()
            .context("OpenAI returned an error status")?;

        let stream = response.bytes_stream();
        let text_stream = stream.map(|result| {
            let bytes = result?;
            parse_openai_sse_chunk(&bytes)
        });

        Ok(Box::pin(text_stream))
    }
}

fn build_responses_input(messages: &Option<Vec<Value>>, fallback_prompt: &str) -> Vec<Value> {
    if let Some(msgs) = messages {
        let mut converted = Vec::new();
        for msg in msgs {
            append_responses_items_from_chat_message(msg, &mut converted);
        }
        if !converted.is_empty() {
            return converted;
        }
    }

    vec![json!({
        "type": "message",
        "role": "user",
        "content": [{
            "type": "input_text",
            "text": fallback_prompt
        }]
    })]
}

fn append_responses_items_from_chat_message(message: &Value, items: &mut Vec<Value>) {
    let Some(role) = message.get("role").and_then(|v| v.as_str()) else {
        return;
    };

    if role == "tool" {
        let Some(call_id) = message.get("tool_call_id").and_then(|v| v.as_str()) else {
            return;
        };

        let output_value = match message.get("content") {
            Some(Value::String(text)) => Value::String(text.clone()),
            Some(other @ Value::Array(_)) => other.clone(),
            Some(other @ Value::Object(_)) => Value::String(other.to_string()),
            Some(other) => Value::String(other.to_string()),
            None => Value::String(String::new()),
        };

        items.push(json!({
            "type": "function_call_output",
            "call_id": call_id,
            "output": output_value
        }));
        return;
    }

    let content = message.get("content").unwrap_or(&Value::Null);

    let content_items = if content.is_array() {
        content.as_array().cloned().unwrap_or_default()
    } else if let Some(text) = content.as_str() {
        let kind = if role == "assistant" {
            "output_text"
        } else {
            "input_text"
        };
        vec![json!({ "type": kind, "text": text })]
    } else if content.is_object() {
        vec![content.clone()]
    } else {
        vec![json!({
            "type": "text",
            "text": content.to_string()
        })]
    };

    items.push(json!({
        "type": "message",
        "role": role,
        "content": content_items
    }));

    if role == "assistant" {
        if let Some(tool_calls) = message.get("tool_calls").and_then(|v| v.as_array()) {
            for call in tool_calls {
                let Some(call_id) = call.get("id").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(function) = call.get("function") else {
                    continue;
                };
                let Some(name) = function.get("name").and_then(|v| v.as_str()) else {
                    continue;
                };
                let arguments = function
                    .get("arguments")
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}");

                items.push(json!({
                    "type": "function_call",
                    "call_id": call_id,
                    "name": name,
                    "arguments": arguments
                }));
            }
        }
    }
}

fn build_responses_tools(tools: Option<&Vec<Value>>) -> Vec<Value> {
    tools
        .map(|items| {
            items
                .iter()
                .filter_map(|tool| {
                    let name = tool.get("name")?.as_str()?;
                    let description = tool.get("description").and_then(|v| v.as_str()).unwrap_or("");
                    let params = tool.get("input_schema")?.clone();
                    Some(json!({
                        "type": "function",
                        "name": name,
                        "description": description,
                        "strict": false,
                        "parameters": params
                    }))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_responses_completion(body: Value) -> Result<CompletionResponse> {
    let output_items = if let Some(arr) = body.get("output").and_then(|v| v.as_array()) {
        arr.clone()
    } else if let Some(arr) = body
        .get("response")
        .and_then(|r| r.get("output"))
        .and_then(|v| v.as_array())
    {
        arr.clone()
    } else {
        Vec::new()
    };

    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for item in output_items {
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match item_type {
            "message" => {
                if let Some(contents) = item.get("content").and_then(|v| v.as_array()) {
                    for entry in contents {
                        match entry.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                            "output_text" | "text" => {
                                if let Some(text) = entry.get("text").and_then(|v| v.as_str()) {
                                    text_parts.push(text.to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            "function_call" => {
                if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                    let call_id = item
                        .get("call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(name)
                        .to_string();
                    let args = item
                        .get("arguments")
                        .and_then(|v| v.as_str())
                        .unwrap_or("{}");
                    let input = normalize_tool_arguments(args);
                    tool_calls.push(ToolCall {
                        id: call_id,
                        name: name.to_string(),
                        input,
                    });
                }
            }
            "custom_tool_call" => {
                if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                    let call_id = item
                        .get("call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(name)
                        .to_string();
                    let raw = item
                        .get("input")
                        .and_then(|v| v.as_str())
                        .unwrap_or("{}");
                    let input = normalize_tool_arguments(raw);
                    tool_calls.push(ToolCall {
                        id: call_id,
                        name: name.to_string(),
                        input,
                    });
                }
            }
            _ => {}
        }
    }

    let text = text_parts.join("\n");
    Ok(CompletionResponse {
        text,
        tool_calls,
        stop_reason: None,
    })
}

fn normalize_tool_arguments(payload: &str) -> Value {
    serde_json::from_str(payload).unwrap_or_else(|_| Value::String(payload.to_string()))
}

#[allow(dead_code)]
fn parse_openai_sse_chunk(bytes: &Bytes) -> Result<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut result = String::new();

    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                break;
            }

            if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                if let Some(choice) = chunk.choices.first() {
                    if let Some(content) = &choice.delta.content {
                        result.push_str(content);
                    }
                }
            }
        }
    }

    Ok(result)
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    call_type: String,
    function: OpenAiFunction,
}

#[derive(Debug, Deserialize)]
struct OpenAiFunction {
    name: String,
    arguments: String,
}
