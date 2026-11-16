# Contexte de build = racine du dépôt (docker build -f api/sillon/Dockerfile .), fichiers envoyés : ../../.dockerignore.
# Rust : version de ../../rust-toolchain.toml. L'image d'exécution porte ffmpeg et ffprobe (analyse et transcodage audio).
FROM rust:1.99.0-slim-trixie AS chef
RUN cargo install cargo-chef --locked --version 0.1.78
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json -p sillon
COPY . .
RUN cargo build --release --locked -p sillon

FROM debian:trixie-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends ffmpeg ca-certificates \
  && rm -rf /var/lib/apt/lists/* \
  && useradd --system --uid 65532 --no-create-home sillon \
  && mkdir -p /tmp/sillon && chown sillon /tmp/sillon
COPY --from=build /src/target/release/sillon /app/sillon
COPY plbls.env /etc/plbls.env
USER 65532
ENV SILLON_CACHE=/tmp/sillon
EXPOSE 8080
ENTRYPOINT ["/app/sillon"]
