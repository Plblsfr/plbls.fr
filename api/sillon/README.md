# sillon

API de Sillon, service de musique hors d'Estompe : documentation fonctionnelle dans [`docs/sillon`](../../docs/sillon/README.md),
routes dans [`openapi.yaml`](../../docs/sillon/openapi.yaml).

| Module | Rôle |
|---|---|
| `config.rs` | variables `SILLON_*` et communes (valeurs dans [`plbls.env`](../../plbls.env), secrets dans [`deploy/sillon/.env.example`](../../deploy/sillon/.env.example)) |
| `routes.rs` | table des routes : chacune a une permission ou une exception motivée ; tests de couverture et de conformité à OpenAPI (dans les deux sens) |
| `garde.rs`, `identite.rs`, `droits.rs` | gateway (`/auth/me`, cookie ou Bearer), aegis (manifeste, `/v1/effective`), sections de `GET /me`, CORS et CSRF |
| `etat.rs` | état partagé (configuration, base, client HTTP, aegis, verrou de synchronisation) et tâches de fond (manifeste, synchronisation périodique) |
| `interne.rs` | santé, disponibilité, `GET /me` |
| `http.rs` | réponses JSON et erreurs `{error, message}`, comparaison de jetons à temps constant |
| `lidarr.rs` | client de l'API v1 de Lidarr (`Client` : artistes, albums, pistes, fichiers, images ; `requete`, `envoyer`, `get` pour les autres appels) |
| `catalogue.rs` | synchronisation (une à la fois, relance), présence des pistes, albums et artistes, accès aux fichiers pour les lots suivants |
| `filtre.rs`, `ffprobe.rs` | filtre des fichiers (extension, double extension, nom suspect, conteneur, flux audio) et analyse ffprobe |
| `images.rs` | pochettes et images d'artistes : téléchargées depuis Lidarr, cache en base, trois tailles |
| `bibliotheque.rs` | routes de la bibliothèque, recherche, administration du catalogue, webhook ; JSON publics réutilisables |
| `lecture.rs` | `POST /tracks/{id}/playback` (choix direct ou transcodé, jeton, gains) et `GET /stream/{jeton}` (droits revérifiés, direct, cache ou encodage en cours) |
| `flux.rs` | jetons de flux signés (`Billet`, `signer`, `valider`), `Range`, service d'un fichier ou d'un fichier en cours d'encodage |
| `transcodage.rs` | qualités et formats, ligne ffmpeg, `Transcodeur` : cache disque, encodages simultanés plafonnés, éviction LRU |
| `volume.rs` | mesure EBU R128 par ffmpeg (`mesurer`), stockage par piste et `contenu_le`, gains piste et album |
| `analyse/` | analyse audio : `decodage.rs` (ffmpeg en PCM mono 22 050 Hz, lu en flux), `descripteurs.rs` (FFT, `Extracteur`, `Descripteurs`, vecteur de 43 dimensions), `file.rs` (file de tâches en base), `voisins.rs` (`Voisins` en mémoire), `mod.rs` (ouvriers, pause pendant un transcodage, rechargement, routes d'administration) |
| `preferences.rs` | `GET`/`PUT /preferences` (qualité et découverte par personne, écriture partielle) |
| `playlists.rs` | playlists privées (éléments ordonnés, plage déplacée, doublons signalés, plafonds) ; aides communes : corps JSON (`objet`, `texte`, `champ_uuid`…), `contenu` (pistes, album, artiste ou playlist d'une requête), `pistes_de_source`, `pistes_json` |
| `likes.rs` | likes de pistes, albums et artistes ; « Titres likés » |
| `file.rs` | file de lecture par personne, révision et 409, « lire ensuite », « ajouter à la file », aléatoire, piste suivante disponible |
| `ordre.rs` | ordre de lecture aléatoire (`ordonner`, pur ; `melanger` et `prolonger`, qui chargent les données), insertion des suggestions (`inserer`, `a_prolonger`) et déplacement d'une plage (`deplacer`) |
| `recommandation/` | `score.rs` (score pur, `Poids`, `Habitude`, `Candidat`, `Detail`), `mod.rs` (`charger` : entrées du score en quelques requêtes ; `Donnees` : `noter`, `choisir` ; routes `similar`, `dismiss`, `explain`) |
| `listenbrainz.rs` | artistes similaires et enregistrements populaires par MBID, gardés en base, débit limité, pause après erreur |
| `decouverte.rs` | recherche en tâche de fond d'un titre absent et envoi de son album par `ajouts`, promotion des découvertes likées, `GET /admin/discovery` |
| `historique.rs` | écoutes (fin, écoute qui compte, source, suggérée), écoutes récentes, plus écoutés par période |
| `ajouts.rs` | ajout autonome (`sillon:add`) : recherche d'absents, ajout d'un album ou d'un artiste, envoi à Lidarr, suivi (file, historique, webhooks), « Mes ajouts » |
| `quota.rs` | réglages de l'ajout (`SILLON_LIDARR_RACINE`…), occupation du disque, estimation des tailles par le profil de qualité |
| `admin_ajouts.rs` | administration des ajouts : occupation, ajouts de tous, relance, suppression par Lidarr avec exclusion, albums par taille (filtre des albums jamais écoutés) |
| `migrations/` | schéma de la base `sillon` (`0001_sillon.sql` vide ; `0002_catalogue.sql` : catalogue ; `0003_lecture.sql` : mesures de volume, préférences ; `0004_playlists.sql` : playlists, likes, file, historique ; `0005_ajouts.sql` : ajouts et exclusions ; `0006_analyse.sql` : analyses et file de tâches ; `0007_recommandation.sql` : similarités, refus, préférence et colonnes de découverte) |

## Schéma du catalogue

| Table | Contenu |
|---|---|
| `artistes`, `albums`, `pistes` | identifiant public `id` (UUID), MBID (`mbid` ; `mbid_enregistrement` et `mbid_piste` pour une piste), `lidarr_id` (jamais exposé), `present`, `present_le` (dernière arrivée), colonnes générées `cle` (titre sans accents, minuscules) et `recherche` (`tsvector`) |
| `pistes` | `album_id`, `artiste_id`, `disque`, `numero`, `duree`, `explicite` ; unique par `(album_id, mbid_enregistrement)` ; jamais supprimée (absente quand plus aucun fichier) |
| `albums` | `type`, `types_secondaires`, `sortie`, `annee` |
| `fichiers` | `piste_id`, `chemin` réel (sous `SILLON_MEDIAS`), `taille`, `modifie_le`, `conteneur`, `codec`, `debit`, `frequence`, `canaux`, `bits`, `duree`, `contenu_le` (date du dernier changement de contenu : nouveau fichier ou fichier modifié), `vu_le` ; supprimé quand il disparaît |
| `genres`, `artistes_genres`, `albums_genres` | genres de Lidarr par artiste et par album |
| `fichiers_ecartes` | chemin, nom, raison |
| `synchronisations` | par source (`lidarr`) : dernière réussite, dernier essai, erreur |
| `images` | cache des images (clé `<type>/<uuid>/<taille>`, type `cover`, `poster` ou `fanart` ; version = URL Lidarr) |
| `mesures` | par `(piste_id, contenu_le)` du fichier : `niveau` (LUFS intégré), `pic` (dBTP), `plage` (LRA), `echantillons`, `frequence` ; la mesure d'un fichier remplacé efface l'ancienne |
| `analyses` | par `piste_id` : `contenu_le` du fichier analysé, `version`, `vecteur` (`real[]`, 43 dimensions à l'échelle nominale), `tempo` (battements par minute, nul sans pulsation), `tonalite` (0 = do … 11 = si), `majeur` |
| `taches_analyse` | par `fichier_id` : `contenu_le`, `version`, `etat` (`attente`, `en_cours`, `faite`, `erreur`), `tentatives`, `erreur` (sans chemin) |
| `preferences` | par `sub` : `qualite` (`originale`, `haute`, `normale`, `basse`), `decouverte` (vraie par défaut) |
| `artistes_similaires`, `enregistrements_populaires`, `similarites_releves` | résultats de ListenBrainz par MBID d'artiste (similaire et score entre 0 et 1 ; enregistrement, titre, album, écoutes, rang) et date du dernier relevé par genre (`similaires`, `populaires`) |
| `refus`, `refus_albums` | « ne plus proposer » : piste par personne ; MBID de l'album d'une découverte refusée, par personne |
| `ajouts` | `type` (`album`, `artiste`), `mbid`, titre et artiste, `origine` (`manuel`, `decouverte`), auteur (`sub`, `nom`), `etat` (`envoye`, `telechargement`, `partiel`, `disponible`, `echec`, `supprime`), `raison`, `estimation` (octets), `albums` (MBID des albums visés), identifiants Lidarr (jamais exposés), `surveille` (surveillance et recherche faites), `envoye_le` ; un seul ajout actif par `(type, mbid)` ; pour une découverte : `similaire_a` (nom de l'artiste écouté), `enregistrement` et `titre_piste` (titre visé), `promu_le` (découverte devenue ajout ordinaire) |
| `exclusions` | MBID d'album ou d'artiste supprimé par un admin, qui ne peut plus être ajouté |

## Schéma des playlists, de la file et de l'historique (`0004_playlists.sql`)

| Table | Contenu |
|---|---|
| `playlists` | `sub`, `nom`, `description`, dates |
| `playlists_pistes` | élément : `id`, `playlist_id`, `piste_id`, `position` (unique par playlist, contrainte différée), `ajoute_le` |
| `likes` | `(sub, type, cible)` : type `track`, `album` ou `artist` |
| `files` | une ligne par `sub` : `revision`, source (`source_type`, `source_id`, `source_nom`), `index_courant`, `position`, `aleatoire`, `repetition`, `graine`, `suggestions` |
| `files_elements` | `rang` (ordre de lecture), `id` de l'élément, `piste_id`, `rang_source` (ordre de la source, rendu quand l'aléatoire s'arrête), `suggeree` |
| `ecoutes` | `sub`, `piste_id`, `debut`, `duree_ecoutee`, `duree_piste`, `fin` (`completed`, `skipped`, `interrupted`), `compte`, source, `suggeree` |

Les références à `pistes` n'ont pas de `ON DELETE` : une piste n'est jamais supprimée.

Fonction SQL `sans_accents(text)` et configuration de recherche `sillon` (`unaccent` puis `simple`).

## Interfaces pour les autres modules

- `catalogue::synchroniser(&AppState)` : lance une synchronisation, ou en demande une à la fin de celle en cours.
- `catalogue::fichier_de_piste(&PgPool, Uuid) -> Option<FichierPiste>` : fichier d'une piste (chemin réel, conteneur, codec, débit,
  fréquence, canaux, bits, durée, taille, `contenu_le`) ; le plus gros si plusieurs. `None` = piste absente.
- `catalogue::fichiers_modifies_depuis(&PgPool, depuis, limite)` : fichiers nouveaux ou changés après `depuis` (par `contenu_le`, puis `id`).
- `catalogue::taille_totale(&PgPool)` : octets de tous les fichiers du catalogue.
- `catalogue::recalculer_presence(&PgPool)` : recalcule `present` des pistes, albums et artistes.
- `lidarr::Client::new(&config::Lidarr, &reqwest::Client)` : `requete(méthode, "/chemin")` (préfixe `/api/v1`, en-tête `X-Api-Key`), `envoyer`,
  `get`, `liste`, et les lectures typées (`artistes`, `albums`, `pistes`, `fichiers`, `image`, `version`). Erreurs en texte, sans URL.
- `bibliotheque::{COLONNES_PISTE, JOINTURES_PISTE, piste_publique, pistes_par_ids}` (et leurs équivalents artiste et album) : JSON public
  d'une piste, dans l'ordre des identifiants donnés ; `curseur`, `lire_curseur` : pagination.
- `ordre::melanger(&AppState, &Contexte).await -> Vec<Place>` : seule construction de l'ordre aléatoire de la file. `Contexte` : `sub`,
  `graine` (gardée dans `files.graine`), `premier` (élément mis en tête), `elements` (identifiant d'élément et piste), `suggestions` ;
  `Place` : élément existant ou nouvelle piste, `suggeree` (une `Place` sans élément devient un élément suggéré). `ordre::prolonger`
  complète les suggestions de la fenêtre à venir. Les calculs sont purs (`ordre::ordonner`, `ordre::inserer` sur `recommandation::Donnees`).
- `recommandation::charger(&AppState, &Demande { sub, source, exclues, graine, candidats, maintenant })` : entrées du score ;
  `Donnees::noter(piste, précédente) -> score::Detail`, `Donnees::choisir`, `Donnees::ecartee` ; `recommandation::metas`, `artistes_principaux`.
- `listenbrainz::similaires(&AppState, mbid)`, `listenbrainz::populaires(&AppState, mbid)` : lecture en base, rafraîchie si besoin ;
  jamais d'erreur quand le service est en panne (résultats gardés, ou liste vide).
- `decouverte::declencher(&AppState, &Personne, source)` : lance la recherche en tâche de fond (sans effet si une recherche tourne déjà
  pour la personne) ; `decouverte::promouvoir(&PgPool)` ; `quota::budget_decouverte`, `quota::decouvertes_du_jour`.
- `file::avancer(présentes, index, pas, boucle)` : prochaine piste disponible ; `file::element(pool, sub, id)` : `suggeree` d'un élément.
- `playlists::{contenu, pistes_de_source, pistes_json}` : pistes d'une requête ou d'une source, JSON public des pistes en gardant
  l'ordre et les doublons ; `playlists::plafond(message)` : 422 `limit_reached`.
- `ajouts::ajouter_album(&AppState, DemandeAjout { mbid, personne: Auteur { sub, nom }, origine })` (et `ajouter_artiste`, `ajouter(…, Genre, …)`) :
  chemin d'ajout complet hors requête HTTP (`origine` : `Origine::Manuel` ou `Origine::Decouverte`, limite journalière comptée par origine) ;
  `Ok(Resultat::Envoye(id) | DejaPresent | EnCours(id) | Refuse(Refus::DisquePlein { occupation, estimation } | LimiteJournaliere { limite } | Exclu | BudgetDecouverte { utilise, budget }))`,
  `Err(ErreurAjout)` (`reponse_erreur` la traduit en HTTP, `reponse_resultat` le résultat).
- `ajouts::est_exclu(&PgPool, &[mbid])`, `ajouts::suivre(&AppState)` (met à jour les états), `ajouts::relancer(&AppState, id)`.
- `quota::occupation(&AppState) -> Occupation { taille, en_cours, plafond }` (`depasse(estimation)`, `public()`) ; `quota::estimer(durée, débit)`.
- `config::chemin_local(...)` : traduction et vérification d'un chemin Lidarr (sous `SILLON_MEDIAS`, liens résolus).
- `volume::mesurer(ffmpeg, &Path) -> Mesure` (niveau, pic, plage, échantillons, fréquence ; un seul décodage), `volume::enregistrer`,
  `volume::lire(&PgPool, piste, contenu_le)`, `volume::mesure_de(&PgPool, ffmpeg, &FichierPiste)` (lue en base, sinon mesurée et
  enregistrée), `volume::gains(&PgPool, piste, Option<&Mesure>)`, `volume::non_mesurees_de_l_album`.
- `preferences::qualite_de(&PgPool, sub) -> Qualite`.
- `AppState::lecture().transcodeur()` : `Transcodeur` (`lancements`, `en_cours`, `lecteurs`, `plafonner`, `taille`).

## Ajouter une route

Une entrée dans `routes::table()` (méthode, chemin, garde `Garde::Droit(..)` ou exception motivée, handler) et la même route dans
`docs/sillon/openapi.yaml` : sinon `routes::tests::routes_et_openapi_identiques` échoue. Le test
`tests_api::chaque_route_protegee_refuse_sans_session_et_sans_permission` la couvre ensuite sans changement ; un paramètre de chemin y
reçoit un UUID d'exemple (`exemple()` dans `tests_api.rs`).

## Lancer

`just sillon-dev-run` (après `just dev-up`, avec le gateway et aegis), ou `cargo run -p sillon` avec les variables exportées.
Image : `docker build -f api/sillon/Dockerfile .` (contexte : racine du dépôt), avec ffmpeg et ffprobe.

## Tests

`TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/postgres just test-sillon` : une base jetable par test, migrations
appliquées, faux gateway et faux aegis (`testutil.rs` : `env()`, `env_avec()`, personnes `personne`, `alice`, `bruno`, `admin`).
Les tests de l'ajout (`ajouts.rs`, module `tests`) étendent ce faux Lidarr (`lidarr_ajouts` : recherche MusicBrainz `mb_artistes`,
`mb_albums`, création d'artiste, surveillance, commandes, file, historique, suppressions).
Les tests du catalogue (`tests_api.rs`, `bibliotheque()`) montent un faux Lidarr (`Monde`, modifiable pendant le test) et de vrais fichiers
flac, mp3 et opus générés par ffmpeg dans un dossier temporaire ; ils demandent ffmpeg et ffprobe (sinon ignorés, avec un message).
Playlists, likes, file et historique : tests dans les modules (`playlists::tests` : `catalogue()` insère un petit catalogue en base,
`creer()`, `pistes_de()`), dont des appels simultanés (tâches tokio) pour les déplacements et les révisions de la file ; la piste retirée
du catalogue passe par `tests_api::bibliotheque()`.
Les tests de lecture (`lecture.rs`, module `tests`) réutilisent `bibliotheque_avec()` (cache dans un dossier temporaire) et, pour le
plafond d'encodages et l'échec d'encodage, un faux ffmpeg (script qui attend un fichier témoin ou échoue pour `libopus`, et passe la
main au vrai ffmpeg sinon).
Sans `TEST_DATABASE_URL`, les tests passent par testcontainers (ignorés sans Docker).
- `AppState::analyse().voisins() -> Arc<Voisins>` : vecteurs normalisés des pistes présentes et analysées (instantané, remplacé à chaque
  rechargement). `voisins(piste, n) -> Option<Vec<(Uuid, f32)>>` (les plus proches d'abord, sans la piste), `distance(a, b) -> Option<f32>`,
  `vecteur(piste) -> Option<&[f32]>`, `barycentre(&[Uuid]) -> Option<Vec<f32>>` (pistes non analysées ignorées),
  `plus_proches(&vecteur, n, &exclure) -> Vec<(Uuid, f32)>`, `len()`. Piste non analysée : `None`. Balayage complet : moins de
  50 ms par requête sur 50 000 pistes en build de test non optimisé.
- `analyse::signaler(&AppState)` : alimente la file et réveille les ouvriers (appelé à la fin de `catalogue::synchroniser`) ;
  `analyse::analyser(ffmpeg, chemin) -> Result<Descripteurs, String>` ; `analyse::recharger(&AppState)` ; `analyse::VERSION`.
