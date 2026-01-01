# build stage
FROM rust:1.75-slim AS builder

WORKDIR /app

# copy manifests
COPY Cargo.toml Cargo.lock* ./

# create dummy src and bench to cache dependencies
RUN mkdir -p src benches && \
    echo "fn main() {}" > src/main.rs && \
    echo "fn main() {}" > benches/search_benchmarks.rs
RUN cargo build --release && rm -rf src benches

# copy actual source
COPY src ./src
COPY benches ./benches

# build for release
RUN touch src/main.rs && cargo build --release

# runtime stage - small as fuck
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/quickset /app/quickset

# default environment
ENV QUICKSET_HOST=0.0.0.0
ENV QUICKSET_PORT=8080
ENV QUICKSET_AUTH_LEVEL=none
ENV QUICKSET_LOG=info

EXPOSE 8080

CMD ["./quickset"]
