# Contexte de build : web/estompe (docker build -f web/estompe/plateforme/Dockerfile web/estompe)
FROM nginx:1-alpine
COPY plateforme/docker/default.conf /etc/nginx/conf.d/default.conf
COPY plateforme/docker/40-estompe-config.sh /docker-entrypoint.d/40-estompe-config.sh
RUN chmod +x /docker-entrypoint.d/40-estompe-config.sh
# Front construit une seule fois par la CI (npm run build dans web/estompe) ; en local, le lancer avant docker build
COPY dist/ /usr/share/nginx/html/
RUN ! grep -rlq __TAURI_INTERNALS__ /usr/share/nginx/html || (echo 'web/estompe/dist est un build Tauri, pas le build web' >&2; exit 1)
# Installeur Windows signé et latest.json : produits par la CI avant le build de l'image (vide en local)
COPY plateforme/dist-updates/ /usr/share/nginx/updates/
EXPOSE 80
