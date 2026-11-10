# Films et séries : mettre en place Jellyfin

Jellyfin reste dans son propre compose. Estompe ne l'expose pas : seule l'API le joint, par un réseau Docker partagé, et les applications n'appellent que l'API.

## 1. Réseau partagé

Créer le réseau une fois sur l'hôte, puis le déclarer comme externe dans le compose de Jellyfin :

```sh
docker network create estompe-jellyfin
```

```yaml
services:
  jellyfin:
    networks: [estompe-jellyfin]
networks:
  estompe-jellyfin:
    external: true
```

Ne publier aucun port (`ports:`) ni domaine pour Jellyfin une fois la configuration terminée.

## 2. Bibliothèques

- Monter les dossiers de Radarr et Sonarr **en lecture seule** (`:ro`).
- Créer une bibliothèque « Films » (type films) et une bibliothèque « Séries » (type séries).
- Laisser l'analyse des médias active : Estompe contrôle le conteneur et les flux à partir de ces métadonnées, et écarte un fichier non analysé (`sans_flux_video`).

## 3. Compte et clé de service

- Créer un compte de service dédié à Estompe, avec accès aux deux bibliothèques.
- Tableau de bord > Clés API : créer une clé dédiée à Estompe (`JELLYFIN_API_KEY`).
- Désactiver les autres comptes.

## 4. Réduire la surface

- Réseau : désactiver l'accès distant.
- Retirer les plugins inutiles.

## 5. Transcodage

- Dossier temporaire de transcodage dédié, sur un volume de taille suffisante.
- Limiter les transcodages simultanés, au moins autant que `FILMS_MAX_TRANSCODAGES`.
- Activer l'accélération matérielle si l'hôte en dispose.

Chaque spectateur en HLS occupe un transcodage. Au-delà de la limite, l'API répond `serveur_charge` : mesurer la charge avant de la relever.

## 6. Estompe (variables)

| Variable | Valeur |
|---|---|
| `BLOCK_FILMS` | `true` |
| `JELLYFIN_URL` | `http://jellyfin:8096` |
| `JELLYFIN_API_KEY` | la clé de l'étape 3 |
| `STREAM_SECRET` | `openssl rand -hex 32` |

Dans `deploy/estompe/.env.example` (secrets dans `secret/estompe` de Vault), sauf `JELLYFIN_NETWORK=estompe-jellyfin`, dans le `.env` du compose (`deploy/.env.example`).
`FILMS_MAX_FLUX` (2 par utilisateur), `FILMS_MAX_TRANSCODAGES` (2 au total) et `FILMS_CACHE_SECONDES` (60) sont facultatives.

## 7. Permission

Créer la permission `estompe:movies` dans permission.plbls.fr et l'accorder aux personnes concernées. Sans elle, l'onglet est masqué et les routes du bloc répondent 403.

## 8. Fichiers écartés

Seules ces extensions sont proposées : `.mkv .mp4 .m4v .webm .mov .avi`, et les sous-titres texte `.srt .vtt .ass .ssa`. Les fichiers écartés (extension, double extension, conteneur incohérent, absence de flux vidéo, nom suspect) sont visibles dans l'administration d'Estompe.
