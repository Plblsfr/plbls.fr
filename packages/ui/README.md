# @plbls/ui

Le design system « surligneur » de plbls.fr : blanc cassé, noir, rose pastel en
aplat. **Aucun framework** — jetons en variables CSS, une feuille de styles à
classes `plbls-*`, et des web components pour les pièces qui ont un
comportement. Utilisable depuis n'importe quel HTML, JS, React, Vue, gabarit
serveur ou projet SCSS. Palette, typographie et règles : [`DESIGN_SYSTEM.md`](./DESIGN_SYSTEM.md).

Extrait de `projects/plbls-ui` de `plbls-app/plbls-fr-web` (version Angular
0.2.0, commit `b26c856`) ; le site pourra le consommer à la place.

## Utilisation

**1. Polices** (facultatif — sans elles : `system-ui`) :

```html
<link href="https://fonts.googleapis.com/css2?family=Bricolage+Grotesque:opsz,wght@12..96,500;12..96,700&family=Instrument+Sans:wght@400;500&display=swap" rel="stylesheet" />
```

**2. Styles et web components** :

```html
<link rel="stylesheet" href="plbls-ui.css" />
<script type="module" src="plbls-ui.esm.js"></script>
```

```html
<button class="plbls-button plbls-button--contour">Envoyer</button>

<div class="plbls-field">
  <label class="plbls-field__label" for="mail">Adresse e-mail</label>
  <input class="plbls-input" id="mail" type="email" />
</div>

<plbls-modal id="m" title="Confirmer ?">…</plbls-modal>
<script type="module">
  import { toast } from './plbls-ui.esm.js';
  document.getElementById('m').open = true;
  toast.success('Enregistré');
</script>
```

Sans module : `<script src="plbls-ui.js">` expose `window.plbls` (`toast`, `passwordScore`) et enregistre les balises.

## Thème clair / sombre

Clair par défaut. `<html data-theme="dark">` active le sombre,
`<html data-theme="auto">` suit le réglage du système. En JS :
`import { setTheme, getTheme, initTheme } from '@plbls/ui'` (`setTheme('dark')`
mémorise le choix dans `localStorage`, événement `plbls-theme`). Détails :
[`DESIGN_SYSTEM.md`](./DESIGN_SYSTEM.md#thème-sombre).

## Fichiers produits (`dist/`, générés, non versionnés)

| Fichier                              | Contenu                                         |
| ------------------------------------ | ----------------------------------------------- |
| `plbls-ui.css` / `plbls-ui.min.css`  | Jetons + socle + tous les composants            |
| `tokens.css`                         | Jetons seuls (variables sur `:root`)            |
| `plbls-ui.esm.js` / `.esm.min.js`    | Web components + comportements (ES module)      |
| `plbls-ui.js`                        | Même chose en script classique (IIFE)           |

## SCSS

```scss
@use '@plbls/ui/scss';          // tout
@use '@plbls/ui/scss/tokens';   // jetons seuls
```

Les partials (`src/scss/components/_*.scss`) sont indépendants : un composant
non utilisé n'est pas compilé si on ne le `@use` pas.

## Composants

Voir [`DESIGN_SYSTEM.md`](./DESIGN_SYSTEM.md#composants) et la vitrine
[`docs/index.html`](./docs/index.html). Le balisage de référence de chaque
composant est en commentaire en tête de son partial SCSS.

Web components, tous en DOM léger (les classes CSS s'appliquent) :

| Balise                    | Attributs / propriétés          | Événement          |
| ------------------------- | ------------------------------- | ------------------ |
| `plbls-modal`             | `open`, `title`                 | `plbls-close`      |
| `plbls-dropdown-menu`     | enfants `data-action`, `slot="trigger"` | `plbls-action` |
| `plbls-tabs`              | tablist en enfant               | `plbls-change`     |
| `plbls-otp-input`         | `length`, `error`               | `plbls-complete`   |
| `plbls-pagination`        | `page`, `total`, `controlled`   | `plbls-change`     |
| `plbls-data-table`        | `caption`, `.columns`, `.rows`  | `plbls-row-click`  |
| `plbls-dropzone`          | `accept`, `multiple`, `text`    | `plbls-files`      |
| `plbls-copy-field`        | `label`, `value`                | —                  |
| `plbls-password-strength` | `value` ou `for="id-du-champ"`  | —                  |
| `plbls-toast-container`   | + API `toast.show/success/error/info` | —            |

## Développement

```bash
npm ci --prefix ../..   # workspace npm : un seul package-lock.json, à la racine du dépôt
npm run build   # SCSS → CSS, JS → ESM + IIFE dans dist/
npm run watch
npm run serve   # vitrine sur http://localhost:4300/docs/
```

Node 26 (`.nvmrc` de la racine). `dist/` n'est pas versionné : il est reconstruit par la CI et par l'image Docker.
Lancer `npm run build` avant `npm run serve` ou d'ouvrir `docs/index.html` en local.

## Publier une release

Les jobs `image-ui` (image) et `ui-dist` (archives) de `.github/workflows/livraison.yml`
s'exécutent au push d'un tag `ui-vX.Y.Z` : ils vérifient que le tag égale la version de `package.json`,
construisent `dist/`, publient l'image `ghcr.io/plblsfr/plbls-ui` (`X.Y.Z` et `latest`) et créent la release GitHub
(nom = tag, notes = section correspondante du `CHANGELOG.md`).

```bash
# 1. monter la version dans package.json + ajouter « ## x.y.z » au CHANGELOG.md, fusionner dans main
git tag ui-vX.Y.Z
git push origin ui-vX.Y.Z
```

Pièces jointes : `plbls-ui-vX.Y.Z.zip`, `plbls-ui-vX.Y.Z.tgz` (tout `dist/`)
et chaque fichier de `dist/` seul (`plbls-ui.css`, `plbls-ui.min.css`,
`tokens.css`, `tokens.min.css`, `plbls-ui.esm.js`, `plbls-ui.esm.min.js`, `plbls-ui.js`, `plbls-ui.esm.d.ts`, `plbls-ui.esm.min.d.ts`).

## Projets du monorepo

Estompe, permission, pli et le thème Keycloak ne téléchargent rien : `npm run ui:sync` (racine du dépôt, script
`scripts/sync-ui.mjs`) construit ce paquet et copie dans chacun le même dossier `plbls-ui/` (`plbls-ui.esm.min.js`,
`plbls-ui.esm.min.d.ts`, `plbls-ui.min.css`, `tokens.min.css`, `VERSION`, `README.md`). Les fichiers sont commités
(hors ligne, CSP sans CDN) ; `npm run ui:check` (job `front` du workflow `ci`) échoue si une copie diffère de ce paquet.
Après un changement ici : `npm run ui:sync` à la racine et commiter les copies avec le changement.

## Consommer une release dans un projet externe

Téléchargement direct d'un fichier de la release :

```bash
V=ui-v0.3.0
BASE=https://github.com/Plblsfr/plbls.fr/releases/download/$V
curl -fsSLO $BASE/plbls-ui.min.css
curl -fsSLO $BASE/plbls-ui.esm.min.js
# ou toute l'archive : curl -fsSL $BASE/plbls-ui-$V.tgz | tar xz
```

Épingler la version dans un script (ex. `scripts/fetch-ui.sh`) et committer ou
ignorer les fichiers récupérés (`public/vendor/plbls-ui/`), puis :

```html
<link rel="stylesheet" href="/vendor/plbls-ui/plbls-ui.min.css" />
<script type="module" src="/vendor/plbls-ui/plbls-ui.esm.min.js"></script>
```

Le dépôt est privé : passer par `gh release download $V -R Plblsfr/plbls.fr` plutôt que `curl`.

## CDN (cdn.plbls.fr)

Le dépôt publie aussi une image nginx qui sert `dist/` :

| URL                                      | Contenu                                    | Cache    |
| ---------------------------------------- | ------------------------------------------ | -------- |
| `https://cdn.plbls.fr/ui/2.0.0/…`        | une version exacte, **immuable**           | 1 an     |
| `https://cdn.plbls.fr/ui/2/…`            | dernière 2.x                               | 5 min    |
| `https://cdn.plbls.fr/ui/latest/…`       | dernière version                           | 5 min    |

```html
<link rel="stylesheet" href="https://cdn.plbls.fr/ui/2/plbls-ui.min.css" />
<script type="module" src="https://cdn.plbls.fr/ui/2/plbls-ui.esm.min.js"></script>
```

En production, préférer une version exacte ou la majeure (`/ui/2/`) à `latest`. CORS est ouvert (`*`).

**Publier** : changer la version dans `package.json` (et `CHANGELOG.md`), fusionner dans `main`, puis pousser un tag
`ui-vX.Y.Z`. La CI (`.github/workflows/livraison.yml`) construit et publie `ghcr.io/plblsfr/plbls-ui` (`X.Y.Z`, `latest`
et le SHA ; un push sur `main` ne publie que `edge`). Au
démarrage, l'image copie `dist/` dans le volume `/srv` : une version déjà publiée n'est **jamais écrasée**
(un avertissement s'affiche dans les logs si son contenu diffère), seuls `latest` et la majeure bougent.
Une pré-version (`2.1.0-beta.1`) ne touche pas aux alias.

**Réglages du dépôt GitHub** : secret `DOKPLOY_WEBHOOK_URL` (optionnel). Le registre ghcr.io s'authentifie avec `GITHUB_TOKEN`.

**Côté Dokploy** : application Docker (ou Compose avec `docker-compose.yml` collé tel quel), image
`ghcr.io/plblsfr/plbls-ui:latest`, identifiants du registre `ghcr.io` (PAT avec `read:packages`),
**volume sur `/srv`** (sans lui, les anciennes versions disparaissent à chaque déploiement), domaine
`cdn.plbls.fr` vers le port `80`, sonde `GET /health`. Test local :

```bash
docker compose up --build   # remplacer `image:` par `build: .` pour tester sans registre
```

Le CDN pourra accueillir d'autres ressources sous d'autres préfixes (`/fonts/`, `/img/`…) en posant des
fichiers dans le volume.
