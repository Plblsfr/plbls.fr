# Architecture

## Contexte et objectifs

`auth-gateway` est le point d'entrée unique de l'authentification pour plbls.fr et ses sous-domaines : il fait le lien entre les clients, Keycloak et les micro-services backend. Keycloak reste le fournisseur d'identité et garde la page de connexion.

- **Clients visés** : fronts web (plbls.fr et `*.plbls.fr`), applis mobiles, appli Windows, extension navigateur.
- **Rôle** : BFF pour le web (tokens jamais exposés au navigateur), validation des JWT pour les clients natifs, reverse proxy qui injecte l'identité vers les micro-services.
- **Stack** : Rust + Axum, sessions dans Redis.
- **Coexistence** : le monolithe actuel continue de tourner à côté ; les fronts migrent un par un vers le gateway, sans big bang.

## Vue d'ensemble

```text
 Web (plbls.fr, *.plbls.fr)  ── cookie ──┐
 Mobile                      ── Bearer ──┤
 Windows                     ── Bearer ──┼──►  auth-gateway  ──► micro-services (réseau interne)
 Extension navigateur        ── Bearer ──┘      │      │
        │                                       │      └──► Redis (sessions)
        └── PKCE (login) ──► Keycloak ◄─────────┘ (OIDC, JWKS)

 Fronts non migrés ───────────────────────────────────────► monolithe (appel direct)
```

Tous les clients appellent le gateway ; seul lui parle à Keycloak, à Redis et aux micro-services. Les applis natives et l'extension vont aussi chez Keycloak pour se connecter (PKCE), puis reviennent en `Bearer`. Le monolithe reste appelé en direct par les fronts tant qu'ils n'ont pas migré.
