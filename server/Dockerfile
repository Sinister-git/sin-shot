# Build stage — produces a fully static binary via musl
FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --target x86_64-unknown-linux-musl

# Runtime stage — minimal alpine, no runtime deps
FROM alpine:3.21
RUN apk add --no-cache ca-certificates
WORKDIR /app
RUN mkdir -p /app/uploads
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/sin-shot-server /app/sin-shot-server

ENV STORAGE_DIR=/app/uploads
ENV PORT=8080
ENV MAX_FILE_SIZE_MB=25
ENV BASE_URL=https://sinister.ovh

EXPOSE 8080
ENTRYPOINT ["/app/sin-shot-server"]
