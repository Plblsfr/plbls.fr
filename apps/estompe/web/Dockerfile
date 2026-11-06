# Contexte de build : la racine du dépôt (docker build -f web/Dockerfile .)
FROM nginx:1-alpine
COPY web/docker/default.conf /etc/nginx/conf.d/default.conf
COPY web/docker/40-estompe-config.sh /docker-entrypoint.d/40-estompe-config.sh
RUN chmod +x /docker-entrypoint.d/40-estompe-config.sh
# Front construit une seule fois par la CI (npm run build dans frontend/) ; en local, le lancer avant docker build
COPY frontend/dist/ /usr/share/nginx/html/
RUN ! grep -rlq __TAURI_INTERNALS__ /usr/share/nginx/html || (echo 'frontend/dist est un build Tauri, pas le build web' >&2; exit 1)
# Installeur Windows signé et latest.json : produits par la CI avant le build de l'image (vide en local)
COPY web/dist-updates/ /usr/share/nginx/updates/
EXPOSE 80
