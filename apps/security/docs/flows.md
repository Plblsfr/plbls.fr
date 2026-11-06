# Flows d'authentification

Le web passe par le BFF et ne voit jamais un token ; les clients natifs font du Authorization Code + PKCE directement avec Keycloak puis appellent le gateway en `Bearer`.

## Web (plbls.fr et sous-domaines) — BFF

1. Le front redirige vers `GET /auth/login?redirect=https://app.plbls.fr/...`.
2. Le gateway génère `state`, `nonce` et un `code_verifier` PKCE, les stocke dans Redis (TTL 10 min) et redirige vers la page de login Keycloak.
3. Keycloak renvoie sur `GET /auth/callback?code=...&state=...`.
4. Le gateway échange le code (client confidentiel + PKCE), vérifie l'ID token (signature, `iss`, `aud`, `nonce`), crée la session dans Redis.
5. Il pose le cookie `__Secure-plbls_sid` (HttpOnly, Secure, SameSite=Lax, `Domain=.plbls.fr`) puis redirige vers l'URL d'origine.
6. Les appels suivants vers `/api/...` portent juste le cookie ; le gateway rafraîchit l'access token tout seul quand il expire.

Inscription : `GET /auth/register?redirect=...` suit exactement les mêmes étapes, mais Keycloak affiche son formulaire d'inscription (e-mail vérifié obligatoire, `verifyEmail` dans le realm). Après validation de l'e-mail, l'utilisateur est renvoyé sur le callback et obtient sa session.

## Mobile et Windows — PKCE natif (RFC 8252)

1. L'appli ouvre le navigateur système sur Keycloak (jamais une WebView embarquée).
2. Retour via un schéma privé (`fr.plbls.app:/oauth2redirect`) sur mobile, via une redirection loopback `http://127.0.0.1:<port>/callback` sur Windows.
3. L'appli échange le code chez Keycloak et stocke les tokens dans le coffre de l'OS (Keychain, Keystore, Credential Manager).
4. Elle appelle le gateway avec `Authorization: Bearer <access_token>` et rafraîchit elle-même auprès de Keycloak.

## Extension navigateur — PKCE

1. `chrome.identity.launchWebAuthFlow` (ou `browser.identity` sur Firefox) ouvre Keycloak.
2. Redirection vers `https://<extension-id>.chromiumapp.org/`, échange du code chez Keycloak.
3. Tokens gardés dans `chrome.storage.session` (mémoire, effacé à la fermeture), appels en `Bearer` comme le natif.
