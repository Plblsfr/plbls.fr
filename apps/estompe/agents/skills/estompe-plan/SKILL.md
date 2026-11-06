---
name: estompe-plan
description: Conçoit le plan d'implémentation d'un ticket Estompe (issue GitHub) à partir de son brief, puis écrit .ai/GH-<n>/plan.md. À utiliser avec /estompe-plan <n>.
argument-hint: <numéro de l'issue>
disable-model-invocation: true
allowed-tools: Bash(git *), Bash(gh issue view *), mcp__github__issue_read, Read, Write, Edit, Glob, Grep
---

# Plan du ticket $ARGUMENTS

Rôle : décider comment faire, sans écrire de code ni créer de branche.

## Conventions communes
- `<n>` : numéro de l'issue GitHub de `Plblsfr/plbls.fr` donné par `$ARGUMENTS` (`12` ou `#12` : retirer le `#`). Le ticket se désigne par `#<n>`.
- Dossier de travail : `.ai/GH-<n>/` à la racine du dépôt principal (parent de `git rev-parse --path-format=absolute --git-common-dir`). Jamais commité.
- Issue : `gh issue view <n> --repo Plblsfr/plbls.fr --comments` (ou l'outil MCP `issue_read`).

## 1. Prérequis
- `.ai/GH-<n>/brief.md` doit exister. Sinon, s'arrêter et renvoyer vers `/estompe-grill-me <n>`.
- Si sa section « Points ouverts » n'est pas vide, s'arrêter et lister ces points.
- Relire l'issue (`gh issue view`) : si sa description a changé depuis le brief, le signaler et demander avec AskUserQuestion s'il faut refaire le brief.
- Si `plan.md` existe déjà, demander avec AskUserQuestion s'il faut le réviser ou repartir de zéro. Ne jamais effacer une section « Revue » existante.

## 2. Étude
- `git fetch origin`, puis lire `AGENTS.md` (conventions, pièges connus, commandes de build et de test ; pour un nouveau bloc backend, `backend/README.md` « Ajouter un bloc » fournit les étapes à reprendre dans le plan) et le code concerné sur `origin/main` (`git grep … origin/main`, `git show origin/main:<chemin>`).
- Repérer les classes et fichiers de test existants qui couvriront le changement. Aucune nouvelle classe de test : les nouveaux tests vont dans la classe de test existante du code testé.
- Si une décision d'architecture n'est pas couverte par le brief, poser la question avec AskUserQuestion avant d'écrire le plan.

## 3. plan.md
Écrire `.ai/GH-<n>/plan.md` :

```markdown
# Plan #<n> — <titre de l'issue>

## Branche
<type>/<slug-court>

## Approche
## Étapes
- [ ] E1 : <action> — fichiers : <chemins> — couvre : C1, C2
## Tests
- <classe ou fichier de test existant> : <cas ajoutés> — couvre : C1
## Changelog technique
changelog.d/<slug-court>.md : <résumé de l'entrée>
## Pièges et risques
## Mise à jour d'AGENTS.md
## Critères non couverts par des tests
- C3 : <comment il sera vérifié>
```

- Type de branche : `feat/` pour une fonctionnalité visible par les utilisateurs (minor +1), sinon `fix/`, `chore/`, `docs/` ou `ci/` (patch +1). `release/` est réservé à la release. Le slug est en minuscules, mots séparés par des tirets, sans numéro de ticket.
- Bug : la première étape est la reproduction (test qui échoue avant la correction).
- Chaque critère `C*` du brief est couvert par au moins une étape ou un test, ou figure dans « Critères non couverts par des tests » avec la façon de le vérifier.
- Les étapes sont assez précises pour être exécutées sans redécider quoi que ce soit.
- Présenter le plan et demander avec AskUserQuestion s'il est validé ou à corriger.

## 4. Sortie
Résumer : branche, nombre d'étapes, critères couverts. Prochaine étape : `/estompe-implement <n>`.

## Règles
- Ne jamais écrire de code ni créer de branche.
- Ne jamais deviner sur un point ambigu : poser la question.
