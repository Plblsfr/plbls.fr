# Contexte de build : web/estompe, et la racine du dépôt nommée « racine » pour plbls.env
# (docker build --build-context racine=. -f web/estompe/plateforme/Dockerfile web/estompe)
FROM nginxinc/nginx-unprivileged:1.31-alpine@sha256:b5047b9b76845eac124358373c6852377c52e4b05834a47647e955dd91cb8f1e
USER root
COPY plateforme/docker/default.conf /etc/nginx/conf.d/default.conf
COPY --chmod=755 plateforme/docker/40-estompe-config.sh /docker-entrypoint.d/40-estompe-config.sh
COPY --from=racine plbls.env /etc/plbls.env
# Front construit une seule fois par la CI (npm run build dans web/estompe) ; en local, le lancer avant docker build
COPY dist/ /usr/share/nginx/html/
RUN ! grep -rlq __TAURI_INTERNALS__ /usr/share/nginx/html || (echo 'web/estompe/dist est un build Tauri, pas le build web' >&2; exit 1)
# Installeur Windows signé et latest.json : produits par la CI avant le build de l'image (vide en local)
COPY plateforme/dist-updates/ /usr/share/nginx/updates/
RUN touch /usr/share/nginx/html/config.js /etc/nginx/estompe-headers.inc && chown 101:0 /usr/share/nginx/html/config.js /etc/nginx/estompe-headers.inc
USER 101
EXPOSE 8080
