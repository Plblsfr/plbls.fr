# Contexte de build = racine du dépôt (docker build -f api/pli/Dockerfile .), liste dans Dockerfile.dockerignore.
# Rust : version de ../../rust-toolchain.toml. web/pli/public est servi tel quel, copies de @plbls/ui comprises (vendor/plbls-ui).
FROM rust:1.99.0-slim AS chef
RUN cargo install cargo-chef --locked --version 0.1.78
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json -p pli
COPY . .
RUN cargo build --release --locked -p pli

FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
COPY --from=build /src/target/release/pli /app/pli
COPY web/pli/public /app/public
ENV PORT=8080
EXPOSE 8080
ENTRYPOINT ["/app/pli"]
