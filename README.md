# codex-asr

Unofficial Rust CLI/library for Codex Desktop's one-shot dictation ASR endpoint.

It reuses a local Codex ChatGPT login from `$CODEX_HOME/auth.json` or
`~/.codex/auth.json`, then uploads an audio file to:

```text
https://chatgpt.com/backend-api/transcribe
```

This is not an official OpenAI API surface. Treat it as a local automation tool
for an already-signed-in Codex Desktop environment.

## Safety

`codex-asr` reuses a personal Codex/ChatGPT login token. Do not expose it, or a
REST server backed by it, to the public internet.

- Bind REST servers to loopback by default (`127.0.0.1`).
- Use `--api-key` or `CODEX_ASR_SERVER_KEY` for the local REST wrapper.
- Treat the REST API key as local access control only; it is not an upstream
  OpenAI or ChatGPT token.
- Avoid `--no-api-key` unless the server is reachable only by trusted local
  processes.
- This endpoint is reverse-engineered from Codex Desktop behavior and may change
  without notice.

## Install

```bash
cargo install codex-asr
```

GitHub Release installers are generated with `cargo-dist`:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/wangnov/codex-asr/releases/download/v0.1.1/codex-asr-installer.sh \
  | sh
```

PowerShell:

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://github.com/wangnov/codex-asr/releases/download/v0.1.1/codex-asr-installer.ps1 | iex"
```

`cargo-binstall` can install the matching GitHub Release binary directly:

```bash
cargo binstall codex-asr
```

From this checkout:

```bash
cargo install --path .
```

Library consumers that do not need the local REST server can avoid the Axum/Tokio
server dependencies:

```toml
codex-asr = { version = "0.1", default-features = false }
```

## CLI

```bash
codex-asr audio.wav
codex-asr transcribe audio.wav --language zh
codex-asr audio.wav --content-type audio/webm --json
codex-asr raw-audio --content-type audio/wav
codex-asr voice.silk --silk-decoder /path/to/rust-silk
```

Auth defaults to local Codex auth. For the smallest explicit input surface, pass
only a bearer token:

```bash
codex-asr audio.wav --bearer "$TOKEN"
CODEX_ASR_BEARER="$TOKEN" codex-asr audio.wav
```

`ChatGPT-Account-Id` is decoded from the bearer token when possible. Override it
only if your token does not contain that claim:

```bash
codex-asr audio.wav --bearer "$TOKEN" --account-id acct_...
```

`.silk` and `.slk` inputs are decoded with an external `rust-silk` CLI before
upload. The decoder is resolved in this order:

1. `--silk-decoder <path>`
2. `CODEX_ASR_SILK_DECODER`
3. `rust-silk` on `PATH`
4. `$HOME/rust-silk/target/release/rust-silk`
5. `$HOME/rust-silk/target/debug/rust-silk`

The default SILK decode sample rate is 24000 Hz. Override it with:

```bash
codex-asr voice.silk --silk-sample-rate 16000
```

The backend is sensitive to multipart filenames. If an input path has no useful
audio extension but you pass a known `--content-type`, `codex-asr` sends a
synthetic filename such as `raw-audio.wav`. You can override it explicitly:

```bash
codex-asr raw-audio --content-type audio/wav --filename voice.wav
```

## REST Server

`codex-asr` can also serve a small OpenAI Whisper-compatible REST surface:

```bash
codex-asr serve --api-key local_dev_key --host 127.0.0.1 --port 8788 --concurrency 16
```

Then call it with an OpenAI-style multipart request:

```bash
curl http://127.0.0.1:8788/v1/audio/transcriptions \
  -H 'Authorization: Bearer local_dev_key' \
  -F model=whisper-1 \
  -F file=@audio.wav
```

Implemented routes:

| Route | Notes |
| --- | --- |
| `GET /healthz` | no auth required |
| `POST /v1/audio/transcriptions` | OpenAI-style route |
| `POST /audio/transcriptions` | short alias |

Implemented multipart fields:

| Field | Handling |
| --- | --- |
| `file` | required |
| `model` | accepted for SDK compatibility, ignored |
| `language` | forwarded to Codex `/transcribe` |
| `response_format` | supports `json`, `text`, `verbose_json` |
| `prompt`, `temperature`, `timestamp_granularities` | accepted and ignored |

`srt` and `vtt` response formats return HTTP 400 because the Codex endpoint does
not provide timestamps. REST auth defaults to `CODEX_ASR_SERVER_KEY` or
`--api-key`; use `--no-api-key` only on trusted loopback.

OpenAI SDK examples live in `examples/`:

```bash
python3 -m pip install openai
CODEX_ASR_SERVER_KEY=local_dev_key \
  python3 examples/python_openai_sdk.py audio.wav

npm install openai
CODEX_ASR_SERVER_KEY=local_dev_key \
  node examples/node_openai_sdk.mjs audio.wav
```

The Python example disables environment proxy discovery for the local SDK client
because some systems route localhost traffic through a proxy unless `trust_env`
is disabled.

## Library

```rust
use codex_asr::{CodexAsrClient, TranscribeOptions};

let client = CodexAsrClient::from_codex_home()?;
let result = client.transcribe_file("audio.wav", TranscribeOptions::default())?;
println!("{}", result.text);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Audio Formats

The Codex Desktop endpoint appears to inspect the actual audio container, not
only the multipart content type.

These formats were tested successfully when uploaded directly:

| Container / codec | Suggested content type |
| --- | --- |
| WAV PCM | `audio/wav` |
| MP3 | `audio/mpeg` |
| M4A or MP4 AAC | `audio/mp4` |
| FLAC | `audio/flac` |
| Ogg Opus | `audio/ogg` |
| WebM Opus | `audio/webm` |

Files with no recognizable audio extension should be uploaded with a known
`--content-type`; the CLI will add a matching multipart filename extension.

These formats are supported by the `codex-asr` CLI through local preprocessing:

| Input | Handling |
| --- | --- |
| SILK v3 (`#!SILK_V3`) | decoded to temporary WAV with `rust-silk` |
| WeChat/Tencent SILK (`0x02 + #!SILK_V3`) | decoded to temporary WAV with `rust-silk` |

These were rejected by the endpoint during local testing when uploaded directly:

| Format | Result |
| --- | --- |
| ADTS AAC (`.aac`) | HTTP 500, ASR API error |
| AIFF | HTTP 500, ASR API error |
| CAF AAC | HTTP 500, ASR API error |
| Raw PCM stream | HTTP 500, ASR API error |
| SILK v3 (`#!SILK_V3`) | HTTP 500, ASR API error |
| WeChat/Tencent SILK (`0x02 + #!SILK_V3`) | HTTP 500, ASR API error |

## Endpoint Notes

Local probes against the Codex Desktop endpoint showed these practical edges:

- Empty files return HTTP 500 with `Error in ASR API`.
- One second of silence returns HTTP 200 with an empty transcript.
- Very short non-silent clips can return unstable text; avoid treating tiny
  snippets as reliable.
- A missing or misleading multipart filename extension can make an otherwise
  valid audio file fail. `codex-asr` compensates when `--content-type` is known.
- Parallel batches up to 96 short WAV uploads succeeded locally without 429 or
  5xx responses, but this is not a public API contract. Keep any REST wrapper
  concurrency bounded, especially for longer audio.

## Shape

Default shape is CLI + library crate + local REST wrapper. The REST wrapper is
kept local-first because this tool handles a user bearer token.
