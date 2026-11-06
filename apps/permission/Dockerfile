# Contexte de build : la racine du dépôt (docker build -f apps/permission/Dockerfile .), pour le package-lock.json commun
# et le workspace Cargo. Fichiers envoyés : Dockerfile.dockerignore. Rust : version de ../../rust-toolchain.toml.
FROM node:26-slim AS front
WORKDIR /repo
COPY package.json package-lock.json ./
COPY packages/ui/package.json packages/ui/
COPY apps/estompe/frontend/package.json apps/estompe/frontend/
COPY apps/permission/frontend/package.json apps/permission/frontend/
COPY apps/pli/package.json apps/pli/
RUN npm ci --no-audit --no-fund --workspace apps/permission/frontend
COPY apps/permission/frontend apps/permission/frontend/
RUN npm run build --workspace apps/permission/frontend

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

# Tests unitaires : docker build -f apps/permission/Dockerfile --target test . (utilisé par la CI)
FROM build AS test
RUN cargo test --release --locked -p permission

FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /app
COPY --from=build /src/target/release/permission /app/permission
COPY apps/permission/backend/config /app/config
COPY --from=front /repo/apps/permission/frontend/dist /app/static
EXPOSE 8080
ENTRYPOINT ["/app/permission"]
