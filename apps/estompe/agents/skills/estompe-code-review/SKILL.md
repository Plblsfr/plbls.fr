---
name: estompe-code-review
description: Revue d'un ticket CairnEstompe, au plus deux passes - revue 1 par trois relecteurs (qualité, sécurité, conformité) avec filtrage ; revue 2 ciblée sur les corrections. Rapport ajouté à .ai/EST-xxx/plan.md. À utiliser avec /estompe-code-review EST-xxx.
argument-hint: EST-<numéro>
disable-model-invocation: true
allowed-tools: Agent, Bash(git *), Read, Edit, Glob, Grep, mcp__EstompeCairnMcp__get_ticket, mcp__claude_ai_EstompeCairnMcp__get_ticket, mcp__EstompeCairnMcp__get_page, mcp__claude_ai_EstompeCairnMcp__get_page
---

# Revue du ticket $ARGUMENTS

Rôle : orchestrer les relecteurs et consolider. Aucune correction. **Au plus deux revues par ticket** : ce qui est trouvé au-delà devient un ticket, pas un tour de plus.

## Conventions communes
- `<CLÉ>` : `$ARGUMENTS` sous la forme `EST-123`. Dossier `.ai/<CLÉ>/` à la racine du dépôt principal (parent de `git rev-parse --path-format=absolute --git-common-dir`). Worktree `<racine>/.worktree/<slug-court>` (branche de `plan.md`).

## 1. Prérequis et mode
- `brief.md`, `plan.md`, `verify.md` et le worktree existent, arbre propre, au moins un commit au-dessus de `origin/main`. `verify.md` absent : s'arrêter, renvoyer vers `/estompe-implement`.
- `git fetch origin` ; base = `git -C <worktree> merge-base origin/main HEAD`.
- Compter les sections « Revue » de `plan.md` :
  - **0 → revue 1** : diff complet `<base>...HEAD` ;
  - **1 → revue 2** : diff des seules corrections, `<sha de la revue 1>..HEAD` ;
  - **2 → refuser** : dire que le plafond est atteint et renvoyer vers `/estompe-mr`.

## 2. Relecteurs
Un seul message, outil Agent (`general-purpose`), lecture seule (`git -C <worktree> diff|log|show|grep`, lecture de fichiers). Ils reçoivent des chemins et des consignes, pas l'avis de la session. Référence : `<worktree>/AGENTS.md`.

Consignes communes :
- Ne signaler que ce qui est **prouvable dans ce diff** (citer la ligne et le scénario qui échoue). Pas de goût personnel, rien de ce qu'un linter ou `check.sh` attrape déjà, pas de remarque « par principe ». Dire explicitement si rien n'est à signaler.
- Chaque remarque : gravité, fichier:ligne, problème, scénario concret d'échec, correction attendue, confiance (haute / moyenne).
- Gravités : **bloquant** = bug, faille, perte de données, critère non tenu ; **important** = défaut réel mais contournable ; **mineur** = tout le reste.

Revue 1 :
- **Qualité** : erreurs et cas limites non gérés, duplication d'un helper existant, cohérence avec `AGENTS.md`, tests absents pour une logique non triviale ou placés dans un nouveau fichier alors qu'un existe, commentaires ajoutés (interdits).
- **Sécurité** : contrôle d'accès côté serveur, validation des entrées, injections et XSS, fuites de secrets ou de données, migrations et anciens clients, dépendances ajoutées, déni de service évident.
- **Conformité** : lire le ticket (`get_ticket`, pages liées, commentaires), `brief.md`, `plan.md`, `verify.md`. Chaque critère `C*` est-il tenu, le hors périmètre respecté, les étapes réellement faites ? Une preuve de `verify.md` qui ne tient pas est un bloquant.

Revue 2 : un seul relecteur, qui ne reçoit que le diff des corrections et la liste des corrections de la revue 1. Il vérifie que chacune est résolue et que les corrections n'ont rien cassé. **Il ne cherche pas de nouveaux défauts** dans le reste du code.

## 3. Filtrage
- Fusionner les doublons, garder la gravité la plus haute.
- Vérifier soi-même dans le code **chaque** remarque bloquante ou importante ; écarter (en le notant) celles qui sont fausses, non reproductibles, ou de confiance moyenne sans scénario.
- Le reste (mineurs, « à améliorer ») ne bloque pas et n'est pas à corriger : le lister dans « Non bloquant » pour la PR.

## 4. Rapport
Ajouter à la fin de `.ai/<CLÉ>/plan.md`, sans toucher au reste :

```markdown
## Revue N — AAAA-MM-JJ (commit <sha court>)

### Remarques retenues
### Écartées
- <remarque> : <pourquoi>
### Non bloquant (pour la PR)
### Corrections à faire
- [ ] R<N>.1 [bloquant|important] <fichier:ligne> — <correction attendue>
### Verdict
à corriger | prêt pour la MR
```

- Verdict « prêt pour la MR » s'il ne reste aucun bloquant ni important. Les mineurs ne sont jamais des corrections.
- Revue 2 avec un bloquant encore ouvert : verdict « à corriger » ; l'utilisateur décide (corriger à la main, ou accepter et ouvrir un ticket).

## 5. Sortie
Remarques par gravité, verdict. « À corriger » après la revue 1 : `/estompe-implement <CLÉ>` puis `/estompe-code-review <CLÉ>` (revue 2). « Prêt » : `/estompe-mr <CLÉ>`.

## Règles
- Ne jamais modifier le code, commiter ni pousser. Ne jamais effacer une revue précédente.
