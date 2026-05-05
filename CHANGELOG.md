# Changelog

## 0.1.1 - 2026-05-05

### Added

- `cargo-binstall` metadata for GitHub Release binary installs.
- Normal GitHub Actions CI for formatting, tests, feature-gated checks, clippy,
  and packaging.

### Changed

- Updated install documentation for the published crate, cargo-dist installers,
  and cargo-binstall.

## 0.1.0 - 2026-05-05

Initial release.

### Added

- CLI and library client for Codex Desktop's one-shot ASR endpoint.
- Local Codex ChatGPT auth reuse from `$CODEX_HOME/auth.json` or `~/.codex/auth.json`.
- Explicit bearer-token mode with automatic ChatGPT account id extraction.
- Multipart upload to `https://chatgpt.com/backend-api/transcribe`.
- Audio content-type inference and multipart filename repair for extensionless inputs.
- External `rust-silk` CLI preprocessing for standard SILK v3 and WeChat/Tencent SILK files.
- OpenAI Whisper-compatible local REST wrapper:
  - `POST /v1/audio/transcriptions`
  - `POST /audio/transcriptions`
  - `GET /healthz`
- REST server local bearer-key protection and configurable upstream concurrency.
- Python OpenAI SDK, Node OpenAI SDK, and curl examples.
- Optional `server` feature; library consumers can disable default features to avoid Axum/Tokio.

### Supported Direct Upload Formats

- WAV PCM
- MP3
- M4A or MP4 AAC
- FLAC
- Ogg Opus
- WebM Opus

### Known Limits

- This is not an official OpenAI API surface.
- `srt` and `vtt` response formats are not supported because the Codex endpoint does not return timestamps.
- `prompt`, `temperature`, and `timestamp_granularities` are accepted by the REST wrapper for SDK compatibility but ignored.
- ADTS AAC, AIFF, CAF AAC, raw PCM streams, and direct SILK uploads were rejected by the upstream endpoint during local testing.
- SILK support requires an external `rust-silk` binary.
