# Tirelire

Suivi des comptes bancaires de plbls.fr. Ce document décrit ce qui existe aujourd'hui : l'import des relevés de compte PDF du Crédit
Mutuel de Bretagne (CMB), la liste des comptes, des relevés et des opérations, la suppression d'un relevé, les catégories, les
règles de classement, les commerçants et la file « à classer ». Les statistiques (mois, évolution, habitudes) n'existent pas encore.

| Où | Quoi |
|---|---|
| [`api/tirelire`](../../api/tirelire/README.md) | API Rust (axum, sqlx), membre du workspace Cargo, base `tirelire` de PostgreSQL |
| [`openapi.yaml`](openapi.yaml) | routes, vérifiées par un test (chaque route du code y figure, et rien d'autre) |
| [`web/estompe/src/sections/tirelire`](../../web/estompe/README.md) | section de l'app Estompe (`#/tirelire`) : import, opérations, à classer, catégories et règles |
| [`deploy/tirelire/.env.example`](../../deploy/tirelire/.env.example) | secrets, service `tirelire-api` de [`deploy/docker-compose.yml`](../../deploy/docker-compose.yml) |

## Permission et cloisonnement

Déclarée à aegis par manifeste (application `tirelire`, renvoyé toutes les 10 minutes), fermée par défaut :

| Permission | Ouvre |
|---|---|
| `tirelire:access` | importer ses relevés, voir ses comptes, ses relevés et ses opérations, supprimer un relevé, gérer ses catégories, ses règles et ses commerçants |

Chaque route relève de cette permission ou d'une exception motivée (santé, `GET /me`) : la table des routes l'impose
(`api/tirelire/src/routes.rs`) et un test parcourt toutes les routes (401 sans session ou avec une session invalide, 403 sans la permission).
aegis injoignable : 503 `aegis_unavailable` sur les routes soumises à la permission ; `GET /me` répond quand même, `access` faux.
Sans `AEGIS_TOKEN_TIRELIRE`, tout est refusé.

Authentification : cookie de session du gateway (`__Secure-plbls_sid`) ou `Authorization: Bearer`, vérifié par `GET /auth/me` du gateway
(cache de 30 s). Requête modifiante avec cookie : `Origin` dans `TIRELIRE_ALLOWED_ORIGINS`, sinon 403 `bad_origin`.

Les données sont cloisonnées par personne : chaque ligne des tables `comptes`, `releves`, `operations`, `categories`, `regles`,
`commercants` et `variantes` porte le `sub` de son propriétaire, et toute lecture ou modification filtre sur celui de la session.
Il n'y a ni route d'administration ni partage : un identifiant d'une autre personne répond 404 dans un chemin, 400 dans un corps.

## Confidentialité

- Le PDF n'est pas conservé : il est lu en mémoire, puis seuls restent en base son empreinte SHA-256, les métadonnées du relevé,
  ses opérations et le rapport de contrôle. Aucune donnée ne quitte le service : la lecture du PDF est faite dans le processus, sans service tiers.
- Les journaux ne contiennent ni libellé, ni commerçant, ni mot-clé, ni montant, ni IBAN, ni nom de fichier : un import y laisse le nombre d'opérations et de pages.
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

## Catégorisation

### Catégories

Au premier accès d'une personne (première route de la section ou premier import), le service crée son jeu de catégories et ses règles
par défaut, une seule fois (`personnes`) : ce qu'elle renomme, archive ou supprime ensuite n'est jamais recréé.

| Nature (`kind`) | Catégories par défaut | Rôle |
|---|---|---|
| `fixe` | Logement, Crédit, Assurances, Télécom, Énergie, Abonnements, Frais bancaires, Impôts et taxes | dépense fixe |
| `variable` | Courses, Restaurants, Transports, Santé, Loisirs et sorties, Shopping, Divers | dépense variable |
| `epargne` | Épargne | un virement vers un livret n'est pas une dépense |
| `revenu` | Revenus | |
| `neutre` | Transferts neutres | mouvement entre ses propres comptes, sans effet |

Une catégorie se renomme, change de nature, reçoit un plafond mensuel facultatif (centimes) et s'archive (`PATCH /categories/{id}`) ;
archivée, elle garde ses opérations et ses règles mais ne peut plus être choisie. Elle ne se supprime pas. `color` est le rang de sa
teinte, fixé à la création : l'app donne une teinte aux huit premiers rangs (Logement, Courses, Restaurants, Shopping, Loisirs et
sorties, Transports, Santé, Épargne) et un point gris aux suivants, toujours avec le nom.

### Commerçants

Chaque opération reçoit une clé de commerçant ou de tiers (`normalisation.rs`, `operations.commercant_cle`), déduite du commerçant brut
d'un paiement par carte, sinon du libellé : majuscules, accents et ponctuation retirés, mots techniques de tête retirés (`PRLV`, `SEPA`,
`VIR`, `INST`, puis un mot de direction et une civilité pour un virement), nombres et références retirés (un nombre de trois chiffres
au plus reste en tête de nom), formes juridiques et code pays retirés en fin (`SA`, `SAS`, `SARL`, `FR`…). Le nom affiché en est tiré
(`Marche Colline Villeneuve`). Sur un relevé du CMB la ville n'est pas séparable du commerçant : deux villes donnent deux clés.

Une clé appartient à un commerçant (`variantes`) : un commerçant est créé pour chaque clé nouvelle. `POST /merchants/{id}/merge`
rattache les clés d'autres commerçants à celui-ci, `POST /merchants/{id}/split` en détache ; ces rattachements restent en base et
s'appliquent aux imports suivants, y compris après la suppression de tous les relevés.

### Classement

Une opération prend, dans l'ordre (`classement.rs`) :

1. la catégorie choisie pour elle seule (`category_source: operation`), que rien ne modifie ensuite ;
2. la catégorie de son commerçant (`commercant`) : reclasser une opération avec `scope: merchant` la fixe pour tout le commerçant ;
3. la catégorie de la première règle qui lui convient (`regle`), dans l'ordre des règles ;
4. sinon aucune : elle est « à classer » (`GET /unclassified`, regroupée par commerçant, et `GET /unclassified/count`).

Une règle convient quand le type de l'opération est le sien (s'il est donné), que le titre de sa section est le sien (s'il est donné)
et que le libellé contient l'un de ses mots-clés, en mots entiers et sans tenir compte de la casse, des accents ni de la ponctuation
(`LOYER` ne convient pas à `EMPLOYER`). Une règle créée se place en tête ; `PUT /rules/order` fixe l'ordre.

Règles par défaut, dans l'ordre : type `echeance_pret` → Crédit ; type `cotisation` → Frais bancaires ; section
`SERVICES ET FRAIS BANCAIRES` → Frais bancaires ; `LIVRET`, `EPARGNE` → Épargne ; `SALAIRE` → Revenus ; `LOYER` → Logement ;
`ASSURANCE`, `ASSURANCES`, `MUTUELLE` → Assurances ; `IMPOT`, `IMPOTS`, `DGFIP`, `TRESOR PUBLIC` → Impôts et taxes ; `TELECOM` → Télécom ;
`ELECTRICITE`, `ENERGIE`, `GAZ` → Énergie ; `PHARMACIE`, `MEDECIN`, `DOCTEUR`, `DENTISTE`, `OPTICIEN`, `LABORATOIRE`, `HOPITAL`,
`CLINIQUE` → Santé ; `BOULANGERIE`, `BOUCHERIE`, `EPICERIE`, `SUPERMARCHE`, `HYPERMARCHE`, `SUPERETTE`, `PRIMEUR`, `FROMAGERIE`,
`POISSONNERIE` → Courses ; `RESTAURANT`, `BRASSERIE`, `PIZZERIA`, `CREPERIE`, `BISTROT`, `TRAITEUR` → Restaurants ; `SNCF`, `PEAGE`,
`PARKING`, `TAXI`, `CARBURANT`, `AUTOROUTE`, `STATIONNEMENT` → Transports ; `CINEMA`, `THEATRE`, `MUSEE`, `LIBRAIRIE`, `PISCINE` →
Loisirs et sorties. Elles ne contiennent que des types d'opération et des mots génériques, aucune enseigne : ce qu'elles ne
reconnaissent pas reste à classer.

Le classement est rejoué sur toutes les opérations de la personne (`classement::rejouer`) à chaque import, dans la transaction de
l'import, et à chaque changement de règle, de catégorie de commerçant, de fusion ou de séparation, dans la transaction du changement.
Il est idempotent : rejoué sans changement, il ne modifie aucune ligne. Les modifications d'une même personne sont sérialisées par un
verrou sur sa ligne de `personnes`. `personnes.normalisation` porte la version de la normalisation appliquée à ses opérations
(`classement::NORMALISATION`) : les opérations importées avant cette étape, ou avant un changement de la normalisation, sont
normalisées et classées à l'accès suivant.

### Totaux

`bilan.rs` (`bilan`, `calculer`) additionne une période par catégorie : un crédit classé dans une catégorie de dépense (un
remboursement) diminue le total de cette catégorie ; l'épargne, les revenus et les transferts neutres sont comptés à part, hors
dépenses. Aucune route ne l'expose encore.

## Données

Migrations `api/tirelire/migrations/` (`0001_tirelire.sql`, `0002_categories.sql`) :

| Table | Contenu |
|---|---|
| `comptes` | un compte par personne, banque (`cmb`) et IBAN : BIC, numéro de compte, référence courte, titulaire |
| `releves` | numéro, année, période (`debut`, `date_arrete`), soldes, totaux, nombre de pages, empreinte SHA-256 du PDF, rapport de contrôle |
| `operations` | rang dans le relevé, page, dates d'opération, de valeur et d'achat, section, type, libellé brut, détails, commerçant brut, montant en centimes et sens, clé de commerçant, commerçant, catégorie, origine du classement (`operation`, `commercant`, `regle`) et règle appliquée |
| `personnes` | une ligne par personne : jeu par défaut créé, version de la normalisation |
| `categories` | nom (unique par personne, sans tenir compte de la casse), clé des catégories par défaut, nature, plafond mensuel, rang de teinte, rang, archivée |
| `regles` | catégorie, mots-clés normalisés, type et section facultatifs, rang, clé des règles par défaut |
| `commercants` | nom affiché, catégorie de tout le commerçant |
| `variantes` | clé de commerçant → commerçant, par personne ; `manuelle` après une fusion ou une séparation |

`operations.commercant_id`, `operations.categorie_id` et `operations.regle_id` ont une clé étrangère (`ON DELETE SET NULL`).
Supprimer un relevé supprime ses opérations ; un compte disparaît avec son dernier relevé ; les commerçants, leurs variantes, les
catégories et les règles restent.

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
