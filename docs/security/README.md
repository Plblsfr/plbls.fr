# auth-gateway — documentation

`auth-gateway` est le point d'entrée unique de l'authentification pour plbls.fr et ses sous-domaines : il fait le lien entre les clients, Keycloak et les micro-services backend. Keycloak reste le fournisseur d'identité et garde la page de connexion.

Domaine du gateway : **security.plbls.fr**. Hébergement : **Docker Compose**.

| Document | Contenu |
| --- | --- |
| [architecture.md](architecture.md) | Contexte, objectifs, vue d'ensemble |
| [flows.md](flows.md) | Flows d'authentification (web BFF, mobile/Windows, extension) |
| [gateway.md](gateway.md) | Routage, en-têtes d'identité, erreurs |
| [keycloak.md](keycloak.md) | Clients, scope d'audience, réglages du realm |
| [endpoints.md](endpoints.md) | Endpoints du service |
| [security.md](security.md) | Cookie, CSRF, CORS, JWT, refresh, logout |
| [stack.md](stack.md) | Stack technique et découpage des modules |
| [decisions.md](decisions.md) | Décisions d'architecture et d'hébergement |

Source : document d'architecture « auth-gateway plbls.fr — Architecture » (2026-10-12).

Lancer et déployer : voir le [README du service](../../api/security/README.md).
