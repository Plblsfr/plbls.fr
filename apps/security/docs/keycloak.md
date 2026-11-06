# Configuration Keycloak

Un client par type d'application, tous dans le même realm, et un scope commun qui ajoute l'audience `plbls-api` que le gateway exige.

| Client | Type | Redirect URIs | Notes |
| --- | --- | --- | --- |
| `plbls-gateway` | Confidentiel | `https://security.plbls.fr/auth/callback` | Standard flow, PKCE S256, post-logout `https://plbls.fr/*` et `https://*.plbls.fr/*` |
| `plbls-mobile` | Public | `fr.plbls.app:/oauth2redirect` | PKCE S256 obligatoire |
| `plbls-desktop` | Public | `http://127.0.0.1/*` | Loopback, port dynamique (RFC 8252) |
| `plbls-extension` | Public | `https://<extension-id>.chromiumapp.org/` | PKCE S256, une entrée par navigateur |

> Le document d'origine indiquait `api.plbls.fr` ; le domaine retenu pour le gateway est `security.plbls.fr`.

## À faire

- [ ] Créer le client scope `plbls-api` avec un mapper *Audience* (included custom audience `plbls-api`, ajouté à l'access token), et l'ajouter en *Default* aux quatre clients.
- [ ] Activer *Revoke Refresh Token* (rotation) dans les réglages du realm.
- [ ] Access token à 5 min ; SSO Session Idle selon le confort voulu (le gateway cale le TTL Redis sur `refresh_expires_in`).
- [ ] Pour chaque client public : *Proof Key for Code Exchange Code Challenge Method* = `S256`, *Direct access grants* désactivé.
- [ ] Thème de login Keycloak aux couleurs de plbls.fr (optionnel, indépendant du gateway).

## Inscription

Realm : `registrationAllowed`, `registrationEmailAsUsername`, `verifyEmail`, `resetPasswordAllowed`, `bruteForceProtected`, et un `smtpServer` (en dev : Mailpit, boîte web sur http://localhost:8025). En production le SMTP est à configurer à la main dans le realm : sans lui, les nouveaux inscrits ne peuvent pas valider leur e-mail. Point d'entrée côté gateway : `GET /auth/register`.

## Rôles

Rôles realm **et** rôles par client (`resource_access`) : décision prise, à gérer dans le gateway.

## URL publique et URL interne

Le gateway parle à Keycloak de deux façons :

| Usage | URL | Variable |
| --- | --- | --- |
| Navigateur (login, logout) et `iss` des tokens | publique, ex. `https://auth.plbls.fr/realms/plbls` | `GATEWAY__OIDC__ISSUER` |
| Gateway → Keycloak (discovery, token, JWKS, révocation) | interne, ex. `http://keycloak:8080/realms/plbls` | `GATEWAY__OIDC__INTERNAL_ISSUER` |

Côté Keycloak, `KC_HOSTNAME` doit valoir l'URL publique : l'`issuer` renvoyé par la discovery est alors identique, même quand on l'interroge par l'adresse interne (le gateway refuse de démarrer le login sinon, avec un message explicite). Derrière un reverse proxy ou Cloudflare, ajouter `KC_PROXY_HEADERS=xforwarded` et `KC_HTTP_ENABLED=true`. Le gateway et Keycloak doivent partager un réseau Docker. `INTERNAL_ISSUER` vide : tout passe par l'URL publique.
