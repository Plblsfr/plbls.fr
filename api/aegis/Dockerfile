# Contexte de build : la racine du dépôt (docker build -f api/aegis/Dockerfile .), pour le workspace Cargo.
# Fichiers envoyés : Dockerfile.dockerignore. Rust : version de ../../rust-toolchain.toml.
FROM rust:1.99.0-slim-trixie AS chef
RUN cargo install cargo-chef --locked --version 0.1.78
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json -p aegis
COPY . .
RUN cargo build --release --locked -p aegis

FROM build AS test
RUN cargo test --release --locked -p aegis

FROM gcr.io/distroless/cc-debian13:nonroot
WORKDIR /app
COPY --from=build /src/target/release/aegis /app/aegis
COPY api/aegis/config /app/config
COPY plbls.env /etc/plbls.env
EXPOSE 8080
ENTRYPOINT ["/app/aegis"]
