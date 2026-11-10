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

Les variables propres à l'import sont passées à la commande seulement, jamais dans l'onglet Environment de Dokploy :

| Variable | Rôle |
|---|---|
| `DATABASE_URL`, `DATABASE_USER`, `DATABASE_PASSWORD` | communes, déjà dans le conteneur `entracte-api` : bases `entracte` et `estompe` du serveur partagé (`postgres-main`) |
| `ENTRACTE_IMPORT_ESTOMPE_DATABASE_URL` | facultative : URL complète d'une autre base d'Estompe (défaut : base `estompe` du serveur commun) |
| `ENTRACTE_IMPORT_JELLYFIN_URL` | Jellyfin joint depuis `entracte-api` (réseau `JELLYFIN_NETWORK`), par exemple `http://jellyfin:8096` |
| `ENTRACTE_IMPORT_JELLYFIN_API_KEY` | clé d'API Jellyfin (celle d'Estompe, `JELLYFIN_API_KEY`, ou une clé créée pour l'import) |

## Procédure en production

1. Lancer l'import depuis le conteneur `entracte-api` (il joint `postgres-main` par `postgres-net` et Jellyfin par
   `JELLYFIN_NETWORK`) ; la clé est lue dans le shell, pas écrite dans la commande :

   ```sh
   read -rs ENTRACTE_IMPORT_JELLYFIN_API_KEY && export ENTRACTE_IMPORT_JELLYFIN_API_KEY
   docker exec -e ENTRACTE_IMPORT_JELLYFIN_API_KEY \
     -e ENTRACTE_IMPORT_JELLYFIN_URL=http://jellyfin:8096 <conteneur entracte-api> /app/entracte importer-estompe
   ```

2. Relire le rapport : lignes lues, personnes, progressions importées et gardées, « déjà vus » importés, lignes écartées, puis la liste
   des titres non résolus (`nom [id Jellyfin] : raison (n lignes)`). Un titre absent du catalogue d'Entracte peut être ajouté à
   Radarr/Sonarr, synchronisé, puis l'import relancé.

En local (variables de `deploy/dev.env` chargées) : `ENTRACTE_IMPORT_JELLYFIN_URL=… ENTRACTE_IMPORT_JELLYFIN_API_KEY=… cargo run -p entracte -- importer-estompe`.
Code et tests : `api/entracte/src/import_estompe.rs` (deux bases PostgreSQL, l'une avec les tables du bloc films d'Estompe, et un faux Jellyfin).
