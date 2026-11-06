# AGENTS.md

Monorepo plbls.fr : `apps/` (produits), `packages/` (libs partagées), `.github/workflows/` (CI).
L'infrastructure (Keycloak, PostgreSQL, stack média) est dans le dépôt privé `plbls.fr-infra`.

## Git
- **Ne jamais ajouter de ligne `Co-Authored-By: Claude …` dans les messages de commit.**
- Ne pas ajouter non plus de mention « Generated with Claude Code » dans les commits.
- Ne jamais forcer un push ; ne pas pousser directement sur `main`, passer par une PR.
- Tags de livraison : `<composant>-vX.Y.Z` (`pli`, `security`, `permission`, `ui`, `estompe-api`, `estompe-app` ; `keycloak` dans `plbls.fr-infra`).

## Rust
- Workspace Cargo unique à la racine (`Cargo.toml`) : membres `apps/security`, `apps/permission/backend`, `apps/pli/server`, `apps/estompe/backend/*`.
  Un seul `Cargo.lock`, un seul `rust-toolchain.toml` (version aussi dans les `FROM rust:` des Dockerfiles, vérifiée par la CI), un seul `deny.toml`.
- Versions et features communes dans `[workspace.dependencies]` ; un membre écrit `dep.workspace = true` (features en plus si besoin) et `[lints] workspace = true`.
- Nouveau backend : l'ajouter à `members`, à `.dockerignore` racine (contexte des images Rust) et aux listes `Dockerfile.dockerignore` de pli et permission, et ses chemins (`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml`) aux `paths` de sa CI, qui appelle `_rust-lint.yml`.
- Exception : les apps Tauri d'Estompe (`apps/estompe/{crates,windows,android}`) restent hors du workspace, avec leur `Cargo.lock` et la toolchain stable.
- Contrôles : `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo deny check`.

Chaque projet peut avoir son propre `AGENTS.md` (par exemple `apps/estompe/AGENTS.md`) : il complète celui-ci pour son dossier.

## Qualité du code non Rust (lint, types, tests)
Mêmes commandes en local et en CI (workflow `lint`, déclenché par chemin) : `just lint`, `just check`, `just test` (voir le `justfile`).
- **Format** : `.editorconfig` à la racine (UTF-8, LF, 2 espaces ; 4 pour Rust et Python ; tabulations pour les Makefile).
  Le formateur de Biome n'est pas imposé : le code existant est mis en forme à la main, en lignes denses, et le formater réécrirait
  des dizaines de milliers de lignes. Pour l'imposer à un dossier, l'activer dans `overrides` de `biome.json`.
- **JS, TS, CSS, JSON** : Biome (version épinglée dans le `package.json` racine, config `biome.json`), règles recommandées moins celles
  qui contredisent le style du dépôt (`!`, `any`, `!important`…). `npm run lint` à la racine vérifie tout le dépôt. Exclus : copies de
  `@plbls/ui` (`vendor/plbls-ui`), fichiers générés ou tiers (`src-tauri/gen`, mediapipe) et `apps/pli/server` (remplacé par Rust).
- **Svelte** : `svelte-check` (`npm run check` du projet). **Types** : chaque frontal étend `tsconfig.base.json` (strict, ES2022).
- **Tests unitaires** : vitest dans chaque frontal et dans `packages/ui` (`src/**/*.test.ts` ou à côté du fichier testé) ;
  **e2e** : Playwright dans le projet testé (`<frontal>/e2e/*.spec.ts`, `playwright.config.ts`, `npm run e2e`).
- **Scripts shell** : `#!/usr/bin/env bash` puis `set -euo pipefail` ; `#!/bin/sh` puis `set -eu` seulement pour ce qui tourne dans
  une image Alpine ou nginx. `shellcheck` doit passer (`scripts/lint-shell.sh` vérifie aussi le shebang et la ligne `set`).
- **Autres scripts** : bash pour l'orchestration, `.mjs` quand le projet est déjà en Node. Pas de nouveau script Python.
- **Python (décision)** : les scripts Python existants (tests de bout en bout de `security`, `seed.py` et `smoke.py` de `permission`)
  sont gardés : stdlib seulement, aucune dépendance, ils marchent. Le code commun (HTTP sans redirection, appels JSON, vérifications)
  est dans `scripts/python/devcommun.py`, importé par le `devlib.py` de chaque projet. Ils sont vérifiés par `ruff` (config `ruff.toml`,
  version épinglée dans `.github/workflows/lint.yml`). Un script réécrit passe en `.mjs` ou en test Rust du projet.
