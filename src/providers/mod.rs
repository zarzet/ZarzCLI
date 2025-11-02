use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

use crate::cli::Provider;

mod anthropic;
mod openai;
mod glm;

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model: String,
    pub system_prompt: Option<String>,
    pub user_prompt: String,
    pub max_output_tokens: u32,
    pub temperature: f32,
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub text: String,
}

#[allow(dead_code)]
pub type StreamChunk = Result<String>;
#[allow(dead_code)]
pub type CompletionStream = Pin<Box<dyn Stream<Item = StreamChunk> + Send>>;

#[async_trait]
pub trait CompletionProvider: Send + Sync {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse>;
    #[allow(dead_code)]
    async fn complete_stream(&self, request: &CompletionRequest) -> Result<CompletionStream>;
}

pub enum ProviderClient {
    Anthropic(anthropic::AnthropicClient),
    OpenAi(openai::OpenAiClient),
    Glm(glm::GlmClient),
}

impl ProviderClient {
    pub fn new(
        provider: Provider,
        api_key: Option<String>,
        endpoint_override: Option<String>,
        timeout_override: Option<u64>,
    ) -> Result<Self> {
        match provider {
            Provider::Anthropic => Ok(Self::Anthropic(
                anthropic::AnthropicClient::from_env(api_key, endpoint_override, timeout_override)?,
            )),
            Provider::OpenAi => Ok(Self::OpenAi(
                openai::OpenAiClient::from_env(api_key, endpoint_override, timeout_override)?,
            )),
            Provider::Glm => Ok(Self::Glm(
                glm::GlmClient::from_env(api_key, endpoint_override, timeout_override)?,
            )),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ProviderClient::Anthropic(_) => "anthropic",
            ProviderClient::OpenAi(_) => "openai",
            ProviderClient::Glm(_) => "glm",
        }
    }
}

#[async_trait]
impl CompletionProvider for ProviderClient {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        match self {
            ProviderClient::Anthropic(client) => client.complete(request).await,
            ProviderClient::OpenAi(client) => client.complete(request).await,
            ProviderClient::Glm(client) => client.complete(request).await,
        }
    }

    async fn complete_stream(&self, request: &CompletionRequest) -> Result<CompletionStream> {
        match self {
            ProviderClient::Anthropic(client) => client.complete_stream(request).await,
            ProviderClient::OpenAi(client) => client.complete_stream(request).await,
            ProviderClient::Glm(client) => client.complete_stream(request).await,
        }
    }
}
