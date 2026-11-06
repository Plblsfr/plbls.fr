# Sécurité

Le navigateur ne détient qu'un identifiant de session opaque ; tout le reste vit dans Redis ou chez Keycloak.

- **Cookie** : `__Secure-plbls_sid`, 32 octets aléatoires, HttpOnly, Secure, SameSite=Lax, `Domain=.plbls.fr`. Redis ne stocke que son SHA-256 : une fuite de Redis ne donne pas de cookies utilisables.
- **CSRF** : pour une requête authentifiée par cookie avec une méthode non sûre (POST, PUT, PATCH, DELETE), l'en-tête `Origin` doit être une origine autorisée, sinon 403. Les requêtes `Bearer` ne sont pas concernées.
- **CORS** : `Access-Control-Allow-Credentials` uniquement pour `https://plbls.fr` et `https://*.plbls.fr` (+ origines d'extension listées).
- **Validation JWT** : signature RS256 via le JWKS Keycloak (cache, rechargement si `kid` inconnu), `iss`, `exp`, audience `plbls-api`, `azp` = client du realm portant le scope `plbls-api` (API admin Keycloak, liste en cache 60 s, relue sur `azp` inconnu au plus toutes les 10 s ; compte de service du gateway avec `view-clients`).
- **Login CSRF** : `/auth/login` pose un cookie court `<cookie>_login` (HttpOnly, `Path=/auth`, SameSite=Lax, sans `Domain`) qui contient le `state` ; le callback le exige et l'efface. Une URL de callback fournie par un tiers ne peut donc pas connecter la victime au compte de l'attaquant.
- **Chemins du proxy** : un chemin contenant un segment `.` ou `..` (même encodé en `%2e`), `%2f`, `%5c` ou `\` reçoit 400. Sans cela l'upstream normaliserait le chemin et sortirait de la route choisie (donc de ses règles d'accès).
- **Refresh concurrent** : un verrou Redis (`SET NX`, 15 s, plus long que le timeout de 10 s vers Keycloak) par session évite que deux requêtes simultanées grillent le refresh token tourné.
- **Panne de Keycloak** : la session n'est supprimée que si Keycloak refuse explicitement le refresh token (400). Une erreur réseau ou un 5xx donne 503 et garde la session.
- **JWKS** : un `kid` inconnu ne recharge les clés qu'une fois par 30 s (cache vide : toujours). Une rotation de clé peut donc rejeter des tokens neufs pendant 30 s au plus.
- **WebSocket** : l'authentification est vérifiée à l'ouverture ; la connexion est fermée (1001) après `server.ws_max_lifetime_secs` (900 s par défaut) pour forcer une reconnexion, donc une nouvelle authentification.
- **Proxy** : timeout d'inactivité de 60 s sur la réponse de l'upstream (pas de durée totale, le streaming continue).
- **Confiance inter-services** : les micro-services ne sont joignables que par le réseau interne (Docker network) et vérifient `X-Identity-Sig`. mTLS possible plus tard sans changer le contrat.
- **Logout** : POST uniquement, avec `Origin` autorisé. Révocation du refresh token chez Keycloak, suppression de la session, puis `end_session_endpoint` avec `id_token_hint`.

- **Rate limiting** : `/auth/login`, `/auth/register` et `/auth/callback` sont limités par IP (compteur Redis à fenêtre fixe, `rate_limit.auth_per_minute`, 30 par défaut, 0 = désactivé) ; 429 avec `Retry-After`. L'IP est la dernière entrée de `X-Forwarded-For` (ajoutée par le reverse proxy de confiance), sinon l'adresse du pair. Le gateway ne voit pas les soumissions du formulaire d'inscription (elles vont directement chez Keycloak) : le reverse proxy doit aussi limiter `auth.plbls.fr`, et le realm active `bruteForceProtected`.
- **Inscription** : page hébergée par Keycloak, e-mail vérifié obligatoire avant la première connexion ; le gateway ne manipule jamais le mot de passe.
- **MCP** : `POST /mcp` n'accepte qu'un token statique (`GATEWAY__MCP__TOKEN`, comparaison à temps constant), indépendant de Keycloak. Il ne sert que de la documentation et les routes configurées (jamais les upstreams) ; sans token configuré la route répond 404.
- **Routes internes** : `/internal/*` n'exige qu'un token statique (`GATEWAY__INTERNAL__TOKEN`, temps constant, 404 si vide) et doit être bloqué par le reverse proxy public (appel par le réseau Docker uniquement). `/internal/users` interroge l'API admin Keycloak via le compte de service du client `plbls-gateway` (rôle `realm-management:view-users`, à accorder en production) ; `email` est obligatoire, jamais d'export complet.

## Points ouverts

- Journal d'audit des connexions.
