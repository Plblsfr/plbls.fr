use std::{collections::HashMap, sync::LazyLock};

use axum::http::Method;
use serde_json::{Value, json};

use crate::roles::{self, Perm};

pub const APP: &str = "estompe";
pub const SERVEUR: &str = "server";

pub const ACCES: &str = "estompe:access";
pub const FILMS: &str = "estompe:films.use";
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
    e(
        "access",
        "Accès",
        "Utiliser Estompe",
        "Se connecter à Estompe, avec ou sans compte activé",
    )
    .ferme(),
    e(
        "servers.create",
        "Serveurs",
        "Créer un serveur",
        "Créer de nouveaux serveurs",
    )
    .routes(&["POST /servers"]),
    e(
        "servers.join",
        "Serveurs",
        "Rejoindre un serveur",
        "Rejoindre un serveur avec un lien d'invitation",
    )
    .routes(&["POST /server-invites/{code}/join"]),
    e(
        "servers.invite",
        "Serveurs",
        "Inviter",
        "Créer des liens d'invitation (plafond du droit de serveur « Créer des invitations »)",
    )
    .serveur()
    .plafond(roles::CREATE_INVITES),
    e(
        "servers.icon",
        "Serveurs",
        "Icône de serveur",
        "Définir ou retirer l'icône d'un serveur qu'on gère",
    )
    .routes(&["PUT /servers/{id}/icon", "DELETE /servers/{id}/icon"]),
    e(
        "messages.send",
        "Messages",
        "Écrire dans les salons",
        "Envoyer des messages (plafond du droit de serveur « Envoyer des messages »)",
    )
    .serveur()
    .plafond(roles::SEND_MESSAGES),
    e(
        "dms.use",
        "Messages",
        "Messages privés",
        "Ouvrir des conversations privées et y écrire",
    )
    .routes(&["POST /dms", "POST /dms/{id}/messages"]),
    e(
        "files.upload",
        "Messages",
        "Joindre des fichiers",
        "Envoyer des fichiers (plafond du droit de serveur « Joindre des fichiers »)",
    )
    .serveur()
    .routes(&["POST /files"])
    .plafond(roles::ATTACH_FILES),
    e(
        "reactions.add",
        "Messages",
        "Réagir",
        "Ajouter des réactions (plafond du droit de serveur « Ajouter des réactions »)",
    )
    .serveur()
    .routes(&["PUT /messages/{id}/reactions/{emoji}"])
    .plafond(roles::ADD_REACTIONS),
    e(
        "polls.create",
        "Messages",
        "Créer des sondages",
        "Plafond du droit de serveur « Créer des sondages »",
    )
    .serveur()
    .plafond(roles::CREATE_POLLS),
    e("polls.vote", "Messages", "Voter", "Voter aux sondages").routes(&["PUT /messages/{id}/vote"]),
    e(
        "mention.everyone",
        "Messages",
        "Mentionner tout le monde",
        "Plafond du droit de serveur « Mentionner @tout le monde »",
    )
    .serveur()
    .plafond(roles::MENTION_EVERYONE),
    e(
        "search.use",
        "Messages",
        "Rechercher",
        "Rechercher dans les messages",
    )
    .routes(&[
        "GET /search",
        "GET /channels/{cid}/search",
        "GET /dms/{id}/search",
    ]),
    e(
        "gifs.use",
        "Messages",
        "GIF",
        "Chercher et envoyer des GIF (Giphy)",
    )
    .bloc("gifs")
    .routes(&[
        "POST /gifs/import",
        "GET /gifs/search",
        "GET /gifs/trending",
    ]),
    e(
        "link-preview.use",
        "Messages",
        "Aperçus de liens",
        "Afficher l'aperçu des liens",
    )
    .bloc("apercu-liens")
    .routes(&["GET /link-preview"]),
    e(
        "emojis.manage",
        "Messages",
        "Gérer les emojis",
        "Ajouter, renommer ou supprimer des emojis de serveur",
    )
    .bloc("emojis")
    .routes(&[
        "POST /servers/{id}/emojis",
        "PATCH /servers/{id}/emojis/{eid}",
        "DELETE /servers/{id}/emojis/{eid}",
    ]),
    e(
        "vocal.join",
        "Vocal",
        "Rejoindre le vocal",
        "Parler dans les salons vocaux (plafond de « Se connecter » et « Parler »)",
    )
    .serveur()
    .bloc("vocal")
    .routes(&["POST /voice/{cid}/token"])
    .plafond(roles::CONNECT | roles::SPEAK),
    e(
        "vocal.video",
        "Vocal",
        "Caméra",
        "Activer sa caméra (plafond du droit de serveur « Vidéo »)",
    )
    .serveur()
    .bloc("vocal")
    .plafond(roles::VIDEO),
    e(
        "vocal.screen",
        "Vocal",
        "Partage d'écran",
        "Partager son écran (plafond du droit de serveur « Partager l'écran »)",
    )
    .serveur()
    .bloc("vocal")
    .plafond(roles::SHARE_SCREEN),
    e(
        "music.use",
        "Vocal",
        "Musique",
        "Chercher de la musique et piloter la musique d'un salon vocal",
    )
    .bloc("musique")
    .routes(&[
        "GET /music/search",
        "GET /music/resolve",
        "PUT /voice/{cid}/music/**",
        "POST /voice/{cid}/music/**",
        "DELETE /voice/{cid}/music/**",
    ]),
    e(
        "watch.organize",
        "Vocal",
        "Regarder ensemble",
        "Lancer et piloter « Regarder ensemble » (plafond du droit de serveur)",
    )
    .serveur()
    .bloc("regarder-ensemble")
    .routes(&[
        "PUT /voice/{cid}/watch/**",
        "POST /voice/{cid}/watch/**",
        "DELETE /voice/{cid}/watch/**",
    ])
    .plafond(roles::ORGANIZE_WATCH),
    e(
        "draw2gether.use",
        "Vocal",
        "Draw2Gether",
        "Jouer à Draw2Gether dans un salon vocal",
    )
    .bloc("draw2gether")
    .routes(&[
        "PUT /voice/{cid}/draw/**",
        "POST /voice/{cid}/draw/**",
        "DELETE /voice/{cid}/draw/**",
    ]),
    e(
        "transcription.use",
        "Vocal",
        "Transcription",
        "Transcrire les messages vocaux",
    )
    .bloc("transcription")
    .routes(&["POST /files/{id}/transcript", "PUT /files/{id}/transcript"]),
    e(
        "films.use",
        "Médias",
        "Films et séries",
        "Bibliothèque Jellyfin et séances de cinéma",
    )
    .ferme()
    .bloc("films")
    .routes(&[
        "* /films/**",
        "* /voice/{cid}/cinema/**",
        "GET /servers/{id}/cinema",
    ]),
    e(
        "videos.use",
        "Médias",
        "Salons Vidéos",
        "Créer des listes, planifier des séances et répondre dans les salons « Vidéos »",
    )
    .bloc("videos")
    .routes(&[
        "POST /channels/{cid}/videos/**",
        "PUT /channels/{cid}/videos/**",
        "DELETE /channels/{cid}/videos/**",
    ]),
    e(
        "profile.avatar",
        "Profil",
        "Avatar",
        "Définir son avatar et sa photo par serveur",
    )
    .bloc("avatars")
    .routes(&[
        "PUT /me/avatar",
        "DELETE /me/avatar",
        "PUT /servers/{id}/me/avatar",
        "DELETE /servers/{id}/me/avatar",
    ]),
    e(
        "profile.status",
        "Profil",
        "Statut et activité",
        "Choisir son statut et annoncer son activité",
    )
    .bloc("activite-statut")
    .routes(&["PUT /me/status", "PUT /me/activity"]),
    e(
        "profile.banner",
        "Profil",
        "Bannière",
        "Couleur de la bannière du profil",
    )
    .bloc("activite-statut")
    .routes(&["PUT /me/banner"]),
    e(
        "profile.camera-background",
        "Profil",
        "Fond de caméra",
        "Fond de caméra personnalisé",
    )
    .routes(&["PUT /me/camera-background"]),
    e(
        "notifications.push",
        "Profil",
        "Notifications push",
        "Recevoir les notifications sur ses appareils",
    )
    .bloc("notifications")
    .routes(&[
        "PUT /push/subscription",
        "PUT /push/fcm",
        "PUT /push/desktop",
        "POST /push/test",
    ]),
    e(
        "reports.create",
        "Profil",
        "Signaler un bug",
        "Envoyer des signalements",
    )
    .bloc("signalements")
    .routes(&["POST /reports"]),
    e(
        "admin.view",
        "Administration",
        "Vue d'ensemble",
        "Tableau de bord, statistiques, connexions en direct",
    )
    .ferme()
    .routes(&["GET /admin/overview", "GET /admin/stats", "GET /admin/live"]),
    e(
        "admin.users",
        "Administration",
        "Comptes Estompe",
        "Chercher, suspendre et rétablir des comptes",
    )
    .ferme()
    .routes(&["* /admin/users/**"]),
    e(
        "admin.servers",
        "Administration",
        "Serveurs",
        "Supprimer n'importe quel serveur",
    )
    .ferme()
    .routes(&["* /admin/servers/**"]),
    e(
        "admin.logs",
        "Administration",
        "Journaux",
        "Journal d'administration et erreurs de l'API",
    )
    .ferme()
    .routes(&["GET /admin/log", "GET /admin/errors"]),
    e(
        "admin.bot",
        "Administration",
        "Compte « Estompe »",
        "Envoyer des messages du compte « Estompe »",
    )
    .ferme()
    .routes(&["* /admin/bot/**"]),
    e(
        "admin.badges",
        "Administration",
        "Badges",
        "Créer et attribuer des badges",
    )
    .ferme()
    .bloc("badges")
    .routes(&["* /admin/badges/**"]),
    e(
        "admin.reports",
        "Administration",
        "Signalements",
        "Traiter les signalements",
    )
    .ferme()
    .bloc("signalements")
    .routes(&["* /admin/reports/**"]),
    e(
        "admin.films",
        "Administration",
        "Fichiers écartés",
        "Fichiers de films écartés par l'import",
    )
    .ferme()
    .bloc("films")
    .routes(&["GET /admin/films/rejets"]),
    e(
        "admin.files",
        "Administration",
        "Fichiers signalés",
        "Lire les fichiers joints aux signalements",
    )
    .ferme(),
    e(
        "admin.polls",
        "Administration",
        "Sondages",
        "Clore n'importe quel sondage",
    )
    .ferme(),
];

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

#[cfg(test)]
mod tests {
    use super::*;

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
            garde(&Method::GET, "/films/library").map(|e| e.cle),
            Some("films.use")
        );
        assert_eq!(
            garde(&Method::GET, "/films/stream/{jeton}/u/{n}").map(|e| e.cle),
            Some("films.use")
        );
        assert_eq!(
            garde(&Method::PUT, "/voice/{cid}/music").map(|e| e.cle),
            Some("music.use")
        );
        assert_eq!(
            garde(&Method::GET, "/voice/{cid}/music").map(|e| e.cle),
            None
        );
        assert_eq!(
            garde(&Method::DELETE, "/admin/users/{id}/suspension").map(|e| e.cle),
            Some("admin.users")
        );
        assert_eq!(
            garde(&Method::GET, "/admin/users").map(|e| e.cle),
            Some("admin.users")
        );
        assert_eq!(garde(&Method::GET, "/servers").map(|e| e.cle), None);
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
