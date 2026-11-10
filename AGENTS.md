# AGENTS.md

Monorepo plbls.fr (détail dans `README.md`) : `api/<projet>` (API Rust), `web/<projet>` (fronts servis dans les navigateurs), `apps/` (apps natives),
`deploy/<projet>`, `scripts/<projet>` (outillage commun directement dans `scripts/`), `packages/` (libs partagées), `.github/workflows/` (CI).
Un fichier va dans le dossier de son rôle, jamais dans celui d'un autre projet : par exemple un `.env.example` dans `deploy/<projet>/`, pas à côté du code.
Pas de dossier de documentation : ce fichier, les `README.md` des projets et `deploy/README.md` suffisent ; une documentation décrit l'état actuel.
Déploiement : deux compose pour tout le dépôt, `deploy/docker-compose.dev.yml` (dépendances de dev, variables dans `deploy/dev.env`) et
`deploy/docker-compose.yml` (production, une seule stack Dokploy sans section `environment` : tous les services chargent le `.env` que Dokploy écrit depuis son onglet Environment, secrets résolus depuis Vault ; un nom de variable n'appartient qu'à un projet, sauf les communes de `deploy/.env.example`) ; voir `deploy/README.md`.
L'infrastructure (Keycloak, PostgreSQL, stack média) est dans le dépôt privé `plbls.fr-infra`.

Langue : code, commits et PR en français. Aucun commentaire dans le code.

## Git
- **Ne jamais ajouter de ligne `Co-Authored-By: Claude …` dans les messages de commit.**
- Ne pas ajouter non plus de mention « Generated with Claude Code » dans les commits.
- Une demande = une branche = une PR, créée depuis `origin/main` à jour ; ne jamais empiler une demande sur la branche d'une autre.
- Ne jamais forcer un push ; ne pas pousser directement sur `main`, passer par une PR ; fusion en « merge » classique (ni squash ni rebase).
- Messages de commit : `type(portée): résumé` (`feat`, `fix`, `chore`, `ci`, `docs`) ; release d'Estompe : `chore(release): api-vA.B.C app-vX.Y.Z`.
- Tags de livraison : `<composant>-vX.Y.Z` (`pli`, `security`, `aegis`, `ui`, `estompe-api`, `estompe-app` ; `keycloak` dans `plbls.fr-infra`).
  Jamais écrasés ni supprimés, poussés un par un (`git push origin <tag>`, jamais `--tags`).

## CI
- Un seul workflow de PR, `ci.yml`, sans filtre `paths` : `changements` (`_changements.yml`, une sortie par groupe de chemins passés à
  `scripts/ci-changements.sh`, syntaxe glob de git, `!` pour exclure), puis des jobs conditionnés à ces sorties (`front` générique pour les projets npm,
  `rust-lint` une fois pour tout le workspace, `rust-test`, `estompe-tauri`, images), et le job final `ci`, contrôle obligatoire.
  Modifier la CI elle-même relance tout ; un `*.md` seul ne lance que le lint (liens internes) et le contrôle de changelog d'Estompe.
  Nouveau projet : un groupe de chemins dans `_changements.yml`, un job (ou un espace npm pour `front`) dans `ci.yml`, ajouté aux `needs` de `ci` ; le ruleset ne change pas.
- Pas de tests sur `main` : `livraison.yml` (push `main`, tags `<composant>-vX.Y.Z`) publie seulement ce que la fusion a changé (images `:edge`,
  image versionnée et release sur tag, PR de version d'Estompe, caches partagés écrits sur `main` et lus par les PR).
  Les apps natives d'Estompe (`estompe-livraison-app.yml`) partent de leur tag ou d'un lancement manuel.
  `estompe-beta.yml` (lancement manuel sur la branche d'une PR) publie son front en image `estompe-web:beta`, sans tag ni release.
- Contrôles obligatoires du ruleset (`scripts/configurer-github.sh`) : `ci` et `conventions`. Le job `anciens-controles` recopie `ci`
  sous les noms de l'ancien ruleset, jusqu'à ce que le script soit relancé.
- Toute PR : branche `<type>/<slug-court>` (`feat`, `fix`, `chore`, `docs`, `ci`) ou `release/…`, et une description ; vérifié par `pr-conventions.yml`.
- Actions épinglées par SHA complet (`uses: owner/action@<sha> # vX.Y.Z`) ; `permissions: contents: read` en tête, droits d'écriture au seul job qui publie.
- Publication sur ghcr.io : secret `PACKAGES_PAT` s'il existe, sinon `GITHUB_TOKEN`.

## Rust
- Workspace Cargo unique à la racine (`Cargo.toml`) : membres `api/security`, `api/aegis`, `api/pli`, `api/estompe/*`.
  Un seul `Cargo.lock`, un seul `rust-toolchain.toml` (version aussi dans les `FROM rust:` des Dockerfiles, vérifiée par la CI), un seul `deny.toml`.
- Versions et features communes dans `[workspace.dependencies]` ; un membre écrit `dep.workspace = true` (features en plus si besoin) et `[lints] workspace = true`.
  Une dépendance changée à la racine touche tous les membres.
- Nouveau backend : `api/<nom>`, ajouté à `members`. `.dockerignore` racine et le `Dockerfile.dockerignore` de pli envoient déjà tout `api/` ; le groupe `rust` de `.github/workflows/_changements.yml`
  couvre `api/**`, lui ajouter ses groupes d'image et de tests.
- Exception : les apps Tauri d'Estompe (`apps/estompe/{crates,windows,android}`) restent hors du workspace, avec leur `Cargo.lock` et la toolchain stable.
- Contrôles : `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo deny check`.
- Cargo lit `.cargo/config.toml` depuis le dossier courant et ses parents : les scripts lancent cargo depuis la racine.

## Qualité du code non Rust (lint, types, tests)
Mêmes commandes en local et en CI (job `front` de `ci.yml`, lint lancé selon les chemins touchés) : `just lint`, `just check`, `just test`. Le `justfile` racine est le seul
point d'entrée de l'outillage (pas de Makefile par projet) : une commande de dev d'un projet y devient une recette `<projet>-…`.
- **Format** : `.editorconfig` à la racine (UTF-8, LF, 2 espaces ; 4 pour Rust et Python).
  Le formateur de Biome n'est pas imposé : le code existant est mis en forme à la main, en lignes denses. Pour l'imposer à un dossier, l'activer dans `overrides` de `biome.json`.
- **JS, TS, CSS, JSON** : Biome (version épinglée dans le `package.json` racine, config `biome.json`), règles recommandées moins celles
  qui contredisent le style du dépôt (`!`, `any`, `!important`…). Exclus : copies de `@plbls/ui` (`vendor/plbls-ui`), fichiers générés ou tiers (`src-tauri/gen`, mediapipe).
- **Svelte** : `svelte-check` (`npm run check` du projet). **Types** : chaque frontal étend `tsconfig.base.json` (strict, ES2022).
- **Tests unitaires** : vitest dans chaque frontal et dans `packages/ui` (`src/**/*.test.ts` ou à côté du fichier testé) ;
  **e2e** : Playwright dans le projet testé (`<frontal>/e2e/*.spec.ts`, `playwright.config.ts`, `npm run e2e`).
- **Scripts shell** : `#!/usr/bin/env bash` puis `set -euo pipefail` ; `#!/bin/sh` puis `set -eu` seulement pour ce qui tourne dans
  une image Alpine ou nginx. `shellcheck` doit passer (`scripts/lint-shell.sh` vérifie aussi le shebang et la ligne `set`).
- **Markdown** : liens internes vérifiés par `scripts/check-links.mjs` (`just lint`) ; `CHANGELOG.md`, `changelog.d/` et les copies `vendor/` sont exclus.
- **Autres scripts** : bash pour l'orchestration, `.mjs` quand le projet est déjà en Node. Pas de nouveau script Python : les existants
  (tests de bout en bout de `security`) restent en stdlib, avec le code commun dans `scripts/python/devcommun.py`, vérifiés par `ruff` (`ruff.toml`).

## Estompe

Messagerie écrite et vocale (messages privés, serveurs, salons texte et vocaux). Backend Rust, frontend Svelte 5 (web, PWA, apps Windows
et Android en Tauri), vocal LiveKit. Réparti entre `api/estompe`, `web/estompe`, `apps/estompe`, `deploy/estompe` et `scripts/estompe` ;
`.version`, `CHANGELOG.md`, `changelog.d/` et `agents/skills/` sont à la racine. Lire aussi `api/estompe/README.md` (dont « Ajouter un bloc ») et `web/estompe/README.md`.
Un changement de règle se fait ici puis dans les skills concernés, dans la même PR.

### Tickets et skills
Les skills sont dans `agents/skills/<nom>/SKILL.md` (`.claude/skills` y pointe). Un ticket est une issue GitHub de `Plblsfr/plbls.fr`.

| Skill | Produit |
|---|---|
| `/estompe-grill-me <n>` | `.ai/GH-<n>/brief.md` : questions jusqu'à lever toute ambiguïté, critères `C1`, `C2`… |
| `/estompe-plan <n>` | `.ai/GH-<n>/plan.md` : branche, étapes, tests, couverture de chaque critère |
| `/estompe-implement <n>` | worktree, commits locaux, `.ai/GH-<n>/verify.md` ; lance `scripts/estompe/check.sh` |
| `/estompe-code-review <n>` | section « Revue N » de `plan.md` ; deux revues au plus, seuls les bloquants et importants prouvés bloquent |
| `/estompe-pr <n>` | `changelog.d/<slug-court>.md`, push, PR (`Closes #<n>`), commentaire sur l'issue, suppression du worktree |
| `/estompe-release` | release, tags, release GitHub, issues livrées commentées (rouvertes s'il reste un critère non vérifié) |

- Dossiers locaux jamais commités, à la racine du dépôt principal et exclus par `.git/info/exclude` : `.worktree/<slug-court>/` (un worktree par branche,
  `git worktree add -b <type>/<slug-court> .worktree/<slug-court> origin/main`) et `.ai/GH-<n>/` (fichiers de travail d'un ticket).
- Seul `gh` (ou les outils MCP GitHub de la session) parle à GitHub ; jamais `curl` vers `api.github.com`. Pousser et ouvrir une PR seulement via `/estompe-pr`, `/estompe-release` ou sur demande.
- Un bug se reproduit (test qui échoue) avant d'être corrigé. Un critère qui dépend d'un appareil réel reste « non vérifié » tant que la personne qui l'a n'a pas confirmé.

### Version
Le préfixe de la branche pilote la version de chaque élément (`api`, `web`, `windows`, `android`) de `.version`, recalculée à la fusion
(`scripts/estompe/bump-version.mjs`, job `estompe-version` de `livraison.yml`, nom de branche lu dans le commit de fusion) :
`feat/` → minor +1, `fix/`, `chore/`, `docs/`, `ci/` et le reste → patch +1, `release/` → rien, majeure → à la main dans `.version`.
Éléments concernés : `api/estompe/` → api ; `web/estompe/plateforme/` → web ; `apps/estompe/windows/` → windows ; `apps/estompe/android/` → android ;
le reste de `web/estompe/` → web, windows et android ; `apps/estompe/crates/` → windows et android. La CI tient une PR unique `ci/version`
(« chore(version) », seul `.version` change) à fusionner avant de poser un tag. Ne pas modifier `.version` dans une autre PR, sauf version majeure.

### Release
- Branche `release/api-vA.B.C-app-vX.Y.Z` ; tag annoté sur `main` après sa fusion : `estompe-api-vX.Y.Z` (image `ghcr.io/plblsfr/estompe`),
  `estompe-app-vX.Y.Z` (image `estompe-web`, installeur Windows, APK). Procédure : `/estompe-release`.
- Chaque PR ajoute `changelog.d/<slug-court>.md` (voir `changelog.d/README.md`), sauf branches `ci/`, `docs/`, `release/` : la CI échoue sinon.
  Les fichiers sont regroupés dans `CHANGELOG.md` à la release.

### Vérifier avant de livrer
`scripts/estompe/check.sh` depuis la racine du worktree : seule liste de contrôles (fmt, clippy, tests des crates d'Estompe avec
`TEST_DATABASE_URL`, cargo deny, check et tests du front). Pendant le développement : `cargo check|test $(scripts/estompe/paquets-cargo.sh)`.
- Tests Rust : PostgreSQL de `TEST_DATABASE_URL` (superutilisateur, une base par test) ; sans elle, testcontainers, sinon tests ignorés
  (« ignoré : Docker indisponible ») : vérifier la sortie. Une base créée à côté de `testutil::postgres()` est rattachée par `BasePostgres::supprimer_aussi`.
- Front : `npm ci` à la racine, puis `npm --prefix web/estompe run check|test` (`npx --prefix` depuis la racine ne lit pas la config).
- Un changement de logique non triviale a son test, dans le fichier de test existant du code testé ; un test attend une condition, jamais un délai fixe.
- Dire ce qui n'a **pas** été vérifié (rendu visuel, build Tauri/Windows, appareil réel…) dans la réponse et dans la PR.

### Architecture
- `api/estompe/` : crates `estompe-api` (binaire), `core`, `block` et `blocks/<nom>`. `core` ne dépend d'aucun bloc ; un bloc dépend de `core`,
  de `block` et d'un autre bloc seulement s'il le déclare dans `Needs`.
- **Blocs** : un crate `api/estompe/blocks/<nom>` déclaré dans `block_list()` (`estompe-api/src/main.rs`), `BLOCK_<NOM>=false` le désactive,
  annoncé dans `blocks` de `GET /me` ; côté front `web/estompe/src/blocks/<nom>/` (`registry.ts`, `app.on('<nom>')`). Une fonctionnalité optionnelle = un bloc des deux côtés.
- Contrat : `api/estompe/openapi.yaml` (REST, comparé au code par `openapi_compatible_avec_le_contrat_go`) et `api/estompe/realtime.md`
  (évènements WebSocket) : toute nouvelle route ou nouvel évènement y est documenté.
- Droits (aegis) : toute nouvelle route est rattachée à une permission de `api/estompe/core/src/catalogue.rs` (ou à `HORS_PERMISSION` avec un motif),
  tout droit de serveur est plafonné par une permission. Après un changement du catalogue :
  `AEGIS_DOC=1 cargo test -p estompe-core catalogue::tests::documentation_a_jour` régénère `api/estompe/fonctionnalites.md`.
- `web/estompe/` : code commun à toutes les applications (`src/components`, `src/views`, état dans `src/lib`, interface `Platform`).
  Une plateforme = un dossier : `web/estompe/plateforme/` (nginx), `apps/estompe/windows/`, `apps/estompe/android/` ; code commun des apps : `apps/estompe/crates/core/`.
  Le front ne teste jamais la plateforme : il appelle `platform`.
- Déploiement : services `estompe-*` et `livekit` de `deploy/docker-compose.yml`. Toute nouvelle variable va dans `deploy/estompe/.env.example`
  (un secret : dans `secret/estompe` de Vault, référencé `${{vault.plbls.estompe:CLE}}`).

### Conventions et pièges
- Migrations : `NNN_nom.sql` (numéro suivant) dans `api/estompe/core/migrations/` ; ne jamais modifier une migration fusionnée.
- Temps réel : un changement d'état visible par d'autres passe par un évènement WebSocket (`publishMessage`) géré dans `store.svelte.ts` (`onEvent`), testé dans `store.test.ts`.
- Sécurité : valider toute entrée côté serveur (longueurs, formats, droits) ; le client n'est jamais une source de confiance.
- Interface : suit `plbls-ui` (copie dans `src/vendor/plbls-ui`) ; composants partagés obligatoires (`Modal`, `ConfirmModal`, `ContextMenu`, `Icon`, `Avatar`…),
  jamais `window.confirm`, `alert` ni `prompt` ; un menu dans un modal réutilise `ContextMenu`/`Modal`, pas de `z-index` arbitraire.
- Champs tactiles : la règle `@media (pointer: coarse)` d'`app.css` les force à 16 px ; ne jamais poser `maximum-scale` ni `user-scalable`.
- Tauri : le webview n'envoie pas les en-têtes d'un navigateur, Keycloak et le gateway passent par Rust (`native_fetch`) ; CSP dans `tauri.conf.json`.
  Origine de la webview : `https://tauri.localhost`, à tester dans l'app.
- YouTube : iframe officielle avec `referrerpolicy="strict-origin-when-cross-origin"` (sinon erreur 153).
  Lecture automatique : peut être refusée (`onAutoplayBlocked`), prévoir « Activer le son ».
- Films (Jellyfin) : les applications n'appellent que l'API ; jamais de réponse, identifiant ou chemin Jellyfin renvoyé tel quel ; tout flux passe par une
  session signée (`flux.rs`) ; extensions autorisées dans `filtre.rs` ; la demande HLS porte toujours `VideoBitrate`.
- Salons vocaux : un salon `cinema` est aussi vocal ; tester `servers::est_vocal` / `servers::TYPES_VOCAUX` et `isVoiceKind`, jamais `kind == 'voice'` seul.
- Synchronisation (regarder ensemble, musique, vocal) : aucun délai fixe, des états explicites et des évènements.
- Code minimal : réutiliser l'existant avant d'ajouter ; pas de dépendance pour quelques lignes. Rester agnostique de l'OS et de l'appareil.
