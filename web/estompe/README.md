# Estompe — frontend

Svelte 5 + Vite + TypeScript, un seul code pour le web, la PWA (mobile) et l'app Windows, Android et iOS (Tauri 2). Thème
`@plbls/ui` embarqué dans `src/vendor/plbls-ui/` : copie générée depuis `packages/ui` par `npm run ui:sync` à la racine du dépôt (voir son README), polices auto-hébergées (`@fontsource-variable`).

## Développement

```sh
nvm use                                      # Node 26, lu dans le .nvmrc de la racine du dépôt
cp .env.example .env                         # VITE_API_URL, VITE_GATEWAY_URL
npm ci --prefix ../..                        # workspace npm : un seul package-lock.json, à la racine du dépôt
npm run dev          # http://localhost:5173
npm run check        # svelte-check (TypeScript 6 : svelte-check n'est pas encore compatible avec TypeScript 7)
npm test             # tests unitaires (vitest) : markdown/XSS, API, WebSocket, store, jetons PKCE…
npm run build
npm run e2e          # bout en bout : vrai navigateur + vrai backend (Docker et Rust requis)
npm run e2e:entracte # sections selon les permissions (Entracte seul, Estompe seul, les deux) : API simulées, sans Docker
```

`npm run e2e` compile le backend, lance PostgreSQL dans Docker, un faux auth-gateway (`e2e/fake-gateway.mjs`) et deux
utilisateurs dans deux navigateurs (Chromium via Playwright : `npx playwright install chromium`, puis les bibliothèques
système de Chromium doivent être présentes).

## Structure

- `src/lib` : `api` (REST), `realtime` (WebSocket : ticket, reconnexion, ping), `store.svelte.ts` (état et évènements),
  `markdown` (rendu + DOMPurify), `route.svelte.ts` (routeur sur le hash), `voice.svelte.ts` (LiveKit, chargé à la demande),
  `config.ts` (config d'exécution).
- `src/lib/platform.ts` : interface `Platform` (connexion, mises à jour, notifications, partage natif, raccourci global, espaces, emplacements d'interface…) et objet `platform` que tout le frontend appelle. Le frontend ne teste jamais la plateforme (`isDesktop`, agent, `__TAURI_INTERNALS__`) : chaque plateforme fournit son code. Chaque plateforme (`plateforme/` pour le web, `apps/estompe/windows/` et `apps/estompe/android/` à la racine du dépôt) a un `src/platform.ts` qui exporte son `platform` ; le mode de build choisit lequel (`@platform`, alias Vite) : `vite build` (web), `--mode windows`, `--mode android` (les `.env.windows` et `.env.android` portent les URL de production). `src/platform-init.ts`, premier import de `main.ts`, l'installe. Le code partagé par les deux apps Tauri (OAuth PKCE, `native_fetch`, journal debug) est dans `apps/estompe/crates/core/ts/`.
- Un nouveau comportement propre à une plateforme : ajouter une capacité optionnelle à `Platform` (absente = rien à faire), l'implémenter dans le dossier concerné, jamais un `if` dans `src/`. Les tests de `plateforme/`, `apps/estompe/windows/`, `apps/estompe/android/` et `apps/estompe/crates/core/ts/` tournent avec ceux du frontend (`npm test`).
- `src/blocks/<nom>` : un dossier par bloc optionnel (composants, état, tests), avec les noms et dépendances des blocs du backend (`registry.ts`). Le serveur annonce les blocs actifs dans `blocks` de `GET /me` ; `app.on('<nom>')` masque l'interface d'un bloc désactivé ou dont une dépendance l'est. Un nouveau bloc : l'ajouter à `BLOCKS` (le test échoue pour un dossier non déclaré).
- `src/components`, `src/views` : interface du cœur.
- `src/sections/entracte` : section Entracte (films et séries, API à part, [`docs/entracte`](../../docs/entracte/README.md)). L'app est
  celle de plbls.fr : la messagerie d'Estompe et Entracte en sont deux sections, chacune affichée selon les permissions. Au démarrage,
  `GET /me` d'Estompe et `GET /me` d'Entracte disent ce que la personne a : Entracte seul ouvre la phase `entracte` (écran
  `EntracteShell`, ni WebSocket ni route de la messagerie), les deux affichent Entracte à la place de l'onglet films. Avec `entracteUrl`
  configurée (`ENTRACTE_PUBLIC_URL` de l'image, `VITE_ENTRACTE_URL` au build des apps), `filmsApi` lit l'API d'Entracte : les composants de
  `src/blocks/films` (bibliothèque, fiche, lecteur) servent pendant la bascule, puis rejoindront la section au retrait du bloc films.
- `plateforme/` : plateforme web (son `src/` et l'image nginx). Hors de `web/estompe/` : `apps/estompe/windows/` et `apps/estompe/android/` (coques Tauri, chacune avec son `src-tauri`), `apps/estompe/crates/core` (commandes Rust communes aux deux), `.version` à la racine (version de chaque élément : api, web, windows, android).
- `plateforme/docker` : image nginx. Une même image sert tous les environnements : `API_URL`, `GATEWAY_URL`, `VOICE_URL`
  (et `KEYCLOAK_ISSUER` pour l'app Windows, `ENTRACTE_PUBLIC_URL` pour Entracte) sont lus au démarrage du conteneur et génèrent `config.js` et la CSP.

## Tests de bout en bout

`npm run e2e` (Docker et Rust requis) démarre PostgreSQL, un vrai LiveKit, le vrai backend et le build du front, puis pilote Chromium.
Le micro et la caméra sont factices (injectés dans la page). Si Chromium ne démarre pas faute de bibliothèques système, lancer un
navigateur dans Docker : `docker run --network host -v ~/.cache/ms-playwright:/ms -e PLAYWRIGHT_BROWSERS_PATH=/ms mcr.microsoft.com/playwright:v1.50.0-noble sh -c "cd /tmp && npm i playwright-core@<version> && npx playwright-core run-server --port 3111 --host 127.0.0.1"`
puis `PW_TEST_CONNECT_WS_ENDPOINT=ws://127.0.0.1:3111/ npm run e2e`.

`npm run e2e:entracte` (`playwright.entracte.config.ts`, `e2e/entracte.spec.ts`) construit le front avec des API fictives et les simule
dans la page : il vérifie les trois profils (Entracte seul, Estompe seul, les deux). `CHROMIUM_PATH` désigne un Chromium déjà installé
si la version de Playwright n'a pas le sien. La CI le lance dans le job `front`.

## Application Windows

Nécessite Rust et, sous Windows, WebView2 (présent sur Windows 11). Configuration : `VITE_API_URL`, `VITE_GATEWAY_URL` et
**`VITE_KEYCLOAK_ISSUER`** (ex. `https://auth.plbls.fr/realms/default`) à la compilation : elles sont dans `.env.windows` et `.env.android` (modes `windows` et `android`), sans quoi l'app appelle `tauri.localhost` au lieu de l'API.

```sh
cd ../../apps/estompe/windows && PATH=../../../node_modules/.bin:$PATH tauri build   # installeur NSIS dans apps/estompe/windows/src-tauri/target/release/bundle/nsis (le CLI Tauri est installé par `npm ci` à la racine du dépôt)
```

Connexion : Authorization Code + PKCE avec le client public `plbls-desktop`, dans le navigateur système ; redirection
`http://127.0.0.1:<port>/callback` (serveur loopback local). Le jeton d'accès reste en mémoire ; le refresh token est dans le
Credential Manager de Windows. Les appels API partent en `Authorization: Bearer`. L'origine de la fenêtre est
`https://tauri.localhost` : à ajouter à `ALLOWED_ORIGINS` du backend. Pas de certificat de signature : SmartScreen avertira
(« Informations complémentaires » → « Exécuter quand même »).

### Build de test isolé de la production

`apps/estompe/windows/src-tauri/tauri.test.conf.json` se fusionne à `tauri.conf.json` (`--config`) : identifiant `fr.plbls.estompe.test`, nom « Estompe Test »,
sans mises à jour (plugin et artefacts désactivés). Il s'installe et tourne **à côté** de l'app de production : dossier d'installation
et de données (`%LOCALAPPDATA%\fr.plbls.estompe.test`), entrées du Credential Manager (le service est l'identifiant de l'app) et
raccourcis sont distincts. Pas de schéma d'URL ni de deep link sur Windows (la connexion passe par le serveur loopback, port choisi
par l'OS). Il reste branché sur les mêmes URL (`.env.windows`) : pour viser un autre backend, exporter `VITE_API_URL`,
`VITE_GATEWAY_URL` et `VITE_KEYCLOAK_ISSUER` avant la commande.

```sh
apps/estompe/windows/scripts/build-test-windows.sh <ticket>   # apps/estompe/windows/src-tauri/target/…/bundle/nsis/Estompe Test_<version>_x64-setup.exe, copié dans apps/estompe/windows/dist-test/
```

Lancer l'installeur (installation par utilisateur), puis « Estompe Test » depuis le menu Démarrer. Désinstaller depuis les
Paramètres Windows ; supprimer `%LOCALAPPDATA%\fr.plbls.estompe.test` pour repartir de zéro. L'origine de la fenêtre
(`https://tauri.localhost`) est celle de l'app de production.

### Mises à jour automatiques

L'app vérifie les mises à jour 15 s après le démarrage puis toutes les 6 h, et propose de les installer (installation passive,
sans assistant). La clé publique et l'adresse (`https://estompe.plbls.fr/updates/latest.json`) sont dans `tauri.conf.json` ; la clé
privée est dans `~/.tauri/estompe.key` (hors dépôt, **à sauvegarder** : perdue, plus aucune installation ne peut être mise à jour).

Publier une version : fusionner dans `main`, puis lancer à la main le workflow `estompe-livraison-app` sur `main` (onglet Actions de GitHub, « Run workflow ») ou pousser un tag `estompe-app-vX.Y.Z` (l'API a le sien, `estompe-api-vX.Y.Z` : voir `docs/estompe/AGENTS.md`) ; la fusion seule ne construit plus rien. La CI
(`.github/workflows/estompe-livraison-app.yml`) exécute `apps/estompe/windows/scripts/release-windows.sh`, embarque l'installeur signé et `latest.json` dans l'image
`estompe-web` (servis sur `/updates/`) : le déploiement de l'image suffit. Secret du dépôt requis : `TAURI_SIGNING_PRIVATE_KEY`
(contenu de `~/.tauri/estompe.key`). Les apps ne se mettent à jour que si `version` a augmenté : la CI incrémente `.version` (racine) à chaque fusion dans `main` (`scripts/estompe/bump-version.mjs`) d'après le préfixe de la branche (`feat/…` : minor +1, le reste : patch +1, `release/…` : sans effet) et les dossiers modifiés, et au build `bump-version.mjs --apply` recopie `windows` et `android` dans `tauri.conf.json`, `Cargo.toml` et `Cargo.lock`. Voir `docs/estompe/AGENTS.md`. Pour une version majeure, modifier `.version` à la main. L'utilisateur voit un bouton « Mettre à jour » (barre latérale et Paramètres) ; il n'est forcé que si `api/estompe/core/updates.json` le demande au moment de la release (voir « Mises à jour forcées »).

L'app n'installe que des paquets signés par la clé correspondant à la clé publique du build.

### Bobyn (alpha)

Replay instantané intégré à l'app Windows : le moteur Rust (capture WGC, encodage AMD AMF, audio WASAPI, MP4) est dans `apps/estompe/windows/src-tauri/bobyn/`
(espace de travail à part ; `cd apps/estompe/windows/src-tauri/bobyn && cargo test` lance les tests portables). L'onglet « Bobyn (alpha) » des paramètres est masqué :
7 clics sur « Version installée » (Paramètres > Application) le débloquent, 7 autres le verrouillent et arrêtent le service. Les réglages sont dans
`bobyn.json` (dossier de configuration de l'app) ; le service ne démarre que si « Activer Bobyn » est coché.

### Mesures de performance

Paramètres › Débogage, mode debug activé : « Démarrer une session », puis les scénarios guidés (repos 5 min, gros salon, défilement automatique
10 s, flux de messages, partie de jeu avec overlays, fenêtre réduite), « Arrêter la session » et « Exporter les mesures » (un JSON et un CSV des échantillons).

- Rust (`apps/estompe/windows/src-tauri/src/mesures.rs`) relève chaque seconde la mémoire privée, le working set, son pic et le temps CPU du processus
  hôte et de **tous** les processus WebView2 (liste fournie par `ICoreWebView2Environment13::GetProcessExtendedInfos`, rafraîchie toutes les 5 s).
  Un processus de rendu est attribué à la fenêtre principale ou à un overlay d'après l'adresse de ses cadres (`#overlay`, `#msg-overlay`, `#bobyn-overlay`).
- Le front (`web/estompe/src/lib/sonde.ts`) mesure les intervalles entre images (seulement pendant les scénarios qui en ont besoin), la latence
  réception WebSocket → affichage, la latence de saisie et le démarrage (lancement du processus → première conversation affichée).
- Hors session, rien ne tourne : pas de fil d'échantillonnage, pas d'écouteur, pas de boucle d'images.
- L'export ne contient ni contenu de message ni identifiant ; rien n'est envoyé automatiquement.

Comparer plusieurs exports (médiane des sessions de chaque version et jeu d'options WebView2, scénario par scénario) :

```sh
node apps/estompe/windows/scripts/synthese-mesures.mjs mesures/ --sortie synthese.md
```

### Options de WebView2 (mémoire)

Au démarrage, avant toute webview, `apps/estompe/windows/src-tauri/src/webview2_options.rs` passe à WebView2, par la variable
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` (même environnement pour la fenêtre principale et les overlays, sinon WebView2 refuse de les créer) :
`--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,SpareRendererForSitePerProcess,BackForwardCache` (les trois premières sont celles de Tauri par défaut,
réunies en une seule liste car seule la dernière `--disable-features` compte), `--renderer-process-limit=1`, `--disable-site-isolation-trials` et `--js-flags=--optimize-for-size`.
Options de l'étude écartées :
- `CalculateNativeWinOcclusion` : sous Windows, c'est elle qui arrête le rendu d'une fenêtre recouverte, par un jeu par exemple ;
- `--disable-extensions` : sans effet, les extensions de WebView2 sont désactivées par défaut ;
- `--disable-component-update` : aucun gain mémoire attendu, et elle peut bloquer la mise à jour de composants du runtime.

Fenêtre principale réduite ou masquée : niveau de mémoire cible bas (`ICoreWebView2_19::MemoryUsageTargetLevel`), rétabli quand elle reprend le focus.
L'interrupteur « Options d'économie de mémoire de WebView2 » (Paramètres › Débogage) écrit `options-webview2` dans `%LOCALAPPDATA%\fr.plbls.estompe` pour
les désactiver au prochain démarrage et comparer les mesures. Une variable `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` déjà définie est laissée telle quelle.
Les options actives figurent dans le journal de debug et dans l'export de mesures.

### Overlay natif (expérimental)

Feature cargo `overlay-natif`, désactivée par défaut : overlay des participants du vocal rendu par Slint, avec son moteur logiciel, dans une fenêtre Win32 transparente,
sans webview. Fonctionnement, rendu comparé et mesures restantes : [docs/estompe/overlay-natif.md](../../docs/estompe/overlay-natif.md).

## Applications Android et iOS (hors stores)

Même code Tauri 2 que l'app Windows, projet Android commité dans `apps/estompe/android/src-tauri/gen/android` (capacités mobiles : `apps/estompe/android/src-tauri/capabilities/mobile.json`). Connexion en PKCE dans le navigateur système ; redirection `fr.plbls.estompe:/callback` (schéma privé, RFC 8252) au lieu du serveur loopback, que le système suspend avec l'app. **Le client Keycloak `plbls-android` (public, PKCE) doit accepter l'URI de redirection `fr.plbls.estompe:/callback`.** La connexion en cours (state, verifier PKCE) est gardée dans le stockage de l'app pendant 10 minutes : si Android recharge la page ou relance l'app pendant que l'utilisateur est dans le navigateur, la page suivante échange le code reçu au lieu de le perdre ; une redirection reçue hors connexion est ignorée, et un échec de l'échange s'affiche sans renvoyer vers Keycloak. Mise à jour intégrée : l'app lit `/updates/android.json` (version et SHA-256 publiés par `release-android.sh`), télécharge l'APK, vérifie sa somme (le natif refuse un APK dont la somme diffère) et ouvre l'installeur Android (permission « installer des applications inconnues » demandée au premier essai) ; la première installation reste manuelle. Le refresh token n'est pas conservé (pas de coffre natif branché) : reconnexion à chaque lancement.

**Android (APK).** Prérequis : JDK 17, SDK Android + NDK 26, `rustup target add aarch64-linux-android`. Créer une fois la clé de signature, **à sauvegarder** (Android refuse de mettre à jour un APK signé avec une autre clé) :

```sh
keytool -genkeypair -keystore ~/.android/estompe.jks -alias estompe -keyalg RSA -keysize 2048 -validity 10000
ANDROID_KEYSTORE_PASSWORD=… apps/estompe/android/scripts/release-android.sh   # → web/estompe/plateforme/dist-updates/Estompe.apk
```

CI : définir la variable de dépôt `ANDROID_APK=1` et les secrets `ANDROID_KEYSTORE_B64` (`base64 -w0 estompe.jks`) et `ANDROID_KEYSTORE_PASSWORD` ; l'APK est servi sur `/updates/Estompe.apk` (boutons « Télécharger » de la connexion et des Paramètres). Installation : autoriser les sources inconnues sur le téléphone. arm64 uniquement (téléphones récents).

**Push (app fermée) via FCM.** Créer un projet Firebase (gratuit), y ajouter l'app Android `fr.plbls.estompe`, puis :
1. télécharger `google-services.json` dans `apps/estompe/android/src-tauri/gen/android/app/` (ignoré par git ; en CI : secret `GOOGLE_SERVICES_JSON_B64`, `base64 -w0 google-services.json`). Sans lui l'APK se construit, sans push ;
2. générer une clé de compte de service (Paramètres du projet › Comptes de service) et mettre son JSON encodé (`base64 -w0 cle.json`) dans `FCM_SERVICE_ACCOUNT` côté API (`deploy/estompe/.env.example`).

Activation : Paramètres › « Notifications push ». Le serveur envoie alors les mentions et messages privés aux destinataires hors ligne ; Google voit le titre et le texte du message passer par FCM.

**iOS.** Non automatisé : il faut un Mac (Xcode) et Apple n'autorise l'installation hors App Store que via un compte gratuit (app valable 7 jours, à re-signer avec AltStore/Sideloadly) ou payant. Sans cela, la PWA reste l'option : Safari › Partager › « Sur l'écran d'accueil ». Pour tenter l'app native sur un Mac : `npm run tauri ios init`, puis `npm run tauri ios build` et signer l'IPA.

## Mises à jour forcées

`api/estompe/core/updates.json` (embarqué par l'API, lu par la CI via `scripts/estompe/update-policy.mjs`) : `mandatory` (la dernière release s'impose), `minVersion` (tout client en dessous doit se mettre à jour, même après plusieurs releases sautées) et `message` (raison affichée), communs, surchargeables dans `platforms.web|android|windows`. La CI les écrit dans `latest.json` (Windows), `android.json` et `web.json` (web et PWA) ; `force` reprend `mandatory` pour les anciens clients, qui ignorent les autres champs. L'app relit le JSON au démarrage, au retour au premier plan et toutes les 30 minutes (`lib/updater.svelte.ts`) : sans forçage, bandeau « Nouvelle version disponible » (« Plus tard » possible) ; avec forçage, écran bloquant « Mise à jour requise » (`ForcedUpdate.svelte`), après 2 minutes de compte à rebours si l'utilisateur est en vocal ou écrit (brouillons déjà enregistrés par le composeur ; le web se recharge alors tout seul). L'API refuse (426) les clients sous `minVersion`, ce qui couvre ceux qui n'ont pas relu le JSON. Remettre `mandatory` à `false` et `minVersion` à `""` une fois la migration terminée (les versions des clients connectés sont dans les journaux de l'API et le tableau de bord d'administration).
