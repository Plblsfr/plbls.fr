---
name: estompe-grill-me
description: Interroge l'utilisateur sur un ticket Estompe (issue GitHub) jusqu'à lever toute ambiguïté, puis écrit .ai/GH-<n>/brief.md et passe le ticket « en cours ». À utiliser avec /estompe-grill-me <n>.
argument-hint: <numéro de l'issue>
disable-model-invocation: true
allowed-tools: Bash(git *), Bash(mkdir *), Bash(gh issue view *), Bash(gh issue list *), Bash(gh issue edit *), Bash(gh issue comment *), Read, Write, Edit, Glob, Grep, mcp__github__issue_read, mcp__github__add_issue_comment
---

# Brief du ticket $ARGUMENTS

Rôle : comprendre la demande, pas la résoudre. Aucun code, aucune branche, aucun worktree.

## Conventions communes
- `<n>` : numéro de l'issue GitHub de `Plblsfr/plbls.fr` donné par `$ARGUMENTS` (`12` ou `#12` : retirer le `#`). Le ticket se désigne par `#<n>`.
- Dossier de travail : `.ai/GH-<n>/` à la racine du dépôt principal, jamais dans un worktree. Racine : `git rev-parse --path-format=absolute --git-common-dir` puis son dossier parent.
- `.ai/` et `.worktree/` ne sont jamais commités : vérifier que `.git/info/exclude` du dépôt principal contient les lignes `.ai/` et `.worktree/`, sinon les ajouter (ce fichier est partagé par tous les worktrees).
- Tickets : issues GitHub, lues avec `gh issue view <n> --repo Plblsfr/plbls.fr --comments` (ou l'outil MCP `issue_read`), commentées avec `gh issue comment <n> --repo Plblsfr/plbls.fr --body-file <fichier>` (ou `add_issue_comment`).

## 1. Préparation
- `git fetch origin`.
- Créer `.ai/GH-<n>/` et vérifier l'exclusion de `.ai/`.
- Si `brief.md` existe déjà : le lire, puis demander avec AskUserQuestion s'il faut le compléter, repartir de zéro ou s'arrêter.

## 2. Lecture
- `gh issue view <n> --repo Plblsfr/plbls.fr --comments`. Introuvable ou fermée : s'arrêter et le dire.
- Lire ses commentaires, les issues qu'elle cite (et leur état) et les issues proches (`gh issue list --repo Plblsfr/plbls.fr --search <mots>`) quand elles éclairent la demande.
- Lire `AGENTS.md` et le code concerné tel qu'il est sur `origin/main` : `git grep <motif> origin/main`, `git show origin/main:<chemin>`. Le but est de poser des questions précises, pas de concevoir la solution.

## 3. Interrogatoire
- Lister tout ce qui n'est pas tranché : comportement attendu, cas limites et erreurs, périmètre et hors périmètre, impact UI / API / données, migration, compatibilité avec les anciens clients, permissions et sécurité, critères de vérification mesurables, actions manuelles (secret, variable d'environnement, accès, migration à valider), dépendances non terminées.
- Poser les questions avec AskUserQuestion, quatre au plus par série, avec des options concrètes et l'option recommandée en premier quand il y en a une.
- Enchaîner les séries tant que les réponses ouvrent de nouvelles questions. S'arrêter quand il n'y a plus d'ambiguïté, pas avant.
- Rester agnostique du système : ne supposer aucun OS ni appareil particulier.
- Ne rien supposer. Une question sans réponse reste dans « Points ouverts ».

## 4. brief.md
Écrire `.ai/GH-<n>/brief.md` :

```markdown
# #<n> — <titre de l'issue>

## Demande
## Contexte
## Périmètre
## Hors périmètre
## Décisions
- <question> → <réponse>
## Critères de vérification
- C1 : <critère mesurable>
## Actions manuelles
## Dépendances
## Points ouverts
```

- Dans « Contexte », mettre les chemins de code et de documentation utiles.
- Numéroter les critères `C1`, `C2`… : les autres skills s'y réfèrent.
- Présenter le brief à l'utilisateur et demander avec AskUserQuestion s'il le valide ou s'il faut le corriger.

## 5. Issue GitHub
- « En cours » : `gh issue edit <n> --repo Plblsfr/plbls.fr --add-assignee @me`.
- `gh issue comment` : décisions prises et critères de vérification, puisque `.ai/` n'est pas partagé.

## 6. Sortie
Résumer le brief en quelques lignes. Prochaine étape : `/estompe-plan <n>`. Si « Points ouverts » n'est pas vide, prévenir que le plan s'arrêtera.

## Règles
- Ne jamais écrire de code ni créer de branche.
- Ne jamais deviner sur un point ambigu : poser la question.
