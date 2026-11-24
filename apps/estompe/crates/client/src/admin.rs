use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::emoji::replier;
use crate::format::date_locale;
use crate::types::{GetMeReponse, GetMeReponseCapabilitiesValeur, GetMeReponseRightsElement};

pub type Droit = GetMeReponseRightsElement;
pub type Capacites = BTreeMap<String, GetMeReponseCapabilitiesValeur>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionAdmin {
    pub id: &'static str,
    pub libelle: &'static str,
    pub icone: &'static str,
    pub droit: Droit,
    pub capacite: Option<&'static str>,
}

const fn section(
    id: &'static str,
    libelle: &'static str,
    icone: &'static str,
    capacite: &'static str,
) -> SectionAdmin {
    SectionAdmin {
        id,
        libelle,
        icone,
        droit: Droit::Admin,
        capacite: Some(capacite),
    }
}

pub const SECTIONS: [SectionAdmin; 8] = [
    section(
        "utilisateurs",
        "utilisateurs",
        "users",
        "estompe:admin.users",
    ),
    section("serveurs", "serveurs", "monitor", "estompe:admin.view"),
    section("badges", "badges", "pin", "estompe:admin.badges"),
    section(
        "signalements",
        "signalements",
        "flag",
        "estompe:admin.reports",
    ),
    section("bot", "bot", "mail", "estompe:admin.bot"),
    section(
        "statistiques",
        "statistiques",
        "chart",
        "estompe:admin.view",
    ),
    section("journaux", "logs", "file", "estompe:admin.logs"),
    SectionAdmin {
        id: "acces",
        libelle: "accès",
        icone: "cadenas",
        droit: Droit::Acces,
        capacite: None,
    },
];

pub fn droits_de(moi: &GetMeReponse) -> Vec<Droit> {
    if let Some(droits) = &moi.rights {
        return droits.clone();
    }
    let mut droits = Vec::new();
    if moi.allowed == Some(true) {
        droits.push(Droit::Access);
    }
    if moi.admin == Some(true) {
        droits.push(Droit::Admin);
    }
    droits
}

pub fn autorise(capacites: Option<&Capacites>, nom: &str, ressource: &str) -> bool {
    let Some(capacites) = capacites else {
        return true;
    };
    let Some(c) = capacites.get(nom) else {
        return false;
    };
    let contient = |liste: &Option<Vec<String>>, r: &str| {
        liste.as_deref().unwrap_or_default().iter().any(|x| x == r)
    };
    if contient(&c.deny, "*") || contient(&c.deny, ressource) {
        return false;
    }
    contient(&c.allow, "*") || contient(&c.allow, ressource)
}

pub fn sections_admin(droits: &[Droit], capacites: Option<&Capacites>) -> Vec<SectionAdmin> {
    SECTIONS
        .iter()
        .filter(|s| {
            droits.contains(&s.droit) && s.capacite.is_none_or(|c| autorise(capacites, c, "*"))
        })
        .copied()
        .collect()
}

pub fn sections_du_compte(moi: &GetMeReponse) -> Vec<SectionAdmin> {
    sections_admin(&droits_de(moi), moi.capabilities.as_ref())
}

pub fn section_admin(id: Option<&str>, sections: &[SectionAdmin]) -> Option<&'static str> {
    sections
        .iter()
        .find(|s| Some(s.id) == id)
        .or(sections.first())
        .map(|s| s.id)
}

pub fn titre_de(section: &SectionAdmin) -> String {
    let mut lettres = section.libelle.chars();
    lettres
        .next()
        .map(|c| c.to_uppercase().chain(lettres).collect())
        .unwrap_or_default()
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompteAdmin {
    pub id: String,
    pub pseudo: String,
    pub created_at: String,
    pub suspended: bool,
    pub servers: i64,
    pub messages: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Detenteur {
    pub id: String,
    pub pseudo: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BadgeAdmin {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub description: String,
    pub icon_version: i64,
    pub sort_order: i64,
    pub created_at: String,
    pub holders: Vec<Detenteur>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServeurAdmin {
    pub id: String,
    pub name: String,
    pub members: i64,
    pub channels: i64,
    pub messages: i64,
    pub last_message_at: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReglagesInstance {
    pub push_web: bool,
    pub push_android: bool,
    pub giphy: bool,
    pub metrics: bool,
    pub allow_video: bool,
    pub livekit_url: String,
    pub max_storage: i64,
    pub origins: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigurationInstance {
    pub settings: ReglagesInstance,
    pub max_file_size: i64,
    pub max_participants: i64,
    pub screen_max_fps: Option<i64>,
    pub screen_max_height: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VueEnsemble {
    pub version: String,
    pub servers: Vec<ServeurAdmin>,
    pub config: ConfigurationInstance,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Statistiques {
    pub users: i64,
    pub new_users_7d: i64,
    pub suspended: i64,
    pub servers: i64,
    pub messages: i64,
    pub messages_24h: i64,
    pub files: i64,
    pub storage_bytes: i64,
    pub errors_24h: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppareilConnecte {
    pub pseudo: String,
    pub platform: String,
    pub version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticipantDirect {
    pub user_id: String,
    pub pseudo: String,
    pub screen: bool,
    pub camera: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SalonDirect {
    pub server: String,
    pub channel: String,
    pub participants: Vec<ParticipantDirect>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Direct {
    pub connections: i64,
    pub online: Vec<AppareilConnecte>,
    pub voice: Vec<SalonDirect>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ActionJournal {
    pub at: String,
    pub actor: String,
    pub action: String,
    pub target: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ErreurServeur {
    pub at: String,
    pub method: String,
    pub path: String,
    pub status: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bot {
    pub pseudo: String,
    pub releases: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Attribution {
    pub granted: i64,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriBadges {
    Ordre,
    Nom,
    Detenteurs,
    Creation,
}

pub const TRIS_BADGES: [(TriBadges, &str); 4] = [
    (TriBadges::Ordre, "Ordre d'affichage"),
    (TriBadges::Nom, "Nom"),
    (TriBadges::Detenteurs, "Nombre de détenteurs"),
    (TriBadges::Creation, "Date de création"),
];

pub fn filtrer_badges(liste: &[BadgeAdmin], recherche: &str, tri: TriBadges) -> Vec<BadgeAdmin> {
    let q = replier(recherche.trim());
    let mut trouves: Vec<BadgeAdmin> = liste
        .iter()
        .filter(|b| {
            q.is_empty() || replier(&b.name).contains(&q) || replier(&b.description).contains(&q)
        })
        .cloned()
        .collect();
    let nom = |a: &BadgeAdmin, b: &BadgeAdmin| replier(&a.name).cmp(&replier(&b.name));
    trouves.sort_by(|a, b| match tri {
        TriBadges::Ordre => a.sort_order.cmp(&b.sort_order).then_with(|| nom(a, b)),
        TriBadges::Nom => nom(a, b),
        TriBadges::Detenteurs => b
            .holders
            .len()
            .cmp(&a.holders.len())
            .then_with(|| nom(a, b)),
        TriBadges::Creation => b.created_at.cmp(&a.created_at).then_with(|| nom(a, b)),
    });
    trouves
}

pub const TYPES_ICONE: [&str; 4] = ["image/svg+xml", "image/webp", "image/png", "image/jpeg"];
pub const EXTENSIONS_ICONE: &[&str] = &["svg", "webp", "png", "jpg", "jpeg"];
pub const OCTETS_SVG_MAX: u64 = 64 << 10;
pub const OCTETS_IMAGE_MAX: u64 = 5 << 20;

pub fn type_d_icone(nom: &str) -> Option<&'static str> {
    let extension = nom.rsplit_once('.')?.1.to_lowercase();
    match extension.as_str() {
        "svg" => Some("image/svg+xml"),
        "webp" => Some("image/webp"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        _ => None,
    }
}

pub fn probleme_d_icone(type_mime: &str, taille: u64) -> &'static str {
    if !TYPES_ICONE.contains(&type_mime) {
        return "Format non pris en charge : SVG, WebP, PNG ou JPEG uniquement.";
    }
    if type_mime == "image/svg+xml" && taille > OCTETS_SVG_MAX {
        return "SVG trop lourd (64 Ko maximum).";
    }
    if taille > OCTETS_IMAGE_MAX {
        return "Image trop lourde.";
    }
    ""
}

pub fn couleur_valide(couleur: &str) -> bool {
    couleur.len() == 7
        && couleur.starts_with('#')
        && couleur[1..].chars().all(|c| c.is_ascii_hexdigit())
}

pub fn nom_de_badge_valide(nom: &str) -> bool {
    (2..=32).contains(&nom.trim().chars().count())
}

pub fn ordre_saisi(texte: &str) -> i64 {
    texte.trim().parse::<i64>().unwrap_or(0).clamp(0, 9999)
}

pub fn detenteurs(n: usize) -> String {
    match n {
        0 => "Aucun détenteur".into(),
        1 => "1 détenteur".into(),
        n => format!("{n} détenteurs"),
    }
}

pub fn attribuer_a(n: usize) -> String {
    format!("Attribuer à {n} compte{}", if n > 1 { "s" } else { "" })
}

pub fn choix_de_detenteurs(
    trouves: &[CompteAdmin],
    choisis: &[CompteAdmin],
    badge: &BadgeAdmin,
) -> Vec<CompteAdmin> {
    trouves
        .iter()
        .filter(|u| {
            !choisis.iter().any(|c| c.id == u.id) && !badge.holders.iter().any(|h| h.id == u.id)
        })
        .cloned()
        .collect()
}

pub fn message_de_suppression(badge: &BadgeAdmin) -> String {
    let n = badge.holders.len();
    let qui = if n == 0 {
        "Personne ne le détient.".to_string()
    } else {
        format!("Il sera retiré à {}.", detenteurs(n).to_lowercase())
    };
    format!(
        "Supprimer le badge « {} » ? {qui} Cette action est définitive.",
        badge.name
    )
}

pub fn nombre(n: i64) -> String {
    let chiffres = n.unsigned_abs().to_string();
    let mut sortie = String::new();
    for (i, c) in chiffres.chars().enumerate() {
        if i > 0 && (chiffres.len() - i).is_multiple_of(3) {
            sortie.push('\u{202f}');
        }
        sortie.push(c);
    }
    if n < 0 {
        format!("-{sortie}")
    } else {
        sortie
    }
}

pub fn mega_octets(n: i64) -> String {
    format!("{} Mo", (n as f64 / f64::from(1 << 20)).round() as i64)
}

pub fn giga_octets(n: i64) -> i64 {
    (n as f64 / f64::from(1 << 30)).round() as i64
}

pub fn usage_stockage(stockage: i64, maximum: i64) -> Option<f32> {
    (maximum > 0).then(|| (stockage as f32 / maximum as f32 * 100.0).min(100.0))
}

pub fn configure(b: bool) -> &'static str {
    if b {
        "configuré"
    } else {
        "non configuré"
    }
}

pub fn detail_livekit(c: &ConfigurationInstance) -> String {
    let ecran = match (c.screen_max_fps, c.screen_max_height) {
        (Some(fps), hauteur) if fps > 0 => {
            format!(" · écran {}p, {fps} i/s", hauteur.unwrap_or_default())
        }
        _ => String::new(),
    };
    format!(
        "{} · {} participants max · caméra et écran {}{ecran}",
        c.settings.livekit_url,
        c.max_participants,
        if c.settings.allow_video {
            "autorisés"
        } else {
            "désactivés"
        }
    )
}

pub fn jour_d_inscription(iso: &str, decalage_minutes: i32) -> String {
    date_locale(iso, decalage_minutes)
        .map(|d| format!("inscrit le {:02}/{:02}/{}", d.jour, d.mois, d.annee))
        .unwrap_or_default()
}

pub fn activite_du_compte(u: &CompteAdmin) -> String {
    format!(
        "{} serveur(s) · {} message(s)",
        u.servers,
        nombre(u.messages)
    )
}

pub fn destinataire_du_bot(saisie: &str) -> String {
    let to = saisie.trim();
    if to.is_empty() {
        "tous les comptes".into()
    } else {
        to.to_string()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn ids(sections: &[SectionAdmin]) -> Vec<&'static str> {
        sections.iter().map(|s| s.id).collect()
    }

    fn caps(v: serde_json::Value) -> Capacites {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn les_sections_suivent_les_droits_du_compte() {
        assert_eq!(ids(&sections_admin(&[Droit::Acces], None)), ["acces"]);
        assert!(sections_admin(&[Droit::Access], None).is_empty());
        let tout = sections_admin(&[Droit::Admin, Droit::Acces], None);
        assert_eq!(tout.first().unwrap().id, "utilisateurs");
        assert_eq!(tout.last().unwrap().id, "acces");
    }

    #[test]
    fn les_fichiers_ecartes_du_bloc_films_ne_sont_plus_une_section() {
        assert!(!sections_admin(&[Droit::Admin], None)
            .iter()
            .any(|s| s.id == "films"));
    }

    #[test]
    fn la_section_retombe_sur_la_premiere_permise() {
        let seules = sections_admin(&[Droit::Acces], None);
        assert_eq!(section_admin(Some("utilisateurs"), &seules), Some("acces"));
        let tout = sections_admin(&[Droit::Admin, Droit::Acces], None);
        assert_eq!(section_admin(None, &tout), Some("utilisateurs"));
        assert_eq!(section_admin(Some("acces"), &tout), Some("acces"));
        assert_eq!(
            section_admin(Some("permissions"), &tout),
            Some("utilisateurs")
        );
        assert_eq!(section_admin(Some("utilisateurs"), &[]), None);
    }

    #[test]
    fn les_droits_se_deduisent_des_anciens_champs_de_me() {
        let ancien: GetMeReponse =
            serde_json::from_value(json!({"activated": true, "allowed": true, "admin": true}))
                .unwrap();
        assert_eq!(droits_de(&ancien), [Droit::Access, Droit::Admin]);
        let nouveau: GetMeReponse = serde_json::from_value(
            json!({"activated": false, "allowed": false, "rights": ["acces"]}),
        )
        .unwrap();
        assert_eq!(droits_de(&nouveau), [Droit::Acces]);
    }

    #[test]
    fn les_capacites_fines_filtrent_les_sections() {
        assert_eq!(ids(&sections_admin(&[Droit::Acces], None)), ["acces"]);
        let c = caps(json!({
            "estompe:admin.users": {"allow": ["*"], "deny": []},
            "estompe:admin.badges": {"allow": ["*"], "deny": ["*"]}
        }));
        assert_eq!(
            ids(&sections_admin(&[Droit::Admin], Some(&c))),
            ["utilisateurs"]
        );
        assert!(sections_admin(&[Droit::Admin], None).len() > 5);
    }

    #[test]
    fn le_refus_l_emporte() {
        let c = caps(json!({
            "estompe:vocal.screen": {"allow": ["*"], "deny": ["srv-1"]},
            "estompe:videos.use": {"allow": ["lib"], "deny": []}
        }));
        assert!(autorise(Some(&c), "estompe:vocal.screen", "srv-2"));
        assert!(!autorise(Some(&c), "estompe:vocal.screen", "srv-1"));
        assert!(!autorise(Some(&c), "estompe:videos.use", "*"));
        assert!(!autorise(Some(&c), "estompe:gifs.use", "*"));
        assert!(autorise(None, "estompe:gifs.use", "*"));
    }

    #[test]
    fn un_compte_sans_droit_n_a_aucune_section() {
        let moi: GetMeReponse =
            serde_json::from_value(json!({"activated": true, "allowed": true})).unwrap();
        assert!(sections_du_compte(&moi).is_empty());
        let admin: GetMeReponse =
            serde_json::from_value(json!({"activated": true, "allowed": true, "admin": true}))
                .unwrap();
        assert_eq!(sections_du_compte(&admin).len(), 7);
    }

    #[test]
    fn le_titre_prend_une_majuscule() {
        assert_eq!(titre_de(&SECTIONS[0]), "Utilisateurs");
        assert_eq!(titre_de(&SECTIONS[7]), "Accès");
        assert_eq!(titre_de(&SECTIONS[6]), "Logs");
    }

    fn badge(nom: &str, ordre: i64, n: usize, cree: &str, description: &str) -> BadgeAdmin {
        BadgeAdmin {
            id: nom.into(),
            name: nom.into(),
            icon: "🏅".into(),
            color: "#000000".into(),
            description: description.into(),
            sort_order: ordre,
            created_at: cree.into(),
            holders: (0..n)
                .map(|i| Detenteur {
                    id: format!("{nom}{i}"),
                    pseudo: format!("{nom}{i}"),
                })
                .collect(),
            ..BadgeAdmin::default()
        }
    }

    fn liste() -> Vec<BadgeAdmin> {
        vec![
            badge("Étoile", 2, 1, "2026-01-02", ""),
            badge("Alpha", 3, 5, "2026-01-01", "Premier arrivé"),
            badge("Zèbre", 1, 0, "2026-01-03", ""),
        ]
    }

    fn noms(l: Vec<BadgeAdmin>) -> Vec<String> {
        l.into_iter().map(|b| b.name).collect()
    }

    #[test]
    fn les_badges_se_trient() {
        let l = liste();
        assert_eq!(
            noms(filtrer_badges(&l, "", TriBadges::Ordre)),
            ["Zèbre", "Étoile", "Alpha"]
        );
        assert_eq!(
            noms(filtrer_badges(&l, "", TriBadges::Nom)),
            ["Alpha", "Étoile", "Zèbre"]
        );
        assert_eq!(
            noms(filtrer_badges(&l, "", TriBadges::Detenteurs)),
            ["Alpha", "Étoile", "Zèbre"]
        );
        assert_eq!(
            noms(filtrer_badges(&l, "", TriBadges::Creation)),
            ["Zèbre", "Étoile", "Alpha"]
        );
    }

    #[test]
    fn les_badges_se_cherchent_sans_accent_ni_casse() {
        let l = liste();
        assert_eq!(
            noms(filtrer_badges(&l, "etoile", TriBadges::Nom)),
            ["Étoile"]
        );
        assert_eq!(
            noms(filtrer_badges(&l, "ARRIVE", TriBadges::Nom)),
            ["Alpha"]
        );
        assert!(filtrer_badges(&l, "zzz", TriBadges::Nom).is_empty());
    }

    #[test]
    fn l_icone_d_un_badge_est_verifiee() {
        for t in TYPES_ICONE {
            assert_eq!(probleme_d_icone(t, 1000), "");
        }
        assert!(probleme_d_icone("image/gif", 10).starts_with("Format"));
        assert!(probleme_d_icone("image/svg+xml", 70 << 10).starts_with("SVG trop lourd"));
        assert_eq!(probleme_d_icone("image/png", 6 << 20), "Image trop lourde.");
        assert_eq!(type_d_icone("a/B.JPG"), Some("image/jpeg"));
        assert_eq!(type_d_icone("x.svg"), Some("image/svg+xml"));
        assert_eq!(type_d_icone("x.gif"), None);
        assert_eq!(type_d_icone("sans"), None);
    }

    #[test]
    fn la_saisie_d_un_badge_est_bornee() {
        assert!(couleur_valide("#3b82f6"));
        assert!(!couleur_valide("3b82f6"));
        assert!(!couleur_valide("#3b82fg"));
        assert!(nom_de_badge_valide(" ab "));
        assert!(!nom_de_badge_valide("a"));
        assert!(!nom_de_badge_valide(&"x".repeat(33)));
        assert_eq!(ordre_saisi("12"), 12);
        assert_eq!(ordre_saisi("-3"), 0);
        assert_eq!(ordre_saisi("99999"), 9999);
        assert_eq!(ordre_saisi("abc"), 0);
    }

    #[test]
    fn les_detenteurs_se_comptent_et_se_choisissent() {
        assert_eq!(detenteurs(0), "Aucun détenteur");
        assert_eq!(detenteurs(1), "1 détenteur");
        assert_eq!(detenteurs(3), "3 détenteurs");
        assert_eq!(attribuer_a(1), "Attribuer à 1 compte");
        assert_eq!(attribuer_a(2), "Attribuer à 2 comptes");
        let b = badge("Alpha", 0, 1, "", "");
        let compte = |id: &str| CompteAdmin {
            id: id.into(),
            pseudo: id.into(),
            ..CompteAdmin::default()
        };
        let trouves = [compte("Alpha0"), compte("bob"), compte("carol")];
        let choix = choix_de_detenteurs(&trouves, &[compte("bob")], &b);
        assert_eq!(choix, [compte("carol")]);
        assert_eq!(
            message_de_suppression(&b),
            "Supprimer le badge « Alpha » ? Il sera retiré à 1 détenteur. Cette action est définitive."
        );
        assert_eq!(
            message_de_suppression(&badge("Z", 0, 0, "", "")),
            "Supprimer le badge « Z » ? Personne ne le détient. Cette action est définitive."
        );
    }

    #[test]
    fn les_nombres_et_tailles_se_lisent_en_francais() {
        assert_eq!(nombre(0), "0");
        assert_eq!(nombre(999), "999");
        assert_eq!(nombre(1234), "1\u{202f}234");
        assert_eq!(nombre(1234567), "1\u{202f}234\u{202f}567");
        assert_eq!(nombre(-1000), "-1\u{202f}000");
        assert_eq!(mega_octets(3 << 20), "3 Mo");
        assert_eq!(giga_octets(10 << 30), 10);
        assert_eq!(usage_stockage(5, 10), Some(50.0));
        assert_eq!(usage_stockage(50, 10), Some(100.0));
        assert_eq!(usage_stockage(5, 0), None);
    }

    #[test]
    fn la_configuration_se_resume() {
        let mut c = ConfigurationInstance {
            max_participants: 25,
            settings: ReglagesInstance {
                livekit_url: "wss://lk".into(),
                allow_video: true,
                ..ReglagesInstance::default()
            },
            ..ConfigurationInstance::default()
        };
        assert_eq!(
            detail_livekit(&c),
            "wss://lk · 25 participants max · caméra et écran autorisés"
        );
        c.screen_max_fps = Some(30);
        c.screen_max_height = Some(1080);
        c.settings.allow_video = false;
        assert_eq!(
            detail_livekit(&c),
            "wss://lk · 25 participants max · caméra et écran désactivés · écran 1080p, 30 i/s"
        );
        assert_eq!(configure(true), "configuré");
        assert_eq!(configure(false), "non configuré");
    }

    #[test]
    fn un_compte_se_resume() {
        let u = CompteAdmin {
            servers: 2,
            messages: 1500,
            created_at: "2026-03-04T10:00:00Z".into(),
            ..CompteAdmin::default()
        };
        assert_eq!(
            activite_du_compte(&u),
            "2 serveur(s) · 1\u{202f}500 message(s)"
        );
        assert_eq!(
            jour_d_inscription(&u.created_at, 0),
            "inscrit le 04/03/2026"
        );
        assert_eq!(destinataire_du_bot("  "), "tous les comptes");
        assert_eq!(destinataire_du_bot(" bob "), "bob");
    }
}
