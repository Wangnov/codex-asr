use std::env;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::Duration;

#[cfg(feature = "server")]
use std::io::Write;
#[cfg(feature = "server")]
use std::net::SocketAddr;
#[cfg(feature = "server")]
use std::sync::Arc;

#[cfg(feature = "server")]
use axum::body::Body;
#[cfg(feature = "server")]
use axum::extract::{DefaultBodyLimit, Multipart, State};
#[cfg(feature = "server")]
use axum::http::{header, HeaderMap, StatusCode};
#[cfg(feature = "server")]
use axum::response::{IntoResponse, Response};
#[cfg(feature = "server")]
use axum::routing::{get, post};
#[cfg(feature = "server")]
use axum::{Json, Router};
use clap::{CommandFactory, Parser, Subcommand};
use codex_asr::{CodexAsrClient, CodexAuth, TranscribeOptions};
#[cfg(feature = "server")]
use serde::Serialize;
use tempfile::NamedTempFile;
#[cfg(feature = "server")]
use tokio::net::TcpListener;
#[cfg(feature = "server")]
use tokio::sync::Semaphore;

type AnyError = Box<dyn std::error::Error + Send + Sync + 'static>;
const DEFAULT_REQUEST_TIMEOUT_SECONDS: u64 = 300;
const DEFAULT_CONNECT_TIMEOUT_SECONDS: u64 = 15;
#[cfg(feature = "server")]
const DEFAULT_MAX_UPLOAD_MB: u64 = 50;

#[derive(Debug, Parser)]
#[command(version, about = "Unofficial Codex Desktop ASR client")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Audio file to transcribe. If no subcommand is used, this is treated as `transcribe <audio>`.
    audio: Option<PathBuf>,

    /// Bearer token. Defaults to CODEX_ASR_BEARER or ~/.codex/auth.json.
    #[arg(long, global = true)]
    bearer: Option<String>,

    /// ChatGPT account id override. Usually decoded from the bearer token.
    #[arg(long, global = true)]
    account_id: Option<String>,

    /// Codex auth file. Defaults to $CODEX_HOME/auth.json or ~/.codex/auth.json.
    #[arg(long, global = true)]
    auth_file: Option<PathBuf>,

    /// Transcribe endpoint.
    #[arg(long, global = true, default_value = codex_asr::DEFAULT_ENDPOINT)]
    endpoint: String,

    /// HTTPS proxy URL. Defaults to CODEX_ASR_PROXY, HTTPS_PROXY, ALL_PROXY, or macOS system proxy.
    #[arg(long, global = true)]
    proxy: Option<String>,

    /// Total upstream request timeout in seconds. Defaults to CODEX_ASR_REQUEST_TIMEOUT_SECONDS or 300.
    #[arg(long, global = true, value_name = "SECONDS")]
    request_timeout_seconds: Option<u64>,

    /// Upstream connect timeout in seconds. Defaults to CODEX_ASR_CONNECT_TIMEOUT_SECONDS or 15.
    #[arg(long, global = true, value_name = "SECONDS")]
    connect_timeout_seconds: Option<u64>,

    /// BCP-47-ish language hint, for example zh or en.
    #[arg(long, global = true)]
    language: Option<String>,

    /// Audio content type. Inferred from filename when omitted.
    #[arg(long, global = true)]
    content_type: Option<String>,

    /// Multipart filename override. Useful when the input path has no audio extension.
    #[arg(long, global = true)]
    filename: Option<String>,

    /// Path to an external rust-silk CLI used to decode .silk before upload.
    #[arg(long, global = true)]
    silk_decoder: Option<PathBuf>,

    /// Sample rate used when decoding .silk to temporary WAV.
    #[arg(long, global = true, default_value_t = 24_000)]
    silk_sample_rate: u32,

    /// Upload .silk as-is instead of decoding it with rust-silk.
    #[arg(long, global = true)]
    no_silk_decode: bool,

    /// Print {"text": "..."} instead of plain text.
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Transcribe an audio file.
    Transcribe {
        /// Audio file to transcribe.
        audio: PathBuf,
    },
    /// Serve an OpenAI Whisper-compatible REST endpoint.
    Serve {
        /// Bind host.
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Bind port.
        #[arg(long, default_value_t = 8788)]
        port: u16,

        /// Local REST API key. Defaults to CODEX_ASR_SERVER_KEY.
        #[arg(long)]
        api_key: Option<String>,

        /// Allow unauthenticated REST requests. Only use on trusted loopback.
        #[arg(long)]
        no_api_key: bool,

        /// Maximum concurrent upstream transcribe requests.
        #[arg(long, default_value_t = 16)]
        concurrency: usize,

        /// Maximum accepted REST upload size in MiB. Defaults to CODEX_ASR_MAX_UPLOAD_MB or 50.
        #[arg(long, value_name = "MIB")]
        max_upload_mb: Option<u64>,
    },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), AnyError> {
    let cli = Cli::parse();
    if matches!(&cli.command, Some(Command::Serve { .. })) {
        return run_serve(&cli);
    }
    run_transcribe(cli)
}

fn run_transcribe(cli: Cli) -> Result<(), AnyError> {
    let audio = match (&cli.command, cli.audio.as_ref()) {
        (Some(Command::Transcribe { audio }), _) => audio.clone(),
        (Some(Command::Serve { .. }), _) => unreachable!("serve is handled before transcribe"),
        (None, Some(audio)) => audio.clone(),
        (None, None) => {
            Cli::command().print_help()?;
            eprintln!();
            std::process::exit(2);
        }
    };

    let auth = load_auth(&cli)?;
    let client = CodexAsrClient::builder(auth)
        .endpoint(cli.endpoint.clone())
        .proxy(cli.proxy.clone().or_else(|| codex_asr::resolve_proxy(None)))
        .timeout(Some(resolve_request_timeout(&cli)?))
        .connect_timeout(Some(resolve_connect_timeout(&cli)?))
        .build()?;
    let silk = SilkDecodeConfig::from_cli(&cli);
    let decoded_silk = maybe_decode_silk(&audio, &silk)?;
    let upload_audio = decoded_silk
        .as_ref()
        .map(|file| file.path())
        .unwrap_or(audio.as_path());
    let decoded_filename = decoded_silk.as_ref().map(|_| decoded_wav_filename(&audio));
    let filename = cli.filename.or(decoded_filename);
    let content_type = if decoded_silk.is_some() {
        Some("audio/wav".to_string())
    } else {
        cli.content_type
    };
    let transcription = client.transcribe_file(
        upload_audio,
        TranscribeOptions {
            language: cli.language,
            content_type,
            filename,
        },
    )?;

    if cli.json {
        println!("{}", serde_json::json!({ "text": transcription.text }));
    } else {
        println!("{}", transcription.text);
    }
    Ok(())
}

fn load_auth(cli: &Cli) -> codex_asr::Result<CodexAuth> {
    if let Some(bearer) = cli
        .bearer
        .clone()
        .or_else(|| std::env::var("CODEX_ASR_BEARER").ok())
    {
        return CodexAuth::from_bearer(bearer, cli.account_id.clone());
    }
    if let Some(path) = &cli.auth_file {
        return CodexAuth::from_auth_file(path);
    }
    CodexAuth::from_codex_home()
}

#[cfg(feature = "server")]
#[derive(Debug, Clone)]
enum AuthSource {
    Bearer {
        token: String,
        account_id: Option<String>,
    },
    File(PathBuf),
}

#[cfg(feature = "server")]
impl AuthSource {
    fn from_cli(cli: &Cli) -> codex_asr::Result<Self> {
        if let Some(bearer) = cli
            .bearer
            .clone()
            .or_else(|| std::env::var("CODEX_ASR_BEARER").ok())
        {
            let source = Self::Bearer {
                token: bearer,
                account_id: cli.account_id.clone(),
            };
            source.load()?;
            return Ok(source);
        }
        let path = cli
            .auth_file
            .clone()
            .unwrap_or_else(codex_asr::default_auth_file);
        let source = Self::File(path);
        source.load()?;
        Ok(source)
    }

    fn load(&self) -> codex_asr::Result<CodexAuth> {
        match self {
            Self::Bearer { token, account_id } => CodexAuth::from_bearer(token, account_id.clone()),
            Self::File(path) => CodexAuth::from_auth_file(path),
        }
    }
}

fn resolve_request_timeout(cli: &Cli) -> Result<Duration, AnyError> {
    resolve_seconds(
        cli.request_timeout_seconds,
        &[
            "CODEX_ASR_REQUEST_TIMEOUT_SECONDS",
            "CODEX_ASR_TIMEOUT_SECONDS",
        ],
        DEFAULT_REQUEST_TIMEOUT_SECONDS,
        "request timeout",
    )
}

fn resolve_connect_timeout(cli: &Cli) -> Result<Duration, AnyError> {
    resolve_seconds(
        cli.connect_timeout_seconds,
        &["CODEX_ASR_CONNECT_TIMEOUT_SECONDS"],
        DEFAULT_CONNECT_TIMEOUT_SECONDS,
        "connect timeout",
    )
}

fn resolve_seconds(
    explicit: Option<u64>,
    env_names: &[&str],
    default: u64,
    label: &str,
) -> Result<Duration, AnyError> {
    let seconds = if let Some(value) = explicit {
        value
    } else {
        resolve_u64_env(env_names)?.unwrap_or(default)
    };
    if seconds == 0 {
        return Err(format!("{label} must be greater than 0 seconds").into());
    }
    Ok(Duration::from_secs(seconds))
}

fn resolve_u64_env(env_names: &[&str]) -> Result<Option<u64>, AnyError> {
    for name in env_names {
        let Ok(value) = env::var(name) else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let parsed = value
            .parse::<u64>()
            .map_err(|_| format!("{name} must be a positive integer"))?;
        return Ok(Some(parsed));
    }
    Ok(None)
}

#[derive(Debug, Clone)]
struct SilkDecodeConfig {
    decoder: Option<PathBuf>,
    sample_rate: u32,
    no_decode: bool,
}

impl SilkDecodeConfig {
    fn from_cli(cli: &Cli) -> Self {
        Self {
            decoder: cli.silk_decoder.clone(),
            sample_rate: cli.silk_sample_rate,
            no_decode: cli.no_silk_decode,
        }
    }
}

fn maybe_decode_silk(
    audio: &Path,
    config: &SilkDecodeConfig,
) -> Result<Option<NamedTempFile>, AnyError> {
    if config.no_decode || !is_silk_path(audio) {
        return Ok(None);
    }
    let decoder = resolve_silk_decoder(config.decoder.as_deref())?;
    let output = tempfile::Builder::new()
        .prefix("codex-asr-silk-")
        .suffix(".wav")
        .tempfile()?;
    let status = ProcessCommand::new(&decoder)
        .arg("decode")
        .arg("-i")
        .arg(audio)
        .arg("-o")
        .arg(output.path())
        .arg("--sample-rate")
        .arg(config.sample_rate.to_string())
        .arg("--quiet")
        .output()?;
    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        let stdout = String::from_utf8_lossy(&status.stdout);
        let detail = [stderr.trim(), stdout.trim()]
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        return Err(format!(
            "rust-silk decode failed with status {}{}",
            status.status,
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        )
        .into());
    }
    let len = output.as_file().metadata()?.len();
    if len <= 44 {
        return Err("rust-silk decode produced an empty WAV".into());
    }
    Ok(Some(output))
}

fn is_silk_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "silk" | "slk"))
        .unwrap_or(false)
}

fn decoded_wav_filename(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty() && !stem.starts_with('.'))
        .unwrap_or("voice");
    format!("{stem}.wav")
}

fn resolve_silk_decoder(explicit: Option<&Path>) -> Result<PathBuf, AnyError> {
    if let Some(path) = explicit {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        return Err(format!("rust-silk decoder not found at {}", path.display()).into());
    }
    if let Ok(path) = env::var("CODEX_ASR_SILK_DECODER") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
        return Err(format!(
            "CODEX_ASR_SILK_DECODER points to missing file {}",
            path.display()
        )
        .into());
    }
    if let Some(path) = find_on_path("rust-silk") {
        return Ok(path);
    }
    for path in fallback_silk_decoder_paths() {
        if path.exists() {
            return Ok(path);
        }
    }
    Err("input is .silk, but rust-silk was not found; pass --silk-decoder or set CODEX_ASR_SILK_DECODER".into())
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|dir| dir.join(binary))
        .find(|path| path.exists())
}

fn fallback_silk_decoder_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        for suffix in [
            ["rust-silk", "target", "release", "rust-silk"],
            ["rust-silk", "target", "debug", "rust-silk"],
        ] {
            let mut path = home.clone();
            for part in suffix {
                path.push(part);
            }
            paths.push(path);
        }
    }
    paths
}

#[cfg(feature = "server")]
fn run_serve(cli: &Cli) -> Result<(), AnyError> {
    let Some(Command::Serve {
        host,
        port,
        api_key,
        no_api_key,
        concurrency,
        max_upload_mb,
    }) = &cli.command
    else {
        unreachable!("run_serve only handles the serve subcommand");
    };
    if *concurrency == 0 {
        return Err("--concurrency must be greater than 0".into());
    }
    let api_key = resolve_server_api_key(api_key.clone(), *no_api_key)?;
    let auth_source = AuthSource::from_cli(cli)?;
    let request_timeout = resolve_request_timeout(cli)?;
    let connect_timeout = resolve_connect_timeout(cli)?;
    let max_upload_bytes = resolve_max_upload_bytes(*max_upload_mb)?;
    let state = ServerState {
        api_key: api_key.map(Arc::from),
        semaphore: Arc::new(Semaphore::new(*concurrency)),
        auth_source,
        endpoint: Arc::from(cli.endpoint.as_str()),
        proxy: cli.proxy.clone().or_else(|| codex_asr::resolve_proxy(None)),
        request_timeout,
        connect_timeout,
        silk: SilkDecodeConfig::from_cli(cli),
        default_language: cli.language.clone(),
    };
    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(serve_http(addr, state, max_upload_bytes))
}

#[cfg(not(feature = "server"))]
fn run_serve(_cli: &Cli) -> Result<(), AnyError> {
    Err("serve is unavailable because codex-asr was built without the `server` feature".into())
}

#[cfg(feature = "server")]
fn resolve_server_api_key(
    explicit: Option<String>,
    no_api_key: bool,
) -> Result<Option<String>, AnyError> {
    if no_api_key {
        return Ok(None);
    }
    let api_key = explicit
        .or_else(|| env::var("CODEX_ASR_SERVER_KEY").ok())
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty());
    api_key
        .map(Some)
        .ok_or_else(|| "serve requires --api-key, CODEX_ASR_SERVER_KEY, or --no-api-key".into())
}

#[cfg(feature = "server")]
fn resolve_max_upload_bytes(explicit_mb: Option<u64>) -> Result<usize, AnyError> {
    let mb = explicit_mb
        .or(resolve_u64_env(&["CODEX_ASR_MAX_UPLOAD_MB"])?)
        .unwrap_or(DEFAULT_MAX_UPLOAD_MB);
    if mb == 0 {
        return Err("max upload size must be greater than 0 MiB".into());
    }
    let bytes = mb
        .checked_mul(1024)
        .and_then(|value| value.checked_mul(1024))
        .ok_or("max upload size is too large")?;
    usize::try_from(bytes).map_err(|_| "max upload size is too large".into())
}

#[cfg(feature = "server")]
async fn serve_http(
    addr: SocketAddr,
    state: ServerState,
    max_upload_bytes: usize,
) -> Result<(), AnyError> {
    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/audio/transcriptions", post(transcriptions))
        .route("/audio/transcriptions", post(transcriptions))
        .layer(DefaultBodyLimit::max(max_upload_bytes))
        .with_state(state);
    let listener = TcpListener::bind(addr).await?;
    let actual = listener.local_addr()?;
    eprintln!(
        "codex-asr listening on http://{actual} (max upload: {} MiB)",
        max_upload_bytes / 1024 / 1024
    );
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(feature = "server")]
#[derive(Clone)]
struct ServerState {
    api_key: Option<Arc<str>>,
    semaphore: Arc<Semaphore>,
    auth_source: AuthSource,
    endpoint: Arc<str>,
    proxy: Option<String>,
    request_timeout: Duration,
    connect_timeout: Duration,
    silk: SilkDecodeConfig,
    default_language: Option<String>,
}

#[cfg(feature = "server")]
async fn healthz() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

#[cfg(feature = "server")]
async fn transcriptions(
    State(state): State<ServerState>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Response, ApiError> {
    authorize(&headers, state.api_key.as_deref())?;
    let request = parse_transcription_multipart(multipart, state.default_language.clone()).await?;
    let response_format = ResponseFormat::parse(request.response_format.as_deref())?;
    let permit = state
        .semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| ApiError::internal("concurrency limiter is closed"))?;
    let worker_state = state.clone();
    let language = request.language.clone();
    let transcription = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        transcribe_uploaded_file(worker_state, request)
    })
    .await
    .map_err(|error| ApiError::internal(format!("transcribe worker failed: {error}")))??;
    Ok(format_transcription_response(
        &transcription.text,
        response_format,
        language.as_deref(),
    ))
}

#[cfg(feature = "server")]
fn authorize(headers: &HeaderMap, expected: Option<&str>) -> Result<(), ApiError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
        });
    if provided == Some(expected) {
        Ok(())
    } else {
        Err(ApiError::unauthorized("missing or invalid bearer token"))
    }
}

#[cfg(feature = "server")]
struct TranscriptionRequest {
    file: NamedTempFile,
    filename: String,
    content_type: String,
    language: Option<String>,
    response_format: Option<String>,
}

#[cfg(feature = "server")]
async fn parse_transcription_multipart(
    mut multipart: Multipart,
    default_language: Option<String>,
) -> Result<TranscriptionRequest, ApiError> {
    let mut file = None;
    let mut language = default_language;
    let mut response_format = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| multipart_error(error, "invalid multipart body"))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let raw_filename = sanitize_filename(field.file_name().unwrap_or("audio"));
                let raw_content_type = field
                    .content_type()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| codex_asr::infer_content_type(&raw_filename).to_string());
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|error| multipart_error(error, "failed to read file"))?;
                if bytes.is_empty() {
                    return Err(ApiError::bad_request("file is empty"));
                }
                let (filename, content_type) =
                    normalize_upload_metadata(&raw_filename, raw_content_type, &bytes);
                let mut temp = tempfile::Builder::new()
                    .prefix("codex-asr-upload-")
                    .suffix(&upload_suffix(&filename, &content_type))
                    .tempfile()
                    .map_err(|error| {
                        ApiError::internal(format!("failed to create temp file: {error}"))
                    })?;
                temp.write_all(&bytes).map_err(|error| {
                    ApiError::internal(format!("failed to write temp file: {error}"))
                })?;
                file = Some(TranscriptionUpload {
                    file: temp,
                    filename,
                    content_type,
                });
            }
            "language" => {
                language = Some(read_text_field(field).await?);
            }
            "response_format" => {
                response_format = Some(read_text_field(field).await?);
            }
            "model"
            | "prompt"
            | "temperature"
            | "timestamp_granularities[]"
            | "timestamp_granularities" => {
                let _ = field.bytes().await;
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }
    let Some(upload) = file else {
        return Err(ApiError::bad_request("missing multipart file field"));
    };
    Ok(TranscriptionRequest {
        file: upload.file,
        filename: upload.filename,
        content_type: upload.content_type,
        language,
        response_format,
    })
}

#[cfg(feature = "server")]
struct TranscriptionUpload {
    file: NamedTempFile,
    filename: String,
    content_type: String,
}

#[cfg(feature = "server")]
async fn read_text_field(field: axum::extract::multipart::Field<'_>) -> Result<String, ApiError> {
    field
        .text()
        .await
        .map(|text| text.trim().to_string())
        .map_err(|error| multipart_error(error, "failed to read text field"))
}

#[cfg(feature = "server")]
fn multipart_error(error: axum::extract::multipart::MultipartError, context: &str) -> ApiError {
    let display = error.to_string();
    let detail = format!("{error:?}");
    if detail.contains("LengthLimitError")
        || detail.contains("length limit")
        || (context.starts_with("failed to read")
            && display.contains("Error parsing `multipart/form-data` request"))
    {
        ApiError::payload_too_large("multipart body exceeds the configured upload limit")
    } else {
        ApiError::bad_request(format!("{context}: {error}"))
    }
}

#[cfg(feature = "server")]
fn transcribe_uploaded_file(
    state: ServerState,
    request: TranscriptionRequest,
) -> Result<codex_asr::Transcription, ApiError> {
    let uploaded_path = request.file.path().to_path_buf();
    let decoded_silk = maybe_decode_silk(&uploaded_path, &state.silk)
        .map_err(|error| ApiError::bad_request(format!("{error}")))?;
    let upload_audio = decoded_silk
        .as_ref()
        .map(|file| file.path())
        .unwrap_or(uploaded_path.as_path());
    let filename = decoded_silk
        .as_ref()
        .map(|_| decoded_wav_filename(Path::new(&request.filename)))
        .or(Some(request.filename));
    let content_type = if decoded_silk.is_some() {
        Some("audio/wav".to_string())
    } else {
        Some(request.content_type)
    };
    let auth = state
        .auth_source
        .load()
        .map_err(|error| ApiError::internal(format!("failed to load Codex auth: {error}")))?;
    let client = CodexAsrClient::builder(auth)
        .endpoint(state.endpoint.as_ref().to_string())
        .proxy(state.proxy)
        .timeout(Some(state.request_timeout))
        .connect_timeout(Some(state.connect_timeout))
        .build()
        .map_err(|error| ApiError::internal(format!("failed to build ASR client: {error}")))?;
    client
        .transcribe_file(
            upload_audio,
            TranscribeOptions {
                language: request.language,
                content_type,
                filename,
            },
        )
        .map_err(|error| ApiError::bad_gateway(format!("upstream transcribe failed: {error}")))
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponseFormat {
    Json,
    Text,
    VerboseJson,
}

#[cfg(feature = "server")]
impl ResponseFormat {
    fn parse(value: Option<&str>) -> Result<Self, ApiError> {
        match value.unwrap_or("json").trim() {
            "" | "json" => Ok(Self::Json),
            "text" => Ok(Self::Text),
            "verbose_json" => Ok(Self::VerboseJson),
            "srt" | "vtt" => Err(ApiError::bad_request(
                "response_format srt/vtt is not supported because timestamps are unavailable",
            )),
            other => Err(ApiError::bad_request(format!(
                "unsupported response_format: {other}"
            ))),
        }
    }
}

#[cfg(feature = "server")]
fn format_transcription_response(
    text: &str,
    format: ResponseFormat,
    language: Option<&str>,
) -> Response {
    match format {
        ResponseFormat::Json => Json(serde_json::json!({ "text": text })).into_response(),
        ResponseFormat::Text => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(Body::from(text.to_string()))
            .expect("valid text response"),
        ResponseFormat::VerboseJson => Json(serde_json::json!({
            "task": "transcribe",
            "language": language,
            "duration": null,
            "text": text,
            "segments": []
        }))
        .into_response(),
    }
}

#[cfg(feature = "server")]
fn sanitize_filename(filename: &str) -> String {
    Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("audio")
        .replace('"', "")
}

#[cfg(feature = "server")]
fn normalize_upload_metadata(
    filename: &str,
    content_type: String,
    bytes: &[u8],
) -> (String, String) {
    let sniffed = sniff_audio(bytes);
    let content_type = if is_generic_content_type(&content_type) {
        sniffed
            .map(|audio| audio.content_type.to_string())
            .unwrap_or(content_type)
    } else {
        content_type
    };
    let filename = if has_recognized_audio_extension(filename) {
        filename.to_string()
    } else if let Some(audio) = sniffed.or_else(|| audio_type_for_content_type(&content_type)) {
        let stem = Path::new(filename)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|stem| !stem.is_empty() && !stem.starts_with('.'))
            .unwrap_or("audio");
        format!("{}.{}", stem, audio.extension)
    } else {
        filename.to_string()
    };
    (filename, content_type)
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AudioType {
    content_type: &'static str,
    extension: &'static str,
}

#[cfg(feature = "server")]
fn sniff_audio(bytes: &[u8]) -> Option<AudioType> {
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(&b"WAVE"[..]) {
        return Some(AudioType {
            content_type: "audio/wav",
            extension: "wav",
        });
    }
    if bytes.starts_with(b"ID3") || looks_like_mp3_frame(bytes) {
        return Some(AudioType {
            content_type: "audio/mpeg",
            extension: "mp3",
        });
    }
    if bytes.starts_with(b"fLaC") {
        return Some(AudioType {
            content_type: "audio/flac",
            extension: "flac",
        });
    }
    if bytes.starts_with(b"OggS") {
        return Some(AudioType {
            content_type: "audio/ogg",
            extension: "ogg",
        });
    }
    if bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        return Some(AudioType {
            content_type: "audio/webm",
            extension: "webm",
        });
    }
    if bytes.get(4..8) == Some(&b"ftyp"[..]) {
        return Some(AudioType {
            content_type: "audio/mp4",
            extension: "m4a",
        });
    }
    if bytes.starts_with(b"#!SILK_V3") || bytes.starts_with(b"\x02#!SILK_V3") {
        return Some(AudioType {
            content_type: "audio/silk",
            extension: "silk",
        });
    }
    None
}

#[cfg(feature = "server")]
fn looks_like_mp3_frame(bytes: &[u8]) -> bool {
    matches!(bytes, [0xff, second, ..] if second & 0xe0 == 0xe0)
}

#[cfg(feature = "server")]
fn is_generic_content_type(content_type: &str) -> bool {
    matches!(
        content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "" | "application/octet-stream" | "binary/octet-stream"
    )
}

#[cfg(feature = "server")]
fn has_recognized_audio_extension(filename: &str) -> bool {
    codex_asr::infer_content_type(filename) != "application/octet-stream"
        || is_silk_path(Path::new(filename))
}

#[cfg(feature = "server")]
fn audio_type_for_content_type(content_type: &str) -> Option<AudioType> {
    match content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "audio/wav" | "audio/x-wav" | "audio/wave" => Some(AudioType {
            content_type: "audio/wav",
            extension: "wav",
        }),
        "audio/mpeg" | "audio/mp3" => Some(AudioType {
            content_type: "audio/mpeg",
            extension: "mp3",
        }),
        "audio/mp4" | "audio/m4a" | "audio/x-m4a" => Some(AudioType {
            content_type: "audio/mp4",
            extension: "m4a",
        }),
        "audio/flac" | "audio/x-flac" => Some(AudioType {
            content_type: "audio/flac",
            extension: "flac",
        }),
        "audio/ogg" => Some(AudioType {
            content_type: "audio/ogg",
            extension: "ogg",
        }),
        "audio/webm" => Some(AudioType {
            content_type: "audio/webm",
            extension: "webm",
        }),
        "audio/silk" | "audio/x-silk" => Some(AudioType {
            content_type: "audio/silk",
            extension: "silk",
        }),
        _ => None,
    }
}

#[cfg(feature = "server")]
fn upload_suffix(filename: &str, content_type: &str) -> String {
    let path = Path::new(filename);
    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        if !extension.is_empty() {
            return format!(".{extension}");
        }
    }
    match content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "audio/wav" | "audio/x-wav" | "audio/wave" => ".wav".to_string(),
        "audio/mpeg" | "audio/mp3" => ".mp3".to_string(),
        "audio/mp4" | "audio/m4a" | "audio/x-m4a" => ".m4a".to_string(),
        "audio/flac" | "audio/x-flac" => ".flac".to_string(),
        "audio/ogg" => ".ogg".to_string(),
        "audio/webm" => ".webm".to_string(),
        "audio/silk" | "audio/x-silk" => ".silk".to_string(),
        _ => ".bin".to_string(),
    }
}

#[cfg(feature = "server")]
#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
    error_type: &'static str,
}

#[cfg(feature = "server")]
impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
            error_type: "invalid_request_error",
        }
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
            error_type: "authentication_error",
        }
    }

    fn payload_too_large(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            message: message.into(),
            error_type: "invalid_request_error",
        }
    }

    fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_GATEWAY,
            message: message.into(),
            error_type: "upstream_error",
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
            error_type: "server_error",
        }
    }
}

#[cfg(feature = "server")]
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status;
        (status, Json(ErrorBody::new(self))).into_response()
    }
}

#[cfg(feature = "server")]
#[derive(Serialize)]
struct ErrorBody {
    error: ErrorPayload,
}

#[cfg(feature = "server")]
impl ErrorBody {
    fn new(error: ApiError) -> Self {
        Self {
            error: ErrorPayload {
                message: error.message,
                error_type: error.error_type,
                param: None,
                code: None,
            },
        }
    }
}

#[cfg(feature = "server")]
#[derive(Serialize)]
struct ErrorPayload {
    message: String,
    #[serde(rename = "type")]
    error_type: &'static str,
    param: Option<String>,
    code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silk_paths_are_detected_case_insensitively() {
        assert!(is_silk_path(Path::new("voice.silk")));
        assert!(is_silk_path(Path::new("voice.SLK")));
        assert!(!is_silk_path(Path::new("voice.wav")));
    }

    #[test]
    fn decoded_wav_filename_uses_original_stem() {
        assert_eq!(
            decoded_wav_filename(Path::new("/tmp/example.silk")),
            "example.wav"
        );
        assert_eq!(decoded_wav_filename(Path::new(".silk")), "voice.wav");
    }

    #[cfg(feature = "server")]
    #[test]
    fn upload_metadata_sniffs_wav_when_sdk_omits_metadata() {
        let wav = b"RIFF\x24\x00\x00\x00WAVEfmt ";
        assert_eq!(
            sniff_audio(wav).map(|audio| audio.content_type),
            Some("audio/wav")
        );
        assert_eq!(
            normalize_upload_metadata("audio", "application/octet-stream".to_string(), wav),
            ("audio.wav".to_string(), "audio/wav".to_string())
        );
    }

    #[cfg(feature = "server")]
    #[test]
    fn upload_metadata_keeps_explicit_audio_filename() {
        let wav = b"RIFF\x24\x00\x00\x00WAVEfmt ";
        assert_eq!(
            normalize_upload_metadata("clip.wav", "application/octet-stream".to_string(), wav),
            ("clip.wav".to_string(), "audio/wav".to_string())
        );
    }
}
