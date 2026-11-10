---
name: estompe-release
description: Prépare et publie une release d'Estompe (branche release/…, changelogs, PR, puis tags, release GitHub et suivi des issues livrées). À utiliser avec /estompe-release.
disable-model-invocation: true
allowed-tools: Bash(git *), Bash(cargo *), Bash(gh *), Bash(node *), Bash(npx *), Bash(npm *), Bash(test *), Bash(rm -r */.ai/GH-*), Read, Edit, Write, Glob, Grep, mcp__github__issue_read, mcp__github__add_issue_comment
---

# Release Estompe

Référence : `AGENTS.md` (section « Estompe » : branches, version, release). Ne jamais commiter sur `main`.

## Conventions
- Jamais de `cd … &&` : lancer cargo et les scripts depuis la racine du worktree, `npm --prefix web/estompe`, `git -C`.
- Issues : `gh issue view|comment|reopen <n> --repo Plblsfr/plbls.fr` (ou les outils MCP `issue_read` et `add_issue_comment`).

## 1. Vérifications préalables
- Dans le dépôt principal : `git fetch origin`, `git switch main`, `git pull`. L'arbre doit être propre (hors `.claude/`, `.ai/` et `.worktree/`), sinon s'arrêter et le dire.
- `gh` connecté : `gh pr list --repo Plblsfr/plbls.fr`. Sinon, s'arrêter et le dire.
- Pour chaque élément livrable, lister les commits depuis son dernier tag : `git log --first-parent --format='%h %s' $(git describe --tags --abbrev=0 --match 'estompe-api-v*')..HEAD`, idem avec `'estompe-app-v*'`. Si aucun, s'arrêter : rien à publier.
- Lire les fichiers de `changelog.d/` (hors `README.md`) et en extraire les références d'issue `(#<n>)`. Pour chacune, `gh issue view <n> --repo Plblsfr/plbls.fr --comments` et lire le dernier commentaire de `/estompe-pr` : lister les critères encore « non vérifié ». Les présenter à l'utilisateur avant de continuer.

## 1 bis. La chaîne de livraison fonctionnera-t-elle ?
Un tag ne se rejoue pas (jamais d'écrasement) : un échec de livraison coûte une version et un cycle complet de PR. Avant de préparer la release, vérifier la chaîne, pas seulement les tests :
- Lister ce qui a changé depuis le dernier tag de chaque élément livré dans `.github/workflows/{livraison,estompe-livraison-app,_image}.yml`, `web/estompe/plateforme/`, `apps/estompe/windows/scripts/`, `apps/estompe/android/scripts/`, `deploy/estompe/`, `deploy/docker-compose.yml` et les Dockerfile : `git diff --stat <dernier-tag>..HEAD -- <chemins>`. Relire chaque changement et suivre le chemin complet : ce que `estompe-livraison-app.yml` ou le job `estompe-api-image` de `livraison.yml` produit, ce que le Dockerfile copie, ce qui est servi.
- Les caches (`actions/cache`) : une clé qui ne dépend pas du fichier modifié restaure l'ancien résultat et saute les étapes conditionnées par `cache-hit`. Vérifier que chaque correctif de la chaîne s'exécute aussi sur cache hit.
- Reproduire en local ce qui peut l'être (`npm --prefix web/estompe run build`, commandes de garde des Dockerfile sur le résultat) et vérifier le contenu livré, pas seulement le code (ex. le bundle web ne contient pas `__TAURI_INTERNALS__`).
- Quand le workflow de livraison ou un Dockerfile a changé depuis le dernier tag, vérifier les exécutions déjà faites (`gh run list --repo Plblsfr/plbls.fr --workflow <fichier>` ; sinon demander à l'utilisateur de lire le dernier run) ou lancer le workflow à la main sur `main` (`workflow_dispatch` : les jobs de build tournent, l'image ne part que sur un tag).
- Ce qui ne peut pas être vérifié avant le tag (build réel de l'image, signature, APK) est dit à l'utilisateur et listé dans la PR de release. Ne pas poser le tag tant qu'une vérification possible reste à faire.
- Après le tag : suivre les workflows jusqu'au bout et rendre compte (réussite, échec, ce qui a été réellement livré).

## 2. Versions
- Elles sont calculées par la CI, pas choisies : chaque fusion `feat/…` fait +1 minor, tout le reste +1 patch, à partir du dernier tag ; une branche `release/` n'ajoute rien.
- Les prévoir : `node scripts/estompe/bump-version.mjs` (depuis la racine du worktree ; historique complet requis), lire les versions affichées, puis `git checkout -- .version` car le script réécrit `.version`.
- Relever la version API (`api` de `.version`) et la version applis (`web`/`windows`/`android`).
- Le nom de la branche est `release/api-vA.B.C-app-vX.Y.Z` (versions `api` et applis attendues de `.version`). Si le calcul est ambigu, demander avec AskUserQuestion. Ne pas choisir seul.
- Une version majeure se pose à la main (les trois fichiers ci-dessus) : poser la question avant.
- Résultat incohérent : poser la question avec AskUserQuestion.

## 3. Branche et vérifications
- `git worktree add -b release/api-vA.B.C-app-vX.Y.Z .worktree/release origin/main`, puis travailler dans ce worktree (jamais sur `main`).
- Lancer :
  ```sh
  test -n "$TEST_DATABASE_URL"
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test $(scripts/estompe/paquets-cargo.sh)
  cargo deny check
  npm --prefix web/estompe run check
  npm --prefix web/estompe test
  ```
- Les tests Rust ne doivent pas être ignorés (« ignoré : Docker indisponible »). S'arrêter si quelque chose échoue.

## 4. Mise à jour forcée
- Demander avec AskUserQuestion si la release doit être **forcée** (correctif de sécurité, API incompatible, bug bloquant) : par défaut non.
- Si oui, demander la plateforme (toutes, web, android, windows), le **message** affiché à l'utilisateur et, si des clients anciens doivent être bloqués, la **version minimale** (`minVersion`, ex. `0.68.0`).
- Les poser dans `api/estompe/core/updates.json` (champs communs `mandatory`, `minVersion`, `message`, ou surcharge dans `platforms.web|android|windows`). Sans forçage, vérifier que le fichier est revenu à `mandatory: false`, `minVersion: ""`, `message: ""` : une valeur laissée par une release précédente s'appliquerait encore.
- `cargo test -p estompe-core` doit passer. Si `minVersion` est posé, le rappeler dans la PR : l'API refuse (426) les clients plus anciens dès son déploiement.

## 5. Fichiers de release
- `api/estompe/core/src/notes_version/changelog.json` (du plus récent au plus ancien) : ajouter en tête `{ "version", "date", "new": [], "fixes": [] }`, `date` au format `AAAA-MM-JJ`, `version` = version **applis**. Si la version applis ne change pas, n'ajouter aucune entrée ; poser la question si des changements visibles par les utilisateurs le justifieraient.
  - Texte court, en français simple, sans jargon, orienté bénéfice : il est envoyé en message privé à chaque utilisateur par le compte « Estompe ».
  - Ne jamais modifier une entrée déjà publiée (elle a déjà été envoyée) : en ajouter une nouvelle.
  - Faire relire le texte à l'utilisateur avec AskUserQuestion avant de l'écrire.
- `CHANGELOG.md` : insérer `## api A.B.C / applis X.Y.Z - AAAA-MM-JJ` sous `## Non publié`, y déplacer les entrées de « Non publié » et celles des fichiers de `changelog.d/` (hors `README.md`), puis supprimer ces fichiers (voir `changelog.d/README.md`).
- Ne pas toucher à `version` dans `tauri.conf.json`.

## 6. Commit, push, PR
- Commit : `chore(release): api-vA.B.C app-vX.Y.Z`.
- `git push -u origin release/api-vA.B.C-app-vX.Y.Z` (ne jamais forcer).
- `gh pr create --repo Plblsfr/plbls.fr --base main --head release/api-vA.B.C-app-vX.Y.Z --title "chore(release): api-vA.B.C app-vX.Y.Z" --body "<corps>"`. Corps : versions et raison du calcul, issues livrées (`#<n>`) et leurs critères non vérifiés, vérifications faites, ce qui n'a pas été vérifié (build Tauri/Windows, images Docker), forçage éventuel, étapes après fusion.
- S'arrêter et demander à l'utilisateur de fusionner en **« merge » classique** (pas de squash ni rebase).

## 7. Après la fusion (l'utilisateur confirme)
- `git switch main && git pull`. Vérifier que HEAD est le commit de fusion de la release.
- Vérifier que les versions de `.version` sur HEAD sont celles écrites dans `changelog.json` et `CHANGELOG.md`. Sinon, s'arrêter et le dire.
- Demander avec AskUserQuestion ce qu'il faut livrer : **API** (`estompe-api-vX.Y.Z`), **applications** (`estompe-app-vX.Y.Z`) ou **les deux** (même commit). Ne proposer que ce dont la version a augmenté depuis le dernier tag correspondant (`git tag -l 'estompe-api-v*'`, `git tag -l 'estompe-app-v*'`).
- Pour chaque tag : vérifier qu'il n'existe pas, `git tag -a <tag> -m "<tag>"`, puis `git push origin <tag>` (ce tag seul, jamais `--tags`). `estompe-api-v*` déclenche `livraison.yml` (job `estompe-api-image` : image Rust de l'API), `estompe-app-v*` déclenche `estompe-livraison-app.yml` (images web, installeur, APK).
- `gh release create <tag> --repo Plblsfr/plbls.fr --verify-tag --title "<tag>" --notes "<Nouveautés / Corrections>"` pour chaque tag (`--notes`, pas `--body`).

## 8. Issues et nettoyage
- Pour chaque issue livrée (fermée par `Closes #<n>` à la fusion de sa PR) : `gh issue comment` avec le tag et la version. Si un de ses critères n'est pas vérifié : `gh issue reopen` et commentaire listant ce qui reste à vérifier.
- Demander avec AskUserQuestion s'il faut supprimer les worktrees (`git worktree remove .worktree/<slug-court>`), les branches locales et les dossiers `.ai/GH-*` des issues livrées et fermées.

## 9. Sortie
Résumer : versions, raison du niveau, PR, tag(s) posé(s), lien des releases, issues fermées et celles rouvertes. Rappeler de vérifier les workflows dans l'onglet Actions de GitHub.

## Règles
- Ne jamais publier si les tests échouent.
- Ne jamais écraser ni supprimer un tag existant, ni forcer un push.
- Ne jamais commiter sur `main`.
- Ne jamais deviner sur un point ambigu : poser la question.
- Ne pas ajouter de commentaires dans le code généré.
