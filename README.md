# plbls.fr
Monorepo : `apps/` (produits), `packages/` (bibliothèques partagées), `.github/workflows/` (CI).
L'infrastructure (Keycloak, PostgreSQL, stack média) est dans un dépôt privé.

## npm

Un seul workspace npm à la racine (`packages/ui`, `apps/estompe/frontend`, `apps/permission/frontend`, `apps/pli`), un seul `package-lock.json`
et une seule version de Node, celle de `.nvmrc` (reprise par les `engines`, la CI et les images Docker ; `node scripts/check-node.mjs` le vérifie).

```sh
nvm use
npm ci
npm run lint        # chaque projet a les mêmes scripts : lint, check, test, build
npm run check       # inclut check-node et ui:check
npm test            # pli demande TEST_DATABASE_URL pour ses tests avec base
npm run build
npm run ui:sync     # après un changement de packages/ui : recopie @plbls/ui dans estompe, permission et pli
```

Les copies de `@plbls/ui` (`*/vendor/plbls-ui/`) sont générées par `scripts/sync-ui.mjs` : ne pas les modifier à la main,
le workflow `workspace-ci` échoue si l'une d'elles diffère de `packages/ui`. Les images de pli, permission et du CDN se construisent
depuis la racine du dépôt (`docker build -f apps/pli/Dockerfile .`).
