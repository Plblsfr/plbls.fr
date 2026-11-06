# Contexte de build = racine du dépôt (workspace Cargo, voir ../../../.dockerignore). Rust : version de ../../../rust-toolchain.toml.
FROM rust:1.99.0-alpine AS chef
RUN apk add --no-cache musl-dev && cargo install cargo-chef --locked --version 0.1.78
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json -p estompe-api
COPY . .
ARG ESTOMPE_REVISION
RUN cargo build --release --locked -p estompe-api && cp target/release/estompe-api /estompe-api
RUN mkdir /data

FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build /estompe-api /estompe-api
COPY --from=build --chown=65532:65532 /data /data
ENV DATA_DIR=/data
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["/estompe-api"]
