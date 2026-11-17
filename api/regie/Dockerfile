# Contexte de build = racine du dépôt (docker build -f api/regie/Dockerfile .), fichiers envoyés : ../../.dockerignore.
# Rust : version de ../../rust-toolchain.toml.
FROM rust:1.99.0-slim-trixie AS chef
RUN cargo install cargo-chef --locked --version 0.1.78
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json -p regie
COPY . .
RUN cargo build --release --locked -p regie

FROM debian:trixie-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/* \
  && useradd --system --uid 65532 --no-create-home regie
COPY --from=build /src/target/release/regie /app/regie
COPY plbls.env /etc/plbls.env
USER 65532
EXPOSE 8080
ENTRYPOINT ["/app/regie"]
