# Décisions

- Nom de domaine du gateway : `security.plbls.fr`.
- Hébergement : Docker Compose.
- Rôles par client (`resource_access`) en plus des rôles realm : oui.
- Pas de route vers le monolithe dans le gateway : un front qui l'appelle encore le fait en direct.
- WebSocket : le proxy relaie l'upgrade (authentification et contrôle d'`Origin` avant l'upgrade).
- Issuer Keycloak : jamais en dur, fourni par `GATEWAY__OIDC__ISSUER` (`.env` en prod ; Keycloak local en dev). C'est l'URL publique (`iss` des tokens, redirections du navigateur). Les appels du gateway vers Keycloak (discovery, token, JWKS, révocation) passent par `GATEWAY__OIDC__INTERNAL_ISSUER` (réseau Docker) pour ne pas traverser Cloudflare.
