# Traiter un ticket avec Claude Code

Un ticket est une issue GitHub de `Plblsfr/plbls.fr` (`#<n>`). Il se traite en cinq commandes, dans l'ordre. Chacune écrit un fichier dans `.ai/GH-<n>/` (local, jamais commité) que la suivante lit.
Règles complètes : `AGENTS.md`. Les skills sont dans `agents/skills/`.

## Avant de commencer
- `gh pr list` répond dans le dépôt (sinon `gh` n'est pas connecté : `gh auth login`) ; `gh issue view <n> --repo Plblsfr/plbls.fr` affiche l'issue.
- Pour les tests Rust : `TEST_DATABASE_URL` pointe vers un PostgreSQL de test.

## Les étapes

| # | Commande | Ce que tu fais | Résultat |
|---|---|---|---|
| 1 | `/estompe-grill-me <n>` | Réponds aux questions jusqu'à ce qu'il n'y ait plus d'ambiguïté, puis valide le brief | `brief.md` avec critères `C1`, `C2`…, issue assignée et commentée |
| 2 | `/estompe-plan <n>` | Valide ou corrige le plan | `plan.md` : branche, étapes, tests |
| 3 | `/estompe-implement <n>` | Rien, sauf si on te demande un appareil réel pour un critère | worktree, code, tests, `scripts/check.sh` vert, `verify.md` |
| 4 | `/estompe-code-review <n>` | Lis le verdict | revue 1 dans `plan.md` |
| 5 | `/estompe-pr <n>` | Rien | changelog, push, PR GitHub (`Closes #<n>`), commentaire sur l'issue, worktree supprimé |

Si la revue 1 demande des corrections : `/estompe-implement <n>` puis `/estompe-code-review <n>` (revue 2, corrections seules), puis `/estompe-pr`.
**Il n'y a jamais plus de deux revues** : les remarques mineures vont dans la PR, un nouveau défaut hors diff devient une issue.

## Après la PR
1. Relire la PR dans GitHub, puis la fusionner en « merge » classique (jamais squash ni rebase : le nom de la branche pilote la version). L'issue se ferme à la fusion.
2. Si la PR demande des changements : `/estompe-implement <n>` recrée le worktree depuis la branche.
3. `/estompe-release` commente l'issue avec le tag livré, et la rouvre s'il reste un critère non vérifié.

## Petits changements sans ticket
Une demande rapide suit quand même la règle n°1 : branche et worktree depuis `origin/main`, commit `type(portée): résumé`, PR à ta demande explicite. Pas de `.ai/`.

## Bon à savoir
- Désigne le ticket par le numéro de son issue (`12` ou `#12`) ; commits au format `type(portée): résumé`, sans référence de ticket.
- `scripts/check.sh` est la seule liste de contrôles ; ne la recopie pas.
- Un critère qui dépend d'un appareil réel reste « non vérifié » tant que tu n'as pas confirmé.
- Un skill se corrige dans `agents/skills/<nom>/SKILL.md`, avec `AGENTS.md` dans la même PR.
