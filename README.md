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

## Dépôt GitHub

Les réglages du dépôt public se posent avec `scripts/configurer-github.sh` (`gh` connecté avec un compte admin du dépôt), relançable sans effet de bord :

- fusion en merge classique seulement, avec le message par défaut (`Merge pull request #N from <compte>/<branche>`, lu par le calcul de version d'Estompe) ;
- ruleset `main` : PR obligatoire, ni suppression ni force-push, contrôles requis `npm`, `changelog`, `conventions`, `estompe-backend-rs`, `estompe-frontend`,
  `pli-ci`, `permission-ci`, `security-ci`, `ui-ci`, branche à jour avant fusion ;
- ruleset `tags de livraison` (`*-v*`) : création, déplacement et suppression réservés aux admins, releases immuables ;
- Actions : `GITHUB_TOKEN` en lecture par défaut, approbation des workflows de tout contributeur externe, actions épinglées par SHA ;
- environnement `livraison` (branche `main` et tags `*-v*`) pour les secrets de signature et le webhook Dokploy ;
- signalement privé des failles, alertes Dependabot, secret scanning avec push protection, CodeQL.

Le contrôle `conventions` (`pr-conventions.yml`, `scripts/ci-conventions-pr.mjs`) refuse une PR dont la branche n'est pas
`<type>/<slug-court>` (type parmi `feat`, `fix`, `chore`, `docs`, `ci` ; slug en minuscules séparé par des tirets, sans numéro de ticket)
ou `release/api-vA.B.C-app-vX.Y.Z` / `release/<composant>-vX.Y.Z`, et une PR sans description. Les branches `dependabot/` sont dispensées ;
modifier la description relance le contrôle.

Ni historique linéaire (incompatible avec le merge classique) ni commits signés obligatoires (le commit de `estompe-version` n'est pas signé).

Un workflow de PR n'a pas de filtre `paths` (un contrôle obligatoire jamais créé bloquerait la PR) : son premier job `changements` appelle
`scripts/ci-changements.sh` avec les chemins suivis, et son dernier job, nommé comme le workflow, résume les autres. Un nouveau workflow
suit ce modèle et ajoute son contrôle à `scripts/configurer-github.sh`.
