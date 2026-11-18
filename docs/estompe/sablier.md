# Évènements de statistiques (Sablier)

L'API d'Estompe émet des évènements vers [Sablier](../sablier/README.md), le service de statistiques, par
`commun::emission::Emetteur` (Redis Stream `sablier:evenements`). L'émission ne bloque jamais une requête : sans `SABLIER_REDIS_URL`, ou
avec un Redis arrêté, Estompe répond de la même façon et les évènements sont abandonnés. Le Redis d'Estompe (`REDIS_URL`) n'est pas
concerné : la file de Sablier est une connexion à part.

Code : [`api/estompe/core/src/sablier.rs`](../../api/estompe/core/src/sablier.rs) (messages, fichiers, réactions, import) et
[`api/estompe/blocks/vocal/src/temps.rs`](../../api/estompe/blocks/vocal/src/temps.rs) (temps en vocal, partage d'écran, salon cinéma).

## Évènements émis

`sub` est toujours la personne comptée (`keycloak_sub`) ; `serveur` et `salon` sont les identifiants que l'API donne au front.

| `type` | Quand | `donnees` |
|---|---|---|
| `estompe.message` | message envoyé (`POST /channels/{id}/messages`, `POST /dms/{id}/messages`) | `serveur`, `salon`, `prive` |
| `estompe.fichier` | même envoi, s'il porte des pièces jointes | `serveur`, `salon`, `prive`, `nombre` (pièces jointes) |
| `estompe.reaction` | réaction ajoutée : un `donnee` pour qui réagit, un `recue` pour l'auteur du message | `sens`, `emoji` |
| `estompe.vocal.temps` | temps passé dans un salon vocal avec au moins une autre personne | `serveur`, `salon`, `secondes` |
| `estompe.vocal.appel` | départ d'un salon vocal | `serveur`, `salon`, `secondes` passées à plusieurs pendant cette présence |
| `estompe.partage_ecran` | partage d'écran | `secondes` |
| `estompe.cinema.temps` | temps passé dans un salon cinéma avec au moins une autre personne | `secondes` |
| `estompe.cinema.seance` | première fois qu'une personne est à plusieurs pendant sa présence dans un salon cinéma | aucune |

Confidentialité :

- aucun évènement ne porte le texte d'un message, sa longueur, un nom de fichier ni l'identité d'une autre personne ;
- message ou fichier d'une conversation privée : `serveur` et `salon` nuls, `prive` vrai, jamais l'identifiant de la conversation ni de
  l'interlocuteur ;
- l'identifiant d'un évènement de réaction contient une empreinte (SHA-256 tronqué de la personne qui réagit et de l'emoji), pas son
  identité : l'évènement `recue` de l'auteur ne désigne pas qui a réagi ;
- vocal : Estompe n'a pas d'appel hors serveur (un salon vocal appartient toujours à un serveur). S'il en vient un, ses évènements
  porteront `serveur` nul et `salon` `"prive"`, jamais l'identifiant de la conversation ni d'un interlocuteur.

Ce qui n'est pas compté : un sondage (son message est écrit par le compte système), les messages du compte système, une réaction retirée,
une réaction à son propre message (pas de `recue`), une réaction à un message du compte système (pas de `recue`), les fichiers ajoutés
par une modification du message.

## Vocal, partage d'écran et salon cinéma

La présence vient des webhooks LiveKit, déjà inscrits dans la table `voice_sessions`. À chaque arrivée, départ ou partage d'écran, et
toutes les 60 secondes, l'API relit cette table et la donne à une machine à états en mémoire (`temps::Compteur`, horloge passée en
paramètre), qui rend les évènements à émettre.

- Le temps ne court que pour les personnes d'un salon où elles sont au moins deux, micro coupé ou non (le micro n'entre pas dans le
  calcul). Seule, une personne n'accumule rien.
- `estompe.vocal.temps` est émis par tranches : à chaque changement de présence dans le salon, et toutes les 60 secondes pour un long
  segment. Les fractions de seconde sont reportées sur la tranche suivante.
- `estompe.vocal.appel` est émis une fois au départ du salon (ou au changement de salon), avec le total des secondes passées à
  plusieurs pendant cette présence ; rien si ce total est nul.
- `estompe.partage_ecran` compte la durée du partage, seule ou à plusieurs, par tranches de 60 secondes et à la fin du partage ; quitter
  le salon termine le partage.
- Un salon `cinema` n'alimente que les statistiques cinéma : `estompe.cinema.temps` au lieu de `estompe.vocal.temps`, et pas
  d'`estompe.vocal.appel`. Une « séance » est une présence dans le salon : `estompe.cinema.seance` est émis une fois par présence, dès
  que la personne est à plusieurs. L'état de lecture du film n'est pas consulté ; le temps réellement regardé est compté par Entracte.
- Une présence est comptée 12 heures au plus (comme `voice_stats`) : un départ que LiveKit n'aurait pas signalé ne compte pas
  indéfiniment.
- Le vocal en arrière-plan est une présence comme une autre.

Limites :

- l'état est en mémoire, comme le hub temps réel : une seule instance d'`estompe-api`. Deux instances compteraient deux fois ;
- au redémarrage, la tranche en cours (moins de 60 secondes), le partage d'écran en cours et le total de l'appel en cours sont perdus.
  Le comptage reprend au premier relevé de `voice_sessions` (60 secondes au plus après l'ouverture de la base) : l'appel repart de
  zéro, et une personne déjà en salon cinéma à plusieurs reçoit une nouvelle `estompe.cinema.seance` ;
- sans `SABLIER_REDIS_URL`, le relevé périodique n'est pas lancé.

## Import de l'historique

Commande unique du binaire `estompe-api`, à lancer une fois après la mise en service de Sablier :

```sh
docker exec <conteneur estompe-api> /estompe-api importer-sablier
```

En local (variables de `deploy/dev.env` chargées) : `cargo run -p estompe-api -- importer-sablier`.

Elle lit la base d'Estompe (`DATABASE_URL`, `DATABASE_USER`, `DATABASE_PASSWORD`) sans rien y écrire et publie dans la file
(`SABLIER_REDIS_URL`, `SABLIER_REDIS_PASSWORD`), en attendant chaque écriture : un évènement par message, par message avec fichiers et par
réaction (donnée et reçue), daté de sa création et marqué `importe`. Elle s'arrête à la première erreur (code 1) et affiche sinon un
bilan : messages, messages avec fichiers, réactions données et reçues, évènements publiés. Sans `SABLIER_REDIS_URL`, elle refuse de
démarrer.

- Un message supprimé avant l'import n'existe plus en base : il n'est pas compté, ni ses réactions ni ses fichiers.
- Le vocal et le partage d'écran passés ne sont pas importés.
- Elle se relance sans doublon : les identifiants sont déterministes et Sablier ignore un identifiant déjà reçu.
- Les évènements émis en temps réel portent les mêmes identifiants (avec `importe` faux) : un import lancé après la mise en service
  n'ajoute rien pour ce qui a déjà été émis.

| Évènement | Identifiant |
|---|---|
| `estompe.message` | `import:estompe:message:<id du message>` |
| `estompe.fichier` | `import:estompe:fichier:<id du message>` |
| `estompe.reaction` | `import:estompe:reaction:<id du message>:<empreinte>:donnee` ou `…:recue` |

Les évènements de vocal, de partage d'écran et de cinéma ont un identifiant aléatoire (UUID v4).

## Visibilité d'un profil pour Sablier

Avant de servir les statistiques qu'une personne partage sur son profil, Sablier demande à Estompe si celle qui regarde peut ouvrir ce
profil : `GET /internal/profile-visible?viewer=<sub>&target_user=<identifiant Estompe>` avec
`Authorization: Bearer <SABLIER_ESTOMPE_TOKEN>`, de serveur à serveur sur le réseau de la stack. Réponse :
`200 {"visible": true, "sub": "<sub de la cible>"}` ou `200 {"visible": false}`.

- La cible est désignée par `target_user` (son identifiant Estompe, `users.id`) ou par `target` (son `sub`), exactement l'un des deux.
  Le front ne connaît que l'identifiant Estompe des autres personnes : Estompe le résout et ne rend le `sub` qu'à Sablier, et
  seulement quand le profil est visible.

- `visible` suit la règle des personnes côtoyées, celle des badges du profil, de l'avatar et du statut (`users::co_members`) : vrai pour
  soi-même, pour une personne qui partage un serveur, et pour une personne avec qui une conversation privée existe ; faux sinon, et pour
  un `sub` ou un identifiant inconnu d'Estompe. Estompe n'a pas de blocage entre personnes.
- Sans `SABLIER_ESTOMPE_TOKEN` dans la configuration : 404. Jeton absent ou faux : 401 (comparaison en temps constant). `viewer`
  manquant ou vide, `target` et `target_user` absents ou donnés ensemble, `target_user` qui n'est pas un uuid : 400.
- La route ne relève d'aucune permission (`HORS_PERMISSION` du catalogue) : elle n'a pas de session, seul le jeton l'ouvre.

## Variables

| Variable | Rôle |
|---|---|
| `SABLIER_REDIS_URL` | adresse de la file ; vide : aucun évènement émis |
| `SABLIER_REDIS_PASSWORD` | mot de passe, ajouté à l'adresse si elle n'en porte pas |
| `SABLIER_FILE_MAX` | plafond du flux (500 000 par défaut) |
| `SABLIER_ESTOMPE_TOKEN` | jeton attendu par `GET /internal/profile-visible` ; vide : route coupée (404) |

Les trois premières sont communes à la stack ([`plbls.env`](../../plbls.env), [`deploy/.env.example`](../../deploy/.env.example)) ;
le jeton est un secret de Sablier ([`deploy/sablier/.env.example`](../../deploy/sablier/.env.example)), et Sablier joint Estompe par
`SABLIER_ESTOMPE_URL` (`plbls.env`). `estompe-api` charge tout le `.env` et partage ses réseaux avec `sablier-api`.
