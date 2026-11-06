# plbls.fr
Monorepo :

| Dossier | Contenu |
|---|---|
| `api/` | API et services Rust (workspace Cargo racine) : `security` (auth-gateway), `permission`, `pli`, `estompe` |
| `web/` | fronts servis dans les navigateurs (workspace npm racine) : `estompe` (et sa plateforme web, image nginx), `permission`, `pli` |
| `apps/` | applications natives : `estompe/windows`, `estompe/android` (Tauri) et leur code commun `estompe/crates` |
| `docs/` | documentation, un dossier par projet (`docs/<projet>/`) et les documents transverses |
| `deploy/` | déploiement et dev local, un dossier par projet : compose, `.env.example`, `dev.env`, realm Keycloak de dev |
| `scripts/` | outillage commun à la racine, scripts d'un projet dans `scripts/<projet>/` |
| `packages/` | bibliothèques partagées (`ui`) |

Les fichiers transverses d'Estompe sont à la racine : `.version`, `CHANGELOG.md`, `changelog.d/`, `agents/skills/` (lus via `.claude/skills`).
La CI est dans `.github/workflows/`.
L'infrastructure (Keycloak, PostgreSQL, stack média) est dans un dépôt privé.
Plan de regroupement des fronts en une seule application : [docs/front-unique.md](docs/front-unique.md).

## npm

Un seul workspace npm à la racine (`packages/ui`, `web/estompe`, `web/permission`, `web/pli`), un seul `package-lock.json`
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
le job `front` du workflow `ci` échoue si l'une d'elles diffère de `packages/ui`. Les images de pli, permission et du CDN se construisent
depuis la racine du dépôt (`docker build -f api/pli/Dockerfile .`).

## Dépôt GitHub

Les réglages du dépôt public se posent avec `scripts/configurer-github.sh` (`gh` connecté avec un compte admin du dépôt), relançable sans effet de bord :

- fusion en merge classique seulement, avec le message par défaut (`Merge pull request #N from <compte>/<branche>`, lu par le calcul de version d'Estompe) ;
- ruleset `main` : PR obligatoire, ni suppression ni force-push, contrôles requis `ci` (job final de `ci.yml`) et `conventions`, branche à jour avant fusion ;
- ruleset `tags de livraison` (`*-v*`) : création, déplacement et suppression réservés aux admins, releases immuables ;
- Actions : `GITHUB_TOKEN` en lecture par défaut, approbation des workflows de tout contributeur externe, actions épinglées par SHA ;
- environnement `livraison` (branche `main` et tags `*-v*`) pour les secrets de signature et le webhook Dokploy ;
- signalement privé des failles, alertes Dependabot, secret scanning avec push protection, CodeQL.

Le contrôle `conventions` (`pr-conventions.yml`, `scripts/ci-conventions-pr.mjs`) refuse une PR dont la branche n'est pas
`<type>/<slug-court>` (type parmi `feat`, `fix`, `chore`, `docs`, `ci` ; slug en minuscules séparé par des tirets, sans numéro de ticket)
ou `release/api-vA.B.C-app-vX.Y.Z` / `release/<composant>-vX.Y.Z`, et une PR sans description. Les branches `dependabot/` sont dispensées ;
modifier la description relance le contrôle.

Ni historique linéaire (incompatible avec le merge classique) ni commits signés obligatoires (le commit de version d'Estompe, job `estompe-version` de `livraison.yml`, n'est pas signé).

Toute PR lance un seul workflow, `ci.yml`, sans filtre `paths` (un contrôle obligatoire jamais créé bloquerait la PR) : `changements`
(`_changements.yml`, qui appelle `scripts/ci-changements.sh` par groupe de chemins) dit ce que la PR touche, les jobs ne tournent que pour
ce qui est touché, et le job final `ci` résume les autres. Après la fusion, `livraison.yml` publie seulement ce qui a changé, sans rejouer les
tests. Un nouveau projet ajoute son groupe de chemins à `_changements.yml` et son job à `ci.yml` (dans les `needs` de `ci`) : le ruleset ne change pas.
Détail : [`docs/estompe/ci.md`](docs/estompe/ci.md).

## Outillage

Le [`justfile`](justfile) est le seul point d'entrée (`just` liste les recettes) : `just lint` (Biome, shellcheck, ruff, liens Markdown internes),
`just check`, `just test`, et les recettes de dev de chaque projet (`just security-dev up`, `just permission-dev-run`…).
Les consignes pour les agents et les contributeurs sont dans [`AGENTS.md`](AGENTS.md) ; un projet peut le compléter par son propre `AGENTS.md`.
