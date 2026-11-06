# App de test Windows

« Estompe Test » est l'app Windows construite à part, pour essayer une branche sans toucher à l'app de production. Les deux
s'installent et tournent **en même temps**.

## Ce qui est distinct de la production

`apps/estompe/windows/src-tauri/tauri.test.conf.json` se fusionne à `tauri.conf.json` (`--config`) :

| | Production | Test |
|---|---|---|
| Nom | Estompe | Estompe Test |
| Identifiant | `fr.plbls.estompe` | `fr.plbls.estompe.test` |
| Dossier d'installation et de données | `%LOCALAPPDATA%\fr.plbls.estompe` | `%LOCALAPPDATA%\fr.plbls.estompe.test` |
| Credential Manager (refresh token) | service `fr.plbls.estompe` | service `fr.plbls.estompe.test` |
| Instance unique (EST-113) | une par utilisateur | indépendante de la production |
| Mises à jour automatiques | oui | **non** (plugin et artefacts désactivés) |
| Signature | clé de l'updater | aucune |

Raccourcis et entrée « Applications » de Windows sont aussi séparés. Pas de schéma d'URL sur Windows : la connexion passe par
le serveur loopback (port choisi par l'OS), donc rien n'entre en conflit.

## Construire

Prérequis : ceux de l'app Windows (voir `web/estompe/README.md`) ; la compilation croisée depuis Linux/WSL utilise mingw et NSIS.

```sh
npm ci
apps/estompe/windows/scripts/build-test-windows.sh EST-123   # <ticket> obligatoire ; PROFILE=ci pour un build plus rapide (sans LTO)
```

Résultat : `Estompe-Test-<ticket>-setup.exe` (ex. `Estompe-Test-EST-123-setup.exe`) dans `apps/estompe/windows/dist-test/` (ignoré par git) **et**,
sous WSL, copié dans le dossier Téléchargements de Windows (`/mnt/c/Users/<utilisateur>/Downloads`, déduit de `$USER` ; `WINUSER`
ou `DOWNLOADS` pour un autre utilisateur ou dossier). La version est celle de `tauri.conf.json`.

## Installer et supprimer

Lancer l'installeur (par utilisateur, SmartScreen avertira : « Informations complémentaires » → « Exécuter quand même »), puis
ouvrir « Estompe Test ». Désinstaller depuis les Paramètres Windows ; supprimer `%LOCALAPPDATA%\fr.plbls.estompe.test` pour
repartir de zéro.

## Limites

- L'app de test vise **le même backend** que la production (`.env.windows`). Pour un autre backend, exporter `VITE_API_URL`,
  `VITE_GATEWAY_URL` et `VITE_KEYCLOAK_ISSUER` avant le build. Les données créées pendant les essais sont donc réelles.
- L'origine de la fenêtre est celle de la production (`https://tauri.localhost`) : la même configuration CORS et Keycloak s'applique.
- Pas de mise à jour : reconstruire et réinstaller par-dessus.
