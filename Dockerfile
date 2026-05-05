# syntax=docker/dockerfile:1.7

FROM rust:1-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked --bin codex-asr && \
    cp /app/target/release/codex-asr /usr/local/bin/codex-asr

FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    useradd --system --uid 10001 --gid nogroup --home-dir /nonexistent --shell /usr/sbin/nologin codex-asr

COPY --from=builder /usr/local/bin/codex-asr /usr/local/bin/codex-asr

USER 10001:65534
EXPOSE 8788

ENTRYPOINT ["codex-asr"]
CMD ["serve", "--host", "0.0.0.0", "--port", "8788"]
