use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use base64::prelude::*;
use reqwest::blocking::{multipart, Client};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;

pub const DEFAULT_ENDPOINT: &str = "https://chatgpt.com/backend-api/transcribe";
pub const DEFAULT_ORIGINATOR: &str = "Codex Desktop";
const DEFAULT_DESKTOP_VERSION: &str = "26.429.30905";

#[derive(Debug, Error)]
pub enum CodexAsrError {
    #[error("failed to read {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {source}")]
    ParseAuth {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("Codex auth at {path} does not contain a ChatGPT access token")]
    MissingAccessToken { path: PathBuf },
    #[error("Codex auth mode is {mode}, not ChatGPT token auth")]
    UnsupportedAuthMode { mode: String },
    #[error("invalid bearer token")]
    InvalidBearer,
    #[error("invalid header value: {0}")]
    InvalidHeader(#[from] reqwest::header::InvalidHeaderValue),
    #[error("failed to build HTTP client: {0}")]
    BuildClient(#[source] reqwest::Error),
    #[error("failed to build multipart request: {0}")]
    BuildMultipart(#[source] reqwest::Error),
    #[error("transcribe request failed: {0}")]
    Request(#[source] reqwest::Error),
    #[error("transcribe request failed with HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("transcribe response did not contain text")]
    MissingText,
}

pub type Result<T> = std::result::Result<T, CodexAsrError>;

#[derive(Debug, Clone)]
pub struct CodexAuth {
    pub access_token: String,
    pub account_id: Option<String>,
    pub path: Option<PathBuf>,
}

impl CodexAuth {
    pub fn from_bearer(token: impl AsRef<str>, account_id: Option<String>) -> Result<Self> {
        let access_token =
            strip_bearer_prefix(token.as_ref()).ok_or(CodexAsrError::InvalidBearer)?;
        let account_id = account_id.or_else(|| account_id_from_access_token(&access_token));
        Ok(Self {
            access_token,
            account_id,
            path: None,
        })
    }

    pub fn from_codex_home() -> Result<Self> {
        Self::from_auth_file(default_auth_file())
    }

    pub fn from_auth_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let raw = fs::read_to_string(&path).map_err(|source| CodexAsrError::ReadFile {
            path: path.clone(),
            source,
        })?;
        let parsed: AuthFile =
            serde_json::from_str(&raw).map_err(|source| CodexAsrError::ParseAuth {
                path: path.clone(),
                source,
            })?;
        let mode = parsed.auth_mode.or(parsed.auth_mode_camel);
        if let Some(mode) = mode {
            if mode != "chatgpt" && mode != "chatgpt_auth_tokens" {
                return Err(CodexAsrError::UnsupportedAuthMode { mode });
            }
        }
        let tokens = parsed.tokens;
        let token = tokens
            .as_ref()
            .and_then(|tokens| tokens.access_token.clone())
            .filter(|token| !token.trim().is_empty())
            .ok_or_else(|| CodexAsrError::MissingAccessToken { path: path.clone() })?;
        let account_id = tokens
            .and_then(|tokens| tokens.account_id)
            .or_else(|| account_id_from_access_token(&token));
        Ok(Self {
            access_token: token,
            account_id,
            path: Some(path),
        })
    }
}

#[derive(Debug, Clone)]
pub struct CodexAsrClient {
    endpoint: String,
    auth: CodexAuth,
    http: Client,
    originator: String,
    user_agent: String,
}

impl CodexAsrClient {
    pub fn builder(auth: CodexAuth) -> CodexAsrClientBuilder {
        CodexAsrClientBuilder::new(auth)
    }

    pub fn from_codex_home() -> Result<Self> {
        Self::builder(CodexAuth::from_codex_home()?).build()
    }

    pub fn transcribe_file(
        &self,
        path: impl AsRef<Path>,
        options: TranscribeOptions,
    ) -> Result<Transcription> {
        let path = path.as_ref();
        let audio = fs::read(path).map_err(|source| CodexAsrError::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;
        let content_type = options
            .content_type
            .unwrap_or_else(|| infer_content_type(path).to_string());
        let filename = options
            .filename
            .unwrap_or_else(|| upload_filename(path, &content_type));
        self.transcribe_bytes(audio, &filename, &content_type, options.language)
    }

    pub fn transcribe_bytes(
        &self,
        audio: Vec<u8>,
        filename: &str,
        content_type: &str,
        language: Option<String>,
    ) -> Result<Transcription> {
        let part = multipart::Part::bytes(audio)
            .file_name(filename.replace('"', ""))
            .mime_str(content_type)
            .map_err(CodexAsrError::BuildMultipart)?;
        let mut form = multipart::Form::new().part("file", part);
        if let Some(language) = language {
            form = form.text("language", language);
        }

        let response = self
            .http
            .post(&self.endpoint)
            .headers(self.auth_headers()?)
            .multipart(form)
            .send()
            .map_err(CodexAsrError::Request)?;
        let status = response.status();
        let body = response.text().map_err(CodexAsrError::Request)?;
        if !status.is_success() {
            return Err(CodexAsrError::Http {
                status: status.as_u16(),
                body: clip_response_body(&body),
            });
        }
        let parsed: TranscribeResponse =
            serde_json::from_str(&body).map_err(|source| CodexAsrError::ParseAuth {
                path: PathBuf::from("<transcribe response>"),
                source,
            })?;
        let text = parsed.text.ok_or(CodexAsrError::MissingText)?;
        Ok(Transcription { text })
    }

    fn auth_headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.auth.access_token))?,
        );
        headers.insert("originator", HeaderValue::from_str(&self.originator)?);
        headers.insert(USER_AGENT, HeaderValue::from_str(&self.user_agent)?);
        if let Some(account_id) = &self.auth.account_id {
            headers.insert("ChatGPT-Account-Id", HeaderValue::from_str(account_id)?);
        }
        Ok(headers)
    }
}

#[derive(Debug, Clone)]
pub struct CodexAsrClientBuilder {
    auth: CodexAuth,
    endpoint: String,
    proxy: Option<String>,
    timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
    originator: String,
    user_agent: String,
}

impl CodexAsrClientBuilder {
    pub fn new(auth: CodexAuth) -> Self {
        let version =
            detect_codex_desktop_version().unwrap_or_else(|| DEFAULT_DESKTOP_VERSION.to_string());
        Self {
            auth,
            endpoint: DEFAULT_ENDPOINT.to_string(),
            proxy: resolve_proxy(None),
            timeout: None,
            connect_timeout: None,
            originator: DEFAULT_ORIGINATOR.to_string(),
            user_agent: format!(
                "{DEFAULT_ORIGINATOR}/{version} ({}; {})",
                env::consts::OS,
                env::consts::ARCH
            ),
        }
    }

    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    pub fn proxy(mut self, proxy: Option<String>) -> Self {
        self.proxy = proxy;
        self
    }

    pub fn timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn connect_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.connect_timeout = timeout;
        self
    }

    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    pub fn build(self) -> Result<CodexAsrClient> {
        let mut builder = Client::builder();
        if let Some(proxy) = self.proxy {
            builder =
                builder.proxy(reqwest::Proxy::https(&proxy).map_err(CodexAsrError::BuildClient)?);
        }
        if let Some(timeout) = self.timeout {
            builder = builder.timeout(timeout);
        }
        if let Some(timeout) = self.connect_timeout {
            builder = builder.connect_timeout(timeout);
        }
        let http = builder.build().map_err(CodexAsrError::BuildClient)?;
        Ok(CodexAsrClient {
            endpoint: self.endpoint,
            auth: self.auth,
            http,
            originator: self.originator,
            user_agent: self.user_agent,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct TranscribeOptions {
    pub language: Option<String>,
    pub content_type: Option<String>,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcription {
    pub text: String,
}

#[derive(Debug, Deserialize)]
struct AuthFile {
    auth_mode: Option<String>,
    #[serde(rename = "authMode")]
    auth_mode_camel: Option<String>,
    tokens: Option<AuthTokens>,
}

#[derive(Debug, Deserialize)]
struct AuthTokens {
    access_token: Option<String>,
    account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TranscribeResponse {
    text: Option<String>,
}

pub fn infer_content_type(path: impl AsRef<Path>) -> &'static str {
    match path
        .as_ref()
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "wav" | "wave" => "audio/wav",
        "webm" => "audio/webm",
        "mp3" => "audio/mpeg",
        "m4a" | "mp4" => "audio/mp4",
        "ogg" | "oga" => "audio/ogg",
        "flac" => "audio/flac",
        _ => "application/octet-stream",
    }
}

fn upload_filename(path: &Path, content_type: &str) -> String {
    let original = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("codex")
        .to_string();
    if infer_content_type(path) != "application/octet-stream" {
        return original;
    }
    let Some(extension) = extension_for_content_type(content_type) else {
        return original;
    };
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty() && !stem.starts_with('.'))
        .unwrap_or("codex");
    format!("{stem}.{extension}")
}

fn extension_for_content_type(content_type: &str) -> Option<&'static str> {
    match content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "audio/wav" | "audio/x-wav" | "audio/wave" => Some("wav"),
        "audio/mpeg" | "audio/mp3" => Some("mp3"),
        "audio/mp4" | "audio/m4a" | "audio/x-m4a" => Some("m4a"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        "audio/ogg" => Some("ogg"),
        "audio/webm" => Some("webm"),
        _ => None,
    }
}

pub fn default_auth_file() -> PathBuf {
    let codex_home = env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".codex"));
    codex_home.join("auth.json")
}

pub fn resolve_proxy(explicit_proxy: Option<&str>) -> Option<String> {
    first_non_empty([
        explicit_proxy.map(str::to_string),
        env::var("CODEX_ASR_PROXY").ok(),
        env::var("CODEX_VOICE_PROXY").ok(),
        env::var("HTTPS_PROXY").ok(),
        env::var("https_proxy").ok(),
        env::var("ALL_PROXY").ok(),
        env::var("all_proxy").ok(),
        system_https_proxy(),
    ])
}

fn first_non_empty(values: impl IntoIterator<Item = Option<String>>) -> Option<String> {
    values
        .into_iter()
        .flatten()
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

fn system_https_proxy() -> Option<String> {
    if cfg!(target_os = "macos") {
        let output = Command::new("scutil").arg("--proxy").output().ok()?;
        if !output.status.success() {
            return None;
        }
        return parse_scutil_https_proxy(&String::from_utf8_lossy(&output.stdout));
    }
    None
}

fn parse_scutil_https_proxy(output: &str) -> Option<String> {
    let mut enabled = false;
    let mut host = None;
    let mut port = None;
    for line in output.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "HTTPSEnable" => enabled = value.trim() == "1",
            "HTTPSProxy" => host = Some(value.trim().to_string()),
            "HTTPSPort" => port = Some(value.trim().to_string()),
            _ => {}
        }
    }
    if enabled {
        Some(format!("http://{}:{}", host?, port?))
    } else {
        None
    }
}

fn strip_bearer_prefix(token: &str) -> Option<String> {
    let trimmed = token.trim();
    let token = trimmed
        .strip_prefix("Bearer ")
        .or_else(|| trimmed.strip_prefix("bearer "))
        .unwrap_or(trimmed)
        .trim();
    (!token.is_empty()).then(|| token.to_string())
}

fn account_id_from_access_token(access_token: &str) -> Option<String> {
    let payload = access_token.split('.').nth(1)?;
    let decoded = BASE64_URL_SAFE_NO_PAD.decode(payload).ok()?;
    let value: Value = serde_json::from_slice(&decoded).ok()?;
    value
        .get("https://api.openai.com/auth")?
        .get("chatgpt_account_id")?
        .as_str()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn detect_codex_desktop_version() -> Option<String> {
    let plist = fs::read_to_string("/Applications/Codex.app/Contents/Info.plist").ok()?;
    let marker = "<key>CFBundleShortVersionString</key>";
    let rest = plist.split_once(marker)?.1;
    let start = rest.find("<string>")? + "<string>".len();
    let end = rest[start..].find("</string>")?;
    Some(rest[start..start + end].to_string())
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn clip_response_body(body: &str) -> String {
    let mut clipped = body.split_whitespace().collect::<Vec<_>>().join(" ");
    clipped.truncate(300);
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_type_is_inferred_from_extension() {
        assert_eq!(infer_content_type("voice.wav"), "audio/wav");
        assert_eq!(infer_content_type("voice.webm"), "audio/webm");
        assert_eq!(
            infer_content_type("voice.unknown"),
            "application/octet-stream"
        );
    }

    #[test]
    fn upload_filename_uses_content_type_when_extension_is_unknown() {
        assert_eq!(
            upload_filename(Path::new("voice"), "audio/wav"),
            "voice.wav"
        );
        assert_eq!(
            upload_filename(Path::new("voice.bin"), "audio/webm"),
            "voice.webm"
        );
        assert_eq!(
            upload_filename(Path::new("voice.wav"), "audio/webm"),
            "voice.wav"
        );
        assert_eq!(
            upload_filename(Path::new("voice.bin"), "application/octet-stream"),
            "voice.bin"
        );
    }

    #[test]
    fn bearer_prefix_is_optional() {
        assert_eq!(
            strip_bearer_prefix("Bearer abc.def").as_deref(),
            Some("abc.def")
        );
        assert_eq!(strip_bearer_prefix("abc.def").as_deref(), Some("abc.def"));
        assert_eq!(strip_bearer_prefix("  ").as_deref(), None);
    }

    #[test]
    fn account_id_can_be_read_from_chatgpt_jwt_payload() {
        let payload = BASE64_URL_SAFE_NO_PAD
            .encode(r#"{"https://api.openai.com/auth":{"chatgpt_account_id":"acct_123"}}"#);
        let token = format!("header.{payload}.sig");
        assert_eq!(
            account_id_from_access_token(&token).as_deref(),
            Some("acct_123")
        );
    }

    #[test]
    fn macos_https_proxy_is_parsed() {
        let output = r#"
<dictionary> {
  HTTPSEnable : 1
  HTTPSProxy : 127.0.0.1
  HTTPSPort : 7892
}
"#;
        assert_eq!(
            parse_scutil_https_proxy(output).as_deref(),
            Some("http://127.0.0.1:7892")
        );
    }
}
