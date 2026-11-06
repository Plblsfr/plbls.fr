# Release et livraison

- Branche de release : `release/api-vA.B.C-app-vX.Y.Z`, avec les versions `api` et applis (`web`/`windows`/`android`) attendues de `.version`.
  Le nom reste unique même avec plusieurs releases par jour ou une release qui ne livre qu'un élément.
- `backend/core/src/notes_version/changelog.json` (du plus récent au plus ancien : `version`, `date` `AAAA-MM-JJ`, `new[]`, `fixes[]`) : notes
  envoyées en message privé à chaque utilisateur par le compte « Estompe », une fois par version.
  - L'entrée porte la **version applis**. Une release qui ne change pas la version applis n'ajoute pas d'entrée : poser la question si des
    changements visibles par les utilisateurs le justifieraient.
  - Texte court, en français simple, sans jargon, orienté bénéfice.
  - Ne jamais modifier une entrée déjà publiée (elle a déjà été envoyée) : en ajouter une nouvelle.
  - Ne se modifie que dans la PR de release, jamais dans celle d'un ticket.
- `CHANGELOG.md` : les entrées de `changelog.d/` y sont regroupées à la release (voir `changelog.d/README.md`).
- Mise à jour forcée : `backend/core/updates.json` (`mandatory`, `minVersion`, `message`, surcharge par `platforms.web|android|windows`).
  L'API refuse (426) les clients sous `minVersion` dès son déploiement. Sans forçage, le fichier revient à `mandatory: false`, `minVersion: ""`, `message: ""`.

Avant toute release, l'agent vérifie la chaîne de livraison (workflows, Dockerfile, caches, contenu produit) et pas seulement les tests : un tag ne se rejoue pas.
Détail : `/estompe-release`, étape « 1 bis ». Exemple vécu (`app-v0.84.3`) : `tauri build` écrasait `frontend/dist` (le site servait le build Windows) et le cache de l'installeur, dont la clé ignore le workflow, sautait la reconstruction du front web.

Un tag annoté, créé sur `main` après la fusion de la PR `release/…`, livre un élément (workflows `.github/workflows/estompe-*.yml`) :

| Tag | Livre | Version lue dans `.version` |
|---|---|---|
| `estompe-api-vX.Y.Z` | image Docker de l'API (`ghcr.io/plblsfr/estompe`, `latest` + `X.Y.Z`) | `api` |
| `estompe-app-vX.Y.Z` | web (image `ghcr.io/plblsfr/estompe-web`), installeur Windows, APK Android | `web`, `windows`, `android` |

Livrer l'un, l'autre ou les deux = poser le ou les tags voulus (les deux sur le même commit pour tout livrer). Il n'y a plus de tag `v*`.
Ne jamais écraser ni supprimer un tag existant ; pousser un tag seul (`push origin <tag>`), jamais `--tags`.

