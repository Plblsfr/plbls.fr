---
name: estompe-grill-me
description: Interroge l'utilisateur sur un ticket CairnEstompe jusqu'à lever toute ambiguïté, puis écrit .ai/EST-xxx/brief.md et passe le ticket « en cours ». À utiliser avec /estompe-grill-me EST-xxx.
argument-hint: EST-<numéro>
disable-model-invocation: true
allowed-tools: Bash(git *), Bash(mkdir *), Read, Write, Edit, Glob, Grep, mcp__EstompeCairnMcp__get_ticket, mcp__claude_ai_EstompeCairnMcp__get_ticket, mcp__EstompeCairnMcp__get_page, mcp__claude_ai_EstompeCairnMcp__get_page, mcp__EstompeCairnMcp__search, mcp__claude_ai_EstompeCairnMcp__search, mcp__EstompeCairnMcp__list_tickets, mcp__claude_ai_EstompeCairnMcp__list_tickets, mcp__EstompeCairnMcp__update_ticket, mcp__claude_ai_EstompeCairnMcp__update_ticket, mcp__EstompeCairnMcp__add_comment, mcp__claude_ai_EstompeCairnMcp__add_comment
---

# Brief du ticket $ARGUMENTS

Rôle : comprendre la demande, pas la résoudre. Aucun code, aucune branche, aucun worktree.

## Conventions communes
- `<CLÉ>` : `$ARGUMENTS` sous la forme `EST-123`. Si seul le numéro est donné, préfixer `EST-`. Les outils Cairn refusent le numéro seul (« accès refusé »).
- Dossier de travail : `.ai/<CLÉ>/` à la racine du dépôt principal, jamais dans un worktree. Racine : `git rev-parse --path-format=absolute --git-common-dir` puis son dossier parent.
- `.ai/` et `.worktree/` ne sont jamais commités : vérifier que `.git/info/exclude` du dépôt principal contient les lignes `.ai/` et `.worktree/`, sinon les ajouter (ce fichier est partagé par tous les worktrees).
- Outils Cairn : `mcp__EstompeCairnMcp__*` ou `mcp__claude_ai_EstompeCairnMcp__*`, selon celui qui est disponible.

## 1. Préparation
- `git fetch origin`.
- Créer `.ai/<CLÉ>/` et vérifier l'exclusion de `.ai/`.
- Si `brief.md` existe déjà : le lire, puis demander avec AskUserQuestion s'il faut le compléter, repartir de zéro ou s'arrêter.

## 2. Lecture
- `get_ticket` avec `<CLÉ>`. Introuvable : s'arrêter et le dire.
- Lire ses dépendances (et leur statut), ses pages liées (`get_page`), ses commentaires, et les tickets proches (`search`) quand ils éclairent la demande.
- Lire `AGENTS.md` et le code concerné tel qu'il est sur `origin/main` : `git grep <motif> origin/main`, `git show origin/main:<chemin>`. Le but est de poser des questions précises, pas de concevoir la solution.

## 3. Interrogatoire
- Lister tout ce qui n'est pas tranché : comportement attendu, cas limites et erreurs, périmètre et hors périmètre, impact UI / API / données, migration, compatibilité avec les anciens clients, permissions et sécurité, critères de vérification mesurables, actions manuelles (secret, variable d'environnement, accès, migration à valider), dépendances non terminées.
- Poser les questions avec AskUserQuestion, quatre au plus par série, avec des options concrètes et l'option recommandée en premier quand il y en a une.
- Enchaîner les séries tant que les réponses ouvrent de nouvelles questions. S'arrêter quand il n'y a plus d'ambiguïté, pas avant.
- Rester agnostique du système : ne supposer aucun OS ni appareil particulier.
- Ne rien supposer. Une question sans réponse reste dans « Points ouverts ».

## 4. brief.md
Écrire `.ai/<CLÉ>/brief.md` :

```markdown
# <CLÉ> — <titre du ticket>

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

## 5. Cairn
- `update_ticket` (avec la `version` lue) : statut « en cours ».
- `add_comment` : décisions prises et critères de vérification, puisque `.ai/` n'est pas partagé.

## 6. Sortie
Résumer le brief en quelques lignes. Prochaine étape : `/estompe-plan <CLÉ>`. Si « Points ouverts » n'est pas vide, prévenir que le plan s'arrêtera.

## Règles
- Ne jamais écrire de code ni créer de branche.
- Ne jamais deviner sur un point ambigu : poser la question.
