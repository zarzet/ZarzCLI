use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use futures::stream::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

use super::{CompletionRequest, CompletionResponse, CompletionStream};

const DEFAULT_ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const DEFAULT_VERSION: &str = "2023-06-01";

pub struct AnthropicClient {
    http: Client,
    endpoint: String,
    api_key: String,
    version: String,
}

impl AnthropicClient {
    pub fn from_env(
        api_key_override: Option<String>,
        endpoint_override: Option<String>,
        timeout_override: Option<u64>,
    ) -> Result<Self> {
        let api_key = api_key_override
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .ok_or_else(|| anyhow::anyhow!("ANTHROPIC_API_KEY is required. Please set it in ~/.zarz/config.toml or as an environment variable"))?;
        let endpoint = endpoint_override
            .or_else(|| std::env::var("ANTHROPIC_API_URL").ok())
            .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());
        let version = std::env::var("ANTHROPIC_API_VERSION")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_VERSION.to_string());

        let timeout_secs = timeout_override
            .or_else(|| {
                std::env::var("ANTHROPIC_TIMEOUT_SECS")
                    .ok()
                    .and_then(|raw| raw.parse::<u64>().ok())
            })
            .unwrap_or(120);

        let http = Client::builder()
            .user_agent("zarz-cli/0.1")
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .context("Failed to build HTTP client for Anthropic")?;

        Ok(Self {
            http,
            endpoint,
            api_key,
            version,
        })
    }

    pub async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut payload = serde_json::Map::new();
        payload.insert("model".to_string(), serde_json::Value::String(request.model.clone()));
        payload.insert(
            "max_tokens".to_string(),
            serde_json::Value::Number(serde_json::Number::from(request.max_output_tokens)),
        );
        payload.insert("temperature".to_string(), json!(request.temperature));
        if let Some(system_prompt) = &request.system_prompt {
            payload.insert(
                "system".to_string(),
                serde_json::Value::String(system_prompt.clone()),
            );
        }
        payload.insert(
            "messages".to_string(),
            json!([{
                "role": "user",
                "content": [{
                    "type": "text",
                    "text": request.user_prompt
                }]
            }]),
        );

        let response = self
            .http
            .post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.version)
            .json(&payload)
            .send()
            .await
            .context("Anthropic request failed")?;

        let response = response.error_for_status().context("Anthropic returned an error status")?;
        let parsed: AnthropicResponse = response
            .json()
            .await
            .context("Failed to decode Anthropic response")?;
        let text = parsed
            .content
            .into_iter()
            .find_map(|block| match block {
                AnthropicResponseBlock::Text { text, .. } => Some(text),
            })
            .ok_or_else(|| anyhow!("Anthropic response did not include text content"))?;
        Ok(CompletionResponse { text })
    }

    #[allow(dead_code)]
    pub async fn complete_stream(&self, request: &CompletionRequest) -> Result<CompletionStream> {
        let mut payload = serde_json::Map::new();
        payload.insert("model".to_string(), serde_json::Value::String(request.model.clone()));
        payload.insert(
            "max_tokens".to_string(),
            serde_json::Value::Number(serde_json::Number::from(request.max_output_tokens)),
        );
        payload.insert("temperature".to_string(), json!(request.temperature));
        payload.insert("stream".to_string(), json!(true));

        if let Some(system_prompt) = &request.system_prompt {
            payload.insert(
                "system".to_string(),
                serde_json::Value::String(system_prompt.clone()),
            );
        }
        payload.insert(
            "messages".to_string(),
            json!([{
                "role": "user",
                "content": [{
                    "type": "text",
                    "text": request.user_prompt
                }]
            }]),
        );

        let response = self
            .http
            .post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.version)
            .json(&payload)
            .send()
            .await
            .context("Anthropic streaming request failed")?;

        let response = response
            .error_for_status()
            .context("Anthropic returned an error status")?;

        let stream = response.bytes_stream();
        let text_stream = stream.map(|result| {
            let bytes = result?;
            parse_anthropic_sse_chunk(&bytes)
        });

        Ok(Box::pin(text_stream))
    }
}

#[allow(dead_code)]
fn parse_anthropic_sse_chunk(bytes: &Bytes) -> Result<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut result = String::new();

    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                break;
            }

            if let Ok(event) = serde_json::from_str::<StreamEvent>(data) {
                match event.event_type.as_str() {
                    "content_block_delta" => {
                        if let Some(delta) = event.delta {
                            if let Some(text) = delta.text {
                                result.push_str(&text);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(result)
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    delta: Option<StreamDelta>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct StreamDelta {
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicResponseBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicResponseBlock {
    #[serde(rename = "text")]
    Text { text: String },
}
