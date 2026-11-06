# Migration et prochaines étapes

Le gateway démarre à côté du monolithe, sans le toucher ; chaque front bascule quand il est prêt.

1. Mettre en place les clients et le scope `plbls-api` dans Keycloak (voir [keycloak.md](keycloak.md)).
2. Déployer le gateway sur `security.plbls.fr` avec Redis (Docker Compose), sans aucune route `/api` : seul le login web est actif.
3. Basculer un premier front web sur `/auth/login` et `/auth/me`, le monolithe reste son backend métier.
4. Ajouter les routes des nouveaux micro-services au fil de leur extraction du monolithe.
5. Brancher les applis natives et l'extension en PKCE, appels en `Bearer` vers le gateway.
6. Quand plus aucun front ne dépend de l'auth du monolithe, la retirer.

## Décisions

- Nom de domaine du gateway : `security.plbls.fr`.
- Hébergement : Docker Compose.
- Rôles par client (`resource_access`) en plus des rôles realm : oui.
- Pas de route vers le monolithe dans le gateway : les fronts gardent deux backends pendant la transition.
- WebSocket : le proxy relaie l'upgrade (authentification et contrôle d'`Origin` avant l'upgrade).
- Issuer Keycloak : jamais en dur, fourni par `GATEWAY__OIDC__ISSUER` (`.env` en prod ; Keycloak local en dev). C'est l'URL publique (`iss` des tokens, redirections du navigateur). Les appels du gateway vers Keycloak (discovery, token, JWKS, révocation) passent par `GATEWAY__OIDC__INTERNAL_ISSUER` (réseau Docker) pour ne pas traverser Cloudflare.

## Questions ouvertes

Aucune pour l'instant.
