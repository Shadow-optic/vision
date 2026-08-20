# syntax=docker/dockerfile:1
FROM rust:1.88-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --release -p vi-api -p vi-ingest

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/vi-api /usr/local/bin/vi-api
COPY --from=builder /app/target/release/vi-ingest /usr/local/bin/vi-ingest
ENV BIND_ADDR=0.0.0.0:8080
EXPOSE 8080
USER nobody
CMD ["vi-api"]
