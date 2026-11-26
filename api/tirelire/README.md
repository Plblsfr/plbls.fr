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
| `releves.rs` | `GET /accounts`, `GET /statements`, `DELETE /statements/{id}`, `GET /operations` |

## Développement

```sh
just test-tirelire      # une base jetable par test (TEST_DATABASE_URL, sinon testcontainers)
just tirelire-dev-run   # 127.0.0.1:8096, variables de deploy/dev.env
```

Aucun PDF dans le dépôt : les fixtures de `tests/fixtures/*.mots` sont des mots positionnés aux données inventées (format décrit en
tête de chaque fichier), et les tests d'API fabriquent leurs PDF à partir d'elles (`testutil::pdf`).
`TIRELIRE_RELEVES_REELS=<dossier hors du dépôt> cargo test -p tirelire releves_reels -- --nocapture` lit chaque PDF du dossier et échoue
si un relevé est refusé ; il n'affiche que des nombres (opérations, pages, écarts). Sans la variable, ce test ne fait rien.
