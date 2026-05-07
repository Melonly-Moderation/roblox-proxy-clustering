FROM cgr.dev/chainguard/rust:latest-dev AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock* ./
COPY src ./src

RUN cargo build --release --locked || cargo build --release

FROM cgr.dev/chainguard/glibc-dynamic:latest AS runtime

WORKDIR /app
COPY --from=builder /app/target/release/roblox-proxy-clustering /usr/local/bin/roblox-proxy-clustering

EXPOSE 8080
CMD ["roblox-proxy-clustering"]
