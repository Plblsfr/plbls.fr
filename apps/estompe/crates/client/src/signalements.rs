use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::format::{date_heure_complete, date_locale};
use crate::http::encoder_composant;
use crate::journal::{formater, lire_entrees};
use crate::types::{
    PatchAdminReportsIdRequete, PatchAdminReportsIdRequeteStatus, PostReportsRequete,
    PostReportsRequeteCategory, PostReportsRequetePlatform,
};

pub type Choix = (&'static str, &'static str);

pub const CATEGORIES: [Choix; 6] = [
    ("", "Non précisée"),
    ("messages", "Messages"),
    ("vocal", "Vocal"),
    ("ecran", "Partage d'écran"),
    ("notifications", "Notifications"),
    ("autre", "Autre"),
];
pub const PLATEFORMES: [Choix; 4] = [
    ("web", "Web"),
    ("pwa", "PWA"),
    ("desktop", "Application PC"),
    ("android", "Android"),
];
pub const STATUTS: [Choix; 5] = [
    ("new", "Nouveau"),
    ("in_progress", "En cours"),
    ("resolved", "Résolu"),
    ("rejected", "Rejeté"),
    ("duplicate", "Doublon"),
];

pub const MAX_CAPTURES: usize = 5;
pub const TITRE_MAX: usize = 120;
pub const TEXTE_MAX: usize = 4000;
pub const LIGNES_DE_JOURNAL: usize = 300;
pub const PLATEFORME: &str = "desktop";
pub const CLE_PLATEFORME: &str = "Plateforme";
pub const FICHIER_BROUILLON: &str = "brouillon-signalement.json";
const EXTRAIT_MAX: usize = 200;

pub fn libelle(liste: &[Choix], id: &str) -> String {
    liste
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, l)| (*l).to_string())
        .unwrap_or_else(|| if id.is_empty() { "—" } else { id }.to_string())
}

pub fn statut(id: &str) -> String {
    STATUTS
        .iter()
        .find(|(i, _)| *i == id)
        .map_or_else(|| id.to_string(), |(_, l)| (*l).to_string())
}

pub fn ton(statut: &str) -> &'static str {
    match statut {
        "new" => "info",
        "in_progress" => "att",
        "resolved" => "ok",
        "rejected" => "dgr",
        _ => "",
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PieceSignalement {
    pub id: String,
    pub expired: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Signalement {
    pub id: String,
    pub created_at: String,
    pub title: String,
    pub category: String,
    pub platform: String,
    pub status: String,
    pub duplicate_of: Option<String>,
    pub author: String,
    pub seen: bool,
    pub description: String,
    pub tech: BTreeMap<String, String>,
    pub logs: String,
    pub notes: String,
    pub attachments: Vec<PieceSignalement>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Brouillon {
    pub title: String,
    pub description: String,
    pub category: String,
}

impl Brouillon {
    pub fn charger(dossier: &Path) -> Self {
        std::fs::read_to_string(dossier.join(FICHIER_BROUILLON))
            .ok()
            .and_then(|texte| serde_json::from_str(&texte).ok())
            .unwrap_or_default()
    }

    pub fn enregistrer(&self, dossier: &Path) {
        let chemin = dossier.join(FICHIER_BROUILLON);
        if self.title.is_empty() && self.description.is_empty() {
            let _ = std::fs::remove_file(chemin);
        } else if let Ok(texte) = serde_json::to_string(self) {
            let _ = std::fs::write(chemin, texte);
        }
    }

    pub fn envoyable(&self) -> bool {
        !self.title.trim().is_empty() && !self.description.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cible {
    Bug,
    Message {
        id: String,
        auteur: String,
        extrait: String,
    },
    Personne {
        id: String,
        pseudo: String,
    },
}

impl Cible {
    pub fn titre_de_la_fenetre(&self) -> &'static str {
        match self {
            Cible::Bug => "Signaler un bug",
            Cible::Message { .. } => "Signaler un message",
            Cible::Personne { .. } => "Signaler une personne",
        }
    }

    pub fn brouillon(&self) -> Option<Brouillon> {
        match self {
            Cible::Bug => None,
            Cible::Message { auteur, .. } => Some(Brouillon {
                title: format!("Message de {auteur}"),
                description: String::new(),
                category: "messages".into(),
            }),
            Cible::Personne { pseudo, .. } => Some(Brouillon {
                title: format!("Comportement de {pseudo}"),
                description: String::new(),
                category: "autre".into(),
            }),
        }
    }

    pub fn informations(&self) -> Vec<(String, String)> {
        match self {
            Cible::Bug => Vec::new(),
            Cible::Message {
                id,
                auteur,
                extrait,
            } => {
                let mut extrait: String = extrait.chars().take(EXTRAIT_MAX).collect();
                if extrait.is_empty() {
                    extrait = "—".into();
                }
                vec![
                    ("Message".into(), id.clone()),
                    ("Auteur".into(), auteur.clone()),
                    ("Extrait".into(), extrait),
                ]
            }
            Cible::Personne { id, pseudo } => {
                vec![("Personne".into(), format!("{pseudo} ({id})"))]
            }
        }
    }
}

pub struct Environnement<'a> {
    pub version: &'a str,
    pub systeme: &'a str,
    pub ecran: &'a str,
    pub vocal: &'a str,
}

pub fn informations_techniques(cible: &Cible, e: &Environnement) -> Vec<(String, String)> {
    let mut infos = cible.informations();
    infos.extend([
        (CLE_PLATEFORME.to_string(), PLATEFORME.to_string()),
        ("Version".into(), e.version.to_string()),
        ("Système".into(), e.systeme.to_string()),
        ("Application".into(), "native (Slint)".into()),
        ("Écran".into(), e.ecran.to_string()),
        ("Vocal".into(), e.vocal.to_string()),
    ]);
    infos
}

pub fn ecran_du_salon(serveur: &str, salon: &str) -> String {
    format!("serveur « {serveur} », salon « {salon} »")
}

pub fn vocal_actif(etat: &str, salon: &str) -> String {
    format!("{etat} dans « {salon} »")
}

pub fn journal_recent(texte: &str, decalage_minutes: i32) -> String {
    let entrees = lire_entrees(texte);
    let debut = entrees.len().saturating_sub(LIGNES_DE_JOURNAL);
    entrees[debut..]
        .iter()
        .map(|e| formater(e, decalage_minutes))
        .collect::<Vec<_>>()
        .join("\n")
}

fn categorie(id: &str) -> Option<PostReportsRequeteCategory> {
    serde_json::from_value(serde_json::Value::String(id.into()))
        .ok()
        .filter(|c| *c != PostReportsRequeteCategory::Inconnu)
}

pub fn requete(
    brouillon: &Brouillon,
    informations: &[(String, String, bool)],
    logs: Option<String>,
    fichiers: Vec<String>,
) -> PostReportsRequete {
    let tech: BTreeMap<String, String> = informations
        .iter()
        .filter(|(_, _, inclus)| *inclus)
        .map(|(cle, valeur, _)| (cle.clone(), valeur.clone()))
        .collect();
    let plateforme = tech
        .contains_key(CLE_PLATEFORME)
        .then_some(PostReportsRequetePlatform::Desktop);
    PostReportsRequete {
        title: brouillon.title.trim().to_string(),
        description: brouillon.description.trim().to_string(),
        category: categorie(&brouillon.category),
        platform: plateforme,
        tech: Some(tech),
        logs: Some(logs.unwrap_or_default()),
        file_ids: Some(fichiers),
    }
}

pub fn modification(
    statut: &str,
    origine: &str,
    notes: &str,
) -> Result<PatchAdminReportsIdRequete, &'static str> {
    let status: PatchAdminReportsIdRequeteStatus =
        serde_json::from_value(serde_json::Value::String(statut.into()))
            .ok()
            .filter(|s| *s != PatchAdminReportsIdRequeteStatus::Inconnu)
            .ok_or("Statut inconnu.")?;
    let duplicate_of = if status == PatchAdminReportsIdRequeteStatus::Duplicate {
        if origine.is_empty() {
            return Err("Choisis le signalement d'origine.");
        }
        Some(origine.to_string())
    } else {
        None
    };
    Ok(PatchAdminReportsIdRequete {
        status,
        duplicate_of,
        notes: Some(notes.chars().take(TEXTE_MAX).collect()),
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filtres {
    pub statut: String,
    pub categorie: String,
    pub plateforme: String,
    pub auteur: String,
    pub recherche: String,
}

impl Filtres {
    pub fn chemin(&self) -> String {
        let parametres: Vec<String> = [
            ("status", &self.statut),
            ("category", &self.categorie),
            ("platform", &self.plateforme),
            ("author", &self.auteur),
            ("q", &self.recherche),
        ]
        .into_iter()
        .filter(|(_, v)| !v.trim().is_empty())
        .map(|(cle, v)| format!("{cle}={}", encoder_composant(v.trim())))
        .collect();
        if parametres.is_empty() {
            "/admin/reports".into()
        } else {
            format!("/admin/reports?{}", parametres.join("&"))
        }
    }
}

pub fn date_courte(iso: &str, decalage_minutes: i32) -> String {
    date_locale(iso, decalage_minutes)
        .map(|d| format!("{:02}/{:02}/{}", d.jour, d.mois, d.annee))
        .unwrap_or_default()
}

pub fn resume_mes_signalements(s: &Signalement, decalage_minutes: i32) -> String {
    let date = date_courte(&s.created_at, decalage_minutes);
    if s.category.is_empty() {
        date
    } else {
        format!("{date} · {}", libelle(&CATEGORIES, &s.category))
    }
}

pub fn resume_admin(s: &Signalement, decalage_minutes: i32) -> String {
    format!(
        "{} · {} · {}",
        s.author,
        date_heure_complete(&s.created_at, decalage_minutes).unwrap_or_default(),
        libelle(&CATEGORIES, &s.category)
    )
}

pub fn formater_signalement(s: &Signalement, decalage_minutes: i32) -> String {
    let auteur = if s.author.is_empty() { "?" } else { &s.author };
    let tech = s
        .tech
        .iter()
        .map(|(k, v)| format!("- {k} : {v}"))
        .collect::<Vec<_>>()
        .join("\n");
    let entete = format!(
        "Signalé par {auteur} le {} · {} · {} · {}",
        date_heure_complete(&s.created_at, decalage_minutes).unwrap_or_default(),
        libelle(&CATEGORIES, &s.category),
        libelle(&PLATEFORMES, &s.platform),
        statut(&s.status)
    );
    [
        format!("# {}", s.title),
        entete,
        s.description.clone(),
        if tech.is_empty() {
            String::new()
        } else {
            format!("## Informations techniques\n{tech}")
        },
        if s.logs.is_empty() {
            String::new()
        } else {
            format!("## Logs\n{}", s.logs)
        },
    ]
    .into_iter()
    .filter(|p| !p.is_empty())
    .collect::<Vec<_>>()
    .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vide() -> Brouillon {
        Brouillon::default()
    }

    #[test]
    fn le_brouillon_se_conserve_puis_se_vide() {
        let dossier = tempfile::tempdir().unwrap();
        assert_eq!(Brouillon::charger(dossier.path()), vide());
        Brouillon {
            title: "Le vocal coupe".into(),
            description: "après 5 min".into(),
            category: "vocal".into(),
        }
        .enregistrer(dossier.path());
        assert_eq!(Brouillon::charger(dossier.path()).title, "Le vocal coupe");
        vide().enregistrer(dossier.path());
        assert_eq!(Brouillon::charger(dossier.path()).description, "");
        assert!(!dossier.path().join(FICHIER_BROUILLON).exists());
    }

    #[test]
    fn un_brouillon_illisible_repart_de_zero() {
        let dossier = tempfile::tempdir().unwrap();
        std::fs::write(dossier.path().join(FICHIER_BROUILLON), "pas du json").unwrap();
        assert_eq!(Brouillon::charger(dossier.path()), vide());
    }

    #[test]
    fn le_texte_copie_reprend_titre_informations_techniques_et_logs() {
        let s = Signalement {
            title: "Bug".into(),
            author: "alice".into(),
            created_at: "2026-10-26T10:00:00Z".into(),
            category: "vocal".into(),
            platform: "web".into(),
            status: "new".into(),
            description: "détail".into(),
            tech: BTreeMap::from([("Version".into(), "1.0".into())]),
            logs: "ligne".into(),
            ..Signalement::default()
        };
        let t = formater_signalement(&s, 60);
        assert!(t.contains("# Bug"), "{t}");
        assert!(t.contains("Vocal · Web · Nouveau"), "{t}");
        assert!(t.contains("- Version : 1.0"), "{t}");
        assert!(t.contains("## Logs\nligne"), "{t}");
        assert!(
            t.contains("Signalé par alice le 26/10/2026 11:00:00"),
            "{t}"
        );
    }

    #[test]
    fn le_texte_copie_omet_les_parties_vides() {
        let s = Signalement {
            title: "Bug".into(),
            status: "duplicate".into(),
            ..Signalement::default()
        };
        let t = formater_signalement(&s, 0);
        assert!(
            t.contains("Signalé par ? le  · Non précisée · — · Doublon"),
            "{t}"
        );
        assert!(!t.contains("##"), "{t}");
    }

    #[test]
    fn les_libelles_suivent_le_front() {
        assert_eq!(libelle(&CATEGORIES, "ecran"), "Partage d'écran");
        assert_eq!(libelle(&CATEGORIES, ""), "Non précisée");
        assert_eq!(libelle(&PLATEFORMES, ""), "—");
        assert_eq!(libelle(&PLATEFORMES, "linux"), "linux");
        assert_eq!(statut("in_progress"), "En cours");
        assert_eq!(ton("resolved"), "ok");
        assert_eq!(ton("duplicate"), "");
    }

    fn environnement() -> Environnement<'static> {
        Environnement {
            version: "0.4.0",
            systeme: "linux x86_64",
            ecran: "accueil",
            vocal: "inactif",
        }
    }

    #[test]
    fn la_requete_ne_joint_que_les_informations_cochees() {
        let infos: Vec<(String, String, bool)> =
            informations_techniques(&Cible::Bug, &environnement())
                .into_iter()
                .map(|(k, v)| {
                    let garde = k != "Vocal";
                    (k, v, garde)
                })
                .collect();
        let brouillon = Brouillon {
            title: "  Le vocal coupe ".into(),
            description: "après 5 min\n".into(),
            category: "vocal".into(),
        };
        let r = requete(&brouillon, &infos, None, vec!["f1".into()]);
        assert_eq!(r.title, "Le vocal coupe");
        assert_eq!(r.description, "après 5 min");
        assert_eq!(r.category, Some(PostReportsRequeteCategory::Vocal));
        assert_eq!(r.platform, Some(PostReportsRequetePlatform::Desktop));
        let tech = r.tech.unwrap();
        assert_eq!(tech.get("Version").map(String::as_str), Some("0.4.0"));
        assert!(!tech.contains_key("Vocal"));
        assert_eq!(r.logs.as_deref(), Some(""));
        assert_eq!(r.file_ids.unwrap(), ["f1"]);
        let sans_plateforme: Vec<_> = infos
            .iter()
            .map(|(k, v, _)| (k.clone(), v.clone(), k != CLE_PLATEFORME))
            .collect();
        let r = requete(
            &Brouillon::default(),
            &sans_plateforme,
            Some("l".into()),
            Vec::new(),
        );
        assert_eq!((r.platform, r.category), (None, None));
        assert_eq!(r.logs.as_deref(), Some("l"));
    }

    #[test]
    fn un_message_ou_une_personne_se_signalent_avec_leur_contexte() {
        let message = Cible::Message {
            id: "m1".into(),
            auteur: "bob".into(),
            extrait: "x".repeat(300),
        };
        assert_eq!(message.titre_de_la_fenetre(), "Signaler un message");
        let b = message.brouillon().unwrap();
        assert_eq!(
            (b.title.as_str(), b.category.as_str()),
            ("Message de bob", "messages")
        );
        let infos = informations_techniques(&message, &environnement());
        assert_eq!(infos[0], ("Message".into(), "m1".into()));
        assert_eq!(infos[2].1.chars().count(), 200);
        assert_eq!(infos[3].0, CLE_PLATEFORME);
        let personne = Cible::Personne {
            id: "u2".into(),
            pseudo: "alice".into(),
        };
        assert_eq!(personne.brouillon().unwrap().category, "autre");
        assert_eq!(
            personne.informations(),
            [("Personne".into(), "alice (u2)".into())]
        );
        assert_eq!(Cible::Bug.brouillon(), None);
        assert_eq!(Cible::Bug.titre_de_la_fenetre(), "Signaler un bug");
    }

    #[test]
    fn un_doublon_exige_son_signalement_d_origine() {
        assert_eq!(
            modification("duplicate", "", "n").unwrap_err(),
            "Choisis le signalement d'origine."
        );
        let m = modification("duplicate", "r2", "n").unwrap();
        assert_eq!(m.duplicate_of.as_deref(), Some("r2"));
        let m = modification("resolved", "r2", "note").unwrap();
        assert_eq!(
            (m.status, m.duplicate_of, m.notes.as_deref()),
            (
                PatchAdminReportsIdRequeteStatus::Resolved,
                None,
                Some("note")
            )
        );
        assert!(modification("perdu", "", "").is_err());
    }

    #[test]
    fn les_filtres_vides_ne_sont_pas_envoyes() {
        assert_eq!(Filtres::default().chemin(), "/admin/reports");
        let f = Filtres {
            statut: "new".into(),
            auteur: " ali ce ".into(),
            recherche: "écran".into(),
            ..Filtres::default()
        };
        assert_eq!(
            f.chemin(),
            "/admin/reports?status=new&author=ali%20ce&q=%C3%A9cran"
        );
    }

    #[test]
    fn mes_signalements_affichent_la_date_et_la_categorie() {
        let mut s = Signalement {
            created_at: "2026-10-26T23:30:00Z".into(),
            ..Signalement::default()
        };
        assert_eq!(resume_mes_signalements(&s, 60), "27/10/2026");
        s.category = "vocal".into();
        assert_eq!(resume_mes_signalements(&s, 0), "26/10/2026 · Vocal");
        s.author = "bob".into();
        assert_eq!(resume_admin(&s, 0), "bob · 26/10/2026 23:30:00 · Vocal");
    }

    #[test]
    fn le_journal_joint_garde_les_dernieres_lignes() {
        let texte: String = (0..310)
            .map(|i| format!("{{\"t\":{i}000,\"l\":\"info\",\"s\":\"t\",\"m\":\"ligne {i}\"}}\n"))
            .collect();
        let joint = journal_recent(&texte, 0);
        let lignes: Vec<&str> = joint.lines().collect();
        assert_eq!(lignes.len(), LIGNES_DE_JOURNAL);
        assert!(lignes[0].ends_with("t: ligne 10"), "{}", lignes[0]);
        assert!(lignes[299].ends_with("t: ligne 309"));
    }
}
