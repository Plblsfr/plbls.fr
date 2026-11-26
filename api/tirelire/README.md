# tirelire

API de Tirelire, suivi des comptes bancaires de plbls.fr : documentation fonctionnelle dans [`docs/tirelire`](../../docs/tirelire/README.md),
routes dans [`openapi.yaml`](../../docs/tirelire/openapi.yaml).

| Module | Rôle |
|---|---|
| `config.rs` | variables `TIRELIRE_*` et communes (valeurs dans [`plbls.env`](../../plbls.env), secrets dans [`deploy/tirelire/.env.example`](../../deploy/tirelire/.env.example)) |
| `routes.rs` | table des routes : chacune a une permission ou une exception motivée ; tests de couverture et de conformité à OpenAPI (dans les deux sens) |
| `garde.rs`, `identite.rs`, `droits.rs` | gateway (`/auth/me`, cookie ou Bearer), aegis (manifeste, `/v1/effective`), CORS et CSRF |
| `etat.rs` | état partagé (configuration, base, aegis) et envoi périodique du manifeste |
| `interne.rs` | santé, disponibilité, `GET /me` |
| `http.rs` | réponses JSON et erreurs `{error, message}` |
| `mots.rs` | mots positionnés d'une page (`Mot`, `Page`), regroupement en lignes, lecture du format texte des fixtures (`lire`) |
| `extraction.rs` | PDF vers mots positionnés (`mots_du_pdf`) : lecture du PDF avec la crate `lopdf`, interprétation des opérateurs de texte du contenu de chaque page (matrices, police, taille, espacements), assemblage des glyphes en mots, texte vertical marqué, plafonds de pages, de glyphes et de contenu décompressé |
| `metriques.rs` | caractères et largeurs des polices standard (Helvetica, Times, Courier) que les relevés n'embarquent pas : nom de glyphe, largeur par police (métriques AFM d'Adobe), table WinAnsi |
| `cmb.rs` | parseur des relevés du Crédit Mutuel de Bretagne (`analyser`), sur les mots positionnés seulement : en-tête, sections, opérations et détails, soldes, totaux, contrôles |
| `import.rs` | `POST /statements/import` : empreinte, lecture hors du fil asynchrone, enregistrement en une transaction, réponse par fichier |
| `releves.rs` | `GET /accounts`, `GET /statements`, `DELETE /statements/{id}`, `GET /operations` (page et totaux de la sélection), `GET /operations/{id}`, `PUT /operations/{id}/category` |
| `selection.rs` | filtres des opérations (`Filtre`, `Selection::lire`) : recherche sans casse ni accents, période (date d'opération ou d'achat), relevé, catégorie, commerçant, montant, sens, type ; condition SQL commune (`OU`, `lier!`) et résumé de la sélection (`resumer` : nombre, débits, crédits, première et dernière date) |
| `export.rs` | `GET /operations/export.csv` : lecture et envoi en flux, cellules CSV (`cellule` : guillemets, protection contre les formules), montants et dates au format français, nom du fichier |
| `normalisation.rs` | texte normalisé d'un libellé (`texte`, `contient` en mots entiers) et clé de commerçant ou de tiers d'une opération (`tiers`, `nom`) |
| `classement.rs` | moteur de règles (`choisir`), rejeu idempotent sur toutes les opérations d'une personne (`rejouer`), premier accès et verrou par personne (`verrouiller`, `pret`), version de la normalisation |
| `categories.rs` | catégories et règles par défaut (`CATEGORIES`, `REGLES`), `GET` et `POST /categories`, `PATCH /categories/{id}` |
| `regles.rs` | `GET` et `POST /rules`, `PATCH` et `DELETE /rules/{id}`, `PUT /rules/order` |
| `commercants.rs` | `GET /merchants`, `GET` et `PATCH /merchants/{id}`, catégorie, fusion et séparation, file `GET /unclassified` et son compteur |
| `bilan.rs` | totaux d'une liste d'opérations par catégorie : remboursements déduits, épargne, revenus et transferts hors dépenses |
| `stats.rs` | statistiques par période, sans accès à la base (`Calcul`) : périodes calendaires ou par relevé et leur couverture, rattachement des opérations, totaux, lignes par catégorie, écarts et moyennes, dépassements classés, cumul jour par jour, projection, détail d'une catégorie |
| `evolution.rs` | suivi dans le temps, sans accès à la base : fenêtre (`Fenetre`), historique par catégorie et totaux (`historique`), tendance de Theil-Sen (`tendance`), solde jour par jour et trous entre relevés (`soldes`), comparaison d'une année sur l'autre (`annees`) |
| `habitudes.rs` | habitudes sur une fenêtre, sans accès à la base (`habitudes`) : classement des commerçants, petits achats répétés et annualisation, répartition par jour de la semaine et par semaine du mois, opérations inhabituelles (`inhabituelles` : médiane et écart absolu médian) |
| `recurrents.rs` | paiements récurrents, sans accès à la base : cadences et tolérances (`Cadence`), séries par commerçant (`detecter`), prochaine échéance, arrêt, alertes et totaux (`recurrents`) |
| `mois.rs` | `GET /stats/months`, `GET /stats/month`, `GET /stats/month/categories/{id}`, `GET /stats/history`, `GET /stats/balance`, `GET /stats/years`, `GET /stats/habits`, `GET /operations/{id}/unusual` : lecture des opérations, des catégories et des relevés de la personne, puis `stats.rs`, `evolution.rs` et `habitudes.rs` |
| `abonnements.rs` | `GET /stats/recurring`, `PATCH /stats/recurring/{id}` : état par personne (table `recurrents`), puis `recurrents.rs` |

## Développement

```sh
just test-tirelire      # une base jetable par test (TEST_DATABASE_URL, sinon testcontainers)
just tirelire-dev-run   # 127.0.0.1:8096, variables de deploy/dev.env
```

Aucun PDF dans le dépôt : les fixtures de `tests/fixtures/*.mots` sont des mots positionnés aux données inventées (format décrit en
tête de chaque fichier), et les tests d'API fabriquent leurs PDF à partir d'elles (`testutil::pdf`).
`TIRELIRE_RELEVES_REELS=<dossier hors du dépôt> cargo test -p tirelire releves_reels -- --nocapture` lit chaque PDF du dossier et échoue
si un relevé est refusé ; `releves_reels_categorises` les importe dans une base jetable et compte ce que donnent la normalisation et les
règles par défaut ; `releves_reels_statistiques` vérifie sur eux les invariants des statistiques dans les deux modes (somme des catégories et de
« à classer » égale aux dépenses, reste à vivre, fin du cumul, nombre d'opérations, solde) ; `releves_reels_evolution` vérifie ceux de l'évolution
(historique égal aux dépenses de chaque période pour chaque fenêtre, solde jour par jour égal au nouveau solde de chaque relevé, jours couverts et
trous qui remplissent la fenêtre, cumul annuel égal à la somme des catégories) ; `releves_reels_habitudes` vérifie ceux des habitudes pour chaque
mode et chaque fenêtre (dépenses variables égales à celles de `/stats/month`, sommes des jours et des semaines, jours couverts) et la cohérence
des totaux des récurrents. Ils n'affichent que des nombres (opérations, pages,
écarts, opérations classées et à classer, commerçants distincts, périodes, récurrents et alertes détectés), jamais un libellé ni un montant.
Sans la variable, ces tests ne font rien.
