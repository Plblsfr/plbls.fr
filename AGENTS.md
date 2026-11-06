# AGENTS.md

Monorepo plbls.fr : `apps/` (produits), `packages/` (libs partagées), `.github/workflows/` (CI).
L'infrastructure (Keycloak, PostgreSQL, stack média) est dans le dépôt privé `plbls.fr-infra`.

## Git
- **Ne jamais ajouter de ligne `Co-Authored-By: Claude …` dans les messages de commit.**
- Ne pas ajouter non plus de mention « Generated with Claude Code » dans les commits.
- Ne jamais forcer un push ; ne pas pousser directement sur `main`, passer par une PR.
- Tags de livraison : `<composant>-vX.Y.Z` (`pli`, `security`, `permission`, `ui`, `estompe-api`, `estompe-app` ; `keycloak` dans `plbls.fr-infra`).

## CI
- Workflows de PR sans filtre `paths` : premier job `changements` (`scripts/ci-changements.sh <chemins>`), jobs conditionnés à sa sortie,
  dernier job nommé comme le workflow (contrôle obligatoire). Un nouveau contrôle s'ajoute au ruleset de `scripts/configurer-github.sh`.
- Toute PR : branche `<type>/<slug-court>` (`feat`, `fix`, `chore`, `docs`, `ci`) ou `release/…`, et une description ; vérifié par `pr-conventions.yml`.
- Actions épinglées par SHA complet (`uses: owner/action@<sha> # vX.Y.Z`) ; `permissions: contents: read` en tête, droits d'écriture au seul job qui publie.

## Rust
- Workspace Cargo unique à la racine (`Cargo.toml`) : membres `apps/security`, `apps/permission/backend`, `apps/estompe/backend/*`.
  Un seul `Cargo.lock`, un seul `rust-toolchain.toml` (version aussi dans les `FROM rust:` des Dockerfiles, vérifiée par la CI), un seul `deny.toml`.
- Versions et features communes dans `[workspace.dependencies]` ; un membre écrit `dep.workspace = true` (features en plus si besoin) et `[lints] workspace = true`.
- Nouveau backend : l'ajouter à `members`, à `.dockerignore` racine (contexte des images Rust) et ses chemins (`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml`) aux `paths` de sa CI, qui appelle `_rust-lint.yml`.
- Exception : les apps Tauri d'Estompe (`apps/estompe/{crates,windows,android}`) restent hors du workspace, avec leur `Cargo.lock` et la toolchain stable.
- Contrôles : `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo deny check`.

Chaque projet peut avoir son propre `AGENTS.md` (par exemple `apps/estompe/AGENTS.md`) : il complète celui-ci pour son dossier.
