# Build stage — produces a fully static binary via musl
FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
ARG TARGETARCH
RUN case "$TARGETARCH" in \
      arm64) RUST_TARGET=aarch64-unknown-linux-musl ;; \
      *)     RUST_TARGET=x86_64-unknown-linux-musl ;; \
    esac && \
    rustup target add "$RUST_TARGET" && \
    cargo build --release --target "$RUST_TARGET" && \
    cp "/app/target/$RUST_TARGET/release/sin-shot-server" /app/sin-shot-server

# Runtime stage — minimal alpine, no runtime deps
FROM alpine:3.21
RUN apk add --no-cache ca-certificates
WORKDIR /app
RUN mkdir -p /app/uploads
COPY --from=builder /app/sin-shot-server /app/sin-shot-server

ENV STORAGE_DIR=/app/uploads
ENV PORT=8080
ENV MAX_FILE_SIZE_MB=25
ENV BASE_URL=https://sinister.ovh
# Google OAuth client ID for gallery auth (optional — disables auth if empty)
ENV GOOGLE_CLIENT_ID=
# Comma-separated CORS origins (defaults to BASE_URL origin)
ENV CORS_ORIGINS=

EXPOSE 8080
ENTRYPOINT ["/app/sin-shot-server"]
