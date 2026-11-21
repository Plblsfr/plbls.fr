use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::autocompletion::{Autocompletion, GenreProposition, Proposition};
use estompe_client::brouillons::Brouillons;
use estompe_client::format::taille_lisible;
use estompe_client::markdown::{analyser, texte_brut};
use estompe_client::mention::Mentionnables;
use estompe_client::pieces::{EtatPiece, ListePieces, Piece};
use estompe_client::store::messages::{compteur, id_message, Brouillon};
use estompe_client::store::{CleConversation, Store};
use estompe_client::televersement::SourceEnvoi;
use estompe_client::types::Message;
use estompe_client::video::FichierTemporaire;
use slint::{ComponentHandle, Model};

use crate::{
    BandeauSaisie, DonneesSaisie, GenreBandeau, PieceSaisie, Principale, PropositionSaisie,
};

pub type Vignette = slint::SharedPixelBuffer<slint::Rgba8Pixel>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePiece {
    pub nom: String,
    pub type_mime: String,
    pub source: SourceEnvoi,
    pub temporaire: Option<Arc<FichierTemporaire>>,
}

pub const DELAI_ENREGISTREMENT: Duration = Duration::from_millis(300);
pub const LONGUEUR_APERCU: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Reponse {
        id: String,
        auteur: String,
        texte: String,
    },
    Modification {
        id: String,
        texte: String,
        pieces: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Affichage {
    pub texte: String,
    pub indice: String,
    pub mode: Mode,
    pub compteur: String,
    pub alerte: bool,
    pub erreur: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Envoyer(CleConversation, Brouillon),
    Modifier {
        id: String,
        contenu: String,
        fichiers: Option<Vec<String>>,
    },
    Supprimer {
        cle: CleConversation,
        id: String,
    },
}

pub fn apercu(contenu: &str) -> String {
    let texte = texte_brut(&analyser(contenu, &Mentionnables::default()).blocs);
    let une_ligne = texte.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut apercu: String = une_ligne.chars().take(LONGUEUR_APERCU).collect();
    if une_ligne.chars().count() > LONGUEUR_APERCU {
        apercu.push('…');
    }
    apercu
}

pub fn indice_de(nom: &str, prive: bool) -> String {
    if prive {
        format!("Écrire à {nom}")
    } else if nom.is_empty() {
        "Écrire un message".to_string()
    } else {
        format!("Écrire dans #{nom}")
    }
}

#[derive(Debug, Default)]
pub struct EtatSaisie {
    pub cle: Option<CleConversation>,
    pub texte: String,
    pub mode: Mode,
    pub indice: String,
    pub erreur: String,
    pub curseur: usize,
    pub auto: Option<Autocompletion>,
    pub choisie: usize,
    pub pieces: HashMap<CleConversation, ListePieces>,
    pub edition: ListePieces,
    pub prochaine_piece: u64,
}

impl EtatSaisie {
    pub fn affichage(&self) -> Affichage {
        let (compteur, alerte) = match compteur(&self.texte) {
            Some((n, alerte)) => (
                format!("{n} / {}", estompe_client::store::messages::LONGUEUR_MAX),
                alerte,
            ),
            None => (String::new(), false),
        };
        Affichage {
            texte: self.texte.clone(),
            indice: self.indice.clone(),
            mode: self.mode.clone(),
            compteur,
            alerte,
            erreur: self.erreur.clone(),
        }
    }

    pub fn ouvrir(
        &mut self,
        brouillons: &mut Brouillons,
        cle: Option<CleConversation>,
        indice: String,
    ) {
        if self.cle == cle {
            self.indice = indice;
            return;
        }
        self.quitter(brouillons);
        self.texte = cle
            .as_ref()
            .map(|c| brouillons.texte(&c.to_string()).to_string())
            .unwrap_or_default();
        self.cle = cle;
        self.mode = Mode::Normal;
        self.edition = ListePieces::default();
        self.indice = indice;
        self.erreur.clear();
    }

    fn quitter(&mut self, brouillons: &mut Brouillons) {
        if let (Some(cle), Mode::Normal | Mode::Reponse { .. }) = (&self.cle, &self.mode) {
            brouillons.definir(&cle.to_string(), &self.texte);
        }
    }

    pub fn modifie(&mut self, brouillons: &mut Brouillons, texte: &str) -> bool {
        self.texte = texte.to_string();
        self.erreur.clear();
        match (&self.cle, &self.mode) {
            (Some(cle), Mode::Normal | Mode::Reponse { .. }) => {
                brouillons.definir(&cle.to_string(), texte)
            }
            _ => false,
        }
    }

    pub fn envoyer(&mut self, brouillons: &mut Brouillons, texte: &str) -> Option<Action> {
        let cle = self.cle.clone()?;
        self.texte = texte.to_string();
        let contenu = texte.trim().to_string();
        let modification = matches!(self.mode, Mode::Modification { .. });
        let liste = if modification {
            Some(&self.edition)
        } else {
            self.pieces.get(&cle)
        };
        let sans_piece = liste.is_none_or(ListePieces::est_vide);
        if contenu.is_empty() && sans_piece {
            return None;
        }
        if let Some(bloquant) = liste.and_then(ListePieces::bloquant) {
            self.erreur = bloquant.to_string();
            return None;
        }
        let action = match std::mem::take(&mut self.mode) {
            Mode::Modification {
                id,
                texte: avant,
                pieces: avant_pieces,
            } => {
                let pieces = std::mem::take(&mut self.edition).identifiants();
                self.texte = brouillons.texte(&cle.to_string()).to_string();
                let fichiers = (pieces != avant_pieces).then_some(pieces);
                if contenu == avant.trim() && fichiers.is_none() {
                    return None;
                }
                Action::Modifier {
                    id,
                    contenu,
                    fichiers,
                }
            }
            mode => {
                let reponse_a = match mode {
                    Mode::Reponse { id, .. } => Some(id),
                    _ => None,
                };
                self.texte.clear();
                brouillons.effacer(&cle.to_string());
                let pieces = self
                    .pieces
                    .remove(&cle)
                    .map(|l| l.pieces_jointes())
                    .unwrap_or_default();
                Action::Envoyer(
                    cle,
                    Brouillon {
                        contenu,
                        reponse_a,
                        pieces,
                        ..Brouillon::default()
                    },
                )
            }
        };
        self.erreur.clear();
        Some(action)
    }

    pub fn liste_courante(&mut self) -> Option<&mut ListePieces> {
        if matches!(self.mode, Mode::Modification { .. }) {
            return Some(&mut self.edition);
        }
        let cle = self.cle.clone()?;
        Some(self.pieces.entry(cle).or_default())
    }

    pub fn pieces_courantes(&self) -> Option<&ListePieces> {
        if matches!(self.mode, Mode::Modification { .. }) {
            return Some(&self.edition);
        }
        self.pieces.get(self.cle.as_ref()?)
    }

    pub fn liste_de(&mut self, piece: u64) -> Option<&mut ListePieces> {
        std::iter::once(&mut self.edition)
            .chain(self.pieces.values_mut())
            .find(|l| l.pieces.iter().any(|p| p.cle == piece))
    }

    pub fn cles_des_pieces(&self) -> HashSet<u64> {
        std::iter::once(&self.edition)
            .chain(self.pieces.values())
            .flat_map(|l| l.pieces.iter().map(|p| p.cle))
            .collect()
    }

    pub fn nouvelle_piece(&mut self) -> u64 {
        self.prochaine_piece += 1;
        self.prochaine_piece
    }

    pub fn repondre(&mut self, message: &Message) {
        let auteur = message
            .author
            .as_ref()
            .and_then(|p| p.pseudo.clone())
            .unwrap_or_default();
        if matches!(self.mode, Mode::Modification { .. }) {
            self.texte.clear();
            self.edition = ListePieces::default();
        }
        self.mode = Mode::Reponse {
            id: id_message(message).to_string(),
            auteur,
            texte: apercu(message.content.as_deref().unwrap_or_default()),
        };
    }

    pub fn modifier(&mut self, brouillons: &mut Brouillons, message: &Message) {
        self.quitter(brouillons);
        let texte = message.content.clone().unwrap_or_default();
        let pieces = message.attachments.clone().unwrap_or_default();
        let mut prochaine = self.prochaine_piece;
        self.edition = ListePieces::depuis(&pieces, || {
            prochaine += 1;
            prochaine
        });
        self.prochaine_piece = prochaine;
        self.mode = Mode::Modification {
            id: id_message(message).to_string(),
            texte: texte.clone(),
            pieces: self.edition.identifiants(),
        };
        self.texte = texte;
        self.erreur.clear();
    }

    pub fn proposer(&mut self, auto: Option<Autocompletion>) -> bool {
        let change = self.auto != auto;
        if change {
            self.choisie = 0;
        }
        self.auto = auto;
        change
    }

    pub fn naviguer(&mut self, pas: i32) {
        let n = self.auto.as_ref().map_or(0, |a| a.propositions.len()) as i32;
        if n > 0 {
            self.choisie = (self.choisie as i32 + pas).rem_euclid(n) as usize;
        }
    }

    pub fn choisir(&mut self, index: usize) -> Option<(String, usize)> {
        let auto = self.auto.as_ref()?;
        auto.appliquer(&self.texte, self.curseur.min(self.texte.len()), index)
    }

    pub fn inserer(&mut self, insere: &str, ancre: usize) -> Option<(String, usize)> {
        let (a, b) = (ancre.min(self.curseur), ancre.max(self.curseur));
        estompe_client::mention::remplacer(&self.texte, a, b.min(self.texte.len()), insere)
    }

    pub fn annuler(&mut self, brouillons: &Brouillons) -> bool {
        match std::mem::take(&mut self.mode) {
            Mode::Normal => false,
            Mode::Reponse { .. } => true,
            Mode::Modification { .. } => {
                self.edition = ListePieces::default();
                self.texte = self
                    .cle
                    .as_ref()
                    .map(|c| brouillons.texte(&c.to_string()).to_string())
                    .unwrap_or_default();
                true
            }
        }
    }
}

#[derive(Clone)]
pub struct Saisie {
    etat: Arc<Mutex<EtatSaisie>>,
    brouillons: Arc<Mutex<Brouillons>>,
    enregistrement: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    televersements: Arc<Mutex<HashMap<u64, tokio::task::JoinHandle<()>>>>,
    vignettes: Arc<Mutex<HashMap<u64, Vignette>>>,
    sources: Arc<Mutex<HashMap<u64, SourcePiece>>>,
}

impl Default for Saisie {
    fn default() -> Self {
        Self::nouvelle(Brouillons::en_memoire())
    }
}

fn genre_slint(genre: GenreProposition) -> crate::GenreProposition {
    match genre {
        GenreProposition::Personne => crate::GenreProposition::Personne,
        GenreProposition::Groupe => crate::GenreProposition::Groupe,
        GenreProposition::Role => crate::GenreProposition::Role,
        GenreProposition::Salon => crate::GenreProposition::Salon,
        GenreProposition::Emoji | GenreProposition::EmojiServeur => crate::GenreProposition::Emoji,
    }
}

pub fn detail_de_piece(piece: &Piece) -> String {
    match &piece.etat {
        EtatPiece::Preparation => "préparation…".to_string(),
        EtatPiece::Conversion { pourcentage } => format!("conversion… {pourcentage} %"),
        EtatPiece::Envoi { envoyes, total } => {
            format!(
                "envoi… {} %",
                (envoyes * 100).checked_div(*total).unwrap_or(0)
            )
        }
        EtatPiece::Envoyee(_) => taille_lisible(piece.taille),
        EtatPiece::Echec(raison) => format!("échec : {raison}"),
    }
}

fn piece_slint(piece: &Piece, vignette: Option<&Vignette>) -> PieceSaisie {
    let (progression, en_cours) = match &piece.etat {
        EtatPiece::Preparation => (0.0, true),
        EtatPiece::Conversion { pourcentage } => (f32::from(*pourcentage) / 100.0, true),
        EtatPiece::Envoi { envoyes, total } => (
            if *total == 0 {
                1.0
            } else {
                *envoyes as f32 / *total as f32
            },
            true,
        ),
        _ => (1.0, false),
    };
    PieceSaisie {
        cle: piece.cle.min(i32::MAX as u64) as i32,
        nom: piece.nom.as_str().into(),
        detail: detail_de_piece(piece).into(),
        vignette: vignette
            .map(|v| slint::Image::from_rgba8(v.clone()))
            .unwrap_or_default(),
        a_vignette: vignette.is_some(),
        progression,
        en_cours,
        echec: matches!(piece.etat, EtatPiece::Echec(_)),
    }
}

fn bandeau_slint(mode: &Mode) -> BandeauSaisie {
    match mode {
        Mode::Normal => BandeauSaisie::default(),
        Mode::Reponse { auteur, texte, .. } => BandeauSaisie {
            genre: GenreBandeau::Reponse,
            auteur: auteur.as_str().into(),
            texte: texte.as_str().into(),
        },
        Mode::Modification { texte, .. } => BandeauSaisie {
            genre: GenreBandeau::Modification,
            auteur: Default::default(),
            texte: apercu(texte).into(),
        },
    }
}

impl Saisie {
    pub fn nouvelle(brouillons: Brouillons) -> Self {
        Self {
            etat: Arc::new(Mutex::new(EtatSaisie::default())),
            brouillons: Arc::new(Mutex::new(brouillons)),
            enregistrement: Arc::new(Mutex::new(None)),
            televersements: Arc::new(Mutex::new(HashMap::new())),
            vignettes: Arc::new(Mutex::new(HashMap::new())),
            sources: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn avec_etat<R>(&self, f: impl FnOnce(&mut EtatSaisie) -> R) -> R {
        f(&mut self.etat())
    }

    pub fn noter_televersement(&self, piece: u64, tache: tokio::task::JoinHandle<()>) {
        let ancienne = self
            .televersements
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(piece, tache);
        if let Some(ancienne) = ancienne {
            ancienne.abort();
        }
    }

    pub fn televersements_en_cours(&self) -> usize {
        self.televersements
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|t| !t.is_finished())
            .count()
    }

    pub fn definir_source(&self, piece: u64, source: SourcePiece) {
        self.sources
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(piece, source);
    }

    pub fn source(&self, piece: u64) -> Option<SourcePiece> {
        self.sources
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&piece)
            .cloned()
    }

    pub fn definir_vignette(&self, piece: u64, vignette: Vignette) {
        self.vignettes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(piece, vignette);
    }

    pub fn nettoyer_pieces(&self) {
        let presentes = self.etat().cles_des_pieces();
        self.televersements
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|cle, tache| {
                let garder = presentes.contains(cle);
                if !garder {
                    tache.abort();
                }
                garder && !tache.is_finished()
            });
        self.vignettes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|cle, _| presentes.contains(cle));
        self.sources
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|cle, _| presentes.contains(cle));
    }

    pub fn afficher_pieces(&self, f: &Principale) {
        self.nettoyer_pieces();
        let lignes: Vec<PieceSaisie> = {
            let etat = self.etat();
            let vignettes = self.vignettes.lock().unwrap_or_else(|e| e.into_inner());
            etat.pieces_courantes()
                .map(|l| {
                    l.pieces
                        .iter()
                        .map(|p| piece_slint(p, vignettes.get(&p.cle)))
                        .collect()
                })
                .unwrap_or_default()
        };
        let g = f.global::<DonneesSaisie>();
        let modele = g.get_pieces();
        let Some(vec) = modele
            .as_any()
            .downcast_ref::<slint::VecModel<PieceSaisie>>()
        else {
            g.set_pieces(slint::ModelRc::new(slint::VecModel::from(lignes)));
            return;
        };
        let memes =
            vec.row_count() == lignes.len() && vec.iter().zip(&lignes).all(|(a, b)| a.cle == b.cle);
        if !memes {
            vec.set_vec(lignes);
            return;
        }
        for (i, ligne) in lignes.into_iter().enumerate() {
            if vec.row_data(i).as_ref() != Some(&ligne) {
                vec.set_row_data(i, ligne);
            }
        }
    }

    pub fn definir_brouillons(&self, brouillons: Brouillons) {
        *self.brouillons() = brouillons;
    }

    fn etat(&self) -> MutexGuard<'_, EtatSaisie> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn brouillons(&self) -> MutexGuard<'_, Brouillons> {
        self.brouillons.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn conversations_en_brouillon(&self) -> HashSet<String> {
        self.brouillons().cles().map(str::to_string).collect()
    }

    pub fn affichage(&self) -> Affichage {
        self.etat().affichage()
    }

    pub fn mode(&self) -> Mode {
        self.etat().mode.clone()
    }

    pub fn cle(&self) -> Option<CleConversation> {
        self.etat().cle.clone()
    }

    pub fn afficher(&self, f: &Principale) {
        let a = self.affichage();
        let g = f.global::<DonneesSaisie>();
        if g.get_texte() != a.texte.as_str() {
            g.set_texte(a.texte.as_str().into());
        }
        g.set_indice(a.indice.as_str().into());
        g.set_bandeau(bandeau_slint(&a.mode));
        g.set_compteur(a.compteur.as_str().into());
        g.set_compteur_alerte(a.alerte);
        g.set_erreur(a.erreur.as_str().into());
        self.afficher_pieces(f);
    }

    pub fn demander_focus(&self, f: &Principale) {
        let g = f.global::<DonneesSaisie>();
        g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
    }

    pub fn ouvrir(&self, f: &Principale, cle: Option<CleConversation>, indice: String) -> bool {
        let (change, autre) = {
            let mut brouillons = self.brouillons();
            let mut etat = self.etat();
            let avant = brouillons.cles().count();
            let ancien = etat.cle.clone();
            etat.ouvrir(&mut brouillons, cle.clone(), indice);
            let autre = ancien != cle;
            if autre {
                etat.auto = None;
            }
            (autre && brouillons.cles().count() != avant, autre)
        };
        let g = f.global::<DonneesSaisie>();
        g.set_visible(cle.is_some());
        if autre {
            g.set_propositions(slint::ModelRc::default());
        }
        self.afficher(f);
        change
    }

    pub fn modifie(&self, f: &Principale, texte: &str, runtime: &tokio::runtime::Handle) -> bool {
        let presence_change = {
            let mut brouillons = self.brouillons();
            let avant = self
                .etat()
                .cle
                .as_ref()
                .is_some_and(|c| brouillons.a_un_brouillon(&c.to_string()));
            let mut etat = self.etat();
            etat.modifie(&mut brouillons, texte);
            let apres = etat
                .cle
                .as_ref()
                .is_some_and(|c| brouillons.a_un_brouillon(&c.to_string()));
            avant != apres
        };
        self.planifier_enregistrement(runtime);
        let a = self.affichage();
        let g = f.global::<DonneesSaisie>();
        g.set_compteur(a.compteur.as_str().into());
        g.set_compteur_alerte(a.alerte);
        if g.get_erreur() != "" {
            g.set_erreur("".into());
        }
        presence_change
    }

    pub fn envoyer(&self, f: &Principale, texte: &str) -> Option<Action> {
        let action = {
            let mut brouillons = self.brouillons();
            self.etat().envoyer(&mut brouillons, texte)
        };
        self.afficher(f);
        action
    }

    pub fn repondre(&self, f: &Principale, message: &Message) {
        self.etat().repondre(message);
        self.afficher(f);
        self.demander_focus(f);
    }

    pub fn modifier(&self, f: &Principale, message: &Message) {
        {
            let mut brouillons = self.brouillons();
            self.etat().modifier(&mut brouillons, message);
        }
        self.afficher(f);
        self.demander_focus(f);
    }

    pub fn annuler(&self, f: &Principale) -> bool {
        let annule = {
            let brouillons = self.brouillons();
            self.etat().annuler(&brouillons)
        };
        if annule {
            self.afficher(f);
        }
        annule
    }

    pub fn texte(&self) -> String {
        self.etat().texte.clone()
    }

    pub fn definir_curseur(&self, curseur: usize) {
        self.etat().curseur = curseur;
    }

    pub fn placer(&self, f: &Principale, texte: &str, curseur: usize) {
        let g = f.global::<DonneesSaisie>();
        g.set_texte(texte.into());
        g.set_curseur(curseur as i32);
        g.set_ancre(curseur as i32);
        g.set_curseur_demande(curseur as i32);
        g.set_demande_curseur(g.get_demande_curseur().wrapping_add(1));
        self.etat().curseur = curseur;
    }

    pub fn proposer(
        &self,
        f: &Principale,
        auto: Option<Autocompletion>,
        image: impl Fn(&Proposition) -> slint::Image,
    ) {
        if !self.etat().proposer(auto) {
            return;
        }
        let (titre, lignes) = match &self.etat().auto {
            Some(a) => (
                a.declencheur.titre(),
                a.propositions
                    .iter()
                    .map(|p| PropositionSaisie {
                        genre: genre_slint(p.genre),
                        libelle: p.libelle.as_str().into(),
                        detail: p.detail.as_str().into(),
                        image: image(p),
                    })
                    .collect::<Vec<_>>(),
            ),
            None => ("", Vec::new()),
        };
        let g = f.global::<DonneesSaisie>();
        g.set_titre_propositions(titre.into());
        g.set_propositions(slint::ModelRc::new(slint::VecModel::from(lignes)));
        g.set_proposition_choisie(0);
    }

    pub fn propositions(&self) -> Option<Autocompletion> {
        self.etat().auto.clone()
    }

    pub fn naviguer(&self, f: &Principale, pas: i32) {
        let choisie = {
            let mut e = self.etat();
            e.naviguer(pas);
            e.choisie
        };
        f.global::<DonneesSaisie>()
            .set_proposition_choisie(choisie as i32);
    }

    pub fn choisir(&self, index: usize) -> Option<(String, usize)> {
        self.etat().choisir(index)
    }

    pub fn inserer(&self, insere: &str, ancre: usize) -> Option<(String, usize)> {
        self.etat().inserer(insere, ancre)
    }

    pub fn signaler_erreur(&self, f: &Principale, erreur: &str) {
        self.etat().erreur = erreur.to_string();
        f.global::<DonneesSaisie>().set_erreur(erreur.into());
    }

    pub fn enregistrer_maintenant(&self) {
        let _ = self.brouillons().enregistrer();
    }

    fn planifier_enregistrement(&self, runtime: &tokio::runtime::Handle) {
        if !self.brouillons().a_enregistrer() {
            return;
        }
        let brouillons = self.brouillons.clone();
        let tache = runtime.spawn(async move {
            tokio::time::sleep(DELAI_ENREGISTREMENT).await;
            let _ = brouillons
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .enregistrer();
        });
        if let Some(ancienne) = self
            .enregistrement
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .replace(tache)
        {
            ancienne.abort();
        }
    }
}

pub fn executer(
    store: &Store,
    action: Action,
) -> impl std::future::Future<Output = Result<(), String>> + Send + 'static {
    let store = store.clone();
    async move {
        match action {
            Action::Envoyer(cle, brouillon) => {
                store.envoyer(&cle, brouillon).await;
                Ok(())
            }
            Action::Modifier {
                id,
                contenu,
                fichiers,
            } => store
                .modifier_message(&id, &contenu, fichiers)
                .await
                .map_err(|e| format!("Modification impossible : {}", e.message())),
            Action::Supprimer { cle, id } => store
                .supprimer_message(&cle, &id)
                .await
                .map_err(|e| format!("Suppression impossible : {}", e.message())),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn message(id: &str, auteur: &str, contenu: &str) -> Message {
        serde_json::from_value(json!({"id": id, "channel_id": "c1", "author": {"id": auteur, "pseudo": auteur}, "content": contenu}))
            .unwrap()
    }

    fn c(id: &str) -> CleConversation {
        CleConversation::Salon(id.into())
    }

    #[test]
    fn le_brouillon_suit_la_conversation() {
        let mut b = Brouillons::en_memoire();
        let mut e = EtatSaisie::default();
        e.ouvrir(&mut b, Some(c("1")), "Écrire dans #a".into());
        e.modifie(&mut b, "bonjour");
        e.ouvrir(&mut b, Some(c("2")), "Écrire dans #b".into());
        assert_eq!(e.texte, "");
        e.ouvrir(&mut b, Some(c("1")), "Écrire dans #a".into());
        assert_eq!(e.texte, "bonjour");
        assert_eq!(b.texte("c:1"), "bonjour");
    }

    #[test]
    fn envoyer_vide_le_champ_et_le_brouillon() {
        let mut b = Brouillons::en_memoire();
        let mut e = EtatSaisie::default();
        e.ouvrir(&mut b, Some(c("1")), String::new());
        e.modifie(&mut b, "  salut\n");
        let action = e.envoyer(&mut b, "  salut\n").unwrap();
        assert_eq!(action, Action::Envoyer(c("1"), Brouillon::texte("salut")));
        assert_eq!(e.texte, "");
        assert!(!b.a_un_brouillon("c:1"));
        assert_eq!(e.envoyer(&mut b, "   "), None);
    }

    #[test]
    fn la_reponse_part_avec_son_message_et_echap_l_annule() {
        let mut b = Brouillons::en_memoire();
        let mut e = EtatSaisie::default();
        e.ouvrir(&mut b, Some(c("1")), String::new());
        e.repondre(&message("7", "bob", "une **longue** question"));
        assert_eq!(
            e.mode,
            Mode::Reponse {
                id: "7".into(),
                auteur: "bob".into(),
                texte: "une longue question".into()
            }
        );
        let Some(Action::Envoyer(_, brouillon)) = e.envoyer(&mut b, "oui") else {
            panic!("envoi attendu");
        };
        assert_eq!(brouillon.reponse_a.as_deref(), Some("7"));
        assert_eq!(e.mode, Mode::Normal);
        e.repondre(&message("8", "bob", "x"));
        assert!(e.annuler(&b));
        assert_eq!(e.mode, Mode::Normal);
        assert!(!e.annuler(&b));
    }

    #[test]
    fn modifier_reprend_le_texte_puis_rend_le_brouillon() {
        let mut b = Brouillons::en_memoire();
        let mut e = EtatSaisie::default();
        e.ouvrir(&mut b, Some(c("1")), String::new());
        e.modifie(&mut b, "brouillon en cours");
        e.modifier(&mut b, &message("3", "me", "ancien"));
        assert_eq!(e.texte, "ancien");
        e.modifie(&mut b, "nouveau");
        assert_eq!(b.texte("c:1"), "brouillon en cours");
        let action = e.envoyer(&mut b, "nouveau").unwrap();
        assert_eq!(
            action,
            Action::Modifier {
                id: "3".into(),
                contenu: "nouveau".into(),
                fichiers: None
            }
        );
        assert_eq!(e.texte, "brouillon en cours");
        e.modifier(&mut b, &message("3", "me", "ancien"));
        assert_eq!(
            e.envoyer(&mut b, "ancien"),
            None,
            "texte inchangé : rien n'est envoyé"
        );
        e.modifier(&mut b, &message("3", "me", "ancien"));
        assert!(e.annuler(&b));
        assert_eq!(e.texte, "brouillon en cours");
    }

    fn jointe(id: &str) -> estompe_client::types::Attachment {
        estompe_client::types::Attachment {
            id: Some(id.into()),
            name: Some(format!("{id}.txt")),
            ..Default::default()
        }
    }

    #[test]
    fn les_pieces_partent_avec_le_message_une_fois_envoyees() {
        let mut b = Brouillons::en_memoire();
        let mut e = EtatSaisie::default();
        let limites = estompe_client::fichiers::Limites::default();
        e.ouvrir(&mut b, Some(c("1")), String::new());
        let cle = e.nouvelle_piece();
        e.liste_courante()
            .unwrap()
            .ajouter(cle, "a.txt", &limites)
            .unwrap();
        assert_eq!(e.envoyer(&mut b, ""), None);
        assert_eq!(e.erreur, estompe_client::pieces::ATTENDRE_LES_FICHIERS);
        e.ouvrir(&mut b, Some(c("2")), String::new());
        assert!(e.pieces_courantes().is_none());
        e.liste_de(cle).unwrap().terminer(cle, Ok(jointe("f1")));
        e.ouvrir(&mut b, Some(c("1")), String::new());
        let Some(Action::Envoyer(_, brouillon)) = e.envoyer(&mut b, "") else {
            panic!("envoi avec la seule pièce jointe attendu");
        };
        assert_eq!(brouillon.requete().file_ids, ["f1"]);
        assert!(e.pieces_courantes().is_none());
        assert!(e.cles_des_pieces().is_empty());
    }

    #[test]
    fn modifier_les_pieces_d_un_message_envoie_la_liste_finale() {
        let mut b = Brouillons::en_memoire();
        let mut e = EtatSaisie::default();
        e.ouvrir(&mut b, Some(c("1")), String::new());
        let mut m = message("3", "me", "texte");
        m.attachments = Some(vec![jointe("f1"), jointe("f2")]);
        e.modifier(&mut b, &m);
        assert_eq!(e.pieces_courantes().unwrap().identifiants(), ["f1", "f2"]);
        assert_eq!(e.envoyer(&mut b, "texte"), None, "rien n'a changé");
        e.modifier(&mut b, &m);
        let premiere = e.edition.pieces[0].cle;
        e.liste_de(premiere).unwrap().retirer(premiere);
        assert_eq!(
            e.envoyer(&mut b, "texte"),
            Some(Action::Modifier {
                id: "3".into(),
                contenu: "texte".into(),
                fichiers: Some(vec!["f2".into()])
            })
        );
        e.modifier(&mut b, &m);
        assert!(e.annuler(&b));
        assert!(e.edition.est_vide());
    }

    #[test]
    fn le_compteur_apparait_pres_de_la_limite() {
        let mut e = EtatSaisie {
            texte: "a".repeat(3600),
            ..EtatSaisie::default()
        };
        assert_eq!(e.affichage().compteur, "3600 / 4000");
        assert!(!e.affichage().alerte);
        e.texte = "a".repeat(3950);
        assert!(e.affichage().alerte);
        e.texte = "a".into();
        assert_eq!(e.affichage().compteur, "");
    }

    #[test]
    fn les_propositions_se_parcourent_en_boucle_et_s_appliquent_au_curseur() {
        let mut e = EtatSaisie {
            texte: "salut @al ok".into(),
            curseur: 9,
            ..EtatSaisie::default()
        };
        let proposition = |nom: &str| Proposition {
            genre: GenreProposition::Personne,
            libelle: format!("@{nom}"),
            detail: String::new(),
            emoji: String::new(),
            insertion: format!("@{nom} "),
        };
        assert!(e.proposer(Some(Autocompletion {
            declencheur: estompe_client::autocompletion::Declencheur::Mention,
            debut: 6,
            propositions: vec![proposition("alice"), proposition("malik")],
        })));
        e.naviguer(-1);
        assert_eq!(e.choisie, 1);
        e.naviguer(1);
        assert_eq!(e.choisie, 0);
        assert_eq!(e.choisir(1), Some(("salut @malik  ok".to_string(), 13)));
        assert!(e.proposer(None));
        assert_eq!(e.choisir(0), None);
        e.curseur = 5;
        assert_eq!(e.inserer("😂", 0), Some(("😂 @al ok".to_string(), 4)));
    }

    #[test]
    fn l_indice_nomme_la_conversation() {
        assert_eq!(indice_de("général", false), "Écrire dans #général");
        assert_eq!(indice_de("bob", true), "Écrire à bob");
        assert_eq!(apercu(&"mot ".repeat(40)).chars().count(), 81);
    }
}
