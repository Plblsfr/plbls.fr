use estompe_client::format::{heure_de, jour_de};
use estompe_client::recherche::{
    aller_a, extrait, suggestions, ContexteRecherche, Destination, GenreDestination, Morceau,
    Portee, RechercheMessages, Tri, LARGEUR_EXTRAIT,
};
use estompe_client::route::Route;
use estompe_client::sections::Section;
use estompe_client::store::{CleConversation, Etat, GenreSalon};
use estompe_client::types::Message;

use super::projection::{icone_de_section, icone_du_salon, nom_de_section, route_de_section};

pub const HAUTEUR_ENTREE: f32 = 36.0;
pub const HAUTEUR_GROUPE: f32 = 26.0;
pub const MARGE_HAUTE: f32 = 8.0;
pub const FILTRES: &str = "FILTRES";
pub const ALLER_A: &str = "ALLER À";

fn id(valeur: &Option<String>) -> &str {
    valeur.as_deref().unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cible {
    Serveur(String),
    Route(Route),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Completer(String),
    Aller(Cible),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entree {
    pub titre_groupe: &'static str,
    pub icone: &'static str,
    pub libelle: String,
    pub detail: String,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PorteeProposee {
    pub portee: Portee,
    pub libelle: String,
    pub court: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub texte: String,
    pub trouve: bool,
    pub espace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resultat {
    pub titre_groupe: String,
    pub id: String,
    pub auteur: String,
    pub date: String,
    pub segments: Vec<Segment>,
    pub fichiers: String,
}

fn destination(
    genre: GenreDestination,
    libelle: &str,
    detail: &str,
    icone: &'static str,
    cible: Cible,
) -> Destination<Cible> {
    Destination {
        genre,
        libelle: libelle.to_string(),
        detail: detail.to_string(),
        icone,
        cible,
    }
}

pub fn destinations(etat: &Etat, sections: &[Section]) -> Vec<Destination<Cible>> {
    let serveurs = etat.serveurs_ordonnes();
    let mut sortie = vec![destination(
        GenreDestination::Accueil,
        "Accueil",
        "",
        "maison",
        Cible::Route(Route::Accueil),
    )];
    for s in &serveurs {
        sortie.push(destination(
            GenreDestination::Serveur,
            id(&s.name),
            "Serveur",
            "calque",
            Cible::Serveur(id(&s.id).to_string()),
        ));
    }
    for section in sections {
        sortie.push(destination(
            GenreDestination::Section,
            nom_de_section(*section),
            "",
            icone_de_section(*section),
            Cible::Route(route_de_section(*section)),
        ));
    }
    for s in &serveurs {
        for salon in etat.salons(id(&s.id)) {
            sortie.push(destination(
                GenreDestination::Salon,
                id(&salon.name),
                id(&s.name),
                icone_du_salon(salon),
                Cible::Route(Route::Salon {
                    serveur: id(&s.id).to_string(),
                    salon: Some(id(&salon.id).to_string()),
                    message: None,
                }),
            ));
        }
    }
    for d in &etat.messages_prives {
        let pseudo = d.other.as_ref().map_or("", |p| id(&p.pseudo));
        sortie.push(destination(
            GenreDestination::Prive,
            pseudo,
            "Message privé",
            "arobase",
            Cible::Route(Route::MessagePrive {
                id: id(&d.id).to_string(),
                message: None,
            }),
        ));
    }
    sortie
}

fn nom_du_prive<'a>(etat: &'a Etat, prive: &str) -> Option<&'a str> {
    etat.messages_prives
        .iter()
        .find(|d| id(&d.id) == prive)
        .and_then(|d| d.other.as_ref())
        .map(|p| id(&p.pseudo))
}

pub fn portees(
    etat: &Etat,
    serveur: Option<&str>,
    conversation: Option<&CleConversation>,
) -> Vec<PorteeProposee> {
    let mut sortie = vec![PorteeProposee {
        portee: Portee::Partout,
        libelle: "Partout".into(),
        court: "Partout".into(),
    }];
    if let Some(s) = serveur.and_then(|s| etat.serveur(s)) {
        let nom = id(&s.name);
        sortie.push(PorteeProposee {
            portee: Portee::Serveur(id(&s.id).to_string()),
            libelle: format!("Ce serveur ({nom})"),
            court: nom.to_string(),
        });
    }
    match conversation {
        Some(CleConversation::Salon(salon)) => {
            let nom = etat.salon(salon).map_or("", |(_, s)| id(&s.name));
            sortie.push(PorteeProposee {
                portee: Portee::Conversation(CleConversation::Salon(salon.clone())),
                libelle: format!("Ce salon (#{nom})"),
                court: format!("#{nom}"),
            });
        }
        Some(CleConversation::Prive(prive)) => {
            let nom = nom_du_prive(etat, prive).unwrap_or_default();
            sortie.push(PorteeProposee {
                portee: Portee::Conversation(CleConversation::Prive(prive.clone())),
                libelle: format!("Cette conversation ({nom})"),
                court: format!("@{nom}"),
            });
        }
        None => {}
    }
    sortie
}

fn sans_doublon(valeurs: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut sortie: Vec<String> = Vec::new();
    for v in valeurs {
        if !sortie.contains(&v) {
            sortie.push(v);
        }
    }
    sortie
}

pub fn contexte(
    etat: &Etat,
    portee: &Portee,
    serveur: Option<&str>,
    conversation: Option<&CleConversation>,
) -> ContexteRecherche {
    let serveurs = etat.ids_serveurs_ordonnes();
    let pseudos = match portee {
        Portee::Partout => sans_doublon(
            serveurs
                .iter()
                .flat_map(|s| etat.membres(s).iter().map(|m| id(&m.pseudo).to_string()))
                .chain(
                    etat.messages_prives
                        .iter()
                        .filter_map(|d| d.other.as_ref().map(|p| id(&p.pseudo).to_string())),
                ),
        ),
        _ => sans_doublon(etat.pseudos_de(conversation)),
    };
    let salons = sans_doublon(
        serveurs
            .iter()
            .filter(|s| *portee == Portee::Partout || Some(s.as_str()) == serveur)
            .flat_map(|s| etat.salons(s))
            .filter(|c| c.kind == Some(GenreSalon::Text))
            .map(|c| id(&c.name).to_string()),
    );
    ContexteRecherche {
        pseudos,
        salons,
        serveurs: etat
            .serveurs_ordonnes()
            .iter()
            .map(|s| id(&s.name).to_string())
            .collect(),
    }
}

pub fn entrees(
    saisie: &str,
    contexte: &ContexteRecherche,
    destinations: &[Destination<Cible>],
) -> Vec<Entree> {
    let filtres = suggestions(saisie, contexte)
        .into_iter()
        .enumerate()
        .map(|(i, s)| Entree {
            titre_groupe: if i == 0 { FILTRES } else { "" },
            icone: "filtre",
            libelle: s.libelle,
            detail: String::new(),
            action: Action::Completer(s.insertion),
        });
    let aller = aller_a(saisie, destinations)
        .into_iter()
        .enumerate()
        .map(|(i, d)| Entree {
            titre_groupe: if i == 0 { ALLER_A } else { "" },
            icone: d.icone,
            libelle: d.libelle,
            detail: d.detail,
            action: Action::Aller(d.cible),
        });
    filtres.chain(aller).collect()
}

pub fn voisine(actuelle: Option<usize>, pas: i32, nombre: usize) -> Option<usize> {
    if nombre == 0 {
        return None;
    }
    Some(match actuelle {
        Some(i) if pas > 0 => (i + 1) % nombre,
        Some(0) | None if pas < 0 => nombre - 1,
        Some(i) if pas < 0 => i - 1,
        _ => 0,
    })
}

pub fn haut_de_l_entree(entrees: &[Entree], index: usize) -> f32 {
    let groupes = entrees[..=index.min(entrees.len().saturating_sub(1))]
        .iter()
        .filter(|e| !e.titre_groupe.is_empty())
        .count();
    MARGE_HAUTE + groupes as f32 * HAUTEUR_GROUPE + index as f32 * (HAUTEUR_ENTREE + 2.0)
}

pub fn decalage_pour_voir(haut: f32, vue: f32, decalage: f32) -> f32 {
    if vue <= 0.0 {
        return decalage;
    }
    let bas = haut + HAUTEUR_ENTREE;
    if haut - HAUTEUR_GROUPE < decalage {
        (haut - HAUTEUR_GROUPE).max(0.0)
    } else if bas > decalage + vue {
        bas - vue
    } else {
        decalage
    }
}

pub fn segments(morceaux: &[Morceau]) -> Vec<Segment> {
    let mut sortie: Vec<Segment> = Vec::new();
    for m in morceaux {
        let mut mot = String::new();
        for c in m.texte.chars() {
            if c.is_whitespace() {
                if mot.is_empty() {
                    if let Some(dernier) = sortie.last_mut() {
                        dernier.espace = true;
                    }
                } else {
                    sortie.push(Segment {
                        texte: std::mem::take(&mut mot),
                        trouve: m.trouve,
                        espace: true,
                    });
                }
            } else {
                mot.push(c);
            }
        }
        if !mot.is_empty() {
            sortie.push(Segment {
                texte: mot,
                trouve: m.trouve,
                espace: false,
            });
        }
    }
    sortie
}

pub fn groupe_du_message(etat: &Etat, m: &Message) -> String {
    if let Some(prive) = m.dm_id.as_deref().filter(|d| !d.is_empty()) {
        return format!("@ {}", nom_du_prive(etat, prive).unwrap_or("Message privé"));
    }
    let salon = id(&m.channel_id);
    match (etat.salon(salon), etat.serveur_du_salon(salon)) {
        (Some((_, c)), Some(serveur)) => {
            let nom = etat.serveur(&serveur).map_or("", |s| id(&s.name));
            format!("{nom} › # {}", id(&c.name))
        }
        _ => "Salon".to_string(),
    }
}

pub fn cle_du_message(m: &Message) -> Option<CleConversation> {
    CleConversation::de(m.channel_id.as_deref(), m.dm_id.as_deref())
}

pub fn resultats(
    etat: &Etat,
    messages: &[Message],
    termes: &[String],
    decalage_minutes: i32,
) -> Vec<Resultat> {
    let mut groupes: Vec<(String, Vec<&Message>)> = Vec::new();
    for m in messages {
        let g = groupe_du_message(etat, m);
        match groupes.iter_mut().find(|(nom, _)| *nom == g) {
            Some((_, liste)) => liste.push(m),
            None => groupes.push((g, vec![m])),
        }
    }
    groupes
        .into_iter()
        .flat_map(|(nom, liste)| {
            liste.into_iter().enumerate().map(move |(i, m)| {
                let cree = id(&m.created_at);
                let date = match (
                    jour_de(cree, decalage_minutes),
                    heure_de(cree, decalage_minutes),
                ) {
                    (Some(jour), Some(heure)) => format!("{jour} {heure}"),
                    _ => String::new(),
                };
                let pieces = m.attachments.as_ref().map_or(0, Vec::len);
                Resultat {
                    titre_groupe: if i == 0 { nom.clone() } else { String::new() },
                    id: id(&m.id).to_string(),
                    auteur: m.author.as_ref().map_or("", |a| id(&a.pseudo)).to_string(),
                    date,
                    segments: segments(&extrait(id(&m.content), termes, LARGEUR_EXTRAIT)),
                    fichiers: match pieces {
                        0 => String::new(),
                        1 => "1 fichier".to_string(),
                        n => format!("{n} fichiers"),
                    },
                }
            })
        })
        .collect()
}

pub fn en_tete(recherche: &RechercheMessages) -> String {
    let n = recherche.resultats.len();
    match (n, recherche.encore) {
        (0, _) => String::new(),
        (_, true) => "MESSAGES".to_string(),
        (1, false) => "MESSAGES · 1 RÉSULTAT".to_string(),
        (n, false) => format!("MESSAGES · {n} RÉSULTATS"),
    }
}

pub fn vide(recherche: &RechercheMessages, portee: &Portee) -> (String, String) {
    if recherche.cherche.is_empty()
        || recherche.en_cours
        || !recherche.resultats.is_empty()
        || !recherche.erreur.is_empty()
    {
        return (String::new(), String::new());
    }
    let conseil = if *portee == Portee::Partout {
        String::new()
    } else {
        "Essaie d'élargir à « Partout ».".to_string()
    };
    (
        format!("Aucun résultat pour « {} ».", recherche.cherche),
        conseil,
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct EtatRecherche {
    pub serveur: Option<String>,
    pub conversation: Option<CleConversation>,
    pub portees: Vec<PorteeProposee>,
    pub portee: usize,
    pub tri: Tri,
    pub saisie: String,
    pub entrees: Vec<Entree>,
    pub active: Option<usize>,
    pub messages: RechercheMessages,
    pub recentes: Vec<String>,
    pub vue: f32,
    pub decalage: f32,
}

impl EtatRecherche {
    pub fn nouvelle(
        etat: &Etat,
        serveur: Option<String>,
        conversation: Option<CleConversation>,
        recentes: Vec<String>,
    ) -> Self {
        let portees = portees(etat, serveur.as_deref(), conversation.as_ref());
        let portee = if conversation.is_some() {
            portees.len() - 1
        } else {
            0
        };
        Self {
            serveur,
            conversation,
            portees,
            portee,
            tri: Tri::Pertinence,
            saisie: String::new(),
            entrees: Vec::new(),
            active: None,
            messages: RechercheMessages::default(),
            recentes,
            vue: 0.0,
            decalage: 0.0,
        }
    }

    pub fn portee(&self) -> Portee {
        self.portees
            .get(self.portee)
            .map_or(Portee::Partout, |p| p.portee.clone())
    }

    pub fn modifier(&mut self, etat: &Etat, sections: &[Section], saisie: &str) {
        self.saisie = saisie.to_string();
        let contexte = contexte(
            etat,
            &self.portee(),
            self.serveur.as_deref(),
            self.conversation.as_ref(),
        );
        self.entrees = entrees(saisie, &contexte, &destinations(etat, sections));
        self.active = None;
    }

    pub fn naviguer(&mut self, pas: i32) {
        self.active = voisine(self.active, pas, self.entrees.len());
        if let Some(i) = self.active {
            self.decalage =
                decalage_pour_voir(haut_de_l_entree(&self.entrees, i), self.vue, self.decalage);
        }
    }

    pub fn active_filtre(&self) -> bool {
        self.active
            .and_then(|i| self.entrees.get(i))
            .is_some_and(|e| matches!(e.action, Action::Completer(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coquille::essais::etat;

    fn libelles(e: &[Entree]) -> Vec<(&str, &str)> {
        e.iter()
            .map(|e| (e.titre_groupe, e.libelle.as_str()))
            .collect()
    }

    #[test]
    fn la_palette_va_aux_salons_aux_serveurs_aux_sections_et_aux_conversations() {
        let etat = etat();
        let d = destinations(&etat, &[Section::Entracte]);
        let ctx = contexte(&etat, &Portee::Partout, None, None);
        assert_eq!(
            libelles(&entrees("#", &ctx, &d)),
            [
                (ALLER_A, "Salon vocal"),
                ("", "Musique"),
                ("", "général"),
                ("", "annonces"),
                ("", "Ciné"),
                ("", "équipe"),
            ]
        );
        let tous = entrees("i", &ctx, &d);
        assert_eq!(tous[0].libelle, "Accueil");
        assert!(tous
            .iter()
            .any(|e| e.libelle == "Entracte · films et séries"));
        let equipe = entrees("equipe", &ctx, &d);
        assert_eq!(
            equipe[0].action,
            Action::Aller(Cible::Route(Route::Salon {
                serveur: "s2".into(),
                salon: Some("t1".into()),
                message: None
            }))
        );
        assert_eq!(equipe[0].detail, "Travail");
        assert_eq!(
            entrees("@car", &ctx, &d)[0].action,
            Action::Aller(Cible::Route(Route::MessagePrive {
                id: "d1".into(),
                message: None
            }))
        );
        assert_eq!(
            entrees("trav", &ctx, &d)[0].action,
            Action::Aller(Cible::Serveur("s2".into()))
        );
        assert_eq!(
            libelles(&entrees("dans:g", &ctx, &d)),
            [(FILTRES, "général")]
        );
        let melange = entrees("a", &ctx, &d);
        assert_eq!(melange[0].titre_groupe, FILTRES);
        assert!(melange.iter().any(|e| e.titre_groupe == ALLER_A));
    }

    #[test]
    fn les_portees_suivent_la_conversation_ouverte() {
        let etat = etat();
        let salon = CleConversation::Salon("c1".into());
        let p = portees(&etat, Some("s1"), Some(&salon));
        let courts: Vec<&str> = p.iter().map(|p| p.court.as_str()).collect();
        assert_eq!(courts, ["Partout", "Les amis", "#général"]);
        assert_eq!(p[2].libelle, "Ce salon (#général)");
        let prive = portees(&etat, None, Some(&CleConversation::Prive("d2".into())));
        assert_eq!(prive[1].libelle, "Cette conversation (dave)");
        let r = EtatRecherche::nouvelle(&etat, Some("s1".into()), Some(salon.clone()), Vec::new());
        assert_eq!(r.portee(), Portee::Conversation(salon));
        assert_eq!(
            EtatRecherche::nouvelle(&etat, None, None, Vec::new()).portee(),
            Portee::Partout
        );
        let ctx = contexte(&etat, &Portee::Serveur("s2".into()), Some("s2"), None);
        assert_eq!(ctx.salons, ["équipe"]);
        assert_eq!(ctx.serveurs, ["Les amis", "Travail"]);
    }

    #[test]
    fn les_fleches_bouclent_et_gardent_l_entree_active_visible() {
        assert_eq!(voisine(None, 1, 3), Some(0));
        assert_eq!(voisine(None, -1, 3), Some(2));
        assert_eq!(voisine(Some(2), 1, 3), Some(0));
        assert_eq!(voisine(Some(0), -1, 3), Some(2));
        assert_eq!(voisine(Some(1), -1, 3), Some(0));
        assert_eq!(voisine(Some(1), 1, 0), None);
        assert_eq!(decalage_pour_voir(500.0, 400.0, 0.0), 136.0);
        assert_eq!(decalage_pour_voir(100.0, 400.0, 200.0), 74.0);
        assert_eq!(decalage_pour_voir(100.0, 400.0, 0.0), 0.0);
        assert_eq!(decalage_pour_voir(500.0, 0.0, 0.0), 0.0);
    }

    fn message(id: &str, salon: Option<&str>, prive: Option<&str>, contenu: &str) -> Message {
        serde_json::from_value(serde_json::json!({
            "id": id, "channel_id": salon, "dm_id": prive, "content": contenu,
            "author": {"id": "carol", "pseudo": "carol"}, "created_at": "2026-11-19T08:05:00Z",
            "attachments": if id == "2" { serde_json::json!([{"id": "f1"}, {"id": "f2"}]) } else { serde_json::json!([]) }
        }))
        .unwrap()
    }

    #[test]
    fn les_resultats_sont_groupes_par_conversation_avec_l_extrait_surligne() {
        let etat = etat();
        let messages = [
            message("1", Some("c1"), None, "Une réunion  demain"),
            message("2", None, Some("d1"), "La réunion"),
            message("3", Some("c1"), None, "réunions"),
        ];
        let r = resultats(&etat, &messages, &["reunion".into()], 60);
        let groupes: Vec<(&str, &str)> = r
            .iter()
            .map(|r| (r.titre_groupe.as_str(), r.id.as_str()))
            .collect();
        assert_eq!(
            groupes,
            [("Les amis › # général", "1"), ("", "3"), ("@ carol", "2")]
        );
        assert_eq!(r[0].date, "jeudi 19 novembre 2026 09:05");
        assert_eq!(
            r[0].segments,
            [
                Segment {
                    texte: "Une".into(),
                    trouve: false,
                    espace: true
                },
                Segment {
                    texte: "réunion".into(),
                    trouve: true,
                    espace: true
                },
                Segment {
                    texte: "demain".into(),
                    trouve: false,
                    espace: false
                },
            ]
        );
        assert_eq!(r[1].segments.len(), 2, "« réunion » puis « s » collés");
        assert!(!r[1].segments[0].espace && r[1].segments[0].trouve);
        assert_eq!(r[2].fichiers, "2 fichiers");
        assert_eq!(
            cle_du_message(&messages[1]),
            Some(CleConversation::Prive("d1".into()))
        );
    }

    #[test]
    fn l_en_tete_et_l_etat_vide_suivent_la_recherche() {
        let mut r = RechercheMessages::default();
        assert_eq!(vide(&r, &Portee::Partout), (String::new(), String::new()));
        let d = r.lancer("zzz", Portee::Partout, Tri::Date).unwrap();
        r.recevoir(d.sequence, Ok(Default::default()));
        assert_eq!(
            vide(&r, &Portee::Serveur("s1".into())),
            (
                "Aucun résultat pour « zzz ».".to_string(),
                "Essaie d'élargir à « Partout ».".to_string()
            )
        );
        assert_eq!(vide(&r, &Portee::Partout).1, "");
        assert_eq!(en_tete(&r), "");
        r.resultats = vec![Message::default(); 3];
        assert_eq!(en_tete(&r), "MESSAGES · 3 RÉSULTATS");
        r.encore = true;
        assert_eq!(en_tete(&r), "MESSAGES");
    }
}
