<p align="center">
  <img src="./logo.png" width="220" alt="codex-asr logo">
</p>

<h1 align="center">codex-asr</h1>

<p align="center">
  Reuse your local Codex Desktop ChatGPT login for one-shot ASR.
</p>

<p align="center">
  <a href="https://github.com/Wangnov/codex-asr/releases/latest"><img src="https://img.shields.io/github/v/release/Wangnov/codex-asr?logo=github" alt="Latest release"></a>
  <a href="https://crates.io/crates/codex-asr"><img src="https://img.shields.io/crates/v/codex-asr?logo=rust" alt="Crates.io"></a>
  <a href="https://docs.rs/codex-asr"><img src="https://img.shields.io/docsrs/codex-asr?logo=docs.rs" alt="docs.rs"></a>
  <a href="https://github.com/Wangnov/codex-asr/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Wangnov/codex-asr/ci.yml?branch=main&label=ci" alt="CI"></a>
  <a href="https://github.com/Wangnov/codex-asr/actions/workflows/docker.yml"><img src="https://img.shields.io/github/actions/workflow/status/Wangnov/codex-asr/docker.yml?branch=main&label=docker" alt="Docker"></a>
  <a href="https://github.com/Wangnov/codex-asr/actions/workflows/release.yml"><img src="https://img.shields.io/badge/release-cargo--dist-2ea44f" alt="cargo-dist release automation"></a>
  <a href="https://github.com/Wangnov/codex-asr/blob/main/Cargo.toml"><img src="https://img.shields.io/badge/license-MIT-2ea44f" alt="MIT license"></a>
</p>

<p align="center">
  <a href="#readme-cn">中文</a> · <a href="#readme-en">English</a>
</p>

<p align="center">
  crates.io · cargo-binstall · Homebrew · Docker / GHCR · Rust library · local REST
</p>

---

<a id="readme-cn"></a>

# 中文

Codex.app 里有一个一次性语音输入接口：它把录音上传到
`https://chatgpt.com/backend-api/transcribe`，再拿回文本。这个接口不是
OpenAI Whisper API，也不是公开稳定 API，但它可以复用你已经登录在本机
Codex Desktop / ChatGPT 里的账号。

`codex-asr` 把这件事拆成一个 Rust CLI、一个可发布的 crate，以及一个本地
OpenAI Whisper 风格 REST 包装层。你可以直接转写音频文件，也可以让现有
OpenAI SDK 指向本机 `codex-asr serve`。

## 适合谁用

- 你已经在 Codex Desktop 里登录了 ChatGPT 账号
- 你想在脚本、CLI、Agent 或本地服务里复用 Codex App 的 ASR 能力
- 你只想提供最小输入面，例如一个本地音频文件，或显式传入一个 bearer token
- 你想把 `.silk` / 微信语音先经本机 `rust-silk` 解码，再上传转写
- 你接受这是逆向自 Codex Desktop 行为的本地自动化工具，而不是官方 API

## 安装

### Homebrew

```bash
brew tap wangnov/tap
brew install codex-asr
```

### cargo-binstall

```bash
cargo binstall codex-asr
```

### cargo install

```bash
cargo install codex-asr
```

### GitHub Release 安装脚本

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/Wangnov/codex-asr/releases/latest/download/codex-asr-installer.sh \
  | sh
```

PowerShell:

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://github.com/Wangnov/codex-asr/releases/latest/download/codex-asr-installer.ps1 | iex"
```

### 直接下载二进制

从 [最新 GitHub Release](https://github.com/Wangnov/codex-asr/releases/latest)
下载对应平台的压缩包或安装脚本。

### Docker / GHCR

```bash
docker pull ghcr.io/wangnov/codex-asr:latest
docker run --rm ghcr.io/wangnov/codex-asr:latest --version
```

`main` 分支会通过 GitHub Actions 发布 `latest` 和当前 crate 版本标签。
Docker 镜像默认内置 `rust-silk`，路径是 `/usr/local/bin/rust-silk`。

### 从当前源码安装

```bash
cargo install --path .
```

## 快速上手

```bash
# 直接转写，认证默认读取 $CODEX_HOME/auth.json 或 ~/.codex/auth.json
codex-asr audio.wav

# 给上游 ASR 一个语言提示
codex-asr audio.wav --language zh

# JSON 输出，方便脚本消费
codex-asr audio.wav --json

# 扩展名缺失时显式告诉 multipart content type
codex-asr raw-audio --content-type audio/wav

# 微信 / SILK 语音先通过外部 rust-silk CLI 解码成临时 WAV
codex-asr voice.silk --silk-decoder /path/to/rust-silk
```

## 本地 REST / OpenAI SDK

启动一个本地 Whisper 风格 REST 端点：

```bash
codex-asr serve --api-key local_dev_key --host 127.0.0.1 --port 8788 --concurrency 16
```

然后用 OpenAI 风格 multipart 请求调用：

```bash
curl http://127.0.0.1:8788/v1/audio/transcriptions \
  -H 'Authorization: Bearer local_dev_key' \
  -F model=whisper-1 \
  -F file=@audio.wav
```

OpenAI SDK 示例在 `examples/`：

```bash
python3 -m pip install openai
CODEX_ASR_SERVER_KEY=local_dev_key \
  python3 examples/python_openai_sdk.py audio.wav

npm install openai
CODEX_ASR_SERVER_KEY=local_dev_key \
  node examples/node_openai_sdk.mjs audio.wav
```

REST 路由：

| Route | 说明 |
| --- | --- |
| `GET /healthz` | 健康检查，不需要认证 |
| `POST /v1/audio/transcriptions` | OpenAI Whisper 风格路由 |
| `POST /audio/transcriptions` | 短别名 |

REST 字段兼容范围：

| Field | 处理方式 |
| --- | --- |
| `file` | 必填 |
| `model` | 为 SDK 兼容而接受，忽略 |
| `language` | 转发给 Codex `/transcribe` |
| `response_format` | 支持 `json`、`text`、`verbose_json` |
| `prompt`、`temperature`、`timestamp_granularities` | 为 SDK 兼容而接受，忽略 |

`srt` 和 `vtt` 会返回 HTTP 400，因为 Codex 这个端点不返回时间戳。

## Docker 公网部署

这个部署形态只承诺三件事：公网端口、HTTPS、api-key。`deploy/compose.public.yml`
用 Caddy 自动申请证书，把 80/443 暴露到公网，再反代到容器里的
`codex-asr serve`。应用层鉴权使用 `Authorization: Bearer <CODEX_ASR_SERVER_KEY>`。

前置条件：

- 域名 A/AAAA 记录已经指向服务器
- 服务器公网防火墙放行 80 和 443
- 服务器上已有可用的 Codex / ChatGPT auth 文件
- 已准备一个容器用户可读的 auth 副本

```bash
sudo install -d -m 750 /opt/codex-asr
sudo install -o 10001 -g 65534 -m 0400 ~/.codex/auth.json /opt/codex-asr/auth.json
cp deploy/env.public.example deploy/.env
$EDITOR deploy/.env
docker compose --env-file deploy/.env -f deploy/compose.public.yml up -d
```

`deploy/.env` 里至少要设置：

```bash
CODEX_ASR_DOMAIN=asr.example.com
CODEX_ASR_AUTH_FILE=/opt/codex-asr/auth.json
CODEX_ASR_SERVER_KEY=replace-with-a-long-random-api-key
CODEX_ASR_MAX_UPLOAD_MB=50
CODEX_ASR_REQUEST_TIMEOUT_SECONDS=300
```

调用示例：

```bash
curl https://asr.example.com/healthz

curl https://asr.example.com/v1/audio/transcriptions \
  -H "Authorization: Bearer replace-with-a-long-random-api-key" \
  -F model=whisper-1 \
  -F file=@audio.wav
```

Docker 镜像默认内置 `rust-silk`，所以 `.silk` / 微信语音会在容器内先解码成
临时 WAV 再上传。只有要替换成自定义解码器时，才需要覆盖
`CODEX_ASR_SILK_DECODER`。

## 命令说明

- `codex-asr <audio>`：默认等同于 `codex-asr transcribe <audio>`
- `transcribe`：上传一个音频文件并返回文本
- `serve`：启动本地 OpenAI Whisper 风格 REST 包装层

常用选项：

| Option | 说明 |
| --- | --- |
| `--bearer` | 显式传入 ChatGPT bearer token；默认读取本机 Codex auth |
| `--account-id` | 覆盖 `ChatGPT-Account-Id`；通常会从 bearer token 自动解析 |
| `--auth-file` | 指定 Codex auth 文件 |
| `--endpoint` | 覆盖上游 `/backend-api/transcribe` URL |
| `--proxy` | 指定 HTTPS 代理 |
| `--request-timeout-seconds` | 上游请求总超时，默认 `300` |
| `--connect-timeout-seconds` | 上游连接超时，默认 `15` |
| `--language` | 语言提示，例如 `zh` 或 `en` |
| `--content-type` | 覆盖音频 content type |
| `--filename` | 覆盖 multipart 文件名 |
| `--json` | 输出 `{"text":"..."}` |
| `--silk-decoder` | 指定外部 `rust-silk` CLI |
| `--silk-sample-rate` | SILK 解码成 WAV 时的采样率，默认 `24000` |
| `--no-silk-decode` | 直接上传 `.silk`，不做本地解码 |

`serve` 额外支持：

| Option | 说明 |
| --- | --- |
| `--host` | 绑定地址，默认 `127.0.0.1` |
| `--port` | 绑定端口，默认 `8788` |
| `--api-key` | 本地 REST API key |
| `--no-api-key` | 关闭本地 REST 认证，仅限可信 loopback |
| `--concurrency` | 上游 ASR 并发上限，默认 `16` |
| `--max-upload-mb` | REST 上传体积上限，默认 `50` MiB |

## 认证模型

默认情况下，`codex-asr` 会读取：

1. `$CODEX_HOME/auth.json`
2. `~/.codex/auth.json`

如果你想把输入面压到最小，可以只传 bearer：

```bash
codex-asr audio.wav --bearer "$TOKEN"
CODEX_ASR_BEARER="$TOKEN" codex-asr audio.wav
```

`ChatGPT-Account-Id` 通常会从 bearer token 的 JWT payload 中自动解析。只有当
token 里没有这个 claim 时，才需要手动传：

```bash
codex-asr audio.wav --bearer "$TOKEN" --account-id acct_...
```

可用环境变量：

| Env | 用途 |
| --- | --- |
| `CODEX_ASR_BEARER` | 默认 bearer token |
| `CODEX_ASR_SERVER_KEY` | 本地 REST API key |
| `CODEX_ASR_PROXY` | HTTPS 代理 |
| `CODEX_ASR_REQUEST_TIMEOUT_SECONDS` | 上游请求总超时，默认 `300` |
| `CODEX_ASR_CONNECT_TIMEOUT_SECONDS` | 上游连接超时，默认 `15` |
| `CODEX_ASR_MAX_UPLOAD_MB` | REST 上传体积上限，默认 `50` MiB |
| `CODEX_ASR_SILK_DECODER` | 外部 `rust-silk` CLI 路径；Docker 镜像默认 `/usr/local/bin/rust-silk` |

## 它会改什么

- 读取本机 Codex / ChatGPT auth 文件，但不会写入或刷新它
- `serve` 会在每次转写前重新读取 auth 文件，以便容器中的 auth 副本更新后无需重启服务
- 上传你指定的音频文件到 `https://chatgpt.com/backend-api/transcribe`
- `.silk` / `.slk` 输入默认先在本地解码成临时 WAV，再上传 WAV
- REST server 只做本地包装、字段兼容、并发限制和响应格式转换
- 不会保存转写结果，除非你的调用方自己保存

## 安全边界

`codex-asr` 会复用你的个人 Codex / ChatGPT 登录 token。不要裸露 HTTP 或无
api-key 的 REST server；公网部署至少使用 HTTPS 和应用层 api-key。

- REST 默认绑定 `127.0.0.1`
- 本地 REST key 只是本地访问控制，不是 OpenAI 或 ChatGPT token
- 只有在可信本机回环环境里才使用 `--no-api-key`
- `deploy/compose.public.yml` 的公网形态默认启用 Caddy HTTPS 和 REST api-key
- 这个端点是从 Codex Desktop 行为逆向出来的，可能随 Codex App 更新而变化

## 音频格式

Codex Desktop 的上游端点会检查真实音频容器，不只看 multipart content type。

本地测试中可直接上传的格式：

| Container / codec | 建议 content type |
| --- | --- |
| WAV PCM | `audio/wav` |
| MP3 | `audio/mpeg` |
| M4A 或 MP4 AAC | `audio/mp4` |
| FLAC | `audio/flac` |
| Ogg Opus | `audio/ogg` |
| WebM Opus | `audio/webm` |

通过本地预处理支持的格式：

| Input | 处理方式 |
| --- | --- |
| SILK v3 (`#!SILK_V3`) | 用外部 `rust-silk` 解码成临时 WAV |
| WeChat/Tencent SILK (`0x02 + #!SILK_V3`) | 用外部 `rust-silk` 解码成临时 WAV |

本地测试中直接上传会被上游拒绝的格式：

| Format | 结果 |
| --- | --- |
| ADTS AAC (`.aac`) | HTTP 500, ASR API error |
| AIFF | HTTP 500, ASR API error |
| CAF AAC | HTTP 500, ASR API error |
| Raw PCM stream | HTTP 500, ASR API error |
| SILK v3 (`#!SILK_V3`) | HTTP 500, ASR API error |
| WeChat/Tencent SILK (`0x02 + #!SILK_V3`) | HTTP 500, ASR API error |

如果文件名没有可识别音频扩展名，请传 `--content-type`；CLI 会补一个合适的
multipart 文件名。也可以显式传：

```bash
codex-asr raw-audio --content-type audio/wav --filename voice.wav
```

## 已知边界

- 空文件会返回 HTTP 500 `Error in ASR API`
- 一秒静音 WAV 会返回 HTTP 200 和空文本
- 非常短的非静音片段可能返回不稳定文本
- `prompt`、`temperature`、`timestamp_granularities` 只为 SDK 兼容而接受，不会影响上游
- 本地短 WAV 并发测试到 96 个请求没有出现 429/5xx，但这不是公开 API 承诺
- 建议 REST wrapper 对长音频保持有界并发，默认 `--concurrency 16`
- REST server 默认限制上传体积为 `50` MiB，上游请求总超时为 `300` 秒

## Rust library

不需要 REST server 的库用户可以关闭默认特性，避免引入 Axum/Tokio：

```toml
codex-asr = { version = "0.1", default-features = false }
```

```rust
use codex_asr::{CodexAsrClient, TranscribeOptions};

let client = CodexAsrClient::from_codex_home()?;
let result = client.transcribe_file("audio.wav", TranscribeOptions::default())?;
println!("{}", result.text);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## 平台支持

- CLI 和 library 支持 macOS、Linux、Windows
- Release 二进制覆盖 Apple Silicon macOS、Intel macOS、Linux x64、Linux ARM64、Windows x64
- `serve` 功能默认开启；作为库使用时可通过 `default-features = false` 关闭
- SILK 支持依赖外部 `rust-silk` CLI；普通二进制不内置解码器，Docker 镜像默认内置

## 从源码运行

```bash
cargo run -- --help
cargo run -- audio.wav --json
cargo run -- serve --api-key local_dev_key
```

## 视觉资产

- 顶部 `logo.png` 由 `gpt-image-2-skill` 生成
- `assets/codex.svg` 与 `assets/codex-color.svg` 来自
  [LobeHub Codex (OpenAI) icon](https://lobehub.com/icons/codex)
- `assets/codex-color-reference.png` 是从 `assets/codex-color.svg` 渲染出的参考图；
  生成 `logo.png` 时作为 `--ref-image` 传入。直接上传 SVG 会被上游图片编辑
  接口拒绝，因为它只接受 JPEG、PNG、GIF、WebP 参考图。

---

<a id="readme-en"></a>

# English

Codex.app has a one-shot dictation endpoint: it uploads recorded audio to
`https://chatgpt.com/backend-api/transcribe` and receives text back. This is not
the OpenAI Whisper API and it is not a public stable API, but it can reuse the
ChatGPT account that is already signed in on your local Codex Desktop app.

`codex-asr` packages that behavior as a Rust CLI, a publishable crate, and a
local OpenAI Whisper-style REST shim. You can transcribe files directly, or point
existing OpenAI SDK clients at `codex-asr serve` on localhost.

## Who this is for

- You already use Codex Desktop with a signed-in ChatGPT account
- You want to reuse Codex App ASR from scripts, CLIs, agents, or local services
- You want a tiny input surface: a local audio file, or an explicit bearer token
- You want `.silk` / WeChat voice files decoded through your local `rust-silk` CLI
- You accept that this is a reverse-engineered local automation surface, not an official API

## Install

### Homebrew

```bash
brew tap wangnov/tap
brew install codex-asr
```

### cargo-binstall

```bash
cargo binstall codex-asr
```

### cargo install

```bash
cargo install codex-asr
```

### GitHub Release installer

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/Wangnov/codex-asr/releases/latest/download/codex-asr-installer.sh \
  | sh
```

PowerShell:

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://github.com/Wangnov/codex-asr/releases/latest/download/codex-asr-installer.ps1 | iex"
```

### Direct binary download

Download the matching archive or installer from the
[latest GitHub Release](https://github.com/Wangnov/codex-asr/releases/latest).

### Docker / GHCR

```bash
docker pull ghcr.io/wangnov/codex-asr:latest
docker run --rm ghcr.io/wangnov/codex-asr:latest --version
```

The `main` branch publishes `latest` and the current crate-version tag through
GitHub Actions.
Docker images include `rust-silk` at `/usr/local/bin/rust-silk` by default.

### From this checkout

```bash
cargo install --path .
```

## Quick start

```bash
# Transcribe directly, reading $CODEX_HOME/auth.json or ~/.codex/auth.json
codex-asr audio.wav

# Send a language hint upstream
codex-asr audio.wav --language zh

# JSON output for scripts
codex-asr audio.wav --json

# Extensionless input with explicit multipart content type
codex-asr raw-audio --content-type audio/wav

# Decode WeChat / SILK audio through an external rust-silk CLI first
codex-asr voice.silk --silk-decoder /path/to/rust-silk
```

## Local REST / OpenAI SDK

Start a local Whisper-style REST endpoint:

```bash
codex-asr serve --api-key local_dev_key --host 127.0.0.1 --port 8788 --concurrency 16
```

Call it with an OpenAI-style multipart request:

```bash
curl http://127.0.0.1:8788/v1/audio/transcriptions \
  -H 'Authorization: Bearer local_dev_key' \
  -F model=whisper-1 \
  -F file=@audio.wav
```

OpenAI SDK examples live in `examples/`:

```bash
python3 -m pip install openai
CODEX_ASR_SERVER_KEY=local_dev_key \
  python3 examples/python_openai_sdk.py audio.wav

npm install openai
CODEX_ASR_SERVER_KEY=local_dev_key \
  node examples/node_openai_sdk.mjs audio.wav
```

Routes:

| Route | Notes |
| --- | --- |
| `GET /healthz` | health check, no auth required |
| `POST /v1/audio/transcriptions` | OpenAI Whisper-style route |
| `POST /audio/transcriptions` | short alias |

Multipart fields:

| Field | Handling |
| --- | --- |
| `file` | required |
| `model` | accepted for SDK compatibility, ignored |
| `language` | forwarded to Codex `/transcribe` |
| `response_format` | supports `json`, `text`, `verbose_json` |
| `prompt`, `temperature`, `timestamp_granularities` | accepted for SDK compatibility, ignored |

`srt` and `vtt` return HTTP 400 because the Codex endpoint does not return
timestamps.

## Public Docker Deployment

This deployment shape is intentionally small: a public port, HTTPS, and an
application API key. `deploy/compose.public.yml` uses Caddy to obtain TLS
certificates on ports 80/443 and reverse proxy to `codex-asr serve` inside the
private Docker network. App auth is `Authorization: Bearer <CODEX_ASR_SERVER_KEY>`.

Prerequisites:

- The domain A/AAAA record points at the server
- The server firewall allows ports 80 and 443
- A usable Codex / ChatGPT auth file exists on the server
- A container-readable auth copy has been prepared

```bash
sudo install -d -m 750 /opt/codex-asr
sudo install -o 10001 -g 65534 -m 0400 ~/.codex/auth.json /opt/codex-asr/auth.json
cp deploy/env.public.example deploy/.env
$EDITOR deploy/.env
docker compose --env-file deploy/.env -f deploy/compose.public.yml up -d
```

Set at least these values in `deploy/.env`:

```bash
CODEX_ASR_DOMAIN=asr.example.com
CODEX_ASR_AUTH_FILE=/opt/codex-asr/auth.json
CODEX_ASR_SERVER_KEY=replace-with-a-long-random-api-key
CODEX_ASR_MAX_UPLOAD_MB=50
CODEX_ASR_REQUEST_TIMEOUT_SECONDS=300
```

Example requests:

```bash
curl https://asr.example.com/healthz

curl https://asr.example.com/v1/audio/transcriptions \
  -H "Authorization: Bearer replace-with-a-long-random-api-key" \
  -F model=whisper-1 \
  -F file=@audio.wav
```

Docker images include `rust-silk` by default, so `.silk` / WeChat voice files
are decoded to temporary WAV inside the container before upload. Override
`CODEX_ASR_SILK_DECODER` only when you want to use a custom decoder.

## Commands

- `codex-asr <audio>`: same as `codex-asr transcribe <audio>`
- `transcribe`: upload one audio file and return text
- `serve`: run the local OpenAI Whisper-style REST shim

Common options:

| Option | Notes |
| --- | --- |
| `--bearer` | explicit ChatGPT bearer token; defaults to local Codex auth |
| `--account-id` | override `ChatGPT-Account-Id`; usually decoded from the bearer token |
| `--auth-file` | custom Codex auth file |
| `--endpoint` | override the upstream `/backend-api/transcribe` URL |
| `--proxy` | HTTPS proxy |
| `--request-timeout-seconds` | total upstream request timeout, default `300` |
| `--connect-timeout-seconds` | upstream connect timeout, default `15` |
| `--language` | language hint, for example `zh` or `en` |
| `--content-type` | audio content type override |
| `--filename` | multipart filename override |
| `--json` | print `{"text":"..."}` |
| `--silk-decoder` | external `rust-silk` CLI path |
| `--silk-sample-rate` | WAV sample rate for SILK decode, default `24000` |
| `--no-silk-decode` | upload `.silk` directly without local decoding |

`serve` also supports:

| Option | Notes |
| --- | --- |
| `--host` | bind host, default `127.0.0.1` |
| `--port` | bind port, default `8788` |
| `--api-key` | local REST API key |
| `--no-api-key` | disable local REST auth, only for trusted loopback |
| `--concurrency` | maximum concurrent upstream ASR requests, default `16` |
| `--max-upload-mb` | REST upload size limit, default `50` MiB |

## Auth model

By default, `codex-asr` reads:

1. `$CODEX_HOME/auth.json`
2. `~/.codex/auth.json`

For the smallest explicit input surface, pass only a bearer token:

```bash
codex-asr audio.wav --bearer "$TOKEN"
CODEX_ASR_BEARER="$TOKEN" codex-asr audio.wav
```

`ChatGPT-Account-Id` is usually decoded from the JWT payload. Override it only
when the token does not contain that claim:

```bash
codex-asr audio.wav --bearer "$TOKEN" --account-id acct_...
```

Environment variables:

| Env | Purpose |
| --- | --- |
| `CODEX_ASR_BEARER` | default bearer token |
| `CODEX_ASR_SERVER_KEY` | local REST API key |
| `CODEX_ASR_PROXY` | HTTPS proxy |
| `CODEX_ASR_REQUEST_TIMEOUT_SECONDS` | total upstream request timeout, default `300` |
| `CODEX_ASR_CONNECT_TIMEOUT_SECONDS` | upstream connect timeout, default `15` |
| `CODEX_ASR_MAX_UPLOAD_MB` | REST upload size limit, default `50` MiB |
| `CODEX_ASR_SILK_DECODER` | external `rust-silk` CLI path; Docker defaults to `/usr/local/bin/rust-silk` |

## What it changes

- Reads your local Codex / ChatGPT auth file, but does not write or refresh it
- `serve` rereads the auth file before each transcription so an updated container auth copy takes effect without restarting
- Uploads the audio file you provide to `https://chatgpt.com/backend-api/transcribe`
- Decodes `.silk` / `.slk` inputs to temporary WAV files before upload by default
- The REST server only wraps requests, normalizes SDK fields, limits concurrency, and converts responses
- It does not persist transcripts unless your caller does so

## Safety

`codex-asr` reuses a personal Codex / ChatGPT login token. Do not expose plain
HTTP or an unauthenticated REST server; public deployment should use at least
HTTPS and an application API key.

- REST binds to `127.0.0.1` by default
- The REST API key is local access control only; it is not an OpenAI or ChatGPT token
- Use `--no-api-key` only on trusted loopback
- `deploy/compose.public.yml` enables Caddy HTTPS and REST api-key auth by default
- This endpoint is reverse-engineered from Codex Desktop behavior and may change without notice

## Audio formats

The Codex Desktop endpoint appears to inspect the actual audio container, not
only the multipart content type.

Formats tested successfully when uploaded directly:

| Container / codec | Suggested content type |
| --- | --- |
| WAV PCM | `audio/wav` |
| MP3 | `audio/mpeg` |
| M4A or MP4 AAC | `audio/mp4` |
| FLAC | `audio/flac` |
| Ogg Opus | `audio/ogg` |
| WebM Opus | `audio/webm` |

Formats supported through local preprocessing:

| Input | Handling |
| --- | --- |
| SILK v3 (`#!SILK_V3`) | decoded to temporary WAV with `rust-silk` |
| WeChat/Tencent SILK (`0x02 + #!SILK_V3`) | decoded to temporary WAV with `rust-silk` |

Formats rejected by the upstream endpoint during local direct-upload tests:

| Format | Result |
| --- | --- |
| ADTS AAC (`.aac`) | HTTP 500, ASR API error |
| AIFF | HTTP 500, ASR API error |
| CAF AAC | HTTP 500, ASR API error |
| Raw PCM stream | HTTP 500, ASR API error |
| SILK v3 (`#!SILK_V3`) | HTTP 500, ASR API error |
| WeChat/Tencent SILK (`0x02 + #!SILK_V3`) | HTTP 500, ASR API error |

Files with no recognizable audio extension should be uploaded with a known
`--content-type`; the CLI will add a matching multipart filename extension. You
can override it explicitly:

```bash
codex-asr raw-audio --content-type audio/wav --filename voice.wav
```

## Known limits

- Empty files return HTTP 500 with `Error in ASR API`
- One second of silence returns HTTP 200 with an empty transcript
- Very short non-silent clips can return unstable text
- `prompt`, `temperature`, and `timestamp_granularities` are accepted only for SDK compatibility
- Local short-WAV probes reached 96 concurrent requests without 429/5xx, but that is not a public API contract
- Keep long-audio REST usage bounded; the default is `--concurrency 16`
- The REST server defaults to a `50` MiB upload limit and a `300` second upstream request timeout

## Rust library

Library consumers that do not need the local REST server can avoid Axum/Tokio:

```toml
codex-asr = { version = "0.1", default-features = false }
```

```rust
use codex_asr::{CodexAsrClient, TranscribeOptions};

let client = CodexAsrClient::from_codex_home()?;
let result = client.transcribe_file("audio.wav", TranscribeOptions::default())?;
println!("{}", result.text);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Platforms

- CLI and library support macOS, Linux, and Windows
- Prebuilt binaries are published for Apple Silicon macOS, Intel macOS, Linux x64, Linux ARM64, and Windows x64
- `serve` is enabled by default; library users can disable it with `default-features = false`
- SILK support depends on an external `rust-silk` CLI; regular binaries do not bundle a decoder, while Docker images include one by default

## Run from source

```bash
cargo run -- --help
cargo run -- audio.wav --json
cargo run -- serve --api-key local_dev_key
```

## Visual assets

- The top `logo.png` was generated with `gpt-image-2-skill`
- `assets/codex.svg` and `assets/codex-color.svg` are from the
  [LobeHub Codex (OpenAI) icon](https://lobehub.com/icons/codex)
- `assets/codex-color-reference.png` is rendered from `assets/codex-color.svg`
  and was passed as the `--ref-image` when generating `logo.png`. Passing the
  SVG directly was rejected by the upstream image-editing API because it only
  accepts JPEG, PNG, GIF, and WebP reference images.
