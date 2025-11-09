use anyhow::{anyhow, Context, Result};
use crate::config::{Config, OAuthTokens as StoredOAuthTokens};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use rand::RngCore;
use rand::thread_rng;
use reqwest::Client;
use serde_json::{self, Value};
use sha2::Digest;
use std::io::{self, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tiny_http::{Response, Server, StatusCode};
use tokio::sync::oneshot;
use url::Url;

const AUTH_ISSUER: &str = "https://auth.openai.com";
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const ORIGINATOR: &str = "zarz_cli";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(180);
const SUCCESS_HTML: &str = include_str!("auth_success.html");

struct PkceCodes {
    verifier: String,
    challenge: String,
}

/// Result of ChatGPT OAuth login
pub struct ChatGptLoginResult {
    pub oauth_tokens: StoredOAuthTokens,
    pub api_key: Option<String>,
    pub project_id: Option<String>,
    pub organization_id: Option<String>,
    pub account_id: Option<String>,
}

pub async fn login_with_chatgpt() -> Result<ChatGptLoginResult> {
    println!("[INFO] Starting ChatGPT OAuth login...");
    let pkce = generate_pkce();
    let state = generate_state();
    let mut callback_server = CallbackServer::start(state.clone())
        .context("failed to start local callback server")?;

    let redirect_uri = format!("http://localhost:{}/auth/callback", callback_server.port());
    let authorize_url = build_authorize_url(&redirect_uri, &pkce, &state);

    if webbrowser::open(&authorize_url).is_err() {
        println!("Please open this URL in your browser to continue:\n{authorize_url}");
    } else {
        println!("A browser window was opened for authentication.");
        println!("If it did not open, copy this URL:\n{authorize_url}");
    }

    println!("Waiting for authorization (timeout: {}s)...", LOGIN_TIMEOUT.as_secs());
    let code = match tokio::time::timeout(LOGIN_TIMEOUT, callback_server.wait_for_code()).await {
        Ok(result) => result?,
        Err(_) => return Err(anyhow!("Timed out waiting for browser login.")),
    };

    let client = Client::new();
    let tokens = exchange_code_for_tokens(&client, &code, &redirect_uri, &pkce)
        .await
        .context("failed to exchange authorization code")?;

    // Try to obtain API key - this is optional (like Codex does with .ok())
    // If it fails, we'll use OAuth access_token directly
    let api_key = obtain_api_key(&client, &tokens.id_token).await.ok();

    println!("[INFO] ChatGPT OAuth login complete!");
    if api_key.is_some() {
        println!("   API key issued. Stored alongside OAuth credentials.");
    } else {
        println!("   Using OAuth access token directly (no API key issued).");
    }

    let organization_id = extract_organization_id_from_token(&tokens.id_token);
    let project_id = extract_project_id_from_token(&tokens.id_token);
    let account_id = extract_account_id_from_token(&tokens.id_token);

    // Create OAuth tokens struct
    let oauth_tokens = StoredOAuthTokens {
        access_token: tokens.access_token.clone(),
        refresh_token: tokens.refresh_token.clone(),
        id_token: tokens.id_token.clone(),
    };

    Ok(ChatGptLoginResult {
        oauth_tokens,
        api_key,
        project_id,
        organization_id,
        account_id,
    })
}

struct OAuthTokensResponse {
    id_token: String,
    access_token: String,
    refresh_token: String,
}

async fn exchange_code_for_tokens(
    client: &Client,
    code: &str,
    redirect_uri: &str,
    pkce: &PkceCodes,
) -> Result<OAuthTokensResponse> {
    #[derive(serde::Deserialize)]
    struct TokenResponse {
        id_token: String,
        access_token: String,
        refresh_token: String,
    }

    let params = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", CLIENT_ID),
        ("code_verifier", pkce.verifier.as_str()),
    ];

    let resp = client
        .post(format!("{AUTH_ISSUER}/oauth/token"))
        .form(&params)
        .send()
        .await
        .context("token endpoint request failed")?;

    let status = resp.status();
    let body = resp.bytes().await.context("failed to read token response")?;
    if !status.is_success() {
        let msg = String::from_utf8_lossy(&body);
        return Err(anyhow!(
            "token endpoint returned status {}: {}",
            status,
            msg
        ));
    }

    let parsed: TokenResponse =
        serde_json::from_slice(&body).context("failed to parse token response")?;
    Ok(OAuthTokensResponse {
        id_token: parsed.id_token,
        access_token: parsed.access_token,
        refresh_token: parsed.refresh_token,
    })
}

async fn obtain_api_key(client: &Client, id_token: &str) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct ExchangeResponse {
        access_token: String,
    }

    // Build params for token exchange
    let params = vec![
        (
            "grant_type",
            "urn:ietf:params:oauth:grant-type:token-exchange",
        ),
        ("client_id", CLIENT_ID),
        ("requested_token", "openai-api-key"),
        ("subject_token", id_token),
        ("subject_token_type", "urn:ietf:params:oauth:token-type:id_token"),
    ];

    let resp = client
        .post(format!("{AUTH_ISSUER}/oauth/token"))
        .form(&params)
        .send()
        .await
        .context("api key exchange request failed")?;

    let status = resp.status();
    let body = resp
        .bytes()
        .await
        .context("failed to read api key exchange response")?;
    if !status.is_success() {
        let msg = String::from_utf8_lossy(&body);
        return Err(anyhow!(
            "api key exchange failed with status {}: {}",
            status,
            msg
        ));
    }

    let parsed: ExchangeResponse =
        serde_json::from_slice(&body).context("failed to parse api key exchange response")?;
    Ok(parsed.access_token)
}

fn build_authorize_url(redirect_uri: &str, pkce: &PkceCodes, state: &str) -> String {
    let mut url =
        Url::parse(&format!("{AUTH_ISSUER}/oauth/authorize")).expect("valid authorize url");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair(
            "scope",
            "openid profile email offline_access"
        )
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("state", state)
        .append_pair("originator", ORIGINATOR);
    url.into()
}

fn generate_pkce() -> PkceCodes {
    let mut bytes = [0u8; 64];
    thread_rng().fill_bytes(&mut bytes);
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let digest = sha2::Sha256::digest(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(digest);
    PkceCodes { verifier, challenge }
}

fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

struct CallbackServer {
    port: u16,
    receiver: Option<oneshot::Receiver<Result<String, String>>>,
    finished: Arc<AtomicBool>,
}

impl CallbackServer {
    fn start(expected_state: String) -> Result<Self> {
        let server = bind_server().context("unable to bind local server")?;
        let actual_port = server
            .server_addr()
            .to_ip()
            .and_then(|addr| Some(addr.port()))
            .ok_or_else(|| anyhow!("failed to determine callback port"))?;

        let (tx, rx) = oneshot::channel();
        let finished = Arc::new(AtomicBool::new(false));
        let finished_clone = finished.clone();

        std::thread::spawn(move || run_server(server, expected_state, tx, finished_clone));

        Ok(Self {
            port: actual_port,
            receiver: Some(rx),
            finished,
        })
    }

    fn port(&self) -> u16 {
        self.port
    }

    async fn wait_for_code(&mut self) -> Result<String> {
        let receiver = self
            .receiver
            .take()
            .ok_or_else(|| anyhow!("login server receiver already used"))?;
        let result = receiver
            .await
            .map_err(|_| anyhow!("login server stopped before completing authorization"))?;
        let code = result.map_err(|msg| anyhow!(msg))?;
        Ok(code)
    }
}

impl Drop for CallbackServer {
    fn drop(&mut self) {
        if !self.finished.load(Ordering::SeqCst) {
            let _ = send_shutdown_request(self.port);
        }
    }
}

fn bind_server() -> io::Result<Server> {
    fn start(addr: &str) -> io::Result<Server> {
        Server::http(addr).map_err(|err| io::Error::other(err))
    }
    start("127.0.0.1:1455").or_else(|_| start("127.0.0.1:0"))
}

fn run_server(
    server: Server,
    expected_state: String,
    sender: oneshot::Sender<Result<String, String>>,
    finished: Arc<AtomicBool>,
) {
    let mut sender = Some(sender);
    while let Ok(request) = server.recv() {
        let path = request.url().to_string();
        if path == "/__shutdown" {
            let _ = request.respond(Response::empty(200));
            break;
        }

        if !path.starts_with("/auth/callback") {
            let _ = request
                .respond(Response::from_string("Not Found").with_status_code(StatusCode(404)));
            continue;
        }

        match parse_auth_callback(&path, &expected_state) {
            Ok(AuthCallbackResult::Code(code)) => {
                // Send HTML response with proper Content-Type header
                let html_response = Response::from_string(SUCCESS_HTML)
                    .with_status_code(StatusCode(200))
                    .with_header(
                        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
                            .unwrap_or_else(|_| tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..]).unwrap())
                    );
                let _ = request.respond(html_response);
                if let Some(tx) = sender.take() {
                    let _ = tx.send(Ok(code));
                }
                break;
            }
            Ok(AuthCallbackResult::Error(message)) => {
                let body = format!(
                    "<html><body><h2>Login failed</h2><p>{message}</p></body></html>"
                );
                let _ = request.respond(
                    Response::from_string(body).with_status_code(StatusCode(400)),
                );
                if let Some(tx) = sender.take() {
                    let _ = tx.send(Err(message));
                }
                break;
            }
            Ok(AuthCallbackResult::Pending) => {
                let body = "<html><body><h2>Waiting for authorization...</h2><p>You can close this tab once the OpenAI consent flow finishes.</p></body></html>";
                let _ = request.respond(Response::from_string(body).with_status_code(StatusCode(200)));
                continue;
            }
            Err(message) => {
                let body = format!(
                    "<html><body><h2>Login failed</h2><p>{message}</p></body></html>"
                );
                let _ = request.respond(
                    Response::from_string(body).with_status_code(StatusCode(400)),
                );
                if let Some(tx) = sender.take() {
                    let _ = tx.send(Err(message));
                }
                break;
            }
        }
    }
    if let Some(tx) = sender.take() {
        let _ = tx.send(Err("Login server exited before receiving authorization".to_string()));
    }
    finished.store(true, Ordering::SeqCst);
}

/// Extract organization_id from JWT token
/// JWT format: header.payload.signature
/// We decode the payload (base64url) and extract organizations array
fn decode_jwt_payload(jwt: &str) -> Option<Value> {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    let payload_b64 = parts[1];
    let decoded = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    serde_json::from_slice(&decoded).ok()
}

fn extract_claim_root(payload: &Value) -> Option<&Value> {
    payload.get("https://api.openai.com/auth")
}

fn extract_organization_id_from_token(jwt: &str) -> Option<String> {
    let payload = decode_jwt_payload(jwt)?;
    extract_claim_root(&payload)
        .and_then(|auth| auth.get("organizations"))
        .and_then(|orgs| orgs.as_array())
        .and_then(|arr| arr.first())
        .and_then(|org| org.get("id"))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string())
}

fn extract_project_id_from_token(jwt: &str) -> Option<String> {
    let payload = decode_jwt_payload(jwt)?;
    extract_claim_root(&payload)
        .and_then(|auth| auth.get("project_id"))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string())
}

fn extract_account_id_from_token(jwt: &str) -> Option<String> {
    let payload = decode_jwt_payload(jwt)?;
    extract_claim_root(&payload)
        .and_then(|auth| auth.get("chatgpt_account_id"))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string())
}

fn extract_expiration_from_token(jwt: &str) -> Option<i64> {
    decode_jwt_payload(jwt)
        .and_then(|payload| payload.get("exp").and_then(|v| v.as_i64()))
}

struct RefreshedTokens {
    access_token: String,
    refresh_token: String,
    id_token: Option<String>,
}

async fn refresh_openai_access_token(refresh_token: &str) -> Result<RefreshedTokens> {
    #[derive(serde::Deserialize)]
    struct RefreshResponse {
        access_token: String,
        refresh_token: String,
        #[serde(default)]
        id_token: Option<String>,
    }

    let client = Client::new();
    let resp = client
        .post(format!("{AUTH_ISSUER}/oauth/token"))
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", CLIENT_ID),
        ])
        .send()
        .await
        .context("failed to refresh ChatGPT access token")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("token refresh failed ({}): {}", status, body);
    }

    let parsed: RefreshResponse = resp
        .json()
        .await
        .context("failed to parse token refresh response")?;

    Ok(RefreshedTokens {
        access_token: parsed.access_token,
        refresh_token: parsed.refresh_token,
        id_token: parsed.id_token,
    })
}

pub async fn ensure_openai_oauth_tokens_fresh(config: &mut Config) -> Result<bool> {
    let Some(tokens) = config.openai_oauth_tokens.clone() else {
        return Ok(false);
    };

    let now = Utc::now().timestamp();
    let refresh_threshold = 60; // seconds
    let should_refresh = match extract_expiration_from_token(&tokens.access_token) {
        Some(exp) => exp - now <= refresh_threshold,
        None => true,
    };

    if !should_refresh {
        return Ok(false);
    }

    let refreshed = refresh_openai_access_token(&tokens.refresh_token).await?;
    let mut updated_tokens = tokens;
    updated_tokens.access_token = refreshed.access_token;
    updated_tokens.refresh_token = refreshed.refresh_token;
    if let Some(id_token) = refreshed.id_token {
        updated_tokens.id_token = id_token;
    }

    config.openai_oauth_tokens = Some(updated_tokens);

    if let Some(tokens) = &config.openai_oauth_tokens {
        if let Some(account) = extract_account_id_from_token(&tokens.id_token) {
            config.openai_chatgpt_account_id = Some(account);
        }
        if let Some(project) = extract_project_id_from_token(&tokens.id_token) {
            config.openai_project_id = Some(project);
        }
        if let Some(org) = extract_organization_id_from_token(&tokens.id_token) {
            config.openai_organization_id = Some(org);
        }
    }

    Ok(true)
}

pub async fn prepare_openai_environment(config: &mut Config) -> Result<()> {
    if ensure_openai_oauth_tokens_fresh(config).await? {
        config.save()?;
    }
    config.apply_to_env();
    Ok(())
}

enum AuthCallbackResult {
    Code(String),
    Error(String),
    Pending,
}

fn parse_auth_callback(path: &str, expected_state: &str) -> Result<AuthCallbackResult, String> {
    let parsed = Url::parse(&format!("http://localhost{path}"))
        .map_err(|_| "Unable to parse redirect URL".to_string())?;
    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut error: Option<String> = None;
    let mut error_description: Option<String> = None;
    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            "error_description" => error_description = Some(value.into_owned()),
            _ => {}
        }
    }

    let received_state = state.ok_or_else(|| "Authorization response missing state parameter".to_string())?;
    if received_state != expected_state {
        return Err("State mismatch detected. Please retry the login process.".to_string());
    }

    if let Some(err) = error {
        let desc = error_description.unwrap_or_else(|| "Authorization failed.".to_string());
        return Ok(AuthCallbackResult::Error(format!(
            "{} ({})",
            desc, err
        )));
    }

    if let Some(code) = code {
        return Ok(AuthCallbackResult::Code(code));
    }

    Ok(AuthCallbackResult::Pending)
}

fn send_shutdown_request(port: u16) -> io::Result<()> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.write_all(b"GET /__shutdown HTTP/1.1\r\n")?;
    stream.write_all(format!("Host: 127.0.0.1:{port}\r\n\r\n").as_bytes())?;
    Ok(())
}
