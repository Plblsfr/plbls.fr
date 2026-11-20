use crate::http::{encoder_composant, Client, ErreurApi};
use crate::store::CleConversation;
use crate::types::{Message, MessagePage};

pub const LONGUEUR_MAX: usize = 200;
pub const RECENTES_MAX: usize = 10;
pub const LARGEUR_EXTRAIT: usize = 180;
pub const MARGE_DE_SUITE: f32 = 200.0;
pub const IMPOSSIBLE: &str = "Recherche impossible";
pub const AIDE: &str = "Filtres : de:pseudo · dans:#salon · serveur:nom · mentionne:pseudo · avant:/après:/pendant:AAAA-MM-JJ · a:fichier|image|vidéo|lien|sondage · épinglé:oui";

const FILTRES: [(&str, &str); 9] = [
    ("de", "auteur"),
    ("dans", "salon"),
    ("serveur", "serveur"),
    ("mentionne", "personne mentionnée"),
    ("avant", "AAAA-MM-JJ"),
    ("après", "AAAA-MM-JJ"),
    ("pendant", "AAAA-MM-JJ"),
    ("a", "fichier, image, vidéo, lien, sondage"),
    ("épinglé", "oui"),
];
const POSSESSIONS: [&str; 5] = ["fichier", "image", "vidéo", "lien", "sondage"];

fn plier_caractere(c: char) -> char {
    match c.to_lowercase().next().unwrap_or(c) {
        'à' | 'â' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'î' | 'ï' => 'i',
        'ô' | 'ö' => 'o',
        'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        autre => autre,
    }
}

pub fn plier(texte: &str) -> String {
    texte.chars().map(plier_caractere).collect()
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContexteRecherche {
    pub pseudos: Vec<String>,
    pub salons: Vec<String>,
    pub serveurs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub libelle: String,
    pub insertion: String,
}

fn entre_guillemets(valeur: &str) -> String {
    if valeur.chars().any(char::is_whitespace) {
        format!("\"{valeur}\"")
    } else {
        valeur.to_string()
    }
}

pub fn suggestions(saisie: &str, contexte: &ContexteRecherche) -> Vec<Suggestion> {
    let jeton = saisie
        .split(char::is_whitespace)
        .next_back()
        .unwrap_or_default();
    if jeton.is_empty() || jeton.starts_with('"') || jeton.starts_with('-') {
        return Vec::new();
    }
    let tete = &saisie[..saisie.len() - jeton.len()];
    let Some(coupure) = jeton.find(':') else {
        let debut = plier(jeton);
        return FILTRES
            .iter()
            .filter(|(cle, _)| plier(cle).starts_with(&debut))
            .map(|(cle, aide)| Suggestion {
                libelle: format!("{cle}: {aide}"),
                insertion: format!("{tete}{cle}:"),
            })
            .collect();
    };
    let cle = plier(&jeton[..coupure]);
    let reste = &jeton[coupure + 1..];
    let valeur = plier(reste.strip_prefix(['"', '#', '@']).unwrap_or(reste));
    let oui = ["oui".to_string()];
    let possessions = POSSESSIONS.map(String::from);
    let reserve: &[String] = match cle.as_str() {
        "de" | "mentionne" => &contexte.pseudos,
        "dans" => &contexte.salons,
        "serveur" => &contexte.serveurs,
        "a" => &possessions,
        "epingle" => &oui,
        _ => &[],
    };
    let prefixe = &jeton[..=coupure];
    let diese = if cle == "dans" { "#" } else { "" };
    reserve
        .iter()
        .filter(|v| {
            let v = plier(v);
            v.starts_with(&valeur) && v != valeur
        })
        .take(8)
        .map(|v| Suggestion {
            libelle: v.clone(),
            insertion: format!("{tete}{prefixe}{diese}{} ", entre_guillemets(v)),
        })
        .collect()
}

fn jetons(saisie: &str) -> Vec<&str> {
    let mut sortie = Vec::new();
    let mut reste = saisie;
    loop {
        reste = reste.trim_start();
        if reste.is_empty() {
            return sortie;
        }
        let fin_mot = reste.find(char::is_whitespace).unwrap_or(reste.len());
        let fin = match reste.strip_prefix('"').and_then(|r| r.find('"')) {
            Some(p) => p + 2,
            None => fin_mot,
        };
        sortie.push(&reste[..fin]);
        reste = &reste[fin..];
    }
}

fn est_un_filtre(jeton: &str) -> bool {
    let Some(p) = jeton.find(':') else {
        return false;
    };
    p > 0
        && !jeton[..p].contains('"')
        && jeton.len() > p + 1
        && FILTRES
            .iter()
            .any(|(cle, _)| plier(cle) == plier(&jeton[..p]))
}

pub fn termes_a_surligner(saisie: &str) -> Vec<String> {
    jetons(saisie)
        .into_iter()
        .filter(|j| !j.starts_with('-') && !est_un_filtre(j))
        .flat_map(|j| {
            j.replace('"', "")
                .split(char::is_whitespace)
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .map(|t| {
            plier(&t)
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        })
        .filter(|t| t.chars().count() >= 2)
        .map(|t| {
            if t.chars().count() > 3 && (t.ends_with('s') || t.ends_with('x')) {
                t[..t.len() - 1].to_string()
            } else {
                t
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Morceau {
    pub texte: String,
    pub trouve: bool,
}

impl Morceau {
    fn nouveau(texte: &[char], trouve: bool) -> Self {
        Self {
            texte: texte.iter().collect(),
            trouve,
        }
    }
}

fn positions(dans: &[char], terme: &[char]) -> Vec<(usize, usize)> {
    let mut sortie = Vec::new();
    if terme.is_empty() || terme.len() > dans.len() {
        return sortie;
    }
    let mut i = 0;
    while i + terme.len() <= dans.len() {
        if dans[i..i + terme.len()] == *terme {
            sortie.push((i, i + terme.len()));
            i += terme.len();
        } else {
            i += 1;
        }
    }
    sortie
}

pub fn extrait(contenu: &str, termes: &[String], largeur: usize) -> Vec<Morceau> {
    let texte: Vec<char> = contenu.chars().collect();
    let plie: Vec<char> = texte.iter().map(|c| plier_caractere(*c)).collect();
    let mut trouves: Vec<(usize, usize)> = termes
        .iter()
        .flat_map(|t| positions(&plie, &t.chars().collect::<Vec<_>>()))
        .collect();
    trouves.sort_by_key(|t| t.0);
    let mut fusionnes: Vec<(usize, usize)> = Vec::new();
    for (a, b) in trouves {
        match fusionnes.last_mut() {
            Some(dernier) if a <= dernier.1 => dernier.1 = dernier.1.max(b),
            _ => fusionnes.push((a, b)),
        }
    }
    let debut = fusionnes
        .first()
        .map_or(0, |t| t.0)
        .saturating_sub(largeur / 4);
    let fin = texte.len().min(debut + largeur);
    let mut sortie = Vec::new();
    let mut position = debut;
    for (a, b) in fusionnes {
        if b <= debut || a >= fin {
            continue;
        }
        if a > position {
            sortie.push(Morceau::nouveau(&texte[position..a], false));
        }
        sortie.push(Morceau::nouveau(&texte[a.max(position)..b.min(fin)], true));
        position = b.min(fin);
    }
    if position < fin {
        sortie.push(Morceau::nouveau(&texte[position..fin], false));
    }
    if debut > 0 {
        sortie.insert(0, Morceau::nouveau(&['…'], false));
    }
    if fin < texte.len() {
        sortie.push(Morceau::nouveau(&['…'], false));
    }
    sortie
}

pub fn retenir(recentes: &[String], saisie: &str) -> Vec<String> {
    std::iter::once(saisie.to_string())
        .chain(recentes.iter().filter(|r| *r != saisie).cloned())
        .take(RECENTES_MAX)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenreDestination {
    Accueil,
    Serveur,
    Section,
    Salon,
    Prive,
    Action,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination<C> {
    pub genre: GenreDestination,
    pub libelle: String,
    pub detail: String,
    pub icone: &'static str,
    pub cible: C,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaisiePalette {
    pub prefixe: Option<char>,
    pub texte: String,
    pub libre: bool,
}

pub fn analyser_palette(saisie: &str) -> SaisiePalette {
    let debut = saisie.trim_start();
    let prefixe = debut.chars().next().filter(|c| ['>', '@', '#'].contains(c));
    let reste = prefixe.map_or(debut, |p| &debut[p.len_utf8()..]);
    let caracteres: Vec<char> = saisie.chars().collect();
    let filtre = caracteres.windows(2).enumerate().any(|(i, paire)| {
        !paire[0].is_whitespace()
            && paire[1] == ':'
            && caracteres.get(i + 2).is_none_or(|c| !c.is_whitespace())
    });
    SaisiePalette {
        prefixe,
        texte: plier(reste.trim()),
        libre: !filtre,
    }
}

pub fn aller_a<C: Clone>(saisie: &str, destinations: &[Destination<C>]) -> Vec<Destination<C>> {
    let s = analyser_palette(saisie);
    if !s.libre || s.prefixe == Some('>') || (s.texte.is_empty() && s.prefixe.is_none()) {
        return Vec::new();
    }
    let garder = |genre: GenreDestination| match s.prefixe {
        Some('#') => genre == GenreDestination::Salon,
        Some('@') => genre == GenreDestination::Prive,
        _ => true,
    };
    let limite = if s.texte.is_empty() { 50 } else { 8 };
    destinations
        .iter()
        .filter(|d| garder(d.genre))
        .filter(|d| plier(&d.libelle).contains(&s.texte) || plier(&d.detail).contains(&s.texte))
        .take(limite)
        .cloned()
        .collect()
}

pub fn actions<C: Clone>(saisie: &str, actions: &[Destination<C>]) -> Vec<Destination<C>> {
    let s = analyser_palette(saisie);
    if !s.libre || matches!(s.prefixe, Some('@' | '#')) {
        return Vec::new();
    }
    if s.texte.is_empty() {
        return if s.prefixe == Some('>') {
            actions.to_vec()
        } else {
            Vec::new()
        };
    }
    actions
        .iter()
        .filter(|d| plier(&d.libelle).contains(&s.texte) || plier(&d.detail).contains(&s.texte))
        .take(8)
        .cloned()
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Portee {
    Partout,
    Serveur(String),
    Conversation(CleConversation),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tri {
    Pertinence,
    Date,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemandeRecherche {
    pub sequence: u64,
    pub saisie: String,
    pub portee: Portee,
    pub tri: Tri,
    pub decalage: usize,
}

impl DemandeRecherche {
    pub fn chemin(&self) -> String {
        let tri = match self.tri {
            Tri::Pertinence => "relevance",
            Tri::Date => "date",
        };
        let portee = match &self.portee {
            Portee::Partout => String::new(),
            Portee::Serveur(id) => format!("&server_id={}", encoder_composant(id)),
            Portee::Conversation(CleConversation::Salon(id)) => {
                format!("&channel_id={}", encoder_composant(id))
            }
            Portee::Conversation(CleConversation::Prive(id)) => {
                format!("&dm_id={}", encoder_composant(id))
            }
        };
        format!(
            "/search?q={}&sort={tri}&offset={}{portee}",
            encoder_composant(&self.saisie),
            self.decalage
        )
    }
}

pub fn message_d_erreur(erreur: &ErreurApi) -> String {
    match erreur.message() {
        "" => format!("{IMPOSSIBLE}."),
        detail => format!("{IMPOSSIBLE} : {detail}"),
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RechercheMessages {
    pub resultats: Vec<Message>,
    pub encore: bool,
    pub en_cours: bool,
    pub cherche: String,
    pub erreur: String,
    demande: Option<DemandeRecherche>,
}

impl RechercheMessages {
    pub fn sequence(&self) -> u64 {
        self.demande.as_ref().map_or(0, |d| d.sequence)
    }

    pub fn lancer(&mut self, saisie: &str, portee: Portee, tri: Tri) -> Option<DemandeRecherche> {
        let saisie: String = saisie.trim().chars().take(LONGUEUR_MAX).collect();
        if saisie.is_empty() {
            return None;
        }
        Some(self.demander(saisie, portee, tri, 0))
    }

    fn demander(
        &mut self,
        saisie: String,
        portee: Portee,
        tri: Tri,
        decalage: usize,
    ) -> DemandeRecherche {
        self.en_cours = true;
        let demande = DemandeRecherche {
            sequence: self.sequence() + 1,
            saisie,
            portee,
            tri,
            decalage,
        };
        self.demande = Some(demande.clone());
        demande
    }

    pub fn relancer(&mut self, portee: Portee, tri: Tri) -> Option<DemandeRecherche> {
        let cherche = self.cherche.clone();
        self.lancer(&cherche, portee, tri)
    }

    pub fn suite(&mut self) -> Option<DemandeRecherche> {
        if !self.encore || self.en_cours {
            return None;
        }
        let d = self.demande.clone()?;
        Some(self.demander(d.saisie, d.portee, d.tri, self.resultats.len()))
    }

    pub fn recevoir(&mut self, sequence: u64, resultat: Result<MessagePage, ErreurApi>) -> bool {
        let Some(demande) = self.demande.clone().filter(|d| d.sequence == sequence) else {
            return false;
        };
        self.en_cours = false;
        match resultat {
            Ok(page) => {
                let messages = page.messages.unwrap_or_default();
                if demande.decalage == 0 {
                    self.resultats = messages;
                } else {
                    self.resultats.extend(messages);
                }
                self.encore = page.has_more.unwrap_or(false);
                self.cherche = demande.saisie;
                self.erreur.clear();
            }
            Err(erreur) => {
                self.erreur = message_d_erreur(&erreur);
                if demande.decalage == 0 {
                    self.resultats.clear();
                    self.encore = false;
                    self.cherche = demande.saisie;
                }
            }
        }
        true
    }
}

impl Client {
    pub async fn chercher_messages(
        &self,
        demande: &DemandeRecherche,
    ) -> Result<MessagePage, ErreurApi> {
        self.get(&demande.chemin()).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::serveur_factice::{Reponse, ServeurFactice};

    fn contexte() -> ContexteRecherche {
        ContexteRecherche {
            pseudos: vec!["Alice".into(), "Bob".into()],
            salons: vec!["général".into(), "jeux".into()],
            serveurs: vec!["Mon serveur".into()],
        }
    }

    fn s(libelle: &str, insertion: &str) -> Suggestion {
        Suggestion {
            libelle: libelle.into(),
            insertion: insertion.into(),
        }
    }

    #[test]
    fn propose_les_cles_de_filtre_sans_tenir_compte_des_accents() {
        assert_eq!(
            suggestions("bonjour ap", &contexte()),
            [s("après: AAAA-MM-JJ", "bonjour après:")]
        );
        let insertions: Vec<String> = suggestions("epi", &contexte())
            .into_iter()
            .map(|s| s.insertion)
            .collect();
        assert_eq!(insertions, ["épinglé:"]);
    }

    #[test]
    fn propose_les_valeurs_du_filtre_en_cours() {
        assert_eq!(
            suggestions("truc de:al", &contexte()),
            [s("Alice", "truc de:Alice ")]
        );
        assert_eq!(
            suggestions("dans:g", &contexte())[0].insertion,
            "dans:#général "
        );
        assert_eq!(
            suggestions("serveur:mon", &contexte())[0].insertion,
            "serveur:\"Mon serveur\" "
        );
        let libelles: Vec<String> = suggestions("a:", &contexte())
            .into_iter()
            .map(|s| s.libelle)
            .collect();
        assert_eq!(libelles, ["fichier", "image", "vidéo", "lien", "sondage"]);
    }

    #[test]
    fn ne_propose_rien_pour_une_valeur_complete_un_mot_exclu_ou_entre_guillemets() {
        assert!(suggestions("de:alice", &contexte()).is_empty());
        assert!(suggestions("-de", &contexte()).is_empty());
        assert!(suggestions("\"de", &contexte()).is_empty());
        assert!(suggestions("", &contexte()).is_empty());
        assert!(suggestions("de:Alice ", &contexte()).is_empty());
    }

    #[test]
    fn surligne_sans_filtres_exclusions_ni_guillemets_et_garde_les_racines() {
        assert_eq!(
            termes_a_surligner("\"Réunions\" -demain de:alice a:image salut"),
            ["reunion", "salut"]
        );
        assert_eq!(termes_a_surligner("\"deux mots\" x"), ["deu", "mot"]);
        assert_eq!(
            termes_a_surligner("de: inconnu:valeur"),
            ["de", "inconnuvaleur"]
        );
    }

    fn m(texte: &str, trouve: bool) -> Morceau {
        Morceau {
            texte: texte.into(),
            trouve,
        }
    }

    #[test]
    fn marque_les_termes_sans_tenir_compte_des_accents_ni_de_la_casse() {
        assert_eq!(
            extrait(
                "Une RÉUNION importante",
                &["reunion".into()],
                LARGEUR_EXTRAIT
            ),
            [
                m("Une ", false),
                m("RÉUNION", true),
                m(" importante", false)
            ]
        );
        assert_eq!(
            extrait("aa bb aa", &["aa".into(), "a b".into()], LARGEUR_EXTRAIT),
            [m("aa b", true), m("b ", false), m("aa", true)]
        );
    }

    #[test]
    fn centre_un_long_message_sur_le_premier_terme() {
        let contenu = format!("{} cible {}", "x".repeat(300), "y".repeat(300));
        let p = extrait(&contenu, &["cible".into()], 100);
        assert_eq!(p[0].texte, "…");
        assert_eq!(p.iter().find(|x| x.trouve).unwrap().texte, "cible");
        assert_eq!(p.last().unwrap().texte, "…");
        let total: usize = p.iter().map(|x| x.texte.chars().count()).sum();
        assert!(total < 110);
        assert_eq!(extrait("sans terme", &[], 100), [m("sans terme", false)]);
    }

    #[test]
    fn garde_les_recherches_les_plus_recentes_sans_doublon_et_dix_au_plus() {
        let mut l: Vec<String> = Vec::new();
        for i in 0..12 {
            l = retenir(&l, &format!("q{i}"));
        }
        l = retenir(&l, "q5");
        assert_eq!(l.len(), 10);
        assert_eq!(l[0], "q5");
        assert_eq!(l.iter().collect::<std::collections::HashSet<_>>().len(), 10);
    }

    fn d(genre: GenreDestination, libelle: &str, detail: &str) -> Destination<u8> {
        Destination {
            genre,
            libelle: libelle.into(),
            detail: detail.into(),
            icone: "hash",
            cible: 0,
        }
    }

    fn libelles(saisie: &str, toutes: &[Destination<u8>]) -> Vec<String> {
        aller_a(saisie, toutes)
            .into_iter()
            .map(|d| d.libelle)
            .collect()
    }

    #[test]
    fn la_palette_filtre_par_prefixe_et_par_texte_sans_accents() {
        let toutes = vec![
            d(GenreDestination::Accueil, "Accueil", ""),
            d(GenreDestination::Serveur, "Les amis", "Serveur"),
            d(GenreDestination::Salon, "général", "Les amis"),
            d(GenreDestination::Salon, "équipe", "Travail"),
            d(GenreDestination::Prive, "Élise", "Message privé"),
        ];
        assert!(libelles("", &toutes).is_empty());
        assert!(libelles(">", &toutes).is_empty());
        assert!(libelles("de:eli", &toutes).is_empty());
        assert_eq!(libelles("#", &toutes), ["général", "équipe"]);
        assert_eq!(libelles("@", &toutes), ["Élise"]);
        assert_eq!(libelles("@ eli", &toutes), ["Élise"]);
        assert_eq!(libelles("ami", &toutes), ["Les amis", "général"]);
        assert_eq!(libelles("#ami", &toutes), ["général"]);
        assert_eq!(libelles("EQUI", &toutes), ["équipe"]);
        let nombreuses: Vec<Destination<u8>> = (0..60)
            .map(|i| d(GenreDestination::Salon, &format!("salon {i}"), ""))
            .collect();
        assert_eq!(aller_a("salon", &nombreuses).len(), 8);
        assert_eq!(aller_a("#", &nombreuses).len(), 50);
        assert!(!analyser_palette("de:").libre);
        assert!(analyser_palette("heure : 10").libre);
    }

    #[test]
    fn les_actions_suivent_le_prefixe_et_le_texte() {
        let toutes = vec![
            d(GenreDestination::Action, "Nouveau message privé", ""),
            d(GenreDestination::Action, "Statut : Absent", ""),
            d(GenreDestination::Action, "Statut : En ligne", "actuel"),
        ];
        let libelles = |saisie: &str| -> Vec<String> {
            actions(saisie, &toutes)
                .into_iter()
                .map(|d| d.libelle)
                .collect()
        };
        assert_eq!(libelles(">").len(), 3);
        assert!(libelles("").is_empty());
        assert!(libelles("#statut").is_empty());
        assert!(libelles("@statut").is_empty());
        assert!(libelles("de:statut").is_empty());
        assert_eq!(
            libelles("> statut"),
            ["Statut : Absent", "Statut : En ligne"]
        );
        assert_eq!(libelles("prive"), ["Nouveau message privé"]);
        assert_eq!(libelles("actuel"), ["Statut : En ligne"]);
    }

    #[test]
    fn le_chemin_porte_la_portee_le_tri_et_le_decalage() {
        let mut r = RechercheMessages::default();
        assert!(r.lancer("  ", Portee::Partout, Tri::Pertinence).is_none());
        let demande = r
            .lancer("de:Bob \"un mot\"", Portee::Partout, Tri::Pertinence)
            .unwrap();
        assert_eq!(
            demande.chemin(),
            "/search?q=de%3ABob%20%22un%20mot%22&sort=relevance&offset=0"
        );
        let salon = DemandeRecherche {
            portee: Portee::Conversation(CleConversation::Salon("c1".into())),
            tri: Tri::Date,
            decalage: 25,
            ..demande.clone()
        };
        assert!(salon
            .chemin()
            .ends_with("&sort=date&offset=25&channel_id=c1"));
        let prive = DemandeRecherche {
            portee: Portee::Conversation(CleConversation::Prive("d1".into())),
            ..demande.clone()
        };
        assert!(prive.chemin().ends_with("&dm_id=d1"));
        let serveur = DemandeRecherche {
            portee: Portee::Serveur("s1".into()),
            ..demande
        };
        assert!(serveur.chemin().ends_with("&server_id=s1"));
    }

    fn page(ids: &[&str], encore: bool) -> MessagePage {
        MessagePage {
            messages: Some(
                ids.iter()
                    .map(|id| Message {
                        id: Some(id.to_string()),
                        ..Message::default()
                    })
                    .collect(),
            ),
            has_more: Some(encore),
        }
    }

    fn ids(r: &RechercheMessages) -> Vec<String> {
        r.resultats.iter().filter_map(|m| m.id.clone()).collect()
    }

    #[test]
    fn les_pages_s_ajoutent_et_une_reponse_perimee_est_ignoree() {
        let mut r = RechercheMessages::default();
        let premiere = r.lancer("chat", Portee::Partout, Tri::Pertinence).unwrap();
        let seconde = r.lancer("chien", Portee::Partout, Tri::Date).unwrap();
        assert!(!r.recevoir(premiere.sequence, Ok(page(&["1"], true))));
        assert!(r.suite().is_none(), "une recherche est déjà en cours");
        assert!(r.recevoir(seconde.sequence, Ok(page(&["2", "3"], true))));
        assert_eq!(r.cherche, "chien");
        let suite = r.suite().unwrap();
        assert_eq!((suite.decalage, suite.tri), (2, Tri::Date));
        assert!(r.recevoir(suite.sequence, Ok(page(&["4"], false))));
        assert_eq!(ids(&r), ["2", "3", "4"]);
        assert!(!r.encore && r.suite().is_none());
        let relance = r.relancer(Portee::Serveur("s1".into()), Tri::Date).unwrap();
        assert_eq!((relance.saisie.as_str(), relance.decalage), ("chien", 0));
    }

    #[tokio::test]
    async fn une_erreur_du_serveur_donne_un_message_et_vide_les_resultats() {
        let serveur = ServeurFactice::demarrer([
            Reponse::json(
                200,
                json!({"messages": [{"id": "5", "content": "salut"}], "has_more": false}),
            ),
            Reponse::json(
                400,
                json!({"error": "invalid_query", "message": "Saisie trop courte"}),
            ),
        ])
        .await;
        let client = serveur.client();
        let mut r = RechercheMessages::default();
        let d = r.lancer("salut", Portee::Partout, Tri::Pertinence).unwrap();
        let resultat = client.chercher_messages(&d).await;
        r.recevoir(d.sequence, resultat);
        assert_eq!(ids(&r), ["5"]);
        let d = r.lancer("s", Portee::Partout, Tri::Pertinence).unwrap();
        let resultat = client.chercher_messages(&d).await;
        r.recevoir(d.sequence, resultat);
        assert!(r.resultats.is_empty());
        assert_eq!(r.erreur, "Recherche impossible : Saisie trop courte");
        assert_eq!(r.cherche, "s");
        assert_eq!(
            serveur.requetes()[0].ligne(),
            ("/search?q=salut&sort=relevance&offset=0", "GET")
        );
    }
}
