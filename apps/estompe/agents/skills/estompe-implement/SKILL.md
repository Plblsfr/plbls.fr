---
name: estompe-implement
description: Implémente un ticket Estompe (issue GitHub) selon .ai/GH-<n>/plan.md dans un worktree dédié (première passe ou corrections de revue), lance les contrôles, prouve chaque critère dans verify.md, commit local sans push. À utiliser avec /estompe-implement <n>.
argument-hint: <numéro de l'issue>
disable-model-invocation: true
allowed-tools: Bash(git *), Bash(scripts/check.sh*), Bash(*/scripts/check.sh*), Bash(cargo *), Bash(test *), Bash(npx *), Bash(npm *), Bash(node *), Bash(mkdir *), Read, Write, Edit, Glob, Grep, Bash(gh issue view *), mcp__github__issue_read
---

# Implémentation du ticket $ARGUMENTS

## Conventions communes
- `<n>` : numéro de l'issue GitHub de `Plblsfr/plbls.fr` donné par `$ARGUMENTS` (`12` ou `#12` : retirer le `#`). Le ticket se désigne par `#<n>`.
- Dossier de travail : `.ai/GH-<n>/` à la racine du dépôt principal (parent de `git rev-parse --path-format=absolute --git-common-dir`). Jamais commité.
- Worktree : `<racine>/.worktree/<slug-court>`, branche lue dans la section « Branche » de `plan.md`.
- Jamais de `cd … &&` : `git -C <worktree>`, `npm --prefix <worktree>/frontend`, `<worktree>/scripts/check.sh`.

## 1. Mode
- `.ai/GH-<n>/plan.md` doit exister, sinon renvoyer vers `/estompe-plan <n>`.
- **corrections** : la dernière section « Revue » de `plan.md` a des cases `[ ]` dans « Corrections à faire » ;
- **première passe** : il reste des étapes `[ ]` ;
- sinon, rien à faire : s'arrêter.

## 2. Worktree
- `git fetch origin` (avec le jeton, voir `AGENTS.md`). `.git/info/exclude` du dépôt principal contient `.worktree/` et `.ai/`.
- Absent et branche jamais créée : `git -C <racine> worktree add -b <branche> .worktree/<slug-court> origin/main`.
- Absent mais branche existante (worktree supprimé après la PR) : `git -C <racine> worktree add .worktree/<slug-court> <branche>`.
- Présent : il doit être sur `<branche>`, arbre propre, sinon s'arrêter.
- `npm --prefix <worktree>/../.. ci` (workspace npm à la racine du dépôt) si `node_modules` y est absent.

## 3. Code
- Suivre le plan étape par étape (ou les corrections, dans l'ordre bloquant → important), selon `AGENTS.md`.
- Bug : écrire d'abord le test qui échoue, le constater, puis corriger. Sans repro possible, s'arrêter sans affirmer de cause.
- Plan faux ou incomplet : s'arrêter, expliquer, proposer avec AskUserQuestion de corriger le plan ou de relancer `/estompe-plan`.
- Cocher dans `plan.md` chaque étape ou correction faite. Piège découvert : l'ajouter à `AGENTS.md`.

## 4. Contrôles
- `<worktree>/scripts/check.sh` : corriger jusqu'à ce qu'il passe. Aucune liste de commandes à recopier : le script fait foi.
- Lire la sortie : aucun test Rust « ignoré : Docker indisponible ».

## 5. Critères
Écrire (ou mettre à jour en mode corrections) `.ai/GH-<n>/verify.md` :

```markdown
# Vérification #<n>
Commit : <sha, rempli après le commit>
| Critère | Statut | Preuve ou raison |
|---|---|---|
| C1 | vérifié | test `…` qui passe / commande et sortie observée |
```
- **vérifié** : preuve concrète (test qui couvre le critère et passe, commande lancée, code qui le garantit sans ambiguïté).
- **non vérifié** : la raison et ce qu'il faut faire. Un critère qui dépend d'un appareil réel le reste tant que la personne qui l'a n'a pas confirmé : le demander avec AskUserQuestion et noter sa réponse telle quelle.
- Bug : le test de reproduction cible bien le comportement corrigé.
- Ne jamais marquer « vérifié » sans preuve. Mode corrections : ne ré-examiner que les critères touchés par le diff des corrections.

## 6. Commit
- Pas de `changelog.d/` : rôle de `/estompe-pr`.
- `git -C <worktree> add` des fichiers modifiés, commit `<type>(<portée>): <résumé>` (première passe, type de la branche, voir `AGENTS.md`) ou `<type>(<portée>): corrections de revue`. Aucun push. Ne jamais commiter si `check.sh` échoue.
- Reporter le sha dans `verify.md`.

## 7. Sortie
Résumer : worktree, branche, étapes ou corrections faites, résultat de `check.sh`, critères non vérifiés. Prochaine étape : `/estompe-code-review <n>` (première passe) ; après une revue 2, `/estompe-pr <n>`.

## Règles
- Ne jamais commiter sur `main` ni pousser.
- Ne jamais deviner sur un point ambigu : poser la question.
