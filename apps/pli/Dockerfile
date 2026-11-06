# Contexte de build : la racine du dépôt (docker build -f apps/pli/Dockerfile .), pour le package-lock.json commun.
FROM node:26-alpine
WORKDIR /app
COPY package.json package-lock.json ./
COPY packages/ui/package.json packages/ui/
COPY apps/estompe/frontend/package.json apps/estompe/frontend/
COPY apps/permission/frontend/package.json apps/permission/frontend/
COPY apps/pli/package.json apps/pli/
RUN npm ci --omit=dev --workspace apps/pli && npm cache clean --force
COPY apps/pli/server/ apps/pli/server/
COPY apps/pli/public/ apps/pli/public/
WORKDIR /app/apps/pli
ENV PORT=8080
EXPOSE 8080
USER node
CMD ["node", "server/index.js"]
