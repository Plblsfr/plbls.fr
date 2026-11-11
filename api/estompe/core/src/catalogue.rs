use std::{collections::HashMap, sync::LazyLock};

use axum::http::Method;
use serde_json::{Value, json};

use crate::roles::{self, Perm};

pub const APP: &str = "estompe";
pub const SERVEUR: &str = "server";

pub const ACCES: &str = "estompe:access";
pub const ADMIN_FICHIERS: &str = "estompe:admin.files";
pub const ADMIN_SONDAGES: &str = "estompe:admin.polls";
pub const ADMIN_AEGIS: &str = "aegis:admin";

pub struct Entree {
    pub cle: &'static str,
    pub categorie: &'static str,
    pub libelle: &'static str,
    pub description: &'static str,
    pub serveur: bool,
    pub ouvert: bool,
    pub bloc: Option<&'static str>,
    pub routes: &'static [&'static str],
    pub plafond: Perm,
}

impl Entree {
    pub fn nom(&self) -> String {
        format!("{APP}:{}", self.cle)
    }

    pub fn admin(&self) -> bool {
        self.cle.starts_with("admin.")
    }
}

const fn e(
    cle: &'static str,
    categorie: &'static str,
    libelle: &'static str,
    description: &'static str,
) -> Entree {
    Entree {
        cle,
        categorie,
        libelle,
        description,
        serveur: false,
        ouvert: true,
        bloc: None,
        routes: &[],
        plafond: 0,
    }
}

impl Entree {
    const fn serveur(mut self) -> Self {
        self.serveur = true;
        self
    }

    const fn ferme(mut self) -> Self {
        self.ouvert = false;
        self
    }

    const fn bloc(mut self, b: &'static str) -> Self {
        self.bloc = Some(b);
        self
    }

    const fn routes(mut self, r: &'static [&'static str]) -> Self {
        self.routes = r;
        self
    }

    const fn plafond(mut self, p: Perm) -> Self {
        self.plafond = p;
        self
    }
}

pub static CATALOGUE: &[Entree] = &[
    e("access", "Accès", "Utiliser Estompe", "Lire et suivre ses serveurs, salons et conversations ; base de toutes les autres fonctionnalités")
        .ferme()
        .routes(&[
            "GET /servers", "GET /servers/{id}", "POST /servers/{id}/leave", "GET /servers/{id}/icon",
            "GET /channels/{cid}/messages", "GET /channels/{cid}/pins", "POST /channels/{cid}/read", "GET /channels/{cid}/reads",
            "POST /channels/{cid}/typing", "GET /dms", "GET /dms/{id}/messages", "GET /dms/{id}/pins", "POST /dms/{id}/read",
            "GET /dms/{id}/reads", "POST /dms/{id}/typing", "PATCH /messages/{id}", "DELETE /messages/{id}",
            "PUT /messages/{id}/pin", "DELETE /messages/{id}/pin", "DELETE /messages/{id}/reactions/{emoji}",
            "GET /messages/{id}/poll/votes", "GET /files/{id}", "POST /files/{id}/link", "GET /files/{id}/thumb",
            "GET /emojis", "GET /emojis/{eid}/image", "GET /unread", "GET /notifications", "POST /notifications/read",
            "GET /me/notify-settings", "PUT /me/notify-settings", "GET /me/camera-background", "DELETE /me/camera-background",
            "GET /avatars", "GET /users/{id}/avatar", "GET /servers/{id}/avatars", "GET /servers/{id}/users/{uid}/avatar",
            "GET /statuses", "GET /users/{id}/badges", "GET /badges/{id}/icon", "GET /reports", "GET /push/key",
            "DELETE /push/subscription", "DELETE /push/fcm", "DELETE /push/desktop", "GET /servers/{id}/voice",
            "GET /servers/{id}/voice/stats", "GET /servers/{id}/music", "GET /servers/{id}/watch", "GET /voice/{cid}/music",
            "GET /voice/{cid}/watch", "GET /servers/{id}/cinema", "GET /voice/{cid}/cinema", "GET /voice/{cid}/draw", "GET /channels/{cid}/videos",
            "GET /channels/{cid}/videos/posts/{mid}/thumb", "GET /ws", "POST /ws-ticket",
        ]),
    e("servers.create", "Serveurs", "Créer un serveur", "Créer de nouveaux serveurs").routes(&["POST /servers"]),
    e("servers.join", "Serveurs", "Rejoindre un serveur", "Rejoindre un serveur avec un lien d'invitation")
        .routes(&["POST /server-invites/{code}/join"]),
    e("servers.manage", "Serveurs", "Gérer un serveur", "Renommer, changer l'icône, supprimer ou transmettre un serveur (plafond de « Gérer le serveur »)")
        .serveur()
        .routes(&["PATCH /servers/{id}", "DELETE /servers/{id}", "PUT /servers/{id}/icon", "DELETE /servers/{id}/icon", "POST /servers/{id}/transfer"])
        .plafond(roles::MANAGE_SERVER),
    e("servers.invite", "Serveurs", "Inviter", "Créer et révoquer des liens d'invitation (plafond de « Créer des invitations »)")
        .serveur()
        .routes(&["GET /servers/{id}/invites", "POST /servers/{id}/invites", "DELETE /servers/{id}/invites/{code}"])
        .plafond(roles::CREATE_INVITES),
    e("channels.manage", "Serveurs", "Gérer les salons", "Créer, ordonner, renommer, régler et supprimer des salons (plafond de « Gérer les salons »)")
        .serveur()
        .routes(&[
            "POST /servers/{id}/channels", "PUT /servers/{id}/channels/order", "PATCH /servers/{id}/channels/{cid}",
            "DELETE /servers/{id}/channels/{cid}", "PUT /servers/{id}/channels/{cid}/moderation",
            "GET /servers/{id}/channels/{cid}/overrides", "PUT /servers/{id}/channels/{cid}/overrides/{rid}",
        ])
        .plafond(roles::MANAGE_CHANNELS),
    e("roles.manage", "Serveurs", "Gérer les rôles", "Créer, ordonner, modifier et attribuer les rôles d'un serveur (plafond de « Gérer les rôles »)")
        .serveur()
        .routes(&[
            "POST /servers/{id}/roles", "PATCH /servers/{id}/roles/{rid}", "DELETE /servers/{id}/roles/{rid}",
            "PUT /servers/{id}/roles-order", "PUT /servers/{id}/members/{uid}/roles/{rid}", "DELETE /servers/{id}/members/{uid}/roles/{rid}",
        ])
        .plafond(roles::MANAGE_ROLES),
    e("audit.view", "Serveurs", "Journaux de serveur", "Journal d'administration, historique de modération et journal vocal (plafond de « Voir le journal »)")
        .serveur()
        .routes(&["GET /servers/{id}/audit-log", "GET /servers/{id}/members/{uid}/history", "GET /servers/{id}/voice/moderation-log"])
        .plafond(roles::VIEW_AUDIT_LOG),
    e("members.kick", "Modération", "Expulser", "Expulser un membre (plafond de « Expulser des membres »)")
        .serveur()
        .routes(&["POST /servers/{id}/members/{uid}/kick"])
        .plafond(roles::KICK_MEMBERS),
    e("members.ban", "Modération", "Bannir", "Bannir et lever un bannissement (plafond de « Bannir des membres »)")
        .serveur()
        .routes(&["POST /servers/{id}/members/{uid}/ban", "GET /servers/{id}/bans", "DELETE /servers/{id}/bans/{uid}"])
        .plafond(roles::BAN_MEMBERS),
    e("members.timeout", "Modération", "Exclure temporairement", "Exclusion temporaire et avertissements (plafond de « Exclure temporairement »)")
        .serveur()
        .routes(&["PUT /servers/{id}/members/{uid}/timeout", "DELETE /servers/{id}/members/{uid}/timeout", "POST /servers/{id}/members/{uid}/warn"])
        .plafond(roles::TIMEOUT_MEMBERS),
    e("members.nickname", "Modération", "Pseudos de serveur", "Changer le pseudo d'un membre (plafond de « Gérer les pseudos »)")
        .serveur()
        .routes(&["PUT /servers/{id}/members/{uid}/nickname"])
        .plafond(roles::MANAGE_NICKNAMES),
    e("messages.manage", "Modération", "Gérer les messages", "Supprimer en lot et gérer les messages des autres (plafond de « Gérer les messages »)")
        .serveur()
        .routes(&["POST /channels/{cid}/messages/bulk-delete"])
        .plafond(roles::MANAGE_MESSAGES),
    e("vocal.moderate", "Modération", "Modérer le vocal", "Couper micro ou son, déplacer, déconnecter (plafond des droits de modération vocale)")
        .serveur()
        .bloc("vocal")
        .routes(&["PUT /voice/{cid}/participants/{uid}/mod", "PUT /voice/{cid}/participants/{uid}/move", "POST /voice/{cid}/participants/{uid}/disconnect"])
        .plafond(roles::MUTE_MEMBERS | roles::DEAFEN_MEMBERS | roles::MOVE_MEMBERS | roles::DISCONNECT_MEMBERS),
    e("messages.send", "Messages", "Écrire dans les salons", "Envoyer des messages (plafond de « Envoyer des messages »)")
        .serveur()
        .routes(&["POST /channels/{cid}/messages"])
        .plafond(roles::SEND_MESSAGES),
    e("dms.use", "Messages", "Messages privés", "Ouvrir des conversations privées et y écrire")
        .routes(&["POST /dms", "POST /dms/{id}/messages"]),
    e("files.upload", "Messages", "Joindre des fichiers", "Envoyer des fichiers (plafond de « Joindre des fichiers »)")
        .serveur()
        .routes(&["POST /files"])
        .plafond(roles::ATTACH_FILES),
    e("reactions.add", "Messages", "Réagir", "Ajouter des réactions (plafond de « Ajouter des réactions »)")
        .serveur()
        .routes(&["PUT /messages/{id}/reactions/{emoji}"])
        .plafond(roles::ADD_REACTIONS),
    e("polls.create", "Messages", "Créer des sondages", "Créer et clore ses sondages (plafond de « Créer des sondages »)")
        .serveur()
        .routes(&["POST /messages/{id}/poll/close"])
        .plafond(roles::CREATE_POLLS),
    e("polls.vote", "Messages", "Voter", "Voter aux sondages et retirer son vote").routes(&["PUT /messages/{id}/vote", "DELETE /messages/{id}/vote"]),
    e("mention.everyone", "Messages", "Mentionner tout le monde", "Plafond du droit de serveur « Mentionner @tout le monde »")
        .serveur()
        .plafond(roles::MENTION_EVERYONE),
    e("search.use", "Messages", "Rechercher", "Rechercher dans les messages")
        .routes(&["GET /search", "GET /channels/{cid}/search", "GET /dms/{id}/search"]),
    e("gifs.use", "Messages", "GIF", "Chercher et envoyer des GIF (Giphy)")
        .bloc("gifs")
        .routes(&["POST /gifs/import", "GET /gifs/search", "GET /gifs/trending"]),
    e("link-preview.use", "Messages", "Aperçus de liens", "Afficher l'aperçu des liens")
        .bloc("apercu-liens")
        .routes(&["GET /link-preview"]),
    e("emojis.manage", "Messages", "Gérer les emojis", "Ajouter, renommer ou supprimer des emojis de serveur")
        .bloc("emojis")
        .routes(&["POST /servers/{id}/emojis", "PATCH /servers/{id}/emojis/{eid}", "DELETE /servers/{id}/emojis/{eid}"]),
    e("vocal.join", "Vocal", "Rejoindre le vocal", "Parler dans les salons vocaux (plafond de « Se connecter » et « Parler »)")
        .serveur()
        .bloc("vocal")
        .routes(&["POST /voice/{cid}/token"])
        .plafond(roles::CONNECT | roles::SPEAK),
    e("vocal.video", "Vocal", "Caméra", "Activer sa caméra (plafond du droit de serveur « Vidéo »)")
        .serveur()
        .bloc("vocal")
        .plafond(roles::VIDEO),
    e("vocal.screen", "Vocal", "Partage d'écran", "Partager son écran (plafond du droit de serveur « Partager l'écran »)")
        .serveur()
        .bloc("vocal")
        .plafond(roles::SHARE_SCREEN),
    e("music.use", "Vocal", "Musique", "Chercher de la musique et piloter la musique d'un salon vocal")
        .bloc("musique")
        .routes(&["GET /music/search", "GET /music/resolve", "GET /music/cover/{id}", "PUT /voice/{cid}/music/**", "POST /voice/{cid}/music/**", "DELETE /voice/{cid}/music/**"]),
    e("watch.organize", "Vocal", "Regarder ensemble", "Lancer et piloter « Regarder ensemble » et les séances du salon cinéma, dont le titre est vérifié par Entracte (plafond de « Organiser des séances »)")
        .serveur()
        .bloc("regarder-ensemble")
        .routes(&[
            "PUT /voice/{cid}/watch/**", "POST /voice/{cid}/watch/**", "DELETE /voice/{cid}/watch/**",
            "PUT /voice/{cid}/cinema/**", "POST /voice/{cid}/cinema/**", "DELETE /voice/{cid}/cinema/**",
        ])
        .plafond(roles::ORGANIZE_WATCH),
    e("draw2gether.use", "Vocal", "Draw2Gether", "Jouer à Draw2Gether dans un salon vocal")
        .bloc("draw2gether")
        .routes(&["PUT /voice/{cid}/draw/**", "POST /voice/{cid}/draw/**", "DELETE /voice/{cid}/draw/**"]),
    e("transcription.use", "Vocal", "Transcription", "Transcrire les messages vocaux et corriger une transcription")
        .bloc("transcription")
        .routes(&["POST /files/{id}/transcript", "PUT /files/{id}/transcript"]),
    e("videos.use", "Médias", "Salons Vidéos", "Créer des listes, planifier des séances, marquer vu et répondre dans les salons « Vidéos »")
        .bloc("videos")
        .routes(&["POST /channels/{cid}/videos/**", "PUT /channels/{cid}/videos/**", "DELETE /channels/{cid}/videos/**"]),
    e("profile.avatar", "Profil", "Avatar", "Définir son avatar et sa photo par serveur")
        .bloc("avatars")
        .routes(&["PUT /me/avatar", "DELETE /me/avatar", "PUT /servers/{id}/me/avatar", "DELETE /servers/{id}/me/avatar"]),
    e("profile.status", "Profil", "Statut et activité", "Choisir son statut et annoncer son activité")
        .bloc("activite-statut")
        .routes(&["PUT /me/status", "PUT /me/activity"]),
    e("profile.banner", "Profil", "Bannière", "Couleur de la bannière du profil")
        .bloc("activite-statut")
        .routes(&["PUT /me/banner"]),
    e("profile.camera-background", "Profil", "Fond de caméra", "Fond de caméra personnalisé")
        .routes(&["PUT /me/camera-background"]),
    e("notifications.push", "Profil", "Notifications push", "Recevoir les notifications sur ses appareils")
        .bloc("notifications")
        .routes(&["PUT /push/subscription", "PUT /push/fcm", "PUT /push/desktop", "POST /push/test"]),
    e("reports.create", "Profil", "Signaler un bug", "Envoyer des signalements")
        .bloc("signalements")
        .routes(&["POST /reports"]),
    e("admin.view", "Administration", "Vue d'ensemble", "Tableau de bord, statistiques, connexions en direct")
        .ferme()
        .routes(&["GET /admin/overview", "GET /admin/stats", "GET /admin/live"]),
    e("admin.users", "Administration", "Comptes Estompe", "Chercher, suspendre et rétablir des comptes")
        .ferme()
        .routes(&["* /admin/users/**"]),
    e("admin.servers", "Administration", "Serveurs", "Supprimer n'importe quel serveur")
        .ferme()
        .routes(&["* /admin/servers/**"]),
    e("admin.logs", "Administration", "Journaux", "Journal d'administration et erreurs de l'API")
        .ferme()
        .routes(&["GET /admin/log", "GET /admin/errors"]),
    e("admin.bot", "Administration", "Compte « Estompe »", "Envoyer des messages du compte « Estompe »")
        .ferme()
        .routes(&["* /admin/bot/**"]),
    e("admin.badges", "Administration", "Badges", "Créer et attribuer des badges")
        .ferme()
        .bloc("badges")
        .routes(&["* /admin/badges/**"]),
    e("admin.reports", "Administration", "Signalements", "Traiter les signalements")
        .ferme()
        .bloc("signalements")
        .routes(&["* /admin/reports/**"]),
    e("admin.files", "Administration", "Fichiers signalés", "Lire les fichiers joints aux signalements").ferme(),
    e("admin.polls", "Administration", "Sondages", "Clore n'importe quel sondage").ferme(),
];

pub static CONTROLES_CODE: &[(&str, &str)] = &[
    (
        "admin.files",
        "fichiers::acces : ouvre un fichier hors conversation quand il est joint à un signalement",
    ),
    (
        "admin.polls",
        "sondages::peut_gerer : clore le sondage d'un autre",
    ),
];

pub static HORS_PERMISSION: &[(&str, &str)] = &[
    ("GET /healthz", "supervision : sonde de vie"),
    ("GET /readyz", "supervision : sonde de disponibilité"),
    ("GET /metrics", "supervision : jeton METRICS_TOKEN"),
    (
        "GET /me",
        "état du compte : répond aussi à qui n'a pas estompe:access",
    ),
    (
        "POST /activate",
        "activation du compte : vérifie estompe:access elle-même",
    ),
    ("POST /voice/webhook", "webhook LiveKit signé, sans compte"),
    (
        "GET /internal/aegis/resources/{type}",
        "route interne appelée par aegis avec son jeton",
    ),
    (
        "* /admin/acces/**",
        "administration des accès : jugée par aegis (aegis:admin ou rôle Keycloak)",
    ),
];

pub static BITS_HORS_PLAFOND: &[(Perm, &str)] = &[(
    roles::VIEW_CHANNEL,
    "visibilité des salons, réglée par chaque serveur",
)];

pub fn entree(nom: &str) -> Option<&'static Entree> {
    let cle = nom.strip_prefix("estompe:")?;
    CATALOGUE.iter().find(|e| e.cle == cle)
}

pub fn actives<'a>(blocs: &'a [String]) -> impl Iterator<Item = &'static Entree> + 'a {
    CATALOGUE
        .iter()
        .filter(move |e| e.bloc.is_none_or(|b| blocs.iter().any(|x| x == b)))
}

pub fn manifeste(blocs: &[String], ressources_url: &str) -> Value {
    json!({
        "app": APP,
        "name": "Estompe",
        "description": "Messagerie écrite et vocale",
        "resource_types": [{
            "key": SERVEUR,
            "name": "Serveur",
            "list_url": format!("{ressources_url}/internal/aegis/resources/{SERVEUR}"),
        }],
        "permissions": actives(blocs).map(|e| json!({
            "key": e.cle,
            "category": e.categorie,
            "label": e.libelle,
            "description": e.description,
            "resource_type": e.serveur.then_some(SERVEUR),
            "open_default": e.ouvert,
        })).collect::<Vec<_>>(),
    })
}

pub fn ouvertes() -> impl Iterator<Item = &'static Entree> {
    CATALOGUE.iter().filter(|e| e.ouvert)
}

pub fn plafonnees() -> impl Iterator<Item = &'static Entree> {
    CATALOGUE.iter().filter(|e| e.plafond != 0)
}

struct Motif {
    methode: Option<Method>,
    segments: Vec<&'static str>,
}

fn motif(r: &'static str) -> Motif {
    let (m, chemin) = r.split_once(' ').expect("motif « MÉTHODE /chemin »");
    Motif {
        methode: (m != "*").then(|| Method::from_bytes(m.as_bytes()).expect("méthode")),
        segments: chemin.trim_start_matches('/').split('/').collect(),
    }
}

fn correspond(m: &Motif, methode: &Method, chemin: &str) -> bool {
    if m.methode.as_ref().is_some_and(|x| x != methode) {
        return false;
    }
    let segs: Vec<&str> = chemin.trim_start_matches('/').split('/').collect();
    let mut i = 0;
    for (j, p) in m.segments.iter().enumerate() {
        if *p == "**" {
            return j == m.segments.len() - 1;
        }
        let Some(s) = segs.get(i) else {
            return false;
        };
        if p.starts_with('{') || p == s {
            i += 1;
        } else {
            return false;
        }
    }
    i == segs.len()
}

static MOTIFS: LazyLock<Vec<(Motif, &'static Entree)>> = LazyLock::new(|| {
    CATALOGUE
        .iter()
        .flat_map(|e| e.routes.iter().map(move |r| (motif(r), e)))
        .collect()
});

type Trouves = HashMap<(Method, String), Option<&'static Entree>>;

static TROUVES: LazyLock<std::sync::Mutex<Trouves>> = LazyLock::new(Default::default);

pub fn garde(methode: &Method, chemin: &str) -> Option<&'static Entree> {
    let cle = (methode.clone(), chemin.to_string());
    if let Some(r) = TROUVES.lock().unwrap().get(&cle) {
        return *r;
    }
    let r = MOTIFS
        .iter()
        .find(|(m, _)| correspond(m, methode, chemin))
        .map(|(_, e)| *e);
    let mut t = TROUVES.lock().unwrap();
    if t.len() < 4096 {
        t.insert(cle, r);
    }
    r
}

pub fn motif_couvre(motif_texte: &'static str, methode: &Method, chemin: &str) -> bool {
    correspond(&motif(motif_texte), methode, chemin)
}

pub struct Classement {
    pub permissions: Vec<&'static str>,
    pub exceptions: Vec<&'static str>,
}

pub fn classer(methode: &Method, chemin: &str) -> Classement {
    let mut permissions: Vec<&'static str> = CATALOGUE
        .iter()
        .filter(|e| e.routes.iter().any(|r| motif_couvre(r, methode, chemin)))
        .map(|e| e.cle)
        .collect();
    permissions.dedup();
    Classement {
        permissions,
        exceptions: HORS_PERMISSION
            .iter()
            .filter(|(r, _)| motif_couvre(r, methode, chemin))
            .map(|(r, _)| *r)
            .collect(),
    }
}

pub fn bits_plafonnes() -> Perm {
    plafonnees().fold(0, |p, e| p | e.plafond)
}

pub fn documentation() -> String {
    let mut d = String::from(
        "# Fonctionnalités protégées\n\n\
Fichier généré depuis `api/estompe/core/src/catalogue.rs` : ne pas le modifier à la main. Après un changement du catalogue,\n\
`AEGIS_DOC=1 cargo test -p estompe-core catalogue::tests::documentation_a_jour` le régénère ; sans cette variable, le test échoue\n\
tant que ce fichier ne correspond pas au code.\n\n\
Chaque fonctionnalité d'Estompe a sa permission, envoyée à aegis par manifeste. Des tests déterministes l'imposent :\n\n\
- **chaque route** documentée dans l'OpenAPI relève d'exactement une permission ou d'une exception motivée (liste en fin de fichier) ;\n\
  une nouvelle route sans décision fait échouer `estompe-api` ;\n\
- **chaque droit de serveur** est plafonné par une permission, sauf ceux listés en fin de fichier ;\n\
- **chaque permission** protège au moins une route, plafonne un droit de serveur, ou est contrôlée dans le code (liste) ;\n\
- les permissions utilisées par le front (`app.peut`, sections d'administration) existent dans le catalogue.\n\n\
Mécanismes : la **garde** vérifie la permission sur les routes listées (403 `forbidden`, 503 si aegis est injoignable ; pour une\n\
permission par serveur, il suffit qu'elle soit accordée sur au moins un serveur) ; le **plafond** retire le droit de serveur\n\
correspondant quand la permission est refusée sur ce serveur, propriétaire compris ; l'interface masque ce qui est refusé\n\
(`capabilities` de `GET /me`).\n\n\
« Défaut » : ouverte = accordée à « Tout le monde » à sa première déclaration ; fermée = à accorder explicitement.\n\
Une permission dont le bloc est désactivé n'est pas déclarée.\n",
    );
    let mut categories: Vec<&str> = Vec::new();
    for e in CATALOGUE {
        if !categories.contains(&e.categorie) {
            categories.push(e.categorie);
        }
    }
    let noms_bits = |p: Perm| roles::cles(p).join(" + ");
    for c in categories {
        d.push_str(&format!(
            "\n## {c}\n\n| Permission | Libellé | Défaut | Ressource | Bloc | Routes protégées | Plafond |\n| --- | --- | --- | --- | --- | --- | --- |\n"
        ));
        for e in CATALOGUE.iter().filter(|e| e.categorie == c) {
            let routes = if e.routes.is_empty() {
                CONTROLES_CODE
                    .iter()
                    .find(|(k, _)| *k == e.cle)
                    .map_or("—".to_string(), |(_, r)| {
                        format!("contrôle dans le code : {r}")
                    })
            } else {
                e.routes
                    .iter()
                    .map(|r| format!("`{r}`"))
                    .collect::<Vec<_>>()
                    .join("<br>")
            };
            d.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} | {} |\n",
                e.nom(),
                e.libelle,
                if e.ouvert { "ouverte" } else { "fermée" },
                if e.serveur { "serveur" } else { "—" },
                e.bloc.unwrap_or("—"),
                routes,
                if e.plafond == 0 {
                    "—".to_string()
                } else {
                    noms_bits(e.plafond)
                },
            ));
        }
    }
    d.push_str("\n## Routes hors permission\n\n| Route | Motif |\n| --- | --- |\n");
    for (r, m) in HORS_PERMISSION {
        d.push_str(&format!("| `{r}` | {m} |\n"));
    }
    d.push_str("\n## Droits de serveur non plafonnés\n\n| Droit | Motif |\n| --- | --- |\n");
    for (b, m) in BITS_HORS_PLAFOND {
        d.push_str(&format!("| `{}` | {m} |\n", noms_bits(*b)));
    }
    d.push_str(
        "\n## aegis\n\n| Permission | Libellé | Défaut | Contrôle |\n| --- | --- | --- | --- |\n\
| `aegis:admin` | Administrer les permissions | fermée | toute l'API d'administration ; rôle système « Administrateur aegis » |\n",
    );
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documentation_a_jour() {
        let chemin = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../docs/aegis/fonctionnalites.md"
        );
        let attendu = documentation();
        if std::env::var("AEGIS_DOC").is_ok() {
            std::fs::write(chemin, &attendu).unwrap();
        }
        let actuel = std::fs::read_to_string(chemin).unwrap_or_default();
        assert!(
            actuel == attendu,
            "docs/aegis/fonctionnalites.md ne correspond plus au catalogue : AEGIS_DOC=1 cargo test -p estompe-core catalogue::tests::documentation_a_jour"
        );
    }

    #[test]
    fn chaque_droit_de_serveur_est_plafonne() {
        let couverts = bits_plafonnes();
        let exemptes = BITS_HORS_PLAFOND.iter().fold(0, |p, (b, _)| p | b);
        for (cle, bit) in roles::toutes() {
            let par: Vec<&str> = plafonnees()
                .filter(|e| e.plafond & bit != 0)
                .map(|e| e.cle)
                .collect();
            if exemptes & bit != 0 {
                assert!(
                    par.is_empty(),
                    "{cle} est exempté mais plafonné par {par:?}"
                );
            } else {
                assert_eq!(
                    par.len(),
                    1,
                    "le droit de serveur {cle} doit être plafonné par exactement une permission : {par:?}"
                );
            }
        }
        let declares = roles::toutes().iter().fold(0, |p, (_, b)| p | b);
        assert_eq!(
            couverts | exemptes,
            declares,
            "le catalogue plafonne un droit de serveur inconnu"
        );
    }

    #[test]
    fn chaque_permission_sert_a_quelque_chose() {
        for e in CATALOGUE {
            let code = CONTROLES_CODE.iter().any(|(k, _)| *k == e.cle);
            assert!(
                !e.routes.is_empty() || e.plafond != 0 || code,
                "{} ne protège aucune route, ne plafonne rien et n'est pas déclarée dans CONTROLES_CODE",
                e.cle
            );
            assert!(
                !e.serveur || e.plafond != 0 || e.routes.is_empty(),
                "{} : permission par serveur sans plafond",
                e.cle
            );
        }
        for (k, _) in CONTROLES_CODE {
            assert!(
                CATALOGUE.iter().any(|e| e.cle == *k),
                "{k} absent du catalogue"
            );
        }
        for (r, _) in HORS_PERMISSION {
            motif(r);
        }
    }

    #[test]
    fn cles_uniques_et_valides() {
        let mut vues = std::collections::HashSet::new();
        for e in CATALOGUE {
            assert!(vues.insert(e.cle), "{} en double", e.cle);
            assert!(
                e.cle
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || ".-_".contains(c)),
                "{}",
                e.cle
            );
            assert!(
                !e.admin() || !e.ouvert,
                "{} : l'administration est fermée",
                e.cle
            );
            for r in e.routes {
                motif(r);
            }
        }
    }

    #[test]
    fn correspondance_des_routes() {
        assert_eq!(
            garde(&Method::GET, "/gifs/search").map(|e| e.cle),
            Some("gifs.use")
        );
        assert_eq!(
            garde(&Method::PUT, "/voice/{cid}/music").map(|e| e.cle),
            Some("music.use")
        );
        assert_eq!(
            garde(&Method::GET, "/voice/{cid}/music").map(|e| e.cle),
            Some("access")
        );
        assert_eq!(
            garde(&Method::DELETE, "/admin/users/{id}/suspension").map(|e| e.cle),
            Some("admin.users")
        );
        assert_eq!(
            garde(&Method::GET, "/admin/users").map(|e| e.cle),
            Some("admin.users")
        );
        assert_eq!(
            garde(&Method::GET, "/servers").map(|e| e.cle),
            Some("access")
        );
        assert_eq!(garde(&Method::GET, "/me").map(|e| e.cle), None);
        assert_eq!(
            garde(&Method::POST, "/servers").map(|e| e.cle),
            Some("servers.create")
        );
        assert_eq!(garde(&Method::GET, "/me/avatar").map(|e| e.cle), None);
    }

    #[test]
    fn manifeste_selon_les_blocs() {
        let tous: Vec<String> = CATALOGUE
            .iter()
            .filter_map(|e| e.bloc)
            .map(String::from)
            .collect();
        let m = manifeste(&tous, "http://estompe-api:8080");
        assert_eq!(m["permissions"].as_array().unwrap().len(), CATALOGUE.len());
        assert_eq!(
            m["resource_types"][0]["list_url"],
            "http://estompe-api:8080/internal/aegis/resources/server"
        );
        let sans = manifeste(&[], "x");
        let cles: Vec<&str> = sans["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["key"].as_str().unwrap())
            .collect();
        assert!(cles.contains(&"access") && !cles.contains(&"gifs.use"));
    }
}
