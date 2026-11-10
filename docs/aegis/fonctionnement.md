# Fonctionnement

## Modèle

| Objet | Rôle | Exemple |
| --- | --- | --- |
| **Application** | Service qui déclare ses permissions par manifeste. Clé `[a-z][a-z0-9-]{0,39}` | `estompe`, `pli`, `aegis` |
| **Type de ressource** | Objet d'une application sur lequel une permission peut être restreinte, avec l'URL interne qui en liste les instances | `estompe:server` |
| **Permission** | Fonctionnalité ou action. Nom complet `application:clé`, catégorie, libellé, type de ressource éventuel, ouverture par défaut | `estompe:vocal.screen` |
| **Règle** | Une permission accordée (`allow`) ou refusée (`deny`) à un sujet, partout (`*`) ou sur une ressource, avec expiration et note facultatives | refus de `estompe:gifs.use` à alice |
| **Rôle** | Paquet nommé de règles, toutes applications confondues | `Équipe films` |
| **Groupe** | Ensemble de comptes ; il reçoit des rôles et des règles comme un compte. Appartenance avec expiration possible | `Modérateurs` |
| **Liaison** | Rôle donné à un compte ou un groupe, partout ou sur une ressource, avec expiration possible | `Modérateur` → alice sur le serveur X |
| **Tout le monde** | Rôle système appliqué à tout compte connecté, non supprimable, non liable | |
| **Administrateur aegis** | Rôle système qui porte `aegis:admin` | |

Tables : `apps`, `resource_types`, `permissions`, `accounts` (annuaire), `groups`, `group_members`, `roles`, `rules`,
`role_bindings`, `audit_log` (`api/aegis/migrations`). Toute mutation et son entrée de journal sont dans la même transaction.

## Règle de calcul

Sources d'un compte : le rôle « Tout le monde », les rôles liés au compte, ses groupes (leurs rôles et leurs règles) et
ses règles directes. Une source expirée ou une permission obsolète ne compte pas.

```
règles = règles de toutes les sources pour la permission p
une règle deny sur « * » ou sur r  → REFUS
sinon une règle allow sur « * » ou sur r → AUTORISÉ
sinon → REFUS
```

- **Le refus l'emporte toujours**, d'où qu'il vienne. Pour lever un refus, on le supprime.
- Une permission sans type de ressource ne se règle que partout. Une permission typée vérifiée sans ressource ne regarde que `*`.
- Une **liaison avec portée** (`scope_type` = `app:type`, `scope_id`) ne garde, parmi les règles du rôle, que celles dont la
  permission a ce type, restreintes à cette ressource ; les autres règles du rôle sont ignorées pour cette liaison.
- Le **super-admin** a le rôle Keycloak `server.admin_role` (défaut `aegis-admin`) ou la permission `aegis:admin`. Le rôle
  Keycloak est la porte de secours : on ne peut pas se verrouiller dehors. Un jeton de service n'est jamais administrateur.

Forme calculée renvoyée aux applications (`POST /v1/effective`) et mise en cache :

```json
{"sub": "…", "generation": 42, "valid_until": "2026-11-09T18:00:00Z", "permissions": {
  "estompe:access": {"allow": ["*"], "deny": []},
  "estompe:vocal.screen": {"allow": ["*"], "deny": ["srv-1"]}}}
```

Seules les permissions avec au moins un `allow` y figurent. `valid_until` = la plus proche expiration parmi les sources,
bornée par `cache.ttl_secs` (10 min).

## Manifestes

Chaque application envoie son manifeste au démarrage puis toutes les 10 minutes (`PUT /v1/manifest`, jeton lié à cette
application). Inchangé (empreinte SHA-256) : rien n'est écrit. Une permission absente devient **obsolète** : ses règles sont
gardées mais ignorées, elle revient telle quelle si elle réapparaît ; l'administration peut purger les obsolètes. À la
**première** apparition d'une permission `open_default`, une règle `allow *` est ajoutée à « Tout le monde » (journalisée,
auteur `manifeste:<app>`) ; ensuite le manifeste ne touche plus aux règles.

## Annuaire des comptes

`accounts` (sub, e-mail, nom, dernière activité, présence dans Keycloak) est alimenté par :

1. la dernière activité : chaque `POST /v1/effective` d'une application (au plus une écriture par compte et par heure ; l'e-mail et le nom éventuellement transmis sont ignorés, un jeton de service ne peut pas réécrire le profil d'un compte) et chaque appel d'administration, qui enregistre aussi l'e-mail et le nom des en-têtes signés par la passerelle ;
2. la synchronisation Keycloak toutes les `annuaire.sync_minutes` (15) par `GET /internal/users` de la passerelle (liste complète,
   `GATEWAY__INTERNAL__LIST_ALL=true` côté security) ; un compte absent devient « supprimé de Keycloak » ;
3. la recherche de l'administration, qui interroge Keycloak en direct quand l'annuaire ne trouve rien.

L'API d'administration rend les comptes avec leur e-mail partout (journal compris) ; le `sub` reste visible en second.

## Cache et propagation

```
administration ─► aegis ─► PostgreSQL (vérité)
                    ├─► Redis <prefix>:eff:<génération>:<version>:<sub>   droits calculés, TTL = min(10 min, valid_until)
                    │         <prefix>:generation, <prefix>:ver:<sub>
                    └─► Redis pub/sub <prefix>:invalidation ─► Estompe, pli
```

- Lecture : génération et version du compte lues ensemble, puis Redis, sinon calcul SQL (une requête) puis écriture sous la
  version lue avant le calcul. Redis absent ou en panne : calcul direct, sans erreur ; les échecs de lecture de version,
  d'invalidation et de publication sont journalisés.
- Changement qui touche un compte (règle ou liaison directe, appartenance) : version du compte incrémentée, message `{"type":"compte","sub":"…"}`.
  Un calcul commencé avant le changement écrit sous l'ancienne version, que plus aucune lecture ne consulte.
- Changement large (rôle, groupe, « Tout le monde », manifeste) : génération incrémentée, message `{"type":"tout","generation":n}`.
- Expirations : une tâche par minute journalise `expire` et publie l'invalidation ; `valid_until` garantit déjà qu'aucun cache ne sert un droit expiré.
- Applications : cache mémoire par compte (30 s, borné par `valid_until`), vidé à réception d'un message ; abonnement perdu :
  cache vidé et durée ramenée à 5 s jusqu'à la reconnexion. aegis injoignable et cache vide : **refus**.

## Décisions

- Un service dédié remplace `api/permission` (RBAC par application, comptes en `sub`, permissions créées à la main) ; les
  données ne sont pas reprises : la configuration se refait dans aegis avant la bascule ([integration.md](integration.md)).
- Le système global seulement : les rôles de serveur d'Estompe restent ; aegis leur sert de **plafond** ([fonctionnalites.md](fonctionnalites.md)).
- Un seul niveau d'administration (super-admin).
- Rôles multi-applications, groupes locaux, refus explicites, expiration, portée sur une ressource.
- Une permission par fonctionnalité et par action sensible, déclarée par le code (pas de faute de frappe possible côté administration).
- Journal d'audit sans limite de durée.
