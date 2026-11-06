# Endpoints

Neuf endpoints propres au service, plus le proxy `/api/*` défini par la config.

| Méthode | Chemin | Auth | Rôle |
| --- | --- | --- | --- |
| GET | `/auth/login?redirect=` | — | Démarre le flow BFF, redirige vers Keycloak |
| GET | `/auth/register?redirect=` | — | Idem sur la page d'inscription Keycloak ; même callback, session ouverte après inscription |
| GET | `/auth/callback` | — | Retour Keycloak, crée la session et pose le cookie |
| POST | `/auth/logout?redirect=` | Cookie + `Origin` autorisé | Révoque le refresh token, supprime la session, déconnexion Keycloak (RP-initiated) |
| GET | `/auth/me` | Cookie ou Bearer | Identité courante en JSON (id, nom, email, rôles, client) |
| GET | `/auth/session` | Cookie ou Bearer | `204` si connecté, `401` sinon (sondage léger pour les fronts) |
| GET | `/healthz` | — | Liveness |
| GET | `/readyz` | — | Readiness : Redis joignable |
| POST | `/mcp` | Bearer statique (`GATEWAY__MCP__TOKEN`) | Serveur MCP : docs et routes configurées pour les assistants. `404` si le token est vide |
| GET | `/internal/users?email=&first=&max=` | Bearer statique (`GATEWAY__INTERNAL__TOKEN`) | Interne : comptes dont l'e-mail commence par `email` (insensible à la casse), paginé (`max` 20 par défaut, plafonné par `GATEWAY__INTERNAL__MAX_PAGE_SIZE`, 100 par défaut). Réponse `{users:[{id,email}], first, max, next}` ; `next` = `first` de la page suivante, `null` si dernière. `404` si le token est vide |
| * | `/api/{prefix}/...` | Selon la route | Proxy vers le micro-service avec en-têtes d'identité |

## Réponse de `GET /auth/me`

`200`, `Content-Type: application/json`, `Cache-Control: no-store`. Même format avec un cookie de session ou un Bearer.

```json
{
  "id": "6f1c2a9e-0b1d-4c53-9a77-2d0e5b1f8a10",
  "name": "demo",
  "email": "demo@plbls.fr",
  "roles": ["user", "plbls-mobile:scanner"],
  "client": "plbls-mobile"
}
```

| Champ | Type | Source (jeton Keycloak) |
| --- | --- | --- |
| `id` | string | `sub` (UUID Keycloak) |
| `name` | string | `preferred_username` ; `""` si absent |
| `email` | string | `email` ; `""` si absent |
| `roles` | string[] | rôles realm tels quels + rôles client sous la forme `client:role` |
| `client` | string | `azp` (client OIDC qui a obtenu le jeton) ; `""` si absent |

Les clés sont toujours exactement celles-ci : pas de `sub` ni de `username`. Les champs ne sont jamais `null` ni omis. Sans identité valide : `401`.

Le logout n'accepte que POST (un simple lien ou une image sur un autre site ne doit pas pouvoir déconnecter) : le front utilise un `<form method="post">` ou un `fetch`, depuis une origine `plbls.fr`.

Le paramètre `redirect` n'est accepté que s'il pointe vers une origine autorisée (`plbls.fr`, `*.plbls.fr`), sinon le gateway retombe sur l'URL par défaut : pas d'open redirect.
