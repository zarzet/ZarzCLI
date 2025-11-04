use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use futures::stream::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

use super::{CompletionRequest, CompletionResponse, CompletionStream};

const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";

pub struct OpenAiClient {
    http: Client,
    endpoint: String,
    api_key: String,
}

impl OpenAiClient {
    pub fn from_env(
        api_key_override: Option<String>,
        endpoint_override: Option<String>,
        timeout_override: Option<u64>,
    ) -> Result<Self> {
        let api_key = api_key_override
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .ok_or_else(|| anyhow::anyhow!("OPENAI_API_KEY is required. Please set it in ~/.zarz/config.toml or as an environment variable"))?;
        let endpoint = endpoint_override
            .or_else(|| std::env::var("OPENAI_API_URL").ok())
            .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());

        let timeout_secs = timeout_override
            .or_else(|| {
                std::env::var("OPENAI_TIMEOUT_SECS")
                    .ok()
                    .and_then(|raw| raw.parse::<u64>().ok())
            })
            .unwrap_or(120);

        let http = Client::builder()
            .user_agent("zarz-cli/0.1")
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .context("Failed to build HTTP client for OpenAI")?;

        Ok(Self {
            http,
            endpoint,
            api_key,
        })
    }

    pub async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
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
            let openai_tools: Vec<_> = tools.iter().map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool["name"],
                        "description": tool["description"],
                        "parameters": tool["input_schema"]
                    }
                })
            }).collect();
            payload["tools"] = json!(openai_tools);
        }

        let response = self
            .http
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await
            .context("OpenAI request failed")?;

        let response = response.error_for_status().context("OpenAI returned an error status")?;

        let parsed: OpenAiResponse = response
            .json()
            .await
            .context("Failed to decode OpenAI response")?;

        let first_choice = parsed.choices.into_iter().next()
            .ok_or_else(|| anyhow!("OpenAI response did not include any choices"))?;

        let text = first_choice.message.content.unwrap_or_default();
        let mut tool_calls = Vec::new();

        if let Some(calls) = first_choice.message.tool_calls {
            for call in calls {
                tool_calls.push(super::ToolCall {
                    id: call.id,
                    name: call.function.name,
                    input: call.function.arguments,
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
            .post(&self.endpoint)
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
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiFunction,
}

#[derive(Debug, Deserialize)]
struct OpenAiFunction {
    name: String,
    arguments: serde_json::Value,
}
