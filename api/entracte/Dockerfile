# Contexte de build = racine du dépôt (docker build -f api/entracte/Dockerfile .), fichiers envoyés : ../../.dockerignore.
# Rust : version de ../../rust-toolchain.toml. L'image d'exécution porte ffmpeg et ffprobe (analyse, transcodage HLS, sous-titres)
# et les pilotes VAAPI libres (Intel, AMD) ; NVENC demande en plus le runtime NVIDIA de Docker (docs/entracte/transcodage.md).
FROM rust:1.99.0-slim-trixie AS chef
RUN cargo install cargo-chef --locked --version 0.1.78
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json -p entracte
COPY . .
RUN cargo build --release --locked -p entracte

FROM debian:trixie-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends ffmpeg mesa-va-drivers intel-media-va-driver ca-certificates \
  && rm -rf /var/lib/apt/lists/* \
  && useradd --system --uid 65532 --no-create-home entracte \
  && mkdir -p /tmp/entracte && chown entracte /tmp/entracte
COPY --from=build /src/target/release/entracte /app/entracte
COPY plbls.env /etc/plbls.env
USER 65532
ENV ENTRACTE_TRANSCODE_DIR=/tmp/entracte
EXPOSE 8080
ENTRYPOINT ["/app/entracte"]
