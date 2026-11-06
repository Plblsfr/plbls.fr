# Contexte de build : la racine du dépôt (docker build -f api/permission/Dockerfile .), pour le package-lock.json commun
# et le workspace Cargo. Fichiers envoyés : Dockerfile.dockerignore. Rust : version de ../../rust-toolchain.toml.
FROM node:26-slim AS front
WORKDIR /repo
COPY package.json package-lock.json ./
COPY packages/ui/package.json packages/ui/
COPY web/estompe/package.json web/estompe/
COPY web/permission/package.json web/permission/
COPY web/pli/package.json web/pli/
RUN npm ci --no-audit --no-fund --workspace web/permission
COPY tsconfig.base.json ./
COPY web/permission web/permission/
RUN npm run build --workspace web/permission

FROM rust:1.99.0-slim AS chef
RUN cargo install cargo-chef --locked --version 0.1.78
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json -p permission
COPY . .
RUN cargo build --release --locked -p permission

# Tests unitaires : docker build -f api/permission/Dockerfile --target test . (utilisé par la CI)
FROM build AS test
RUN cargo test --release --locked -p permission

FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
COPY --from=build /src/target/release/permission /app/permission
COPY api/permission/config /app/config
COPY --from=front /repo/web/permission/dist /app/static
EXPOSE 8080
ENTRYPOINT ["/app/permission"]
