FROM rust:1.72-slim-bookworm AS builder

WORKDIR app

COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
     cargo build --release && cp /app/target/release/ffmpeg-swarm .

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y ffmpeg && apt-get clean

WORKDIR app
COPY --from=builder /app/ffmpeg-swarm /usr/local/bin/ffmpeg-swarm

ENTRYPOINT ["/usr/local/bin/ffmpeg-swarm"]
CMD ["server"]