---
name: estompe-pr
description: Finalise un ticket CairnEstompe relu - changelog technique, push, pull request GitHub, commentaire dans Cairn, puis suppression du worktree. À utiliser avec /estompe-pr EST-xxx.
argument-hint: EST-<numéro>
disable-model-invocation: true
allowed-tools: Bash(git *), Bash(gh *), Bash(*/scripts/check.sh*), Read, Write, Edit, Glob, Grep, mcp__EstompeCairnMcp__get_ticket, mcp__claude_ai_EstompeCairnMcp__get_ticket, mcp__EstompeCairnMcp__add_comment, mcp__claude_ai_EstompeCairnMcp__add_comment
---

# Pull request du ticket $ARGUMENTS

## Conventions communes
- `<CLÉ>` : `$ARGUMENTS` sous la forme `EST-123` (préfixer `EST-` si seul le numéro est donné).
- Dossier de travail : `.ai/<CLÉ>/` à la racine du dépôt principal (parent de `git rev-parse --path-format=absolute --git-common-dir`). Jamais commité.
- Worktree : `<racine>/.worktree/<slug-court>` (`<racine>` = dépôt principal, branche de `plan.md`).
- GitHub : seul `gh` parle à GitHub (jamais `curl` ni l'API à la main), sauf le push, fait avec `git`.

## 1. Prérequis
- `gh` connecté : `gh pr list --repo Plblsfr/plbls.fr`. Sinon, s'arrêter et le dire.
- Arbre du worktree propre.
- `verify.md` existe et ne contient aucun critère « non vérifié » sans raison notée.
- `<worktree>/scripts/check.sh` passe sur le `HEAD` actuel (c'est lui, pas le commit de `verify.md`, qui fait foi).
- `plan.md` contient au moins une revue, la dernière a le verdict « prêt pour la PR », et les commits depuis son sha ne sont que les corrections qu'elle demandait (`git log <sha>..HEAD`). Sinon, renvoyer vers `/estompe-code-review <CLÉ>`.
- `git fetch origin` : si `origin/main` a avancé, demander avec AskUserQuestion s'il faut rebaser (puis relancer `check.sh`) ou ouvrir la PR telle quelle.

## 2. Changelog technique
- Obligatoire sauf pour les branches `ci/`, `docs/` et `release/` (la CI échoue sinon).
- Créer `changelog.d/<slug-court>.md` dans le worktree, au format décrit par `changelog.d/README.md`, avec la référence `<CLÉ>` (jamais `#<numéro>` : GitHub en ferait un lien vers une issue ou PR du dépôt).
- Ne pas toucher à `backend/core/src/notes_version/changelog.json` ni à `CHANGELOG.md` : c'est le rôle de `/estompe-release`.
- Commit : `[<CLÉ>] Changelog`.

## 3. Push et PR
- `git -C <worktree> push -u origin <branche>` (ne jamais forcer).
- `gh pr create --repo Plblsfr/plbls.fr --base main --head <branche> --title "[<CLÉ>] <titre du ticket>" --body "<corps>"`.
- Corps de la PR, construit depuis `.ai/<CLÉ>/` :
  - résumé des changements (depuis `plan.md`, section « Approche ») ;
  - tableau des critères (depuis `verify.md`) ;
  - contrôles lancés et résultats ;
  - verdict de la dernière revue et sa section « Non bloquant » ;
  - ce qui n'a pas été vérifié ;
  - référence du ticket `<CLÉ>`.
- Fusion attendue en **« merge » classique** (pas de squash ni rebase), pour que la CI calcule la version à partir du préfixe de branche.

## 4. Cairn
- `add_comment` sur `<CLÉ>` : lien de la PR et tableau **critère → vérifié (preuve) / non vérifié (raison)** repris de `verify.md`.
- Ne pas changer le statut : il reste « en cours » jusqu'à la release.

## 5. Suppression du worktree
- La PR est créée et le commentaire Cairn posté : `git -C <racine> worktree remove .worktree/<slug-court>` (arbre propre, branche déjà poussée). Ne pas supprimer la branche.
- Si la PR demande des changements : `/estompe-implement` recrée le worktree depuis la branche.

## 6. Sortie
Résumer : branche, lien de la PR, fichier de changelog, critères non vérifiés, worktree supprimé. Rappeler de fusionner en « merge » classique.

## Règles
- Ne jamais pousser sur `main` ni forcer un push.
- Ne jamais ouvrir de PR si `check.sh` échoue ou si le verdict de revue ne le permet pas.
- Ne supprimer le worktree qu'après la création de la PR ; jamais s'il reste des modifications non commitées.
- Ne jamais deviner sur un point ambigu : poser la question.
