FROM rust:1-slim-bookworm AS build
WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY web ./web
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/tonight /usr/local/bin/tonight
ENV TONIGHT_DATA_DIR=/data \
    TONIGHT_STEAM_ROOT=/steam \
    TONIGHT_BIND=0.0.0.0:7777
VOLUME ["/data"]
EXPOSE 7777
ENTRYPOINT ["/usr/local/bin/tonight"]
