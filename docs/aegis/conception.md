# aegis : conception

aegis remplace `api/permission`. Ce document fixe la cible et découpe le travail en tickets ; il ne change pas le code.
Les décisions viennent de l'étape « grill me » ; ce qui reste à trancher est listé en fin de document.

## 1. Pourquoi refaire

`api/permission` a été pensé quand chaque projet avait son front : RBAC minimal par application, comptes affichés par leur `sub`,
permissions créées à la main dans l'interface, admin servi par le service lui-même puis relayé par Estompe. Il ne protège que
quelques actions (`estompe:access`, `estompe:admin`, `estompe:movies`, `pli:deposit`).

Avec un seul front (l'administration d'Estompe), on veut :

- que **chaque fonctionnalité** existante soit protégée par une permission, ouverte ou fermée selon la configuration en base ;
- qu'un administrateur règle **précisément** les droits d'un compte (droits directs, refus, expiration, portée sur une ressource, groupes) ;
- voir **l'e-mail** des comptes partout, jamais un uuid seul ;
- que la configuration vive en base (PostgreSQL) avec un **cache Redis** et une propagation immédiate.

## 2. Décisions

| Sujet | Décision |
|---|---|
| Périmètre | Le système global seulement. Les rôles de serveur d'Estompe (bits par rôle, exceptions par salon) restent, aegis leur sert de plafond (§ 9). |
| Service | Nouveau service `api/aegis`, écrit à côté de `api/permission`, supprimé après la bascule. Image et tag `aegis-vX.Y.Z`. |
| Données | Pas d'import : la configuration est refaite dans aegis, en parallèle de l'ancien service, avant la bascule. |
| Déclaration | Chaque application envoie son **manifeste** au démarrage (permissions, types de ressource). L'admin ne crée pas de permission. |
| Sujets | Compte (`sub` Keycloak), groupe local, rôle implicite « Tout le monde » (tout compte connecté). |
| Capacités | Rôles multi-applications, droits directs à un compte, refus explicites, expiration, portée sur une ressource, groupes. |
| Règle de calcul | Le refus l'emporte toujours. Rien d'accordé = refusé. |
| Portée | Types de ressource déclarés par application ; un droit vise le joker `*` ou un id. L'application liste ses ressources (id + nom). |
| Défaut | Le rôle « Tout le monde » porte ce qui est ouvert à tous ; au premier enregistrement d'une permission, il reprend le défaut du manifeste. |
| Finesse | Une permission par fonctionnalité, plus les actions sensibles (environ 50, § 6). |
| Administration | Un seul niveau : super-admin (rôle Keycloak de secours `aegis-admin` ou permission `aegis:admin`). |
| Comptes | Annuaire local (sub, e-mail, nom, dernière activité) alimenté par les applications et synchronisé depuis Keycloak. |
| Cache | Redis côté aegis (droits calculés par compte) + pub/sub d'invalidation écouté par les applications. PostgreSQL reste la source de vérité. |
| Livraison | Ce document, puis une PR par ticket (§ 14). |

## 3. Vocabulaire

| Objet | Rôle | Exemple |
|---|---|---|
| **Application** | Un service qui déclare des permissions. Clé `[a-z][a-z0-9-]{0,39}` | `estompe`, `pli`, `aegis` |
| **Type de ressource** | Une sorte d'objet d'une application sur laquelle une permission peut être restreinte | `estompe:server`, `estompe:library` |
| **Permission** | Une fonctionnalité ou action, déclarée par le manifeste. Nom complet `application:clé` | `estompe:vocal.screen` |
| **Règle** | Une permission accordée (`allow`) ou refusée (`deny`) à un sujet, sur `*` ou une ressource, avec expiration éventuelle | refus de `estompe:gifs.use` à alice |
| **Rôle** | Un paquet nommé de règles, toutes applications confondues | `Membre`, `Équipe films` |
| **Groupe** | Un ensemble de comptes ; il reçoit des rôles et des règles comme un compte | `Modérateurs` |
| **Liaison** | Un rôle donné à un compte ou à un groupe, avec portée et expiration éventuelles | `Modérateur` → alice sur le serveur X |
| **Tout le monde** | Rôle système, lié implicitement à tout compte connecté, non supprimable | |

## 4. Modèle de données

```sql
CREATE TABLE apps (
    key          text PRIMARY KEY CHECK (key ~ '^[a-z][a-z0-9-]{0,39}$'),
    name         text NOT NULL,
    description  text NOT NULL DEFAULT '',
    manifest     jsonb NOT NULL,
    manifest_sha text NOT NULL,
    seen_at      timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE resource_types (
    app      text NOT NULL REFERENCES apps(key) ON DELETE CASCADE,
    key      text NOT NULL CHECK (key ~ '^[a-z][a-z0-9_-]{0,39}$'),
    name     text NOT NULL,
    list_url text NOT NULL,
    PRIMARY KEY (app, key)
);

CREATE TABLE permissions (
    name          text PRIMARY KEY,
    app           text NOT NULL REFERENCES apps(key) ON DELETE CASCADE,
    key           text NOT NULL CHECK (key ~ '^[a-z][a-z0-9_.-]{0,59}$'),
    category      text NOT NULL DEFAULT '',
    label         text NOT NULL,
    description   text NOT NULL DEFAULT '',
    resource_type text,
    open_default  boolean NOT NULL DEFAULT false,
    obsolete_at   timestamptz,
    UNIQUE (app, key),
    FOREIGN KEY (app, resource_type) REFERENCES resource_types(app, key)
);

CREATE TABLE accounts (
    sub          text PRIMARY KEY,
    email        text,
    display_name text,
    last_seen_at timestamptz,
    synced_at    timestamptz,
    in_keycloak  boolean NOT NULL DEFAULT true
);
CREATE INDEX accounts_email ON accounts (lower(email) text_pattern_ops);

CREATE TABLE groups (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name        text NOT NULL UNIQUE,
    description text NOT NULL DEFAULT ''
);

CREATE TABLE group_members (
    group_id   uuid NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    sub        text NOT NULL REFERENCES accounts(sub),
    expires_at timestamptz,
    added_by   text NOT NULL,
    added_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (group_id, sub)
);

CREATE TABLE roles (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name        text NOT NULL UNIQUE,
    description text NOT NULL DEFAULT '',
    system      text UNIQUE CHECK (system IN ('tout-le-monde', 'admin'))
);

CREATE TYPE subject_kind AS ENUM ('role', 'group', 'account');
CREATE TYPE effect AS ENUM ('allow', 'deny');

CREATE TABLE rules (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    subject_kind subject_kind NOT NULL,
    subject_id   text NOT NULL,
    permission   text NOT NULL REFERENCES permissions(name) ON DELETE CASCADE,
    resource     text NOT NULL DEFAULT '*',
    effect       effect NOT NULL,
    expires_at   timestamptz,
    note         text NOT NULL DEFAULT '',
    granted_by   text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    UNIQUE (subject_kind, subject_id, permission, resource)
);

CREATE TABLE role_bindings (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    role_id      uuid NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    subject_kind subject_kind NOT NULL CHECK (subject_kind <> 'role'),
    subject_id   text NOT NULL,
    scope_type   text,
    scope_id     text,
    expires_at   timestamptz,
    granted_by   text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    CHECK ((scope_type IS NULL) = (scope_id IS NULL))
);

CREATE TABLE audit_log (
    id          bigserial PRIMARY KEY,
    at          timestamptz NOT NULL DEFAULT now(),
    actor       text NOT NULL,
    action      text NOT NULL,
    target_kind text NOT NULL,
    target_id   text NOT NULL,
    subject     text,
    detail      jsonb NOT NULL DEFAULT '{}'
);
CREATE INDEX audit_log_subject ON audit_log (subject, id DESC);
```

Points d'intégrité :

- Une règle n'a de ressource précise (`resource <> '*'`) que si sa permission a un `resource_type` (vérifié par l'API et un trigger).
- `subject_id` d'un compte référence `accounts` (créé à la volée si besoin, § 7) ; groupes et rôles sont nettoyés par trigger à la suppression.
- Une permission retirée du manifeste n'est **pas** supprimée : `obsolete_at` est posé, ses règles restent mais ne comptent plus,
  et l'admin les voit barrées. Elle redevient active si elle réapparaît. Une purge manuelle supprime les obsolètes.
- Les rôles système `tout-le-monde` et `admin` (qui porte `aegis:admin`) sont créés par la migration et ne se suppriment pas.
- Toute mutation et son entrée d'audit sont dans la même transaction. L'audit stocke des `sub` ; l'API les rend avec l'e-mail (§ 7).

## 5. Règle de calcul

Pour un compte `s`, les **sources** sont :

1. le rôle « Tout le monde » ;
2. les rôles liés à `s` ;
3. pour chaque groupe dont `s` est membre : les rôles liés au groupe et les règles du groupe ;
4. les règles directes de `s`.

Une liaison, une appartenance ou une règle dont `expires_at <= now()` est ignorée. Une permission obsolète est ignorée.

Une liaison de rôle **avec portée** (`scope_type`, `scope_id`) ne garde, parmi les règles du rôle, que celles dont la permission a ce
type de ressource, et les restreint à `scope_id` (une règle `*` du rôle devient une règle sur `scope_id`). Les autres règles du rôle
sont ignorées pour cette liaison ; l'interface le signale à l'attribution.

Décision `autorise(s, p, r)` :

```
regles = règles de toutes les sources dont permission = p
si une règle deny avec resource ∈ {'*', r} existe → REFUS
sinon si une règle allow avec resource ∈ {'*', r} existe → AUTORISÉ
sinon → REFUS
```

Pour une permission sans type de ressource, `r` vaut `*`. Pour une permission typée appelée sans ressource, seul le joker compte.
Il n'y a pas d'ordre de priorité entre sources : le refus gagne toujours. Pour lever un refus, on le supprime.

Le super-admin (rôle Keycloak `aegis-admin`, réglable) a `aegis:admin` sans passer par la base : c'est la porte de secours, on ne
peut pas se verrouiller dehors. Un refus ne s'applique pas à ce rôle Keycloak. Un jeton de service n'est jamais administrateur.

**Forme calculée** (ce que reçoivent les applications et ce qui est mis en cache) :

```json
{
  "sub": "…",
  "generation": 42,
  "valid_until": "2026-11-09T18:00:00Z",
  "permissions": {
    "estompe:access":       { "allow": ["*"], "deny": [] },
    "estompe:vocal.screen": { "allow": ["*"], "deny": ["srv-7"] },
    "estompe:films.use":    { "allow": ["lib-films"], "deny": [] }
  }
}
```

Seules les permissions ayant au moins un `allow` figurent. Le client applique la même règle : refusé si `deny` contient `*` ou `r`,
autorisé si `allow` contient `*` ou `r`. `valid_until` = la plus proche expiration parmi les sources (ou `now + TTL`).

## 6. Manifeste et catalogue

Chaque application envoie au démarrage, puis toutes les 10 minutes :

```http
PUT /v1/manifest
Authorization: Bearer <jeton du service estompe>

{
  "app": "estompe",
  "name": "Estompe",
  "resource_types": [
    { "key": "server",  "name": "Serveur",      "list_url": "http://estompe-api:8080/internal/aegis/resources/server" },
    { "key": "library", "name": "Bibliothèque", "list_url": "http://estompe-api:8080/internal/aegis/resources/library" }
  ],
  "permissions": [
    { "key": "vocal.screen", "category": "Vocal", "label": "Partager son écran",
      "description": "…", "resource_type": "server", "open_default": true }
  ]
}
```

- Un jeton de service est lié à **une** application (`AEGIS__SERVICES__TOKENS=estompe:<jeton>,pli:<jeton>`) : il ne peut déclarer qu'elle.
- Le manifeste est appliqué en une transaction : ajouts, mises à jour de libellés, obsolescences. Inchangé (`manifest_sha`) = aucune écriture.
- À la **première** apparition d'une permission avec `open_default: true`, une règle `allow *` est ajoutée au rôle « Tout le monde »
  (auteur `manifeste:<app>`, audité). Ensuite, le manifeste ne touche plus jamais aux règles : l'admin décide.
- `list_url` : `GET <list_url>?q=<texte>&ids=<id,…>` avec le jeton d'aegis → `[{ "id": "…", "name": "…" }]` (50 au plus). Sert à la recherche
  et à l'affichage des noms. Injoignable : l'admin affiche l'id brut, marqué « nom indisponible ».
- Le catalogue est généré depuis le code de l'application (constantes Rust), et un test échoue si une route vérifie une permission absente du manifeste.

### Catalogue initial (proposition)

« Plafond » : la permission limite aussi un bit des rôles de serveur (§ 9). « Ouvert » : `open_default: true`.

**estompe** (types de ressource `server`, `library`)

| Catégorie | Permission | Ressource | Défaut | Couvre aujourd'hui |
|---|---|---|---|---|
| Accès | `access` | | fermé | `estompe:access` (`PERMISSION_NAME`) |
| Serveurs | `servers.create` | | ouvert | `POST /servers` |
| | `servers.join` | | ouvert | `/server-invites/{code}/join` |
| | `servers.invite` | server | ouvert | plafond `create_invites` |
| Messages | `messages.send` | server | ouvert | plafond `send_messages` |
| | `dms.use` | | ouvert | `/dms`, messages privés |
| | `files.upload` | server | ouvert | plafond `attach_files`, envoi de fichiers |
| | `reactions.add` | server | ouvert | plafond `add_reactions` |
| | `polls.create` | server | ouvert | plafond `create_polls`, `/messages/{id}/vote` |
| | `search.use` | | ouvert | `/search` |
| | `mention.everyone` | server | ouvert | plafond `mention_everyone` |
| Vocal | `vocal.join` | server | ouvert | plafond `connect` + `speak`, `/voice/{cid}/token` |
| | `vocal.video` | server | ouvert | plafond `video` |
| | `vocal.screen` | server | ouvert | plafond `share_screen` |
| | `transcription.use` | server | ouvert | bloc transcription |
| | `draw2gether.use` | server | ouvert | bloc draw2gether |
| Médias | `films.use` | library | fermé | `estompe:movies` |
| | `watch.organize` | server | ouvert | plafond `organize_watch`, bloc regarder-ensemble |
| | `music.use` | server | ouvert | `/music/*` |
| | `videos.use` | server | ouvert | bloc vidéos de salon |
| | `gifs.use` | | ouvert | `/gifs/*` |
| | `emojis.use` | | ouvert | emojis personnalisés (lecture et usage) |
| | `emojis.manage` | server | ouvert | ajout d'emojis de serveur (sous le bit de serveur existant) |
| | `link-preview.use` | | ouvert | `/link-preview` |
| Profil | `profile.avatar` | | ouvert | `/me/avatar`, avatar par serveur |
| | `profile.status` | | ouvert | `/me/status`, `/me/activity` |
| | `profile.banner` | | ouvert | `/me/banner` |
| | `notifications.push` | | ouvert | `/push/*` |
| | `reports.create` | | ouvert | `POST /reports` |
| Administration | `admin.view` | | fermé | `/admin/overview`, `/admin/stats`, `/admin/live` |
| | `admin.users` | | fermé | `/admin/users` |
| | `admin.servers` | | fermé | `/admin/servers/{id}` |
| | `admin.logs` | | fermé | `/admin/log`, `/admin/errors` |
| | `admin.bot` | | fermé | `/admin/bot*` |
| | `admin.badges` | | fermé | `/admin/badges*` |
| | `admin.reports` | | fermé | `/admin/reports` |
| | `admin.films` | | fermé | `/admin/films/rejets` |
| | `admin.files` | | fermé | droits admin de `fichiers.rs` |

**pli** : `deposit` (fermé), ex `pli:deposit`.

**aegis** : `admin` (fermé, porté par le rôle système `admin`).

La liste exacte est arrêtée dans le ticket d'Estompe (T4), route par route ; tout bloc ajouté plus tard déclare ses permissions dans son manifeste.

## 7. Annuaire des comptes

Deux sources, fusionnées dans `accounts` :

1. **Activité** : quand Estompe ou pli demandent les droits d'un compte, ils transmettent l'e-mail et le nom de l'identité signée
   par la passerelle (`X-User-Email`, `X-User-Name`). aegis met à jour `email`, `display_name`, `last_seen_at` (au plus une écriture par compte et par heure).
2. **Keycloak** : toutes les 15 minutes, aegis parcourt `GET /internal/users` de la passerelle (pagination complète, T1) : ajoute
   les comptes jamais vus, met à jour les e-mails, passe `in_keycloak = false` pour un compte supprimé. La recherche de l'admin
   interroge aussi la passerelle en direct quand l'annuaire ne trouve rien.

Partout dans l'API d'admin, un compte est rendu `{ "sub", "email", "display_name", "last_seen_at", "in_keycloak" }`. L'interface
affiche l'e-mail (le `sub` en infobulle et copiable). Un compte supprimé de Keycloak est signalé, avec une action « purger ses droits ».

## 8. Cache et propagation

```
admin ─► aegis ─► PostgreSQL (vérité)
           │
           ├─► Redis  aegis:eff:<generation>:<sub>   (forme calculée, TTL = min(10 min, valid_until))
           │          aegis:generation               (entier)
           └─► Redis pub/sub  aegis:invalidation  ─► Estompe, pli (abonnés)
```

- Lecture des droits : Redis, sinon calcul en SQL (une requête) puis écriture en Redis. Redis indisponible : calcul direct, pas d'erreur.
- Changement touchant **un** compte (règle ou liaison directe, appartenance à un groupe) : suppression de sa clé, publication `{"sub": "…"}`.
- Changement **large** (rôle, groupe, « Tout le monde », manifeste) : `INCR aegis:generation` (les anciennes clés expirent seules), publication `{"all": true, "generation": n}`.
- **Expirations** : une tâche toutes les minutes repère les sources expirées depuis le dernier passage, écrit l'audit `expire` et publie l'invalidation.
  Le `valid_until` de la forme calculée garantit déjà qu'aucun cache ne sert un droit expiré.
- **Applications** : cache mémoire par compte (TTL 30 s et `valid_until`), vidé à réception d'un message. Abonnement perdu : cache vidé et TTL ramené à 5 s
  jusqu'à la reconnexion. aegis injoignable et cache vide : **refus** (échec fermé), comme aujourd'hui.
- Redis : instance dédiée (ou base logique dédiée) ; utilisateur ACL `aegis` en lecture/écriture sur `aegis:*`, utilisateurs `estompe` et `pli`
  limités à `SUBSCRIBE aegis:invalidation`.

## 9. Intégration Estompe

- `permission.rs` est remplacé par un client aegis (même échec fermé, cache décrit au § 8, abonnement Redis). `Droits::a(nom)` devient
  `Droits::autorise(nom, ressource)`.
- Au démarrage : envoi du manifeste généré depuis les constantes du catalogue. Nouvelle route interne `/internal/aegis/resources/{type}`
  (jeton d'aegis) qui liste serveurs et bibliothèques.
- **Plafond des rôles de serveur** : `Access` reçoit un masque calculé depuis les droits aegis du compte pour le serveur courant
  (chaque bit plafonné est retiré si la permission correspondante est refusée sur ce serveur). Il est appliqué dans `Access::base`
  et `Access::channel`, propriétaire du serveur compris : un serveur ne peut jamais donner ce qu'aegis refuse. Les bits de modération
  (`kick`, `ban`, `manage_roles`, `manage_channels`…) restent purement serveur.
- Toutes les routes du catalogue vérifient leur permission (extracteur `Exige<P>` ou appel explicite pour les permissions à ressource) ;
  le WebSocket vérifie au moment de l'action (envoi de message, entrée en vocal…) et coupe une session vocale dont le droit est retiré (invalidation reçue).
- `GET /me` renvoie la forme calculée réduite à `estompe:*` (`capacites`) ; le front masque boutons et blocs en conséquence (confort seulement).
- `PERMISSION_ADMIN_ROLE` devient `AEGIS_ADMIN_ROLE` ; la section d'administration « permissions » devient « Accès » et relaie `/admin/acces/*`
  vers `/v1/admin/*` d'aegis avec l'identité de l'appelant (aegis reste seul juge).

**pli** : envoie son manifeste (`deposit`), remplace `PermissionChecker` par le même client (pas de type de ressource).

## 10. API d'aegis

Toutes les routes sont documentées en OpenAPI générée (`utoipa`), comme aujourd'hui. Préfixe `/v1`.

**Services** (Bearer de service) :

| Route | Rôle |
|---|---|
| `PUT /manifest` | Déclare l'application du jeton (§ 6) |
| `POST /effective` `{sub, app?, email?, display_name?}` | Forme calculée d'un compte, filtrée sur une application ; met à jour l'annuaire |
| `POST /check` `{sub, permission, resource?}` | `{allowed}` pour un contrôle ponctuel |

**Utilisateur connecté** (identité signée par la passerelle) : `GET /me/effective?app=` (ses propres droits).

**Administration** (super-admin) :

| Route | Rôle |
|---|---|
| `GET /admin/catalog` | Applications, types de ressource, permissions (obsolètes comprises) |
| `GET /admin/resources/{app}/{type}?q=&ids=` | Relais vers `list_url` |
| `GET /admin/accounts?q=&page=` · `GET /admin/accounts/{sub}` | Annuaire (e-mail, préfixe ou sous-chaîne) ; fiche : rôles, groupes, règles directes |
| `GET /admin/accounts/{sub}/effective` | Droits effectifs **expliqués** : pour chaque permission, la décision et la liste des sources (règle, rôle, groupe, liaison, expiration) |
| `POST /admin/simulate` `{sub, permission, resource?}` | Décision + sources qui l'ont produite |
| `POST /admin/accounts/{sub}/copy-from` `{source, mode: "ajouter" \| "remplacer"}` | Copie liaisons, appartenances et règles directes d'un autre compte (aperçu avec `dry_run`) |
| `DELETE /admin/accounts/{sub}/grants` | Purge tous les droits d'un compte |
| `GET/POST/PUT/DELETE /admin/roles[/{id}]` | Rôles et leurs règles (remplacement complet de la liste de règles) |
| `GET/POST/PUT/DELETE /admin/groups[/{id}]` · `PUT/DELETE /admin/groups/{id}/members/{sub}` | Groupes et membres (expiration possible) |
| `GET/POST/DELETE /admin/bindings` | Liaisons rôle → compte ou groupe (portée, expiration) |
| `GET/POST/PATCH/DELETE /admin/rules` | Règles directes de compte ou de groupe |
| `GET /admin/audit?subject=&actor=&action=&before=` | Journal, comptes rendus avec l'e-mail |
| `POST /admin/permissions/purge-obsolete` | Supprime les permissions obsolètes et leurs règles |

MCP (`POST /mcp`, Bearer dédié) : docs et OpenAPI, comme permission.

## 11. Interface d'administration (Estompe › Administration › Accès)

| Écran | Contenu |
|---|---|
| **Comptes** | Recherche par e-mail ou nom ; liste avec dernière activité, nombre de rôles, groupes et refus ; badge « supprimé de Keycloak » |
| **Fiche compte** | Onglets : *Droits effectifs* (tableau par catégorie : autorisé, refusé, partiel par ressource, avec l'origine au survol et la date d'expiration) ; *Attributions* (rôles, groupes, règles directes : ajout avec portée et expiration, retrait) ; *Simulateur* (permission + ressource → décision et raison) ; *Historique* (audit filtré sur le compte) ; action *Copier les droits de…* avec aperçu |
| **Rôles** | Liste ; éditeur en grille par catégorie : pour chaque permission, *non défini / autorisé / refusé*, et ressources précises pour les permissions typées ; comptes et groupes liés. « Tout le monde » en tête, marqué système |
| **Groupes** | Membres (e-mail, expiration), rôles liés, règles du groupe |
| **Catalogue** | Permissions par application et catégorie, avec description, type de ressource, défaut du manifeste, obsolètes barrées ; qui les a (lien vers un filtre) |
| **Journal** | Audit complet, filtres par acteur, compte visé, action, période ; e-mails partout |

Toute modification passe par une confirmation qui résume l'effet (« 12 comptes perdront `estompe:gifs.use` »), calculée par un `dry_run` côté aegis.

## 12. Sécurité

- Identité utilisateur : en-têtes signés par la passerelle (HMAC, 60 s), même code de vérification que permission. Secret vide = refus de démarrer.
- Jetons de service comparés en temps constant, un par application, révocables séparément ; ils ne lisent que les droits et ne déclarent que leur application.
- `list_url` limitée au réseau interne (liste d'hôtes autorisés en configuration), sans redirection, réponse plafonnée.
- Les règles et liaisons sur le rôle système `admin` et la permission `aegis:admin` demandent une confirmation renforcée ; le rôle Keycloak reste la porte de secours.
- Échec fermé partout côté applications.

## 13. Bascule

1. aegis déployé à côté de permission (nouvelle base, Redis, route passerelle `/api/aegis`), Estompe et pli encore sur permission.
2. Estompe et pli livrés avec les deux clients : `AEGIS_URL` vide → permission, renseignée → aegis. Ils envoient leur manifeste dès qu'aegis est configuré.
3. L'admin configure aegis (rôles, groupes, attributions) depuis Estompe, en comparant avec l'ancien écran encore disponible.
4. On renseigne `AEGIS_URL` : bascule sans coupure, réversible en vidant la variable.
5. Après une période d'observation : suppression de `api/permission`, `web/permission`, `docs/permission`, `relais_permission.rs`, de leurs jobs de CI, compose et tags.

## 14. Tickets

| # | Titre | Dépend de | Contenu |
|---|---|---|---|
| T1 | `feat(security): lister tous les comptes sur /internal/users` | | e-mail facultatif, pagination complète, nom affiché |
| T2 | `feat(aegis): socle du service` | | `api/aegis` (workspace), migrations § 4, identité et jetons, manifeste, calcul § 5, `/effective` `/check`, Redis et pub/sub, audit, OpenAPI, MCP, `deploy/aegis`, groupes de CI, tag `aegis` |
| T3 | `feat(aegis): administration` | T2, T1 | annuaire et synchronisation, rôles, groupes, liaisons, règles, explication, simulateur, copie, purge, `dry_run`, tâche d'expiration |
| T4 | `feat(estompe): protéger chaque fonctionnalité par aegis` | T2 | client et abonnement, manifeste et catalogue figé route par route, ressources internes, plafond des rôles de serveur, `capacites` dans `/me`, bascule par `AEGIS_URL` |
| T5 | `feat(estompe): administration des accès` | T3, T4 | écrans du § 11 à la place de la section permissions |
| T6 | `feat(pli): permissions via aegis` | T2 | manifeste, client, bascule par `AEGIS_URL` |
| T7 | infra (`plbls.fr-infra`) | T2 | base, Redis et ACL, secrets, route passerelle |
| T8 | `chore(permission): supprimer l'ancien service` | bascule faite | § 13, étape 5 |

## 15. Questions ouvertes

- **Liaison de rôle avec portée** : le comportement du § 5 (seules les règles du type de ressource de la portée s'appliquent) convient-il, ou faut-il interdire la portée sur un rôle qui mélange des types ?
- **Refus et propriétaire de serveur** : le plafond s'applique-t-il aussi au propriétaire d'un serveur (proposé : oui) ?
- **Coupure en direct** : retirer `vocal.join` doit-il expulser immédiatement d'un salon vocal en cours (proposé : oui, par l'invalidation) ?
- **Durée de conservation** de l'audit (proposé : illimitée, comme aujourd'hui).
