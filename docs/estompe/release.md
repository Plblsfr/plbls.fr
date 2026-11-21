# Release et livraison

- Branche de release : `release/api-vA.B.C-app-vX.Y.Z`, avec les versions `api` et applis (`web`/`windows`/`android`) attendues de `.version`.
  Le nom reste unique même avec plusieurs releases par jour ou une release qui ne livre qu'un élément.
- `api/estompe/core/src/notes_version/changelog.json` (du plus récent au plus ancien : `version`, `date` `AAAA-MM-JJ`, `new[]`, `fixes[]`) : notes
  envoyées en message privé à chaque utilisateur par le compte « Estompe », une fois par version.
  - L'entrée porte la **version applis**. Une release qui ne change pas la version applis n'ajoute pas d'entrée : poser la question si des
    changements visibles par les utilisateurs le justifieraient.
  - Texte court, en français simple, sans jargon, orienté bénéfice.
  - Ne jamais modifier une entrée déjà publiée (elle a déjà été envoyée) : en ajouter une nouvelle.
  - Ne se modifie que dans la PR de release, jamais dans celle d'un ticket.
- `CHANGELOG.md` : les entrées de `changelog.d/` y sont regroupées à la release (voir `changelog.d/README.md`).
- Mise à jour forcée : `api/estompe/core/updates.json` (`mandatory`, `minVersion`, `message`, surcharge par `platforms.web|android|windows|natif` ; `natif` est recopiée dans `natif.json` de l'app native).
  L'API refuse (426) les clients sous `minVersion` dès son déploiement. Sans forçage, le fichier revient à `mandatory: false`, `minVersion: ""`, `message: ""`.

Avant toute release, l'agent vérifie la chaîne de livraison (workflows, Dockerfile, caches, contenu produit) et pas seulement les tests : un tag ne se rejoue pas.
Détail : `/estompe-release`, étape « 1 bis ». Exemple vécu (`app-v0.84.3`) : `tauri build` écrasait `web/estompe/dist` (alors `frontend/dist`) (le site servait le build Windows) et le cache de l'installeur, dont la clé ignore le workflow, sautait la reconstruction du front web.

Un tag annoté, créé sur `main` après la fusion de la PR `release/…`, livre un élément (workflows `.github/workflows/estompe-*.yml`) :

| Tag | Livre | Version lue dans `.version` |
|---|---|---|
| `estompe-api-vX.Y.Z` | image Docker de l'API (`ghcr.io/plblsfr/estompe`, `latest` + `X.Y.Z`) | `api` |
| `estompe-app-vX.Y.Z` | web (image `ghcr.io/plblsfr/estompe-web`), installeur Windows, APK Android | `web`, `windows`, `android` |
| `estompe-natif-vX.Y.Z` | app native (bêta) : installeur Windows NSIS, signature de mise à jour, manifeste `natif.json`, release GitHub en pré-version | `natif` |

Livrer l'un, l'autre ou les deux = poser le ou les tags voulus (les deux sur le même commit pour tout livrer). Il n'y a plus de tag `v*`.
Ne jamais écraser ni supprimer un tag existant ; pousser un tag seul (`push origin <tag>`), jamais `--tags`.

## App native (`estompe-natif-vX.Y.Z`)

Workflow `.github/workflows/estompe-livraison-natif.yml`. Le tag se pose comme les autres, après la fusion de la PR `release/…`, et seulement
si `natif` a monté depuis le dernier tag `estompe-natif-v*` : le workflow échoue si le tag ne correspond pas à `natif` de `.version` (PR `ci/version` pas encore fusionnée).

- Notes de version : le message du tag annoté, `git tag -a estompe-natif-vX.Y.Z -m "<notes courtes, en français simple>"` ; un message
  égal au nom du tag donne des notes vides. Elles vont dans la release et dans `natif.json` (affichées par l'app).
- Le job `windows` construit l'app (MSVC, profil release), télécharge ffmpeg (`apps/estompe/windows/scripts/fetch-ffmpeg.mjs`, même build LGPL
  épinglé que l'app Tauri, somme SHA-256 vérifiée), construit l'installeur (`apps/estompe/natif/installeur/estompe-natif.nsi`, qui l'embarque) et le contrôle
  (`scripts/estompe/natif-installeur-check.sh` : taille plafonnée, contenu dont `ffmpeg.exe`, aucune référence à WebView2). Lancé à la main, il s'arrête là :
  l'installeur est un artefact, rien n'est publié.
- Sur le tag, le job `publier` (environnement `livraison`) signe l'installeur avec minisign (secret `ESTOMPE_NATIF_CLE_MAJ`, commentaire de
  confiance `estompe-natif X.Y.Z`), vérifie la signature avec la clé publique de l'app (`apps/estompe/crates/client/src/mise_a_jour/cle.pub`)
  avant toute publication, écrit `natif.json` (`scripts/estompe/natif-manifeste.mjs`, politique `platforms.natif` de `updates.json`), puis :
  - crée la release `estompe-natif-vX.Y.Z` (pré-version, jamais « latest ») avec `Estompe-natif-beta-X.Y.Z-setup.exe`, son `.minisig` et `natif.json` ;
  - remplace `natif.json` et `Estompe-natif-beta-setup.exe` dans la release fixe `estompe-natif-beta` (créée au premier tag, son tag n'est jamais
    déplacé ; seuls ses fichiers changent). L'app lit `https://github.com/Plblsfr/plbls.fr/releases/download/estompe-natif-beta/natif.json`.
- Ne pas créer la release à la main : le workflow la crée (et ne fait que remplacer ses fichiers s'il est relancé).
- L'installeur n'est pas signé Authenticode : Windows SmartScreen avertit à la première installation (dit dans la release).

