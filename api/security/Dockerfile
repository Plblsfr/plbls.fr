# Contexte de build = racine du dépôt (workspace Cargo, voir ../../.dockerignore). Rust : version de ../../rust-toolchain.toml.
FROM rust:1.99.0-slim-trixie AS chef
RUN cargo install cargo-chef --locked --version 0.1.78
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json -p auth-gateway
COPY . .
RUN cargo build --release --locked -p auth-gateway

# Tests unitaires : docker build -f api/security/Dockerfile --target test . (utilisé par la CI)
FROM build AS test
RUN cargo test --release --locked -p auth-gateway

FROM gcr.io/distroless/cc-debian13:nonroot
WORKDIR /app
COPY --from=build /src/target/release/auth-gateway /app/auth-gateway
COPY api/security/config /app/config
COPY plbls.env /etc/plbls.env
EXPOSE 8080
ENTRYPOINT ["/app/auth-gateway"]
