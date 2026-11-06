# Image du job « windows » de la CI frontend : outils de compilation croisée déjà installés (EST-3).
# Publiée par .github/workflows/estompe-ci-image.yml sous ghcr.io/plblsfr/estompe-ci:4 (et :latest). Le runner garde `latest` en cache et ne le retire pas : à chaque changement
# de ce Dockerfile, incrémenter le numéro ici, dans ci-image.yml et dans livraison-app.yml pour forcer le tirage.
# :4 : SDK et CRT Windows (cargo-xwin) préchargés dans l'image, plus de téléchargement (~2 min) à chaque build.
# :3 (EST-18) : en plus de mingw (build normal), cible MSVC avec cargo-xwin pour le build « alpha » (libwebrtc du SDK LiveKit n'existe pas pour mingw).
# trixie : le paquet nsis de bookworm (3.08) n’a pas Win\RestartManager.nsh, requis par l’installeur Tauri (3.10 ici).
FROM node:26-trixie
# libayatana-appindicator3-dev : le CLI Tauri le détecte même en compilation croisée Windows (« Can't detect any appindicator library »)
RUN apt-get update -q && apt-get install -y -q --no-install-recommends nsis unzip mingw-w64 clang lld llvm libayatana-appindicator3-dev curl ca-certificates git \
    && rm -rf /var/lib/apt/lists/*
# Chemins fixes (hors ~) : ce sont ceux que met en cache le workflow.
# llvm-lib et lld-link : exigés par cc-rs et le lien MSVC ; XWIN_CACHE_DIR : SDK et CRT Windows
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo XWIN_CACHE_DIR=/opt/xwin PATH=/opt/cargo/bin:/usr/lib/llvm-19/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --target x86_64-pc-windows-gnu --target x86_64-pc-windows-msvc \
    && cargo install cargo-xwin --locked
# Précharge le SDK et le CRT Windows : une crate vide compilée pour MSVC déclenche le téléchargement de cargo-xwin dans /opt/xwin
RUN cargo init --lib /tmp/xwin-warm && cd /tmp/xwin-warm && cargo xwin build --target x86_64-pc-windows-msvc && rm -rf /tmp/xwin-warm
