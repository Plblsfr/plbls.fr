# Tirelire

Suivi des comptes bancaires de plbls.fr. Ce document décrit ce qui existe aujourd'hui : l'import des relevés de compte PDF du Crédit
Mutuel de Bretagne (CMB), la liste des comptes, des relevés et des opérations, la suppression d'un relevé. Il n'y a pas encore de
section dans l'app, ni de commerçants, ni de catégories.

| Où | Quoi |
|---|---|
| [`api/tirelire`](../../api/tirelire/README.md) | API Rust (axum, sqlx), membre du workspace Cargo, base `tirelire` de PostgreSQL |
| [`openapi.yaml`](openapi.yaml) | routes, vérifiées par un test (chaque route du code y figure, et rien d'autre) |
| [`deploy/tirelire/.env.example`](../../deploy/tirelire/.env.example) | secrets, service `tirelire-api` de [`deploy/docker-compose.yml`](../../deploy/docker-compose.yml) |

## Permission et cloisonnement

Déclarée à aegis par manifeste (application `tirelire`, renvoyé toutes les 10 minutes), fermée par défaut :

| Permission | Ouvre |
|---|---|
| `tirelire:access` | importer ses relevés, voir ses comptes, ses relevés et ses opérations, supprimer un relevé |

Chaque route relève de cette permission ou d'une exception motivée (santé, `GET /me`) : la table des routes l'impose
(`api/tirelire/src/routes.rs`) et un test parcourt toutes les routes (401 sans session ou avec une session invalide, 403 sans la permission).
aegis injoignable : 503 `aegis_unavailable` sur les routes soumises à la permission ; `GET /me` répond quand même, `access` faux.
Sans `AEGIS_TOKEN_TIRELIRE`, tout est refusé.

Authentification : cookie de session du gateway (`__Secure-plbls_sid`) ou `Authorization: Bearer`, vérifié par `GET /auth/me` du gateway
(cache de 30 s). Requête modifiante avec cookie : `Origin` dans `TIRELIRE_ALLOWED_ORIGINS`, sinon 403 `bad_origin`.

Les données sont cloisonnées par personne : chaque ligne des tables `comptes`, `releves` et `operations` porte le `sub` de son
propriétaire, et toute lecture ou suppression filtre sur celui de la session. Il n'y a ni route d'administration ni partage :
un identifiant d'une autre personne répond 404.

## Confidentialité

- Le PDF n'est pas conservé : il est lu en mémoire, puis seuls restent en base son empreinte SHA-256, les métadonnées du relevé,
  ses opérations et le rapport de contrôle. Aucune donnée ne quitte le service : la lecture du PDF est faite dans le processus, sans service tiers.
- Les journaux ne contiennent ni libellé, ni montant, ni IBAN, ni nom de fichier : un import y laisse le nombre d'opérations et de pages.
  Les lignes non reconnues d'un relevé refusé ne sont rendues qu'à la personne qui l'a envoyé, dans la réponse.
- L'IBAN, le numéro de compte et le titulaire sont en base (ils distinguent les comptes) et rendus à leur seul propriétaire.

## Import

`POST /statements/import` reçoit un ou plusieurs PDF (`multipart/form-data`, champ `file` répété ; 24 fichiers, 8 Mo par fichier et 32 Mo
par envoi au plus). Chaque fichier est traité indépendamment et reçoit son résultat : `imported`, `already_imported` ou `rejected`.

L'import se fait en deux couches :

1. **Extraction** (`extraction.rs`) : le PDF devient, page par page, une liste de mots positionnés (texte, x, y depuis le haut de la
   page, largeur, écriture verticale ou non). Le service interprète lui-même les opérateurs de texte du PDF (lu avec la crate `lopdf`)
   et connaît les largeurs des polices standard, que les relevés n'embarquent pas. Les relevés du CMB placent chaque caractère
   séparément, sans espace : deux caractères forment un même mot quand l'écart entre eux est inférieur à 12 % de la taille de la police.
   Les polices composites sans table `ToUnicode` et le texte des objets externes (`Do`) ne sont pas lus.
2. **Parseur CMB** (`cmb.rs`) : il ne lit que ces mots. Les mots d'une page dont les ordonnées diffèrent de 3 points au plus forment une ligne.

Ce que le parseur lit :

- **En-tête** (au-dessus du tableau de chaque page) : numéro du relevé (`N° 3 - 2025`), date d'arrêté (`Arrêté au …`), titulaire,
  numéro de compte, IBAN et BIC.
- **Tableau** : il commence sous la ligne `Date / Date de Valeur / Opération / Débit / Crédit`, répétée sur chaque page. La position
  de ces titres change d'une page à l'autre : les colonnes sont relues sur chaque page.
- **Soldes** : `ANCIEN SOLDE CRÉDITEUR AU <date>` (ou `DÉBITEUR`, solde alors négatif) et `NOUVEAU SOLDE …`. La période du relevé va
  de la date de l'ancien solde à la date d'arrêté.
- **Sections** : une ligne en majuscules à gauche (`VIREMENTS RECUS`, `VIREMENTS EMIS ET PRELEVEMENTS`, `PAIEMENTS PAR CARTE`,
  `SERVICES ET FRAIS BANCAIRES`, ou tout autre titre de même forme), suivie de `Sous-total : …` ou de `(Suite)` en haut d'une page.
  Le sous-total n'est pas lu ; la section est gardée sur chaque opération.
- **Opérations** : `JJ/MM JJ/MM/AAAA libellé montant`. Le montant est un crédit s'il commence à droite du début du titre
  `Crédit`, un débit sinon, jamais selon la section : un remboursement par carte est un crédit de `PAIEMENTS PAR CARTE`. Les milliers sont séparés par
  une espace (`1 234,56`) ; les montants sont des entiers en centimes. L'année de la date d'opération est celle qui la rapproche
  le plus de la date de valeur.
- **Détails** : les lignes sans date, alignées sur le libellé, sont rattachées à l'opération qui précède, y compris en haut de la page suivante.
- **Types** : `CARTE JJ/MM …` (`carte`, avec la date d'achat et le commerçant brut : le texte qui suit la date, ville comprise, le relevé
  ne les séparant pas), `PRLV …` (`prelevement`), `VIR INST …` (`virement_instantane`), `VIR …` (`virement`), `ECH PRET …`
  (`echeance_pret`), `F COTISATION …` (`cotisation`), sinon `autre`. L'année de la date d'achat est celle qui la rapproche le plus de
  la date d'opération : un achat du 30/12 débité le 02/01/2026 date du 30/12/2025.
- **Fin** : `TOTAL DES OPÉRATIONS DU RELEVÉ` donne le total des débits et celui des crédits, puis vient le nouveau solde ; tout ce qui
  suit est ignoré, comme le texte vertical en marge et les pieds de page.

### Contrôle

Un relevé n'est enregistré que si tout ceci est vrai, sinon il est refusé (`checks_failed`) avec le détail :

- la somme des débits lus égale le total des débits du relevé, de même pour les crédits (`gaps`) ;
- l'ancien solde plus les crédits moins les débits égale le nouveau solde (`gaps`) ;
- aucune ligne du tableau n'est restée non reconnue (`unrecognized_lines`, avec la page) ;
- le numéro du relevé, la date d'arrêté, l'IBAN, les deux soldes, les totaux et l'en-tête du tableau de chaque page ont été trouvés (`missing`).

Le rapport du contrôle d'un relevé accepté est gardé avec lui (`checks`).

### Ré-import

Un relevé est identifié par son compte (IBAN), son numéro, son année et sa date d'arrêté. Renvoyer le même fichier (même empreinte)
ou un autre export du même relevé répond `already_imported` sans rien modifier. Pour réimporter un relevé, le supprimer d'abord.

## Données

Migration `api/tirelire/migrations/0001_tirelire.sql` :

| Table | Contenu |
|---|---|
| `comptes` | un compte par personne, banque (`cmb`) et IBAN : BIC, numéro de compte, référence courte, titulaire |
| `releves` | numéro, année, période (`debut`, `date_arrete`), soldes, totaux, nombre de pages, empreinte SHA-256 du PDF, rapport de contrôle |
| `operations` | rang dans le relevé, page, dates d'opération, de valeur et d'achat, section, type, libellé brut, détails, commerçant brut, montant en centimes et sens |

`operations.commercant_id` et `operations.categorie_id` existent déjà, nulles et sans clé étrangère : les tables des commerçants et
des catégories n'existent pas encore, la contrainte sera ajoutée avec elles.
Supprimer un relevé supprime ses opérations ; un compte disparaît avec son dernier relevé.

## Configuration

| Variable | Rôle |
|---|---|
| `TIRELIRE_PORT` | port d'écoute (8080) |
| `TIRELIRE_ALLOWED_ORIGINS` | origines des fronts (CORS, CSRF) |
| `AEGIS_TOKEN_TIRELIRE` | jeton de l'application `tirelire` chez aegis (secret) |
| `DATABASE_URL`, `DATABASE_USER`, `DATABASE_PASSWORD`, `GATEWAY_URL`, `GATEWAY_INTERNE_URL`, `AEGIS_URL` | communes, voir [`deploy/README.md`](../../deploy/README.md) |

Mise en service : créer la base `tirelire` sur `postgres-main` et le secret `secret/tirelire` de Vault (`AEGIS_TOKEN_TIRELIRE`, lu aussi
par aegis), ajouter [`deploy/tirelire/.env.example`](../../deploy/tirelire/.env.example) à l'onglet Environment de la stack, donner au
service `tirelire-api` son domaine (port 8080), puis accorder `tirelire:access` dans Administration › accès.
