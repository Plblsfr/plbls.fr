# CDN de plbls-ui : nginx qui sert dist/ sous /ui/<version>/ (immuable), /ui/<majeure>/ et /ui/latest/.
# Contexte de build : la racine du dépôt (docker build -f packages/ui/Dockerfile .), pour le package-lock.json commun.
FROM node:26-alpine AS build
WORKDIR /repo
COPY package.json package-lock.json ./
COPY packages/ui/package.json packages/ui/
COPY web/estompe/package.json web/estompe/
COPY web/permission/package.json web/permission/
COPY web/pli/package.json web/pli/
RUN npm ci --workspace packages/ui
COPY packages/ui/scripts packages/ui/scripts
COPY packages/ui/src packages/ui/src
WORKDIR /repo/packages/ui
RUN npm run build && node -p "require('./package.json').version" > VERSION

FROM nginx:1.31-alpine
COPY packages/ui/docker/nginx.conf /etc/nginx/conf.d/default.conf
COPY packages/ui/docker/40-publier.sh /docker-entrypoint.d/40-publier.sh
RUN chmod +x /docker-entrypoint.d/40-publier.sh
# Contenu de cette image ; le script le copie dans /srv (volume) sans toucher aux versions déjà publiées.
COPY --from=build /repo/packages/ui/dist /opt/cdn/dist
COPY --from=build /repo/packages/ui/VERSION /opt/cdn/VERSION
EXPOSE 80
