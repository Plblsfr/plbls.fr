# AGENTS.md

Monorepo plbls.fr (détail dans `README.md`) : `api/<projet>` (API Rust), `web/<projet>` (fronts servis dans les navigateurs), `apps/` (apps natives),
`docs/<projet>`, `deploy/<projet>`, `scripts/<projet>` (outillage commun directement dans `scripts/`), `packages/` (libs partagées), `.github/workflows/` (CI).
Un fichier va dans le dossier de son rôle, jamais dans celui d'un autre projet : par exemple un `.env.example` dans `deploy/<projet>/`, pas à côté du code.
Déploiement : deux compose pour tout le dépôt, `deploy/docker-compose.dev.yml` (dépendances de dev, variables dans `deploy/dev.env`) et
`deploy/docker-compose.yml` (production, une seule stack Dokploy : les backends Rust chargent le `.env` que Dokploy écrit depuis son onglet Environment, secrets résolus depuis Vault, les autres services ne reçoivent que leurs variables par `environment:` interpolé `${VAR}`, jamais de valeur écrite dans le compose ; un nom de variable n'appartient qu'à un projet, sauf les communes de `deploy/.env.example`) ; voir `deploy/README.md`.
L'infrastructure (Keycloak, PostgreSQL, stack média) est dans le dépôt privé `plbls.fr-infra`.

## Git
- **Ne jamais ajouter de ligne `Co-Authored-By: Claude …` dans les messages de commit.**
- Ne pas ajouter non plus de mention « Generated with Claude Code » dans les commits.
- Ne jamais forcer un push ; ne pas pousser directement sur `main`, passer par une PR.
- Tags de livraison : `<composant>-vX.Y.Z` (`security`, `aegis`, `entracte`, `ui`, `estompe-api`, `estompe-app` ; `keycloak` dans `plbls.fr-infra`).

## CI
- Un seul workflow de PR, `ci.yml`, sans filtre `paths` : `changements` (`_changements.yml`, une sortie par groupe de chemins passés à
  `scripts/ci-changements.sh`), puis des jobs conditionnés à ces sorties (`front` générique pour les projets npm, `rust-lint` une fois pour tout
  le workspace, `rust-test`, `estompe-tauri`, images), et le job final `ci`, contrôle obligatoire. Nouveau projet : un groupe de chemins dans
  `_changements.yml`, un job (ou un espace npm pour `front`) dans `ci.yml`, ajouté aux `needs` de `ci` ; le ruleset ne change pas.
- Pas de tests sur `main` : `livraison.yml` (push `main`, tags `<composant>-vX.Y.Z`) publie seulement ce que la fusion a changé (images `:edge`,
  PR de version d'Estompe, caches partagés). Les apps natives d'Estompe (`estompe-livraison-app.yml`) partent de leur tag ou d'un lancement manuel.
  `estompe-beta.yml` (lancement manuel) publie le front d'une PR en image `estompe-web:beta`, sans tag ni release.
- Contrôles obligatoires du ruleset (`scripts/configurer-github.sh`) : `ci` et `conventions`.
- Toute PR : branche `<type>/<slug-court>` (`feat`, `fix`, `chore`, `docs`, `ci`) ou `release/…`, et une description ; vérifié par `pr-conventions.yml`.
- Actions épinglées par SHA complet (`uses: owner/action@<sha> # vX.Y.Z` ; `# stable` pour `dtolnay/rust-toolchain`, qui ne publie pas de tag `vX.Y.Z` : une branche par toolchain) ; `permissions: contents: read` en tête, droits d'écriture au seul job qui publie.
- Détail (déclencheurs, caches, jobs) : `docs/estompe/ci.md`.

## Rust
- Workspace Cargo unique à la racine (`Cargo.toml`) : membres `api/commun` (code partagé : en-têtes d'identité signés, jetons de service, client HTTP sans redirection, manifeste aegis, échappement `LIKE`), `api/security`, `api/aegis`, `api/entracte`, `api/estompe/*`.
  Un seul `Cargo.lock`, un seul `rust-toolchain.toml` (version aussi dans les `FROM rust:` des Dockerfiles, vérifiée par la CI), un seul `deny.toml`.
- Versions et features communes dans `[workspace.dependencies]` ; un membre écrit `dep.workspace = true` (features en plus si besoin) et `[lints] workspace = true`.
- Nouveau backend : `api/<nom>`, ajouté à `members`. `.dockerignore` racine et le `Dockerfile.dockerignore` d'aegis envoient déjà tout `api/` ; le groupe `rust` de `.github/workflows/_changements.yml`
  couvre `api/**`, lui ajouter ses groupes d'image et de tests.
- Exception : les apps Tauri d'Estompe (`apps/estompe/{crates,windows,android}`) restent hors du workspace, avec leur `Cargo.lock` et la toolchain stable.
- Contrôles : `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo deny check`.

Chaque projet peut avoir son propre `AGENTS.md` dans `docs/<projet>/` (par exemple `docs/estompe/AGENTS.md`) : il complète celui-ci pour tous les dossiers du projet.
Il n'est pas chargé automatiquement : avant de toucher à un dossier d'Estompe (`*/estompe/`, `.version`, `changelog.d/`, `agents/skills/`), lire `docs/estompe/AGENTS.md`.

## Qualité du code non Rust (lint, types, tests)
Mêmes commandes en local et en CI (job `front` de `ci.yml`, lint lancé selon les chemins touchés) : `just lint`, `just check`, `just test`. Le `justfile` racine est le seul
point d'entrée de l'outillage (pas de Makefile par projet) : une commande de dev d'un projet y devient une recette `<projet>-…`.
- **Format** : `.editorconfig` à la racine (UTF-8, LF, 2 espaces ; 4 pour Rust et Python).
  Le formateur de Biome n'est pas imposé : le code existant est mis en forme à la main, en lignes denses, et le formater réécrirait
  des dizaines de milliers de lignes. Pour l'imposer à un dossier, l'activer dans `overrides` de `biome.json`.
- **JS, TS, CSS, JSON** : Biome (version épinglée dans le `package.json` racine, config `biome.json`), règles recommandées moins celles
  qui contredisent le style du dépôt (`!`, `any`, `!important`…). `npm run lint` à la racine vérifie tout le dépôt. Exclus : copies de
  `@plbls/ui` (`vendor/plbls-ui`), fichiers générés ou tiers (`src-tauri/gen`, mediapipe).
- **Svelte** : `svelte-check` (`npm run check` du projet). **Types** : chaque frontal étend `tsconfig.base.json` (strict, ES2022).
- **Tests unitaires** : vitest dans chaque frontal et dans `packages/ui` (`src/**/*.test.ts` ou à côté du fichier testé) ;
  **e2e** : Playwright dans le projet testé (`<frontal>/e2e/*.spec.ts`, `playwright.config.ts`, `npm run e2e`).
- **Scripts shell** : `#!/usr/bin/env bash` puis `set -euo pipefail` ; `#!/bin/sh` puis `set -eu` seulement pour ce qui tourne dans
  une image Alpine ou nginx. `shellcheck` doit passer (`scripts/lint-shell.sh` vérifie aussi le shebang et la ligne `set`).
- **Documentation** : décrit l'état actuel (l'historique est dans git, pas de document de migration terminé). Les liens Markdown internes
  sont vérifiés par `scripts/check-links.mjs` (`just lint` ; en PR, job `front` de `ci.yml` dès qu'un `.md` change) ; `CHANGELOG.md`, `changelog.d/` et les copies `vendor/` sont exclus.
- **Autres scripts** : bash pour l'orchestration, `.mjs` quand le projet est déjà en Node. Pas de nouveau script Python.
- **Python (décision)** : les scripts Python existants (tests de bout en bout de `security`)
  sont gardés : stdlib seulement, aucune dépendance, ils marchent. Le code commun (HTTP sans redirection, appels JSON, vérifications)
  est dans `scripts/python/devcommun.py`, importé par `scripts/security/devlib.py`. Ils sont vérifiés par `ruff` (config `ruff.toml`,
  version épinglée dans `.github/workflows/ci.yml`). Un script réécrit passe en `.mjs` ou en test Rust du projet.
