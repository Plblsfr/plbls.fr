# CDN de plbls-ui : nginx qui sert dist/ sous /ui/<version>/ (immuable), /ui/<majeure>/ et /ui/latest/.
# Contexte de build : la racine du dépôt (docker build -f packages/ui/Dockerfile .), pour le package-lock.json commun.
FROM node:26-alpine AS build
WORKDIR /repo
COPY package.json package-lock.json ./
COPY packages/ui/package.json packages/ui/
COPY packages/deepmerge/package.json packages/deepmerge/
COPY web/estompe/package.json web/estompe/
RUN npm ci --workspace packages/ui
COPY packages/ui/scripts packages/ui/scripts
COPY packages/ui/src packages/ui/src
WORKDIR /repo/packages/ui
RUN npm run build && node -p "require('./package.json').version" > VERSION

FROM nginxinc/nginx-unprivileged:1.31-alpine@sha256:b5047b9b76845eac124358373c6852377c52e4b05834a47647e955dd91cb8f1e
USER root
COPY packages/ui/docker/nginx.conf /etc/nginx/conf.d/default.conf
COPY --chmod=755 packages/ui/docker/40-publier.sh /docker-entrypoint.d/40-publier.sh
RUN mkdir -p /srv && chown 101:0 /srv
# Contenu de cette image ; le script le copie dans /srv (volume) sans toucher aux versions déjà publiées.
COPY --from=build /repo/packages/ui/dist /opt/cdn/dist
COPY --from=build /repo/packages/ui/VERSION /opt/cdn/VERSION
USER 101
EXPOSE 8080
