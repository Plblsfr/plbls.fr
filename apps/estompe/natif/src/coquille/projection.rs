use chrono::{Datelike, NaiveDateTime, Timelike};
use estompe_client::format::formater_duree;
use estompe_client::roles::date_en_millisecondes;
use estompe_client::route::Route;
use estompe_client::sections::Section;
use estompe_client::store::messages::LignePrivee;
use estompe_client::store::{est_vocal, CleConversation, Etat, GenreSalon, Presence, Salon};
use estompe_client::temps_reel::EtatReseau;

use crate::{EtatBandeau, GenrePanneau, GenreVue};

pub const LIMITE_RATTRAPAGE: usize = 6;

#[derive(Debug, Clone, PartialEq)]
pub struct DonneesServeur {
    pub id: String,
    pub nom: String,
    pub initiales: String,
    pub version_icone: i64,
    pub actif: bool,
    pub non_lus: i64,
    pub mentions: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DonneesSalon {
    pub id: String,
    pub nom: String,
    pub icone: &'static str,
    pub titre_groupe: &'static str,
    pub genre_groupe: &'static str,
    pub actif: bool,
    pub non_lus: i64,
    pub mentions: usize,
    pub bientot: bool,
    pub occupants: usize,
    pub brouillon: bool,
    pub duree: String,
    pub vocal: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DonneesPrive {
    pub id: String,
    pub pseudo: String,
    pub initiales: String,
    pub teinte: i32,
    pub actif: bool,
    pub non_lus: i64,
    pub mentions: usize,
    pub presence: Presence,
    pub brouillon: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DonneesSection {
    pub section: Section,
    pub actif: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DonneesRattrapage {
    pub cle: String,
    pub titre: String,
    pub detail: String,
    pub icone: &'static str,
    pub nombre: i64,
    pub mention: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DonneesVocal {
    pub cle: String,
    pub nom: String,
    pub lieu: String,
    pub icone: &'static str,
    pub occupants: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DonneesContact {
    pub id: String,
    pub pseudo: String,
    pub initiales: String,
    pub teinte: i32,
    pub presence: Presence,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DonneesCompte {
    pub pseudo: String,
    pub presence: Presence,
    pub statut: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DonneesPanneau {
    pub genre: GenrePanneau,
    pub titre: String,
    pub initiales: String,
    pub serveur: Option<String>,
    pub version_icone: i64,
    pub peut_creer_salon: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DonneesVue {
    pub genre: GenreVue,
    pub titre: String,
    pub sujet: String,
    pub icone: &'static str,
    pub message: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DonneesAccueil {
    pub actif: bool,
    pub non_lus: i64,
    pub mentions: usize,
    pub date: String,
    pub salut: String,
    pub resume: String,
    pub sans_serveur: bool,
    pub contacts: String,
}

const JOURS: [&str; 7] = [
    "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
];
pub const MOIS: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];

fn id(valeur: &Option<String>) -> &str {
    valeur.as_deref().unwrap_or_default()
}

pub fn initiales(nom: &str) -> String {
    nom.chars().take(2).collect::<String>().to_uppercase()
}

pub fn teinte(identifiant: &str) -> i32 {
    let somme = identifiant.chars().fold(0i32, |h, c| {
        let premiere = c.encode_utf16(&mut [0u16; 2])[0];
        h.wrapping_mul(31).wrapping_add(i32::from(premiere))
    });
    (somme.unsigned_abs() % 6) as i32 + 1
}

fn pluriel(n: usize, singulier: &str, pluriel: &str) -> String {
    format!("{n} {}", if n > 1 { pluriel } else { singulier })
}

pub fn nom_de_section(section: Section) -> &'static str {
    match section {
        Section::Entracte => "Entracte · films et séries",
        Section::Sillon => "Sillon · musique",
        Section::Sablier => "Stats",
        Section::Regie => "Régie · administration des médias",
    }
}

pub fn icone_de_section(section: Section) -> &'static str {
    match section {
        Section::Entracte => "film",
        Section::Sillon => "note",
        Section::Sablier => "sablier",
        Section::Regie => "curseurs",
    }
}

pub fn identifiant_de_section(section: Section) -> &'static str {
    match section {
        Section::Entracte => "entracte",
        Section::Sillon => "sillon",
        Section::Sablier => "sablier",
        Section::Regie => "regie",
    }
}

pub fn section_de_l_identifiant(identifiant: &str) -> Option<Section> {
    Section::TOUTES
        .into_iter()
        .find(|s| identifiant_de_section(*s) == identifiant)
}

pub fn route_de_section(section: Section) -> Route {
    match section {
        Section::Entracte => Route::Films {
            section: None,
            id: None,
        },
        Section::Sillon => Route::Musique {
            section: None,
            id: None,
        },
        Section::Sablier => Route::Stats {
            section: None,
            id: None,
            detail: None,
        },
        Section::Regie => Route::Regie { section: None },
    }
}

pub fn section_de_la_route(route: &Route) -> Option<Section> {
    match route {
        Route::Films { .. } => Some(Section::Entracte),
        Route::Musique { .. } => Some(Section::Sillon),
        Route::Stats { .. } => Some(Section::Sablier),
        Route::Regie { .. } => Some(Section::Regie),
        _ => None,
    }
}

pub fn serveur_courant(route: &Route) -> Option<&str> {
    match route {
        Route::Salon { serveur, .. } => Some(serveur),
        _ => None,
    }
}

pub fn salon_affiche<'a>(etat: &'a Etat, serveur: &str, salon: Option<&str>) -> Option<&'a Salon> {
    let salons = etat.salons(serveur);
    salon
        .and_then(|s| salons.iter().find(|c| id(&c.id) == s))
        .or_else(|| salons.iter().find(|c| c.kind == Some(GenreSalon::Text)))
        .or_else(|| salons.first())
}

pub fn salon_de_la_route<'a>(etat: &'a Etat, route: &Route) -> Option<&'a Salon> {
    match route {
        Route::Salon { serveur, salon, .. } => salon_affiche(etat, serveur, salon.as_deref()),
        _ => None,
    }
}

pub fn conversation_de_la_route(etat: &Etat, route: &Route) -> Option<CleConversation> {
    match route {
        Route::MessagePrive { id, .. } => Some(CleConversation::Prive(id.clone())),
        Route::Salon { .. } => salon_de_la_route(etat, route)
            .filter(|s| s.kind == Some(GenreSalon::Text))
            .map(|s| CleConversation::Salon(id(&s.id).to_string())),
        _ => None,
    }
}

pub fn ligne_serveur(etat: &Etat, serveur: &str, route: &Route) -> Option<DonneesServeur> {
    let s = etat.serveur(serveur)?;
    let nom = id(&s.name).to_string();
    Some(DonneesServeur {
        id: serveur.to_string(),
        initiales: initiales(&nom),
        nom,
        version_icone: s.icon_version.unwrap_or(0),
        actif: serveur_courant(route) == Some(serveur),
        non_lus: etat.non_lus_du_serveur(serveur),
        mentions: etat.mentions_du_serveur(serveur),
    })
}

pub fn lignes_serveurs(etat: &Etat, route: &Route) -> Vec<DonneesServeur> {
    etat.ids_serveurs_ordonnes()
        .iter()
        .filter_map(|s| ligne_serveur(etat, s, route))
        .collect()
}

fn est_esquisse(genre: Option<GenreSalon>) -> bool {
    matches!(genre, Some(GenreSalon::Videos | GenreSalon::Music))
}

pub fn icone_du_salon(salon: &Salon) -> &'static str {
    match salon.kind {
        Some(GenreSalon::Text) if salon.locked == Some(true) => "lock",
        Some(GenreSalon::Videos) => "video",
        Some(GenreSalon::Music) => "headphones",
        Some(GenreSalon::Voice) => "volume",
        Some(GenreSalon::Cinema) => "film",
        _ => "hash",
    }
}

pub fn debut_du_salon_vocal(etat: &Etat, salon: &str) -> Option<i64> {
    etat.participants(salon)
        .iter()
        .filter_map(|p| p.depuis.as_deref().and_then(date_en_millisecondes))
        .min()
}

pub fn duree_du_salon_vocal(etat: &Etat, salon: &str, maintenant_ms: i64) -> String {
    debut_du_salon_vocal(etat, salon)
        .map(|debut| formater_duree((maintenant_ms - debut).max(0) as f64 / 1000.0))
        .unwrap_or_default()
}

fn ligne_de_salon(
    etat: &Etat,
    salon: &Salon,
    actif: Option<&str>,
    (titre, genre_groupe): (&'static str, &'static str),
    maintenant_ms: i64,
) -> DonneesSalon {
    let identifiant = id(&salon.id).to_string();
    let cle = CleConversation::Salon(identifiant.clone());
    let lisible = !est_vocal(salon.kind);
    DonneesSalon {
        nom: id(&salon.name).to_string(),
        icone: icone_du_salon(salon),
        titre_groupe: titre,
        genre_groupe,
        actif: actif == Some(identifiant.as_str()),
        non_lus: if lisible { etat.non_lus_de(&cle) } else { 0 },
        mentions: if lisible {
            etat.nombre_mentions(&cle)
        } else {
            0
        },
        bientot: est_esquisse(salon.kind),
        occupants: etat.participants(&identifiant).len(),
        brouillon: false,
        duree: if lisible {
            String::new()
        } else {
            duree_du_salon_vocal(etat, &identifiant, maintenant_ms)
        },
        vocal: !lisible,
        id: identifiant,
    }
}

type GardeDeGroupe = fn(Option<GenreSalon>) -> bool;

pub fn lignes_salons(etat: &Etat, route: &Route, maintenant_ms: i64) -> Vec<DonneesSalon> {
    let Some(serveur) = serveur_courant(route) else {
        return Vec::new();
    };
    let actif = salon_de_la_route(etat, route).map(|s| id(&s.id));
    let salons = etat.salons(serveur);
    let groupes: [(&'static str, &'static str, GardeDeGroupe); 3] = [
        ("SALONS TEXTE", "text", |g| g == Some(GenreSalon::Text)),
        ("ESQUISSES", "videos", est_esquisse),
        ("SALONS VOCAUX", "voice", est_vocal),
    ];
    groupes
        .iter()
        .flat_map(|(titre, genre, garde)| {
            salons
                .iter()
                .filter(|s| garde(s.kind))
                .enumerate()
                .map(move |(i, s)| {
                    ligne_de_salon(
                        etat,
                        s,
                        actif,
                        (if i == 0 { titre } else { "" }, genre),
                        maintenant_ms,
                    )
                })
        })
        .collect()
}

pub fn lignes_prives(privees: &[LignePrivee], route: &Route) -> Vec<DonneesPrive> {
    let courant = match route {
        Route::MessagePrive { id, .. } => Some(id.as_str()),
        _ => None,
    };
    privees
        .iter()
        .map(|l| DonneesPrive {
            actif: courant == Some(l.id.as_str()),
            non_lus: l.non_lus,
            mentions: l.mentions,
            presence: l.presence,
            teinte: teinte(if l.utilisateur.is_empty() {
                &l.pseudo
            } else {
                &l.utilisateur
            }),
            initiales: initiales(&l.pseudo),
            pseudo: l.pseudo.clone(),
            brouillon: false,
            id: l.id.clone(),
        })
        .collect()
}

pub fn marquer_brouillons(
    salons: &mut [DonneesSalon],
    prives: &mut [DonneesPrive],
    a_un_brouillon: impl Fn(&str) -> bool,
) {
    for s in salons {
        s.brouillon = a_un_brouillon(&format!("c:{}", s.id));
    }
    for p in prives {
        p.brouillon = a_un_brouillon(&format!("d:{}", p.id));
    }
}

pub fn lignes_sections(sections: &[Section], route: &Route) -> Vec<DonneesSection> {
    let courante = section_de_la_route(route);
    let mut triees = sections.to_vec();
    triees.sort();
    triees.dedup();
    triees
        .into_iter()
        .map(|section| DonneesSection {
            section,
            actif: courante == Some(section),
        })
        .collect()
}

fn lieu(etat: &Etat, cle: &CleConversation) -> Option<(String, bool)> {
    match cle {
        CleConversation::Prive(identifiant) => etat
            .messages_prives
            .iter()
            .find(|d| id(&d.id) == identifiant)
            .map(|d| {
                let pseudo = d.other.as_ref().map(|p| id(&p.pseudo)).unwrap_or_default();
                (pseudo.to_string(), true)
            }),
        CleConversation::Salon(salon) => etat.salon(salon).map(|(detail, c)| {
            let serveur = detail
                .server
                .as_ref()
                .map(|s| id(&s.name))
                .unwrap_or_default();
            (format!("{serveur} › # {}", id(&c.name)), false)
        }),
    }
}

pub fn lignes_rattrapage(etat: &Etat) -> Vec<DonneesRattrapage> {
    let mut mentions: Vec<_> = etat
        .mentions_comptees()
        .filter_map(|m| {
            let cle = CleConversation::de_mention(m)?;
            let (nom, prive) = lieu(etat, &cle)?;
            Some((m, cle, nom, prive))
        })
        .collect();
    mentions.sort_by(|a, b| id(&b.0.created_at).cmp(id(&a.0.created_at)));
    let avec_mention: Vec<CleConversation> = mentions.iter().map(|m| m.1.clone()).collect();
    let lignes_mentions = mentions
        .iter()
        .take(LIMITE_RATTRAPAGE)
        .map(|(m, cle, nom, prive)| {
            let auteur = m.author.as_ref().map(|p| id(&p.pseudo)).unwrap_or_default();
            let contenu = id(&m.content);
            DonneesRattrapage {
                cle: format!("{cle}#{}", id(&m.message_id)),
                titre: if *prive {
                    format!("{auteur} en message privé")
                } else {
                    format!("{auteur} dans {nom}")
                },
                detail: if contenu.is_empty() {
                    "Pièce jointe".to_string()
                } else {
                    contenu.to_string()
                },
                icone: "arobase",
                nombre: 0,
                mention: true,
            }
        });
    let non_lus = etat
        .non_lus
        .iter()
        .filter(|(cle, n)| {
            **n > 0 && !avec_mention.contains(cle) && etat.courante.as_ref() != Some(*cle)
        })
        .filter_map(|(cle, n)| {
            let (nom, prive) = lieu(etat, cle)?;
            Some(DonneesRattrapage {
                cle: cle.to_string(),
                titre: nom,
                detail: pluriel(*n as usize, "nouveau message", "nouveaux messages"),
                icone: if prive { "arobase" } else { "hash" },
                nombre: *n,
                mention: false,
            })
        })
        .take(LIMITE_RATTRAPAGE);
    lignes_mentions.chain(non_lus).collect()
}

pub fn lignes_vocaux(etat: &Etat) -> Vec<DonneesVocal> {
    etat.vocal
        .iter()
        .filter(|(_, participants)| !participants.is_empty())
        .filter_map(|(salon, participants)| {
            let (detail, c) = etat.salon(salon)?;
            let serveur = detail
                .server
                .as_ref()
                .map(|s| id(&s.name))
                .unwrap_or_default();
            Some(DonneesVocal {
                cle: salon.clone(),
                nom: id(&c.name).to_string(),
                lieu: format!("{serveur} · {} en vocal", participants.len()),
                icone: if c.kind == Some(GenreSalon::Cinema) {
                    "film"
                } else {
                    "vol"
                },
                occupants: participants.len(),
            })
        })
        .collect()
}

fn occupation(etat: &Etat, utilisateur: &str, presence: Presence) -> String {
    let vocal = etat
        .vocal
        .iter()
        .find(|(_, participants)| participants.iter().any(|p| p.utilisateur == utilisateur))
        .and_then(|(salon, _)| etat.salon(salon));
    if let Some((_, salon)) = vocal {
        return format!("En vocal · {}", id(&salon.name));
    }
    match etat.activite_de(utilisateur) {
        "" => super::membres::libelle_presence(presence).to_string(),
        activite => activite.to_string(),
    }
}

pub fn lignes_contacts(etat: &Etat) -> Vec<DonneesContact> {
    etat.messages_prives
        .iter()
        .filter_map(|d| {
            let autre = d.other.as_ref()?;
            let utilisateur = id(&autre.id);
            let presence = etat.presence_de(utilisateur);
            (presence != Presence::HorsLigne).then(|| {
                let pseudo = id(&autre.pseudo).to_string();
                DonneesContact {
                    id: id(&d.id).to_string(),
                    initiales: initiales(&pseudo),
                    teinte: teinte(if utilisateur.is_empty() {
                        &pseudo
                    } else {
                        utilisateur
                    }),
                    detail: occupation(etat, utilisateur, presence),
                    presence,
                    pseudo,
                }
            })
        })
        .collect()
}

pub fn donnees_compte(etat: &Etat) -> DonneesCompte {
    let presence = etat.presence_de(etat.moi());
    let statut = match etat.activite_de(etat.moi()) {
        "" => super::membres::libelle_presence(presence).to_string(),
        activite => activite.to_string(),
    };
    DonneesCompte {
        pseudo: etat.pseudo().to_string(),
        presence,
        statut,
    }
}

pub fn date_longue(maintenant: NaiveDateTime) -> String {
    format!(
        "{} {} {}",
        JOURS[maintenant.weekday().num_days_from_monday() as usize],
        maintenant.day(),
        MOIS[maintenant.month0() as usize]
    )
    .to_uppercase()
}

pub fn donnees_accueil(etat: &Etat, route: &Route, maintenant: NaiveDateTime) -> DonneesAccueil {
    let sans_serveur = etat.serveurs.is_empty();
    let heure = maintenant.hour();
    let salut = if sans_serveur {
        "Bienvenue"
    } else if !(5..18).contains(&heure) {
        "Bonsoir"
    } else {
        "Bonjour"
    };
    let rattrapage = lignes_rattrapage(etat);
    let mentions = rattrapage.iter().filter(|l| l.mention).count();
    let conversations = rattrapage.iter().filter(|l| !l.mention).count();
    let vocaux = lignes_vocaux(etat).len();
    let parties: Vec<String> = [
        (mentions > 0).then(|| pluriel(mentions, "mention", "mentions")),
        (conversations > 0).then(|| {
            pluriel(
                conversations,
                "conversation non lue",
                "conversations non lues",
            )
        }),
        (vocaux > 0).then(|| pluriel(vocaux, "salon vocal actif", "salons vocaux actifs")),
    ]
    .into_iter()
    .flatten()
    .collect();
    let resume = if sans_serveur {
        String::new()
    } else if parties.is_empty() {
        "Rien de neuf pour l'instant.".to_string()
    } else {
        format!("{}. Voici ce qui t'attend.", parties.join(", "))
    };
    let mentions_privees = etat
        .mentions
        .iter()
        .filter(|m| !id(&m.dm_id).is_empty())
        .count();
    let non_lus_prives = etat
        .messages_prives
        .iter()
        .map(|d| etat.non_lus_de(&CleConversation::Prive(id(&d.id).to_string())))
        .sum();
    DonneesAccueil {
        actif: matches!(route, Route::Accueil | Route::MessagePrive { .. }),
        non_lus: non_lus_prives,
        mentions: mentions_privees,
        date: date_longue(maintenant),
        salut: salut.to_string(),
        resume,
        sans_serveur,
        contacts: format!(
            "{} sur {}",
            lignes_contacts(etat).len(),
            etat.messages_prives.len()
        ),
    }
}

pub fn donnees_panneau(etat: &Etat, route: &Route) -> DonneesPanneau {
    if let Some(serveur) = serveur_courant(route) {
        let s = etat.serveur(serveur);
        let titre = s.map(|s| id(&s.name)).unwrap_or_default().to_string();
        return DonneesPanneau {
            genre: GenrePanneau::Serveur,
            initiales: initiales(&titre),
            titre,
            serveur: Some(serveur.to_string()),
            version_icone: s.and_then(|s| s.icon_version).unwrap_or(0),
            peut_creer_salon: estompe_client::creation::peut_creer_salon(etat, serveur),
        };
    }
    let (genre, titre) = match section_de_la_route(route) {
        Some(section) => (GenrePanneau::Section, nom_de_section(section).to_string()),
        None => (GenrePanneau::Accueil, "Messages privés".to_string()),
    };
    DonneesPanneau {
        genre,
        titre,
        initiales: String::new(),
        serveur: None,
        version_icone: 0,
        peut_creer_salon: false,
    }
}

const CONVERSATION: &str = "Ouverture de la conversation…";
const BIENTOT: &str = "Bientôt dans l'app native";

pub fn donnees_vue(etat: &Etat, route: &Route) -> DonneesVue {
    let vue =
        |genre, titre: String, sujet: String, icone, message: &str, detail: String| DonneesVue {
            genre,
            titre,
            sujet,
            icone,
            message: message.to_string(),
            detail,
        };
    match route {
        Route::Salon { serveur, .. } => match salon_de_la_route(etat, route) {
            Some(salon) => {
                let nom = id(&salon.name).to_string();
                let icone = icone_du_salon(salon);
                match salon.kind {
                    Some(GenreSalon::Voice | GenreSalon::Cinema) => vue(
                        GenreVue::Vocal,
                        nom,
                        String::new(),
                        icone,
                        "",
                        String::new(),
                    ),
                    Some(GenreSalon::Videos | GenreSalon::Music) => vue(
                        GenreVue::Esquisse,
                        nom,
                        String::new(),
                        icone,
                        BIENTOT,
                        "Les salons vidéos et musique arrivent avec le lot 9.".to_string(),
                    ),
                    _ => {
                        let sujet = [
                            salon.topic.clone().unwrap_or_default(),
                            salon
                                .slowmode
                                .filter(|s| *s > 0)
                                .map(|s| format!("mode lent {s} s"))
                                .unwrap_or_default(),
                        ]
                        .into_iter()
                        .filter(|p| !p.is_empty())
                        .collect::<Vec<_>>()
                        .join(" · ");
                        vue(
                            GenreVue::Salon,
                            nom,
                            sujet,
                            icone,
                            CONVERSATION,
                            String::new(),
                        )
                    }
                }
            }
            None => vue(
                GenreVue::Salon,
                etat.serveur(serveur)
                    .map(|s| id(&s.name))
                    .unwrap_or_default()
                    .to_string(),
                String::new(),
                "",
                "Aucun salon visible",
                "Ce serveur n'a encore aucun salon que tu peux voir.".to_string(),
            ),
        },
        Route::MessagePrive { id: prive, .. } => {
            let pseudo = lieu(etat, &CleConversation::Prive(prive.clone()))
                .map(|(nom, _)| nom)
                .unwrap_or_else(|| "…".to_string());
            vue(
                GenreVue::Prive,
                pseudo,
                String::new(),
                "arobase",
                CONVERSATION,
                String::new(),
            )
        }
        _ => match section_de_la_route(route) {
            Some(section) => vue(
                GenreVue::Section,
                nom_de_section(section).to_string(),
                String::new(),
                icone_de_section(section),
                BIENTOT,
                format!(
                    "{} arrive avec le lot 10. Elle reste ouverte dans l'app Estompe.",
                    match section {
                        Section::Entracte => "Entracte",
                        Section::Sillon => "Sillon",
                        Section::Sablier => "La section Stats",
                        Section::Regie => "Régie",
                    }
                ),
            ),
            None => vue(
                GenreVue::Accueil,
                "Accueil".to_string(),
                String::new(),
                "",
                "",
                String::new(),
            ),
        },
    }
}

pub fn bandeau(reseau: EtatReseau, deja_connecte: bool) -> EtatBandeau {
    match reseau {
        EtatReseau::Connecte => EtatBandeau::Aucun,
        EtatReseau::Reconnexion if deja_connecte => EtatBandeau::Reconnexion,
        EtatReseau::Reconnexion => EtatBandeau::Aucun,
        EtatReseau::HorsLigne => EtatBandeau::HorsLigne,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coquille::essais::etat;
    use chrono::NaiveDate;
    use estompe_client::store::messages::{lignes_privees, EtatMessages};

    fn salon(serveur: &str, salon: Option<&str>) -> Route {
        Route::Salon {
            serveur: serveur.into(),
            salon: salon.map(String::from),
            message: None,
        }
    }

    fn a(heure: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 19)
            .unwrap()
            .and_hms_opt(heure, 30, 0)
            .unwrap()
    }

    #[test]
    fn initiales_et_teinte_comme_le_front() {
        assert_eq!(initiales("Les amis"), "LE");
        assert_eq!(initiales("é"), "É");
        assert_eq!(initiales(""), "");
        for (id, attendue) in [("carol", 2), ("dave", 3), ("u1", 5), ("Élise", 1)] {
            assert_eq!(teinte(id), attendue, "{id}");
        }
    }

    #[test]
    fn le_rail_suit_l_ordre_choisi_avec_non_lus_et_mentions() {
        let mut e = etat();
        e.ordre_serveurs = vec!["s2".into()];
        let lignes = lignes_serveurs(&e, &salon("s1", None));
        let ids: Vec<&str> = lignes.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, ["s2", "s1"]);
        assert_eq!(
            lignes[1],
            DonneesServeur {
                id: "s1".into(),
                nom: "Les amis".into(),
                initiales: "LE".into(),
                version_icone: 0,
                actif: true,
                non_lus: 4,
                mentions: 1,
            }
        );
        assert_eq!((lignes[0].actif, lignes[0].version_icone), (false, 3));
        assert_eq!(ligne_serveur(&e, "zz", &Route::Accueil), None);
    }

    #[test]
    fn les_salons_sont_groupes_comme_dans_le_panneau_du_front() {
        let e = etat();
        let lignes = lignes_salons(&e, &salon("s1", None), 0);
        let resume: Vec<(&str, &str, &str, bool, bool)> = lignes
            .iter()
            .map(|l| (l.id.as_str(), l.icone, l.titre_groupe, l.actif, l.bientot))
            .collect();
        assert_eq!(
            resume,
            [
                ("c1", "hash", "SALONS TEXTE", true, false),
                ("c2", "lock", "", false, false),
                ("m1", "headphones", "ESQUISSES", false, true),
                ("v1", "volume", "SALONS VOCAUX", false, false),
                ("k1", "film", "", false, false),
            ]
        );
        assert_eq!((lignes[0].mentions, lignes[1].non_lus), (1, 3));
        assert_eq!((lignes[3].occupants, lignes[3].non_lus), (1, 0));
        let choisi = lignes_salons(&e, &salon("s1", Some("v1")), 0);
        assert!(choisi.iter().find(|l| l.id == "v1").unwrap().actif);
        assert!(lignes_salons(&e, &Route::Accueil, 0).is_empty());
    }

    #[test]
    fn la_duree_du_salon_vocal_part_du_plus_ancien_participant_comme_le_front() {
        let mut e = etat();
        let debut = date_en_millisecondes("2026-11-19T09:00:00Z").unwrap();
        assert!(lignes_salons(&e, &salon("s1", None), debut)
            .iter()
            .all(|l| l.duree.is_empty()));
        let liste = e.vocal.get_mut("v1").unwrap();
        liste[0].depuis = Some("2026-11-19T09:00:00Z".into());
        liste.push(estompe_client::store::ParticipantVocal {
            utilisateur: "dave".into(),
            pseudo: "dave".into(),
            depuis: Some("2026-11-19T09:30:00Z".into()),
            ..Default::default()
        });
        let lignes = lignes_salons(&e, &salon("s1", None), debut + 3_725_000);
        let duree = |id: &str| lignes.iter().find(|l| l.id == id).unwrap().duree.clone();
        assert_eq!(duree("v1"), formater_duree(3725.0));
        assert_eq!((duree("c1"), duree("k1")), (String::new(), String::new()));
    }

    #[test]
    fn le_salon_affiche_est_celui_de_la_route_sinon_le_premier_salon_texte() {
        let e = etat();
        let id = |route: &Route| salon_de_la_route(&e, route).and_then(|s| s.id.clone());
        assert_eq!(id(&salon("s1", Some("c2"))).as_deref(), Some("c2"));
        assert_eq!(id(&salon("s1", Some("disparu"))).as_deref(), Some("c1"));
        assert_eq!(id(&salon("s1", None)).as_deref(), Some("c1"));
        assert_eq!(id(&salon("inconnu", None)), None);
        assert_eq!(
            conversation_de_la_route(&e, &salon("s1", None)),
            Some(CleConversation::Salon("c1".into()))
        );
        assert_eq!(conversation_de_la_route(&e, &salon("s1", Some("v1"))), None);
        assert_eq!(
            conversation_de_la_route(
                &e,
                &Route::MessagePrive {
                    id: "d1".into(),
                    message: None
                }
            ),
            Some(CleConversation::Prive("d1".into()))
        );
        assert_eq!(conversation_de_la_route(&e, &Route::Accueil), None);
    }

    #[test]
    fn les_messages_prives_portent_presence_et_non_lus_par_activite() {
        let e = etat();
        let mut activite = EtatMessages::default();
        let lignes = lignes_prives(
            &lignes_privees(&e, &activite),
            &Route::MessagePrive {
                id: "d2".into(),
                message: None,
            },
        );
        assert_eq!(
            lignes[0],
            DonneesPrive {
                id: "d1".into(),
                pseudo: "carol".into(),
                initiales: "CA".into(),
                teinte: 2,
                actif: false,
                non_lus: 2,
                mentions: 0,
                presence: Presence::EnLigne,
                brouillon: false,
            }
        );
        assert!(lignes[1].actif && lignes[1].presence == Presence::HorsLigne);
        activite.remontees_privees = vec!["d2".into()];
        let triees = lignes_prives(&lignes_privees(&e, &activite), &Route::Accueil);
        let ordre: Vec<&str> = triees.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ordre, ["d2", "d1"]);
    }

    #[test]
    fn les_contacts_en_ligne_disent_ce_qu_ils_font_et_le_compte_son_statut() {
        let mut e = etat();
        let contacts = lignes_contacts(&e);
        let resume: Vec<(&str, Presence, &str)> = contacts
            .iter()
            .map(|c| (c.pseudo.as_str(), c.presence, c.detail.as_str()))
            .collect();
        assert_eq!(
            resume,
            [("carol", Presence::EnLigne, "En vocal · Salon vocal")]
        );
        assert_eq!(contacts[0].id, "d1");
        e.vocal.clear();
        e.en_ligne.insert("dave".into());
        e.statuts.insert(
            "dave".into(),
            estompe_client::temps_reel::evenements::StatutPresence::Dnd,
        );
        e.activites.insert("carol".into(), "Tetris".into());
        let details: Vec<String> = lignes_contacts(&e).into_iter().map(|c| c.detail).collect();
        assert_eq!(details, ["Tetris", "Ne pas déranger"]);
        assert_eq!(
            donnees_accueil(&e, &Route::Accueil, NaiveDateTime::default()).contacts,
            "2 sur 2"
        );
        assert_eq!(
            donnees_compte(&e),
            DonneesCompte {
                pseudo: "bob".into(),
                presence: Presence::EnLigne,
                statut: "En ligne".into()
            }
        );
        e.statuts.insert(
            "me".into(),
            estompe_client::temps_reel::evenements::StatutPresence::Idle,
        );
        assert_eq!(donnees_compte(&e).statut, "Absent");
        e.activite_locale = "Factorio".into();
        assert_eq!(donnees_compte(&e).presence, Presence::Absent);
        assert_eq!(donnees_compte(&e).statut, "Factorio");
    }

    #[test]
    fn a_rattraper_met_les_mentions_d_abord_sans_doubler_leur_conversation() {
        let mut e = etat();
        e.courante = Some(CleConversation::Salon("c2".into()));
        let lignes = lignes_rattrapage(&e);
        let resume: Vec<(&str, &str, i64, bool)> = lignes
            .iter()
            .map(|l| (l.cle.as_str(), l.titre.as_str(), l.nombre, l.mention))
            .collect();
        assert_eq!(
            resume,
            [
                ("c:c1#m9", "carol dans Les amis › # général", 0, true),
                ("d:d1", "carol", 2, false),
            ]
        );
        assert_eq!(lignes[0].detail, "@bob tu viens ?");
        assert_eq!(lignes[1].detail, "2 nouveaux messages");
    }

    #[test]
    fn l_accueil_salue_selon_l_heure_et_resume_ce_qui_attend() {
        let e = etat();
        let matin = donnees_accueil(&e, &Route::Accueil, a(9));
        assert_eq!(matin.salut, "Bonjour");
        assert_eq!(matin.date, "JEUDI 19 NOVEMBRE");
        assert_eq!(
            matin.resume,
            "1 mention, 2 conversations non lues, 1 salon vocal actif. Voici ce qui t'attend."
        );
        assert_eq!((matin.actif, matin.non_lus, matin.mentions), (true, 2, 0));
        assert_eq!(donnees_accueil(&e, &Route::Accueil, a(19)).salut, "Bonsoir");
        assert_eq!(donnees_accueil(&e, &Route::Accueil, a(3)).salut, "Bonsoir");
        let vide = donnees_accueil(&Etat::default(), &salon("s1", None), a(9));
        assert_eq!(
            (
                vide.salut.as_str(),
                vide.resume.as_str(),
                vide.sans_serveur,
                vide.actif
            ),
            ("Bienvenue", "", true, false)
        );
        let mut calme = etat();
        calme.non_lus.clear();
        calme.mentions.clear();
        calme.vocal.clear();
        assert_eq!(
            donnees_accueil(&calme, &Route::Accueil, a(9)).resume,
            "Rien de neuf pour l'instant."
        );
    }

    #[test]
    fn la_vue_decrit_le_salon_ou_annonce_le_lot_qui_livrera_le_contenu() {
        let e = etat();
        let texte = donnees_vue(&e, &salon("s1", None));
        assert_eq!(
            (
                texte.genre,
                texte.titre.as_str(),
                texte.sujet.as_str(),
                texte.message.as_str()
            ),
            (
                GenreVue::Salon,
                "général",
                "On parle de tout · mode lent 10 s",
                "Ouverture de la conversation…"
            )
        );
        let vocal = donnees_vue(&e, &salon("s1", Some("v1")));
        assert_eq!(
            (vocal.genre, vocal.titre.as_str(), vocal.message.as_str()),
            (GenreVue::Vocal, "Salon vocal", "")
        );
        assert_eq!(
            donnees_vue(&e, &salon("s1", Some("m1"))).genre,
            GenreVue::Esquisse
        );
        let prive = donnees_vue(
            &e,
            &Route::MessagePrive {
                id: "d1".into(),
                message: None,
            },
        );
        assert_eq!(
            (prive.genre, prive.titre.as_str()),
            (GenreVue::Prive, "carol")
        );
        let section = donnees_vue(&e, &route_de_section(Section::Sillon));
        assert_eq!(
            (section.genre, section.titre.as_str(), section.icone),
            (GenreVue::Section, "Sillon · musique", "note")
        );
        assert!(section.detail.contains("lot 10"));
        assert_eq!(donnees_vue(&e, &Route::Accueil).genre, GenreVue::Accueil);
    }

    #[test]
    fn le_panneau_suit_la_route() {
        let e = etat();
        let serveur = donnees_panneau(&e, &salon("s2", None));
        assert_eq!(
            (
                serveur.genre,
                serveur.titre.as_str(),
                serveur.initiales.as_str(),
                serveur.version_icone
            ),
            (GenrePanneau::Serveur, "Travail", "TR", 3)
        );
        assert_eq!(
            donnees_panneau(&e, &Route::Accueil).titre,
            "Messages privés"
        );
        assert_eq!(
            donnees_panneau(&e, &route_de_section(Section::Regie)).genre,
            GenrePanneau::Section
        );
    }

    #[test]
    fn les_sections_sont_ordonnees_et_la_courante_surlignee() {
        let lignes = lignes_sections(
            &[Section::Regie, Section::Entracte, Section::Regie],
            &route_de_section(Section::Regie),
        );
        assert_eq!(
            lignes,
            [
                DonneesSection {
                    section: Section::Entracte,
                    actif: false
                },
                DonneesSection {
                    section: Section::Regie,
                    actif: true
                },
            ]
        );
        for section in Section::TOUTES {
            assert_eq!(
                section_de_l_identifiant(identifiant_de_section(section)),
                Some(section)
            );
            assert_eq!(
                section_de_la_route(&route_de_section(section)),
                Some(section)
            );
        }
    }

    #[test]
    fn le_bandeau_n_apparait_qu_apres_une_premiere_connexion_ou_hors_ligne() {
        assert_eq!(bandeau(EtatReseau::Reconnexion, false), EtatBandeau::Aucun);
        assert_eq!(bandeau(EtatReseau::Connecte, true), EtatBandeau::Aucun);
        assert_eq!(
            bandeau(EtatReseau::Reconnexion, true),
            EtatBandeau::Reconnexion
        );
        assert_eq!(
            bandeau(EtatReseau::HorsLigne, false),
            EtatBandeau::HorsLigne
        );
    }
}
