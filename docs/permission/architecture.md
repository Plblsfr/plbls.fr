# Architecture

```
navigateur ──► permission.plbls.fr (SPA, fichiers statiques servis par le backend)
     │
     └─ cookie de session ─► security.plbls.fr/api/permission/* ─► permission (Axum) ─► Postgres
                                  (auth-gateway : authentifie, signe l'identité)  ▲
micro-services ───── réseau Docker, Bearer de service ou identité transmise ──────┘
```

## Stack

| Brique | Choix |
| --- | --- |
| Backend | Rust, Axum 0.8, sqlx (Postgres 17, migrations embarquées) |
| Frontend | Svelte 5 + Vite + TypeScript, design system plbls-ui (CSS + web components, version figée dans le dépôt) |
| Déploiement | Image unique (backend + front statique) en distroless, Docker Compose, CI GitHub Actions |

## Authentification des appels

- **Utilisateur** : en-têtes d'identité signés par auth-gateway (`docs/gateway.md` de security) ; `api/commun/src/identite.rs` (`verify`, partagé avec security et aegis) fait la vérification. Secret vide = tout refusé, et le backend refuse de démarrer.
- **Service** : Bearer propre à chaque service, comparé en temps constant ; lecture seule.
- **Administration** : réservée aux utilisateurs portant le rôle Keycloak `server.admin_role` **ou** la permission `permission:admin` (application réservée `permission`, créée par la migration `0002`). Le rôle Keycloak amorce le système et reste la porte de secours : pas de problème d'œuf et de poule au premier démarrage, ni de verrouillage possible.

## Décisions

- Permission **n'appelle pas Keycloak** : le compte est identifié par son `sub`, jamais résolu en nom ou email côté backend (l'UI affiche donc des `sub`). Ajouter une résolution sera une évolution séparée.
- Intégrité dans la base : clés étrangères composites (un rôle ne peut pas porter la permission d'une autre application), regex sur les clés, cascades.
- Toute mutation et son entrée d'audit sont dans la **même transaction**.
- Spec OpenAPI **générée depuis le code** (`utoipa` + `utoipa-axum`) : chaque route est enregistrée avec sa documentation dans `routes!(...)`, la spec ne peut donc pas diverger. Pour une nouvelle route : `#[utoipa::path(...)]` sur le handler, `ToSchema`/`IntoParams` sur ses types, `.routes(routes!(...))` dans le `router()` du module. Un test échoue si la liste des routes documentées change sans qu'on l'ait voulu.

## Questions ouvertes

- Résolution `sub` → nom/email (via l'API admin Keycloak ou un en-tête mémorisé à la connexion).
- Invalidation de cache active vers les services (webhook) si le délai de cache devient gênant.
