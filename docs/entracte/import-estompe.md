# Import de l'historique d'Estompe

Étape 2 de la [bascule](README.md#bascule-depuis-le-bloc-films-destompe) : la sous-commande `entracte importer-estompe` du binaire
d'Entracte recopie les progressions et les « déjà vus » du bloc films d'Estompe (`films_progression`, `films_items`) dans Entracte.
À lancer une fois, Jellyfin encore en marche : c'est lui qui donne l'identifiant TMDB de chaque titre.

## Ce que fait l'import

- Lit la base d'Estompe en lecture seule (transactions `read only` imposées à la connexion) et ne touche à rien d'autre que les tables
  `progression` et `vus` d'Entracte, dans une seule transaction.
- Chaque `films_items.jellyfin_id` est lu dans Jellyfin (`GET /Items?Ids=…`) : un film par son identifiant TMDB, un épisode par
  l'identifiant TMDB de sa série, son numéro de saison et son numéro d'épisode, comme Entracte identifie ses épisodes.
- La personne est la même des deux côtés : `users.keycloak_sub` d'Estompe est le `sub` du gateway qu'Entracte enregistre.
- Une progression garde sa date (`maj` d'Estompe). Elle n'écrase pas une progression d'Entracte de même date ou plus récente.
- Un titre vu dans Estompe devient un « déjà vu » daté de la dernière progression enregistrée (Estompe ne datait pas le vu) ; un vu plus
  récent déjà présent dans Entracte est gardé.
- Rejouable : relancer l'import ne crée aucun doublon (clés `(sub, titre)`) et ne réécrit rien qui n'a pas changé.
- Un titre non résolu n'arrête pas l'import : il est listé dans le rapport avec sa raison (absent de Jellyfin, sans identifiant TMDB,
  série absente de Jellyfin, absent du catalogue d'Entracte). Une ligne de durée invalide est écartée et comptée.
- S'arrête sans rien écrire si Jellyfin ou une des bases est injoignable, ou si le catalogue d'Entracte est vide.

Seuls les titres déjà présents dans le catalogue d'Entracte reçoivent l'historique : la synchronisation avec Radarr et Sonarr doit
être faite avant (Administration › état). Un titre que seul Jellyfin connaissait reste hors d'Entracte et apparaît dans le rapport.

## Variables

Passées à la commande seulement, jamais dans l'onglet Environment de Dokploy :

| Variable | Rôle |
|---|---|
| `ENTRACTE_DATABASE_URL` | base d'Entracte (déjà dans le conteneur `entracte-api`) |
| `ENTRACTE_IMPORT_ESTOMPE_DATABASE_URL` | base d'Estompe, avec un rôle en lecture seule (ci-dessous) |
| `ENTRACTE_IMPORT_JELLYFIN_URL` | Jellyfin joint depuis `entracte-api` (réseau `JELLYFIN_NETWORK`), par exemple `http://jellyfin:8096` |
| `ENTRACTE_IMPORT_JELLYFIN_API_KEY` | clé d'API Jellyfin (celle d'Estompe, `JELLYFIN_API_KEY`, ou une clé créée pour l'import) |

## Procédure en production

1. Rôle en lecture seule dans `estompe-postgres` :

   ```sh
   docker exec -i <conteneur estompe-postgres> psql -U estompe -d estompe <<'SQL'
   CREATE ROLE entracte_import LOGIN PASSWORD '<mot de passe>';
   GRANT SELECT ON users, films_items, films_progression TO entracte_import;
   SQL
   ```

2. Lancer l'import depuis le conteneur `entracte-api` (il joint `estompe-postgres` par le réseau de la stack et Jellyfin par
   `JELLYFIN_NETWORK`) ; les secrets sont lus dans le shell, pas écrits dans la commande :

   ```sh
   read -rs ENTRACTE_IMPORT_ESTOMPE_DATABASE_URL && export ENTRACTE_IMPORT_ESTOMPE_DATABASE_URL
   # postgres://entracte_import:<mot de passe>@estompe-postgres:5432/estompe?sslmode=disable
   read -rs ENTRACTE_IMPORT_JELLYFIN_API_KEY && export ENTRACTE_IMPORT_JELLYFIN_API_KEY
   docker exec -e ENTRACTE_IMPORT_ESTOMPE_DATABASE_URL -e ENTRACTE_IMPORT_JELLYFIN_API_KEY \
     -e ENTRACTE_IMPORT_JELLYFIN_URL=http://jellyfin:8096 <conteneur entracte-api> /app/entracte importer-estompe
   ```

3. Relire le rapport : lignes lues, personnes, progressions importées et gardées, « déjà vus » importés, lignes écartées, puis la liste
   des titres non résolus (`nom [id Jellyfin] : raison (n lignes)`). Un titre absent du catalogue d'Entracte peut être ajouté à
   Radarr/Sonarr, synchronisé, puis l'import relancé.

4. Retirer le rôle :

   ```sh
   docker exec -i <conteneur estompe-postgres> psql -U estompe -d estompe -c 'DROP OWNED BY entracte_import; DROP ROLE entracte_import;'
   ```

En local : `ENTRACTE_DATABASE_URL=… ENTRACTE_IMPORT_ESTOMPE_DATABASE_URL=… ENTRACTE_IMPORT_JELLYFIN_URL=… ENTRACTE_IMPORT_JELLYFIN_API_KEY=… cargo run -p entracte -- importer-estompe`.
Code et tests : `api/entracte/src/import_estompe.rs` (deux bases PostgreSQL, l'une avec les tables du bloc films d'Estompe, et un faux Jellyfin).
