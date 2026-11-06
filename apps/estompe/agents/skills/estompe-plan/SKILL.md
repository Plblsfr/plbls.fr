---
name: estompe-plan
description: Conçoit le plan d'implémentation d'un ticket CairnEstompe à partir de son brief, puis écrit .ai/EST-xxx/plan.md. À utiliser avec /estompe-plan EST-xxx.
argument-hint: EST-<numéro>
disable-model-invocation: true
allowed-tools: Bash(git *), Read, Write, Edit, Glob, Grep, mcp__EstompeCairnMcp__get_ticket, mcp__claude_ai_EstompeCairnMcp__get_ticket, mcp__EstompeCairnMcp__get_page, mcp__claude_ai_EstompeCairnMcp__get_page, mcp__EstompeCairnMcp__search, mcp__claude_ai_EstompeCairnMcp__search
---

# Plan du ticket $ARGUMENTS

Rôle : décider comment faire, sans écrire de code ni créer de branche.

## Conventions communes
- `<CLÉ>` : `$ARGUMENTS` sous la forme `EST-123` (préfixer `EST-` si seul le numéro est donné).
- Dossier de travail : `.ai/<CLÉ>/` à la racine du dépôt principal (parent de `git rev-parse --path-format=absolute --git-common-dir`). Jamais commité.
- Outils Cairn : `mcp__EstompeCairnMcp__*` ou `mcp__claude_ai_EstompeCairnMcp__*`.

## 1. Prérequis
- `.ai/<CLÉ>/brief.md` doit exister. Sinon, s'arrêter et renvoyer vers `/estompe-grill-me <CLÉ>`.
- Si sa section « Points ouverts » n'est pas vide, s'arrêter et lister ces points.
- Relire le ticket (`get_ticket`) : si sa description a changé depuis le brief, le signaler et demander avec AskUserQuestion s'il faut refaire le brief.
- Si `plan.md` existe déjà, demander avec AskUserQuestion s'il faut le réviser ou repartir de zéro. Ne jamais effacer une section « Revue » existante.

## 2. Étude
- `git fetch origin`, puis lire `AGENTS.md` (conventions, pièges connus, commandes de build et de test ; pour un nouveau bloc backend, `backend/README.md` « Ajouter un bloc » fournit les étapes à reprendre dans le plan) et le code concerné sur `origin/main` (`git grep … origin/main`, `git show origin/main:<chemin>`).
- Repérer les classes et fichiers de test existants qui couvriront le changement. Aucune nouvelle classe de test : les nouveaux tests vont dans la classe de test existante du code testé.
- Si une décision d'architecture n'est pas couverte par le brief, poser la question avec AskUserQuestion avant d'écrire le plan.

## 3. plan.md
Écrire `.ai/<CLÉ>/plan.md` :

```markdown
# Plan <CLÉ> — <titre du ticket>

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
Résumer : branche, nombre d'étapes, critères couverts. Prochaine étape : `/estompe-implement <CLÉ>`.

## Règles
- Ne jamais écrire de code ni créer de branche.
- Ne jamais deviner sur un point ambigu : poser la question.
