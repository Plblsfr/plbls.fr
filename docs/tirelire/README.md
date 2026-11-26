# Tirelire

Suivi des comptes bancaires de plbls.fr. Ce document décrit ce qui existe aujourd'hui : l'import des relevés de compte PDF du Crédit
Mutuel de Bretagne (CMB), la liste des comptes, des relevés et des opérations, la suppression d'un relevé, les catégories, les
règles de classement, les commerçants, la file « à classer », la recherche, les filtres et l'export CSV des opérations, et les
statistiques par période (accueil, vue du mois, détail d'une catégorie) et le suivi dans le temps (historique par catégorie,
tendances, solde du compte, comparaison d'une année sur l'autre). Les habitudes n'existent pas encore.

| Où | Quoi |
|---|---|
| [`api/tirelire`](../../api/tirelire/README.md) | API Rust (axum, sqlx), membre du workspace Cargo, base `tirelire` de PostgreSQL |
| [`openapi.yaml`](openapi.yaml) | routes, vérifiées par un test (chaque route du code y figure, et rien d'autre) |
| [`web/estompe/src/sections/tirelire`](../../web/estompe/README.md) | section de l'app Estompe (`#/tirelire`) : accueil, vue du mois, détail d'une catégorie, évolution, import, opérations (recherche, filtres, export), à classer, catégories et règles |
| [`deploy/tirelire/.env.example`](../../deploy/tirelire/.env.example) | secrets, service `tirelire-api` de [`deploy/docker-compose.yml`](../../deploy/docker-compose.yml) |

## Permission et cloisonnement

Déclarée à aegis par manifeste (application `tirelire`, renvoyé toutes les 10 minutes), fermée par défaut :

| Permission | Ouvre |
|---|---|
| `tirelire:access` | importer ses relevés, voir, chercher et exporter ses comptes, ses relevés et ses opérations, voir ses statistiques, supprimer un relevé, gérer ses catégories, ses règles et ses commerçants |

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
- Les journaux ne contiennent ni libellé, ni commerçant, ni mot-clé, ni texte cherché, ni montant, ni IBAN, ni nom de fichier : un import y laisse le nombre d'opérations et de pages.
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

`bilan.rs` (`bilan`) additionne une liste d'opérations par catégorie : un crédit classé dans une catégorie de dépense (un
remboursement) diminue le total de cette catégorie ; l'épargne, les revenus et les transferts neutres sont comptés à part, hors
dépenses. Les statistiques s'appuient dessus.

## Statistiques

`GET /stats/months` (périodes disponibles et leurs totaux), `GET /stats/month` (tout ce qu'affichent l'accueil et la vue du mois, en un
appel) et `GET /stats/month/categories/{id}` (détail d'une catégorie). Tout est calculé par le service (`stats.rs`, sans accès à la
base ; `mois.rs` lit les opérations, les catégories et les relevés de la personne), en centimes entiers ; l'app ne fait qu'afficher.
Les rapports sont des entiers en points de base (1 % = 100), arrondis au plus proche, absents (`null`) quand le dénominateur est nul
ou négatif ; une moyenne est arrondie au centime le plus proche.

### Périodes et rattachement

`mode=calendar` (défaut) ou `mode=statement` ; `period` désigne la période (`AAAA-MM`, ou la date d'arrêté `AAAA-MM-JJ`), la plus
récente à défaut.

- **Date de rattachement** d'une opération : sa date d'achat pour un paiement par carte, sinon sa date d'opération. Un achat du 30
  décembre débité le 2 janvier compte en décembre.
- **Mois calendaire** : une opération appartient au mois de sa date de rattachement. Les périodes disponibles sont les mois couverts
  par un relevé (du lendemain de la date de l'ancien solde à la date d'arrêté) ou qui portent une opération. Un mois est `complete`
  quand les relevés importés le couvrent du premier au dernier jour, sans trou ; il est `open` (en cours) quand il dépasse la date
  d'arrêté du dernier relevé importé : `known_through` est alors cette date, jamais la date du jour, puisque les données viennent
  de relevés. Un mois ni complet ni en cours est partiel (par exemple le premier mois d'un relevé qui commence le 8).
- **Par relevé** : une période par date d'arrêté, de la date de l'ancien solde (`from`) à la date d'arrêté (`to`), toujours
  complète. Une opération appartient à la période du relevé qui la porte ; sur la courbe, elle est placée au jour de sa date de
  rattachement, ramené dans la période (un achat antérieur au relevé compte au premier jour). Plusieurs comptes arrêtés le même jour
  forment une seule période.
- **Période précédente** : le mois calendaire qui précède s'il est disponible, ou le relevé précédent. **Moyennes** : sur les 3 et
  les 12 dernières périodes complètes qui précèdent, ou sur celles qui existent, avec leur nombre (`periods`) ; aucune : `null`.

`GET /operations` et l'export acceptent `date=purchase` : `from` et `to` portent alors sur la date de rattachement, ce qui rend la
liste filtrée identique aux opérations comptées par les statistiques d'un mois calendaire. En mode par relevé, le filtre `statement` suffit.

### Définitions

| Grandeur | Calcul |
|---|---|
| Dépenses | débits moins crédits (remboursements) des catégories `fixe` et `variable`, plus les débits à classer, comptés sur une ligne « À classer » distincte pour que le total reste juste |
| Revenus | crédits moins débits des catégories `revenu`, plus les crédits à classer (`income.unclassified`, à part) |
| Épargne | versements moins retraits des catégories `epargne` |
| Neutre | exclu de tout |
| Reste à vivre | revenus − dépenses − épargne |
| Taux d'épargne | épargne ÷ revenus ; absent si les revenus sont nuls ou négatifs |
| Part fixe, variable, à classer | chacune ÷ dépenses |
| Budget | somme des plafonds mensuels des catégories de dépense non archivées ; absent si aucune n'a de plafond. `uncapped` : dépenses hors catégories plafonnées (catégories sans plafond et « à classer ») |
| Par catégorie | montant, part des dépenses, écart avec la période précédente, avec la moyenne des 3 et des 12 dernières périodes complètes, avec le plafond (`cap_delta`, `cap_bp`), montants des 12 dernières périodes (`trend`) |

Invariant vérifié par les tests, relevés réels compris : la somme des catégories et de « À classer » égale les dépenses, dans les deux modes.

### Ce qui dépasse

Une catégorie de dépense est signalée par rapport à son plafond s'il existe (`reference: cap`), dès 1 € d'écart
(`SEUIL_PLAFOND`) ; sinon par rapport à sa moyenne des 3 dernières périodes complètes (`reference: average`, avec le nombre de
périodes), si l'écart atteint 10 € (`SEUIL_MOYENNE`) et 10 % (`SEUIL_MOYENNE_BP`) et que cette moyenne est strictement positive : une
catégorie sans historique n'est pas un dépassement. La liste est classée par écart en centimes ; `rank_amount` et `rank_percent`
donnent les deux classements (un écart sans pourcentage, plafond à zéro, passe en dernier du second).

### Cumul et projection

`cumulative.current` : dépenses cumulées jour par jour (jusqu'au dernier jour connu pour une période en cours). `previous` et
`average` (cumul moyen des 12 dernières périodes complètes au plus) sont ramenées à la durée de la période : au-delà de leur propre
durée, et au dernier jour, elles portent leur total.

La projection n'existe que pour un mois calendaire en cours, couvert depuis son premier jour et connu sur 5 jours au moins
(`PROJECTION_JOURS_MIN`) ; sinon `projection_reason` dit pourquoi (`complete`, `partial`, `too_early`). Il n'y en a jamais en mode
par relevé : un relevé est toujours complet. Méthode :

- dépenses variables et à classer : montant atteint × jours de la période ÷ jours connus (le rythme des jours connus est prolongé) ;
- charges fixes : chaque catégorie fixe est portée à son montant habituel (moyenne des 3 dernières périodes complètes) si elle
  ne l'a pas encore atteint, sinon gardée à son montant ; elles ne sont pas mises au prorata, un loyer ne se répète pas ;
- `fixed_pending` : charges fixes habituelles pas encore passées ; `budget_delta` : projection moins budget.

## Évolution

`GET /stats/history`, `GET /stats/balance` et `GET /stats/years` (`evolution.rs`, sans accès à la base), avec les définitions
ci-dessus : mêmes périodes, même rattachement, mêmes dépenses. `window` choisit la fenêtre : les `6`, `12` (défaut) ou `24`
dernières périodes, ou `all`.

### Historique par catégorie

Un montant par catégorie de dépense et par période de la fenêtre, « à classer » sur sa ligne, et le total. Invariant vérifié par
les tests, relevés réels compris : pour chaque période, catégories + « à classer » = total = dépenses de `/stats/month`.

- **Moyenne** : sur les périodes complètes de la fenêtre seulement (un mois partiel ou en cours la fausserait) ; aucune : `null`.
- **Plafond** : `over_cap` compte les périodes de la fenêtre, complètes ou non, strictement au-dessus du plafond actuel de la
  catégorie ; `total.over_budget` fait de même avec le budget (somme des plafonds).
- **Graphique** : les 5 catégories au plus fort total sur la fenêtre (`CATEGORIES_DU_GRAPHIQUE`), les autres regroupées dans
  `chart.others`, « à classer » à part.

### Tendance

Calculée par catégorie et pour le total sur les 6 dernières périodes complètes (`TENDANCE_PERIODES`), quelle que soit la
fenêtre affichée, pour que le verdict ne change pas avec elle :

1. pente de Theil-Sen : la médiane des pentes de toutes les paires de périodes. Un mois isolé très haut ou très bas (un gros
   achat, un remboursement) ne la déplace pas, contrairement à une régression par moindres carrés ;
2. `delta` = pente × (nombre de périodes − 1) : la variation entre la première et la dernière période, en centimes ;
3. `delta_bp` = `delta` ÷ moyenne de ces périodes ;
4. `up` ou `down` si `delta` atteint 10 € (`SEUIL_TENDANCE`) et 10 % de la moyenne (`SEUIL_TENDANCE_BP`), `stable` sinon ;
   `insufficient` sous 3 périodes complètes (`TENDANCE_MIN`).

Conséquence assumée : une marche d'escalier au dernier mois (un loyer qui vient d'augmenter) reste « stable » tant qu'elle
n'est pas confirmée ; l'écart du mois se lit dans la vue du mois.

### Solde du compte

Par compte, à partir des soldes imprimés sur les relevés : l'ancien solde à sa date, puis jour par jour le solde de la veille
plus les crédits moins les débits du jour, à la date d'opération (c'est elle qui fait le solde, pas la date d'achat). Le dernier
jour d'un relevé retombe sur son nouveau solde ; `reconciled` le vérifie pour tous les relevés du compte. Les jours qu'aucun
relevé ne couvre ne sont jamais interpolés : la courbe est coupée en `segments` et les trous sont listés dans `gaps`. `min` et
`max` portent sur les jours couverts de la fenêtre, du premier jour de la première période affichée au dernier jour connu.

### Année sur l'autre

`GET /stats/years` compare le mois de la période demandée (la plus récente à défaut) au même mois de l'année précédente, et le
cumul depuis janvier, pour le total, chaque catégorie de dépense et « à classer ». En mode par relevé, un relevé appartient au
mois de sa date d'arrêté. Le cumul comparé (`ytd_compared`) ne porte que sur les mois présents dans les deux années
(`compared_months`), pour ne pas comparer onze mois à trois ; `partial` signale qu'un de ces mois n'est couvert qu'en partie.
Sans aucune période l'année précédente, `has_previous` est faux et les comparaisons valent `null`.

## Recherche, filtres et export

`GET /operations` et `GET /operations/export.csv` partagent la même sélection (`selection.rs`), toujours limitée aux opérations de
la personne ; les filtres se combinent (ET) :

| Paramètre | Sélectionne |
|---|---|
| `q` | les opérations dont le libellé, les détails, le commerçant lu sur le relevé ou le nom du commerçant contiennent chaque mot cherché (8 mots et 100 caractères au plus), sans tenir compte de la casse ni des accents ; `%`, `_` et `\` sont cherchés tels quels (`commun::sql::contient`) |
| `min`, `max` | un montant en centimes, bornes incluses, sans tenir compte du sens ; `min` supérieur à `max` : 400 |
| `direction` | `debit` ou `credit` |
| `type` | `carte`, `prelevement`, `virement`, `virement_instantane`, `echeance_pret`, `cotisation` ou `autre` |
| `statement`, `from`, `to`, `category`, `merchant`, `unclassified` | un relevé, une période (dates d'opération incluses), une catégorie, un commerçant, les opérations sans catégorie |

La casse et les accents sont retirés en SQL des deux côtés de la comparaison (`lower` et `translate`, sans extension PostgreSQL) :
la recherche lit les opérations de la personne sans index dédié, ce qui convient à quelques milliers de lignes par personne.
`GET /operations` rend, en plus de la page, `total`, `total_debits` et `total_credits` : le nombre et les sommes de toute la sélection.

`GET /operations/export.csv` rend toutes les lignes de la sélection, de la plus récente à la plus ancienne, lues et envoyées en flux
(une seule requête SQL, morceaux de 16 Ko). Le fichier s'ouvre dans un tableur français : UTF-8 avec BOM, séparateur `;`, fins de
ligne CRLF, dates `JJ/MM/AAAA`, montants à virgule décimale sans séparateur de milliers. Colonnes : Date, Date de valeur, Date
d'achat, Libellé, Détails (séparés par ` | `), Commerçant (nom du commerçant, sinon celui lu sur le relevé), Catégorie, Nature,
Type, Section, Débit et Crédit (positifs, l'un des deux vide), Montant (signé, négatif pour un débit), Relevé. Une cellule de
texte qui commence par `=`, `+`, `-`, `@`, une tabulation ou un retour à la ligne est précédée d'une apostrophe, pour qu'un
tableur ne l'évalue pas comme une formule ; une cellule qui contient `;`, un guillemet ou un saut de ligne est mise entre
guillemets, guillemets doublés ; les caractères de contrôle sont retirés. Le fichier s'appelle
`tirelire-operations-<début>_<fin>.csv`, d'après `from` et `to`, sinon d'après la première et la dernière date de la sélection.
La réponse n'est pas mise en cache (`Cache-Control: no-store`) et l'export ne laisse rien dans les journaux.

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
