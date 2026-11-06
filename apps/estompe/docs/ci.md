# CI / CD (GitHub Actions)

État actuel : chaque PR lance `estompe-backend-rs`, `estompe-frontend` et `estompe-changelog` (les jobs ne tournent que si la PR touche leurs dossiers, un push annule le run précédent) ; la fusion dans `main` lance `estompe-version`, `estompe-image` (image `edge` si l'API change) et `estompe-ci-image` (si l'image de job change).
Les livraisons (`estompe-image.yml` en `latest` et `X.Y.Z`, `estompe-livraison-app.yml`) partent sur leur tag et créent la release GitHub. Tout reste lançable à la main (`workflow_dispatch`).
Runners GitHub (`ubuntu-latest`) ; une PR d'API crée les jobs `changements`, `lint`, `architecture`, `test`, `image` puis `estompe-backend-rs` ; l'image y est construite sans être publiée.

Un seul bloc `on:` par fichier : garder `workflow_dispatch:` en plus. Les groupes `concurrency` sont déjà en place : sur une PR, un nouveau push
annule le run précédent (`cancel-in-progress` ne vaut `true` que pour `pull_request`) ; les livraisons et `version` ne sont jamais annulés.

## Déclencheurs (déjà écrits dans chaque fichier)

| Workflow | Quand | Bloc `on:` |
|---|---|---|
| `estompe-backend-rs.yml` | toute PR ; jobs lancés si elle touche l'API Rust ou ce qu'elle lit | `pull_request:` ; chemins passés à `scripts/ci-changements.sh` dans le job `changements` |
| `estompe-frontend.yml` | toute PR ; jobs lancés si elle touche le front, les apps ou le code Tauri commun | `pull_request:` ; chemins passés à `scripts/ci-changements.sh` dans le job `changements` |
| `estompe-changelog.yml` | toute PR ; entrée exigée si elle touche `apps/estompe/` | `pull_request: { types: [opened, synchronize, reopened] }` |
| `estompe-version.yml` | fusion dans `main` | `push: { branches: [main], paths: ["apps/estompe/**", "!apps/estompe/docs/**", "!apps/estompe/**.md", "!.github/**"] }` (le job exige `refs/heads/main` et ignore `[skip ci]`) |
| `estompe-ci-image.yml` | `deploy/ci/` change (image de job) | `push: { branches: [main], paths: ["apps/estompe/deploy/ci/**", ".github/workflows/estompe-ci-image.yml"] }` ; incrémenter `:4` dans ce fichier, `deploy/ci/Dockerfile` et `estompe-livraison-app.yml` |
| `estompe-image.yml` | fusion dans `main` touchant l'API (image `edge`) ; tag `estompe-api-v*` | `push: { branches: [main], paths: ["apps/estompe/backend/**", "!apps/estompe/backend/**.md", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".dockerignore"], tags: ["estompe-api-v*"] }` |
| `estompe-livraison-app.yml` | tag `estompe-app-v*` | `push: { tags: ["estompe-app-v*"] }` |

Modèle commun des workflows de PR (job `changements`, dernier job nommé comme le workflow) et contrôles obligatoires du ruleset `main` :
[`AGENTS.md`](../../../AGENTS.md) et [`README.md`](../../../README.md) à la racine. Pour `scripts/ci-changements.sh`, préfixe `!` pour exclure,
syntaxe glob de git (`**/*.md` et non `**.md`) ; hors `pull_request`, tout est lancé.

## Ce que fait chaque workflow

- `estompe-backend-rs.yml` : `lint` (`_rust-lint.yml`, commun à security et permission : toolchain de `rust-toolchain.toml`, version de Rust des Dockerfiles, fmt, clippy de tout le workspace racine, cargo-deny), `architecture` (le cœur sans bloc), `test` (crates d'Estompe) et `image` (contexte : racine du dépôt) en parallèle. Déclenché aussi par `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml` et `.dockerignore` racines. `image` construit l'image sans la publier (`:buildcache` en lecture seule, sans login), la démarre à côté d'un conteneur PostgreSQL et attend `GET /readyz` (60 s au plus : 200 seulement une fois la base ouverte et les migrations appliquées). Conteneurs et tag portent l'identifiant du run, pour que deux PR en parallèle ne se marchent pas dessus. Le cache `:buildcache` n'est alimenté que par `estompe-image.yml` (fusion dans `main` qui touche l'API, ou tag `estompe-api-v*`) : tant qu'elle n'a pas suivi ce changement, et après chaque modification de `Cargo.lock`, `image` compile toutes les dépendances à froid en release. Tests Rust sur le PostgreSQL du service `postgres` (`TEST_DATABASE_URL`) : un seul serveur partagé,
  une base par test, au lieu d'un conteneur par test. Si le runner n'atteint pas ce service par le nom `postgres` (runner sans conteneur de job), retirer `services` et
  `TEST_DATABASE_URL` : les tests retombent sur testcontainers, qui demande l'accès au socket Docker.
- `estompe-frontend.yml` : `front` (`npm ci` à la racine du dépôt, audit du workspace, `npm run check`, `npm test`, build depuis `frontend/`) et `rust` (tests des apps Tauri, GTK/WebKit requis).
- `estompe-changelog.yml` : exige un nouveau `changelog.d/*.md` ; exempté pour `ci/`, `docs/`, `release/`. Ignoré hors `pull_request` (`head_ref` et `base_ref` n'existent qu'en PR).
- `estompe-version.yml` : calcule `.version` (nom de branche lu dans le message du commit de fusion, voir `AGENTS.md`) puis le pousse sur la branche `ci/version-<sha>` et ouvre une PR « chore(version) » (`main` est protégée : pas de push direct). Il faut fusionner cette PR avant de poser un tag `estompe-api-v*` / `estompe-app-v*`. Secret `CI_PAT` pouvant pousser une branche et créer une PR.
  `backend/` fait monter `api`. Relancé à la main sur `main`, il ne fait rien de plus qu'une fois : si la branche existe déjà ou si `.version` ne change pas, il sort sans rien faire.
- `estompe-image.yml` : seule publication de l'image de l'API `ghcr.io/plblsfr/estompe`, construite depuis `backend/` (Rust), cache d'image `:buildcache` dans le registre : `edge` et `sha-…` à chaque fusion dans `main` qui touche l'API ; sur un tag `estompe-api-vX.Y.Z`, `latest` et `X.Y.Z` en plus, puis la release GitHub (notes générées).
- `estompe-livraison-app.yml` : tag `estompe-app-vX.Y.Z` → `front` (garde), installeur Windows signé (image de job `ghcr.io/plblsfr/estompe-ci`), APK (variable `ANDROID_APK=1`), puis image `ghcr.io/plblsfr/estompe-web` et release GitHub. Les jobs `windows` et `android` tournent dans l'environnement `livraison`, qui porte les clés de signature.
- `estompe-ci-image.yml` : publie l'image de job.

## Caches

- Cargo : `actions/cache/restore` puis `save` explicites, clé sur `Cargo.lock` avec repli sur le préfixe (compilation incrémentale depuis le dernier cache), un cache par job
  (`backend-lint-`, `backend-test-`).
  La sauvegarde n'a lieu que sur un cache exact manquant, donc un cache de `Cargo.lock` donné n'est écrit qu'une fois. `CARGO_INCREMENTAL=0` réduit la taille des caches.
- npm : `actions/setup-node` avec `node-version-file: .nvmrc` et `cache: npm` (un seul `package-lock.json`, à la racine du dépôt). Le workflow commun `workspace-ci.yml` vérifie aussi la version de Node et les copies de `@plbls/ui`.
- Docker : cache de couches dans le registre (`:buildcache`), lu par la CI sans publication. Le `Dockerfile` Rust compile les dépendances dans leur couche (`cargo-chef`) : un changement de source
  ne recompile que les crates `estompe-*`.
- sccache non retenu : ajoute un service à héberger pour un gain que le cache de `target/` donne déjà.
