use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::appels::Badge;
use estompe_client::lru::Lru;
use estompe_client::markdown::{analyser, texte_brut};
use estompe_client::mention::Mentionnables;
use estompe_client::store::messages::{auteur, ChangementMessages, Conversation};
use estompe_client::store::{Changement, CleConversation, Store};
use estompe_plateforme::{Indisponible, OuvertureUrl};
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};

use super::defilement::{Alignement, Defilement};
use super::lignes::{
    donnees_message, lignes_de_conversation, mentionnables, premier_non_lu, utilisateur_mentionne,
    ContexteLignes, DonneesAnnexes, DonneesLigne, DonneesMessage, GenreLigne,
};
use super::rendu::{analyser_cible, blocs_affiches, BlocAffiche, Cible, GenreBlocAffiche, Palette};
use crate::coquille::avatars::{CleAvatar, Profils};
use crate::coquille::modeles::{synchroniser, Operation};
use crate::coquille::pont::{badge_slint, couleur_slint, vec_de};
use crate::{
    AnnexesMessage, BlocTexte, DonneesMessagerie, EtatConversation, GenreBloc, GenreLigneMessage,
    LigneMessage, Principale, ReactionMessage, ReponseMessage, Theme,
};

pub type Fuseau = Arc<dyn Fn(i64) -> i32 + Send + Sync>;
pub type Navigateur = Arc<dyn OuvertureUrl + Send + Sync>;

pub const CAPACITE_RENDUS: usize = 512;
pub const DUREE_MISE_EN_EVIDENCE: Duration = Duration::from_millis(1500);
pub const CLE_DEBUT: &str = "debut";
pub const CLE_CHARGEMENT_HAUT: &str = "chargement-haut";
pub const CLE_CHARGEMENT_BAS: &str = "chargement-bas";
pub const LARGEUR_PAR_DEFAUT: f64 = 800.0;
pub const VUE_PAR_DEFAUT: f64 = 600.0;

const LARGEUR_CARACTERE: f64 = 7.4;
const HAUTEUR_LIGNE_TEXTE: f64 = 23.0;
const GOUTTIERE: f64 = 96.0;

thread_local! {
    static MINUTERIE: RefCell<Option<slint::Timer>> = const { RefCell::new(None) };
}

pub fn fuseau_local() -> Fuseau {
    Arc::new(|ms| {
        use chrono::{Local, Offset, TimeZone};
        Local
            .timestamp_millis_opt(ms)
            .single()
            .map(|d| d.offset().fix().local_minus_utc() / 60)
            .unwrap_or(0)
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CleRendu {
    id: String,
    contenu: String,
    modifie: bool,
    reveles: BTreeSet<u32>,
    palette: Palette,
    generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendus {
    pub blocs: Vec<BlocAffiche>,
    pub texte_brut: String,
}

#[derive(Debug, Clone, PartialEq)]
struct LigneFenetre {
    donnees: DonneesLigne,
    rendus: Option<Arc<Rendus>>,
    portrait: bool,
    badges: Vec<Badge>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Proprietes {
    active: bool,
    haut: f32,
    bas: f32,
    defilement: f32,
    etat: Option<EtatConversation>,
    bouton_bas: bool,
    nouveaux: i32,
    saisie: String,
    lecture_seule: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Statistiques {
    pub projections: usize,
    pub conversions: usize,
    pub rendus_calcules: usize,
}

struct Interne {
    cle: Option<CleConversation>,
    serveur: Option<String>,
    non_lus_ouverture: i64,
    nouveau_depuis: Option<String>,
    positionnee: bool,
    mis_en_evidence: Option<String>,
    supprimes: HashSet<String>,
    reveles: HashMap<String, BTreeSet<u32>>,
    lignes: Vec<DonneesLigne>,
    index: HashMap<String, usize>,
    defilement: Defilement,
    ombre_fenetre: Vec<LigneFenetre>,
    rendus: Lru<CleRendu, Arc<Rendus>>,
    mentionnables: Option<Arc<Mentionnables>>,
    generation: u64,
    palette: Palette,
    en_bas_signale: Option<bool>,
    chargement_haut: bool,
    chargement_bas: bool,
    echec_page: bool,
    debut_atteint: bool,
    fin_atteinte: bool,
    chargee: bool,
    erreur: bool,
    focus: bool,
    nouveaux: usize,
    saisie: String,
    lecture_seule: bool,
    avec_portrait: HashSet<String>,
    demandes_avatars: HashSet<CleAvatar>,
    echecs_avatars: HashSet<CleAvatar>,
    demandes_badges: HashSet<String>,
    operations: Vec<(Operation, String)>,
    proprietes: Proprietes,
    statistiques: Statistiques,
}

impl Interne {
    fn nouveau() -> Self {
        let mut defilement = Defilement::default();
        defilement.redimensionner(LARGEUR_PAR_DEFAUT, VUE_PAR_DEFAUT);
        Self {
            cle: None,
            serveur: None,
            non_lus_ouverture: 0,
            nouveau_depuis: None,
            positionnee: false,
            mis_en_evidence: None,
            supprimes: HashSet::new(),
            reveles: HashMap::new(),
            lignes: Vec::new(),
            index: HashMap::new(),
            defilement,
            ombre_fenetre: Vec::new(),
            rendus: Lru::nouveau(CAPACITE_RENDUS),
            mentionnables: None,
            generation: 0,
            palette: Palette::default(),
            en_bas_signale: None,
            chargement_haut: false,
            chargement_bas: false,
            echec_page: false,
            debut_atteint: false,
            fin_atteinte: false,
            chargee: false,
            erreur: false,
            focus: true,
            nouveaux: 0,
            saisie: String::new(),
            lecture_seule: false,
            avec_portrait: HashSet::new(),
            demandes_avatars: HashSet::new(),
            echecs_avatars: HashSet::new(),
            demandes_badges: HashSet::new(),
            operations: Vec::new(),
            proprietes: Proprietes::default(),
            statistiques: Statistiques::default(),
        }
    }

    fn reinitialiser(&mut self) {
        let palette = self.palette.clone();
        let focus = self.focus;
        let generation = self.generation + 1;
        let (largeur, vue) = (self.defilement.largeur(), self.defilement.vue());
        let proprietes = std::mem::take(&mut self.proprietes);
        let statistiques = std::mem::take(&mut self.statistiques);
        let rendus = std::mem::replace(&mut self.rendus, Lru::nouveau(0));
        let echecs = std::mem::take(&mut self.echecs_avatars);
        *self = Self::nouveau();
        self.palette = palette;
        self.focus = focus;
        self.generation = generation;
        self.defilement.redimensionner(largeur, vue);
        self.proprietes = proprietes;
        self.statistiques = statistiques;
        self.rendus = rendus;
        self.echecs_avatars = echecs;
    }
}

pub fn estimer_hauteur(ligne: &DonneesLigne, largeur: f64) -> f64 {
    match ligne.genre {
        GenreLigne::Jour => 40.0,
        GenreLigne::Nouveau => 32.0,
        GenreLigne::Message => ligne
            .message
            .as_ref()
            .map_or(40.0, |m| estimer_message(m, largeur)),
    }
}

fn estimer_message(m: &DonneesMessage, largeur: f64) -> f64 {
    let colonne = (largeur - GOUTTIERE).max(120.0);
    let par_ligne = (colonne / LARGEUR_CARACTERE).max(10.0);
    let lignes: f64 = m
        .contenu
        .split('\n')
        .map(|l| (l.chars().count() as f64 / par_ligne).ceil().max(1.0))
        .sum();
    let mut h = if m.suite { 4.0 } else { 36.0 };
    if !m.contenu.is_empty() {
        h += lignes * HAUTEUR_LIGNE_TEXTE;
    }
    if m.reponse.is_some() {
        h += 24.0;
    }
    if !m.annexes.est_vide() {
        h += 22.0;
    }
    h.max(if m.suite { 26.0 } else { 58.0 })
}

pub fn texte_de_saisie(noms: &[String]) -> String {
    match noms {
        [] => String::new(),
        [a] => format!("{a} est en train d'écrire…"),
        [a, b] => format!("{a} et {b} écrivent…"),
        _ => "Plusieurs personnes écrivent…".to_string(),
    }
}

fn hexa(couleur: slint::Color) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        couleur.red(),
        couleur.green(),
        couleur.blue()
    )
}

pub fn palette_du_theme(f: &Principale) -> Palette {
    let theme = f.global::<Theme>();
    Palette {
        mention: hexa(theme.get_rose_texte()),
        masque: hexa(theme.get_encre()),
        discret: hexa(theme.get_gris()),
    }
}

fn pseudo_ligne(cle: &str) -> DonneesLigne {
    DonneesLigne {
        cle: cle.to_string(),
        genre: GenreLigne::Jour,
        libelle: String::new(),
        message: None,
    }
}

fn genre_pseudo(cle: &str) -> Option<GenreLigneMessage> {
    match cle {
        CLE_DEBUT => Some(GenreLigneMessage::Debut),
        CLE_CHARGEMENT_HAUT | CLE_CHARGEMENT_BAS => Some(GenreLigneMessage::Chargement),
        _ => None,
    }
}

fn hauteur_pseudo(cle: &str) -> f64 {
    if cle == CLE_DEBUT {
        130.0
    } else {
        48.0
    }
}

fn bloc_slint(bloc: &BlocAffiche) -> BlocTexte {
    BlocTexte {
        genre: match bloc.genre {
            GenreBlocAffiche::Paragraphe => GenreBloc::Paragraphe,
            GenreBlocAffiche::Titre => GenreBloc::Titre,
            GenreBlocAffiche::Code => GenreBloc::Code,
        },
        texte: slint::StyledText::from_markdown(&bloc.balisage)
            .unwrap_or_else(|_| slint::StyledText::from_plain_text(&bloc.balisage)),
        code: bloc.code.as_str().into(),
        niveau: i32::from(bloc.niveau),
        citation: i32::from(bloc.citation),
        retrait: i32::from(bloc.retrait),
        puce: bloc.puce.as_str().into(),
    }
}

fn annexes_slint(a: &DonneesAnnexes) -> AnnexesMessage {
    let reactions: Vec<ReactionMessage> = a
        .reactions
        .iter()
        .map(|r| ReactionMessage {
            emoji: r.emoji.as_str().into(),
            nombre: r.nombre.clamp(0, i64::from(i32::MAX)) as i32,
            moi: r.moi,
        })
        .collect();
    AnnexesMessage {
        pieces_jointes: a.pieces_jointes.min(i32::MAX as usize) as i32,
        sondage: a.sondage,
        reactions: ModelRc::new(VecModel::from(reactions)),
        lien: a.lien.as_deref().unwrap_or_default().into(),
        resume: a.resume().into(),
    }
}

fn ligne_slint(l: &LigneFenetre, profils: &Profils) -> LigneMessage {
    let d = &l.donnees;
    if let Some(genre) = genre_pseudo(&d.cle) {
        return LigneMessage {
            cle: d.cle.as_str().into(),
            genre,
            ..LigneMessage::default()
        };
    }
    let Some(m) = &d.message else {
        return LigneMessage {
            cle: d.cle.as_str().into(),
            genre: match d.genre {
                GenreLigne::Nouveau => GenreLigneMessage::Nouveau,
                _ => GenreLigneMessage::Jour,
            },
            libelle: d.libelle.as_str().into(),
            ..LigneMessage::default()
        };
    };
    let couleur = couleur_slint(m.couleur.as_deref());
    let portrait = if l.portrait {
        m.avatar
            .as_ref()
            .and_then(|c| profils.avatar(c))
            .map(Image::from_rgba8)
            .unwrap_or_default()
    } else {
        Image::default()
    };
    let blocs: Vec<BlocTexte> = l
        .rendus
        .as_ref()
        .map(|r| r.blocs.iter().map(bloc_slint).collect())
        .unwrap_or_default();
    LigneMessage {
        cle: d.cle.as_str().into(),
        genre: GenreLigneMessage::Message,
        libelle: SharedString::default(),
        id: m.id.as_str().into(),
        suite: m.suite,
        utilisateur: m.utilisateur.as_str().into(),
        auteur: m.auteur.as_str().into(),
        initiales: m.initiales.as_str().into(),
        teinte: m.teinte,
        portrait,
        a_couleur: couleur.is_some(),
        couleur: couleur.unwrap_or_default(),
        bot: m.bot,
        badge_1: l.badges.first().map(badge_slint).unwrap_or_default(),
        badge_2: l.badges.get(1).map(badge_slint).unwrap_or_default(),
        nombre_badges: l.badges.len().min(i32::MAX as usize) as i32,
        heure: m.heure.as_str().into(),
        date_complete: m.date_complete.as_str().into(),
        systeme: m.systeme,
        epingle: m.epingle,
        modifie: m.modifie,
        date_modification: m.date_modification.as_str().into(),
        mention: m.mention,
        mis_en_evidence: m.mis_en_evidence,
        reponse: m
            .reponse
            .as_ref()
            .map(|r| ReponseMessage {
                presente: true,
                id: r.id.as_str().into(),
                auteur: r.auteur.as_str().into(),
                texte: r.texte.as_str().into(),
                supprimee: r.supprimee,
            })
            .unwrap_or_default(),
        blocs: ModelRc::new(VecModel::from(blocs)),
        texte_brut: l
            .rendus
            .as_ref()
            .map(|r| r.texte_brut.as_str())
            .unwrap_or_default()
            .into(),
        annexes: annexes_slint(&m.annexes),
        etat_envoi: SharedString::default(),
    }
}

fn cles_des_operations(
    anciennes: Vec<String>,
    nouvelles: &[LigneFenetre],
    operations: &[Operation],
) -> Vec<(Operation, String)> {
    let mut cles = anciennes;
    operations
        .iter()
        .map(|operation| {
            let cle = match *operation {
                Operation::Retirer(i) => cles.remove(i),
                Operation::Inserer(i) => {
                    cles.insert(i, nouvelles[i].donnees.cle.clone());
                    nouvelles[i].donnees.cle.clone()
                }
                Operation::Remplacer(i) => nouvelles[i].donnees.cle.clone(),
            };
            (*operation, cle)
        })
        .collect()
}

#[derive(Clone)]
pub struct PontMessagerie {
    interne: Arc<Mutex<Interne>>,
    store: Store,
    fenetre: slint::Weak<Principale>,
    runtime: tokio::runtime::Handle,
    profils: Arc<Profils>,
    fuseau: Arc<Mutex<Fuseau>>,
    navigateur: Arc<Mutex<Navigateur>>,
}

impl PontMessagerie {
    pub fn nouveau(
        store: Store,
        fenetre: slint::Weak<Principale>,
        runtime: tokio::runtime::Handle,
        profils: Arc<Profils>,
    ) -> Self {
        Self {
            interne: Arc::new(Mutex::new(Interne::nouveau())),
            store,
            fenetre,
            runtime,
            profils,
            fuseau: Arc::new(Mutex::new(fuseau_local())),
            navigateur: Arc::new(Mutex::new(Arc::new(Indisponible))),
        }
    }

    fn verrou(&self) -> MutexGuard<'_, Interne> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn definir_fuseau(&self, fuseau: Fuseau) {
        *self.fuseau.lock().unwrap_or_else(|e| e.into_inner()) = fuseau;
    }

    pub fn definir_navigateur(&self, navigateur: Navigateur) {
        *self.navigateur.lock().unwrap_or_else(|e| e.into_inner()) = navigateur;
    }

    pub fn conversation(&self) -> Option<CleConversation> {
        self.verrou().cle.clone()
    }

    pub fn operations(&self) -> Vec<(Operation, String)> {
        self.verrou().operations.clone()
    }

    pub fn statistiques(&self) -> Statistiques {
        self.verrou().statistiques.clone()
    }

    pub fn nombre_de_lignes(&self) -> usize {
        self.verrou().lignes.len()
    }

    pub fn rendus_en_cache(&self) -> usize {
        self.verrou().rendus.len()
    }

    pub fn position(&self) -> f64 {
        self.verrou().defilement.position()
    }

    pub fn colle(&self) -> bool {
        self.verrou().defilement.colle()
    }

    pub fn nouveau_depuis(&self) -> Option<String> {
        self.verrou().nouveau_depuis.clone()
    }

    pub fn cles(&self) -> Vec<String> {
        self.verrou().lignes.iter().map(|l| l.cle.clone()).collect()
    }

    pub fn ouvrir(
        &self,
        f: &Principale,
        cle: Option<CleConversation>,
        serveur: Option<String>,
        non_lus: i64,
    ) {
        {
            let mut i = self.verrou();
            if i.cle == cle {
                return;
            }
            i.reinitialiser();
            i.palette = palette_du_theme(f);
            i.cle = cle.clone();
            i.serveur = serveur;
            i.non_lus_ouverture = non_lus;
            i.lecture_seule = match &cle {
                Some(CleConversation::Prive(id)) => self.store.lire(|etat| {
                    etat.messages_prives.iter().any(|d| {
                        d.id.as_deref() == Some(id.as_str())
                            && d.other.as_ref().and_then(|o| o.id.as_deref()).is_some_and(
                                estompe_client::store::messages::est_compte_estompe,
                            )
                    })
                }),
                _ => false,
            };
        }
        let g = f.global::<DonneesMessagerie>();
        vec_de(&g.get_lignes()).set_vec(Vec::new());
        g.set_defilement(0.0);
        self.reprojeter(f);
    }

    pub fn fermer(&self, f: &Principale) {
        self.ouvrir(f, None, None, 0);
    }

    pub fn etat_chargement(&self, f: &Principale, cle: &CleConversation, reussi: bool) {
        {
            let mut i = self.verrou();
            if i.cle.as_ref() != Some(cle) {
                return;
            }
            i.erreur = !reussi && !i.chargee;
        }
        self.rafraichir(f);
    }

    pub fn appliquer(&self, f: &Principale, changements: &[Changement]) {
        let Some(cle) = self.conversation() else {
            return;
        };
        let serveur = self.verrou().serveur.clone();
        let mut reprojeter = false;
        let mut modifies: Vec<String> = Vec::new();
        let mut evidence: Option<String> = None;
        let mut saisie = false;
        for changement in changements {
            match changement {
                Changement::Messages(m) => match m {
                    ChangementMessages::AjoutesEnFin {
                        conversation,
                        nombre,
                    } if *conversation == cle => {
                        self.noter_ajouts(&cle, *nombre);
                        reprojeter = true;
                    }
                    ChangementMessages::Supprime {
                        conversation,
                        message,
                    } if *conversation == cle => {
                        self.verrou().supprimes.insert(message.clone());
                        reprojeter = true;
                    }
                    ChangementMessages::Modifie {
                        conversation,
                        message,
                    } if *conversation == cle => modifies.push(message.clone()),
                    ChangementMessages::MiseEnEvidence {
                        conversation,
                        message,
                    } if *conversation == cle => evidence = Some(message.clone()),
                    ChangementMessages::Saisie { conversation } if *conversation == cle => {
                        saisie = true
                    }
                    ChangementMessages::AjoutesEnTete { conversation, .. }
                    | ChangementMessages::Insere { conversation, .. }
                    | ChangementMessages::RetiresEnTete { conversation, .. }
                    | ChangementMessages::RetiresEnFin { conversation, .. }
                    | ChangementMessages::Bornes { conversation }
                    | ChangementMessages::Rechargee { conversation }
                        if *conversation == cle =>
                    {
                        reprojeter = true
                    }
                    _ => {}
                },
                Changement::Roles { serveur: s }
                | Changement::Membre { serveur: s, .. }
                | Changement::MembreAjoute { serveur: s, .. }
                | Changement::MembreRetire { serveur: s, .. }
                    if serveur.as_deref() == Some(s.as_str()) =>
                {
                    let mut i = self.verrou();
                    i.mentionnables = None;
                    i.generation += 1;
                    reprojeter = true;
                }
                Changement::Avatar { .. } => reprojeter = true,
                _ => {}
            }
        }
        if saisie {
            self.verrou().saisie = texte_de_saisie(&self.store.saisies(&cle));
        }
        if reprojeter {
            self.reprojeter(f);
        } else {
            for id in &modifies {
                self.modifier_ligne(&cle, id);
            }
            if !modifies.is_empty() || saisie {
                self.rafraichir(f);
            }
        }
        if let Some(id) = evidence {
            self.mettre_en_evidence(f, &id);
        }
    }

    fn noter_ajouts(&self, cle: &CleConversation, nombre: usize) {
        let Some(conversation) = self.store.conversation(cle) else {
            return;
        };
        let moi = self.store.lire(|e| e.moi().to_string());
        let mut i = self.verrou();
        if !i.positionnee {
            return;
        }
        let nouveaux: Vec<String> = conversation
            .messages
            .iter()
            .rev()
            .take(nombre)
            .filter(|m| auteur(m) != moi)
            .map(|m| estompe_client::store::messages::id_message(m).to_string())
            .collect();
        if nouveaux.is_empty() {
            if conversation
                .messages
                .iter()
                .rev()
                .take(nombre)
                .any(|m| auteur(m) == moi)
            {
                i.nouveau_depuis = None;
                i.defilement.aller_en_bas();
            }
            return;
        }
        let lecture = !i.defilement.colle() || !i.focus;
        if lecture && i.nouveau_depuis.is_none() {
            i.nouveau_depuis = nouveaux.last().cloned();
        }
        if !i.defilement.colle() {
            i.nouveaux += nouveaux.len();
        }
    }

    fn contexte_et_mentions(&self) -> Option<(CleConversation, Option<String>)> {
        let i = self.verrou();
        Some((i.cle.clone()?, i.serveur.clone()))
    }

    fn reprojeter(&self, f: &Principale) {
        let Some((cle, serveur)) = self.contexte_et_mentions() else {
            self.rafraichir(f);
            return;
        };
        let conversation = self.store.conversation(&cle).filter(|c| c.chargee);
        let decalage = {
            let fuseau = self.fuseau.lock().unwrap_or_else(|e| e.into_inner()).clone();
            fuseau(chrono::Utc::now().timestamp_millis())
        };
        let moi = self.store.lire(|e| e.moi().to_string());
        {
            let mut i = self.verrou();
            i.statistiques.projections += 1;
            if let Some(c) = &conversation {
                if !i.positionnee && i.nouveau_depuis.is_none() {
                    i.nouveau_depuis = premier_non_lu(&c.messages, i.non_lus_ouverture, &moi);
                }
                i.chargee = true;
                i.erreur = false;
                i.debut_atteint = c.debut_atteint;
                i.fin_atteinte = c.fin_atteinte;
            }
        }
        let lignes = match &conversation {
            Some(c) => self.lignes_de(c, &cle, serveur.as_deref(), decalage),
            None => Vec::new(),
        };
        let mut a_positionner = false;
        {
            let mut i = self.verrou();
            let largeur = i.defilement.largeur();
            let mut toutes: Vec<DonneesLigne> = Vec::with_capacity(lignes.len() + 2);
            if conversation.is_some() {
                toutes.push(pseudo_ligne(if i.debut_atteint {
                    CLE_DEBUT
                } else {
                    CLE_CHARGEMENT_HAUT
                }));
            }
            toutes.extend(lignes);
            if conversation.is_some() && !i.fin_atteinte {
                toutes.push(pseudo_ligne(CLE_CHARGEMENT_BAS));
            }
            let estimations: Vec<(String, f64)> = toutes
                .iter()
                .map(|l| {
                    let h = if genre_pseudo(&l.cle).is_some() {
                        hauteur_pseudo(&l.cle)
                    } else {
                        estimer_hauteur(l, largeur)
                    };
                    (l.cle.clone(), h)
                })
                .collect();
            i.index = toutes
                .iter()
                .enumerate()
                .map(|(n, l)| (l.cle.clone(), n))
                .collect();
            i.lignes = toutes;
            i.defilement.remplacer_lignes(estimations);
            if conversation.is_some() && !i.positionnee {
                i.positionnee = true;
                a_positionner = true;
            }
        }
        if a_positionner {
            let mut i = self.verrou();
            match i.nouveau_depuis.clone() {
                Some(id) => {
                    i.defilement
                        .aller_a(&format!("new-{id}"), Alignement::Haut);
                }
                None => i.defilement.aller_en_bas(),
            }
        }
        self.rafraichir(f);
    }

    fn lignes_de(
        &self,
        c: &Conversation,
        cle: &CleConversation,
        serveur: Option<&str>,
        decalage: i32,
    ) -> Vec<DonneesLigne> {
        let i = self.verrou();
        let nouveau = i.nouveau_depuis.clone();
        let evidence = i.mis_en_evidence.clone();
        let supprimes = i.supprimes.clone();
        drop(i);
        let _ = cle;
        self.store.lire(|etat| {
            lignes_de_conversation(
                &c.messages,
                &ContexteLignes {
                    etat,
                    serveur,
                    nouveau_depuis: nouveau.as_deref(),
                    mis_en_evidence: evidence.as_deref(),
                    supprimes: &supprimes,
                    decalage_minutes: decalage,
                },
            )
        })
    }

    fn modifier_ligne(&self, cle: &CleConversation, id: &str) {
        let Some(conversation) = self.store.conversation(cle) else {
            return;
        };
        let Some(message) = conversation.message(id).cloned() else {
            return;
        };
        let decalage = {
            let fuseau = self.fuseau.lock().unwrap_or_else(|e| e.into_inner()).clone();
            fuseau(chrono::Utc::now().timestamp_millis())
        };
        let (serveur, evidence, supprimes, indice, suite) = {
            let i = self.verrou();
            let Some(&n) = i.index.get(id) else {
                return;
            };
            let suite = i.lignes[n].message.as_ref().is_some_and(|m| m.suite);
            (
                i.serveur.clone(),
                i.mis_en_evidence.clone(),
                i.supprimes.clone(),
                n,
                suite,
            )
        };
        let donnees = self.store.lire(|etat| {
            donnees_message(
                &message,
                suite,
                &ContexteLignes {
                    etat,
                    serveur: serveur.as_deref(),
                    nouveau_depuis: None,
                    mis_en_evidence: evidence.as_deref(),
                    supprimes: &supprimes,
                    decalage_minutes: decalage,
                },
            )
        });
        let mut i = self.verrou();
        let largeur = i.defilement.largeur();
        if let Some(ligne) = i.lignes.get_mut(indice) {
            ligne.message = Some(donnees);
            let estimation = estimer_hauteur(ligne, largeur);
            let cle_ligne = ligne.cle.clone();
            i.defilement.estimer(&cle_ligne, estimation);
        }
    }

    fn mentionnables(&self) -> Arc<Mentionnables> {
        if let Some(m) = self.verrou().mentionnables.clone() {
            return m;
        }
        let cle = self.conversation();
        let m = Arc::new(match &cle {
            Some(cle) => self.store.lire(|etat| mentionnables(etat, cle)),
            None => Mentionnables::default(),
        });
        self.verrou().mentionnables = Some(m.clone());
        m
    }

    fn rendus_de(&self, m: &DonneesMessage) -> Arc<Rendus> {
        let (cle, trouve) = {
            let mut i = self.verrou();
            let cle = CleRendu {
                id: m.id.clone(),
                contenu: m.contenu.clone(),
                modifie: m.modifie,
                reveles: i.reveles.get(&m.id).cloned().unwrap_or_default(),
                palette: i.palette.clone(),
                generation: i.generation,
            };
            let trouve = i.rendus.lire(&cle).cloned();
            (cle, trouve)
        };
        if let Some(r) = trouve {
            return r;
        }
        let contexte = self.mentionnables();
        let rendu = analyser(&m.contenu, &*contexte);
        let rendus = Arc::new(Rendus {
            blocs: if m.contenu.is_empty() && !m.modifie {
                Vec::new()
            } else {
                blocs_affiches(&rendu, &cle.reveles, &cle.palette, m.modifie)
            },
            texte_brut: texte_brut(&rendu.blocs),
        });
        let mut i = self.verrou();
        i.statistiques.rendus_calcules += 1;
        i.rendus.inserer(cle, rendus.clone());
        rendus
    }

    fn rafraichir(&self, f: &Principale) {
        let g = f.global::<DonneesMessagerie>();
        let (fenetre, lignes_fenetre) = {
            let i = self.verrou();
            let fenetre = i.defilement.fenetre();
            let fin = fenetre.fin.min(i.lignes.len());
            let debut = fenetre.debut.min(fin);
            (fenetre, i.lignes[debut..fin].to_vec())
        };
        let mut nouvelles: Vec<LigneFenetre> = Vec::with_capacity(lignes_fenetre.len());
        let mut avatars = Vec::new();
        let mut badges = Vec::new();
        for donnees in lignes_fenetre {
            let (rendus, portrait, badges_ligne) = match &donnees.message {
                Some(m) => {
                    let rendus = self.rendus_de(m);
                    let portrait = m.avatar.as_ref().is_some_and(|c| {
                        let present = self.profils.avatar(c).is_some();
                        if !present {
                            let mut i = self.verrou();
                            if !i.echecs_avatars.contains(c) && i.demandes_avatars.insert(c.clone())
                            {
                                avatars.push(c.clone());
                            }
                        }
                        present
                    });
                    let badges_ligne = match self.profils.badges(&m.utilisateur) {
                        Some(b) => b,
                        None => {
                            if !m.suite
                                && !m.systeme
                                && self.verrou().demandes_badges.insert(m.utilisateur.clone())
                            {
                                badges.push(m.utilisateur.clone());
                            }
                            Vec::new()
                        }
                    };
                    (Some(rendus), portrait, if m.suite { Vec::new() } else { badges_ligne })
                }
                None => (None, false, Vec::new()),
            };
            nouvelles.push(LigneFenetre {
                donnees,
                rendus,
                portrait,
                badges: badges_ligne,
            });
        }
        let modele_rc = g.get_lignes();
        let modele = vec_de(&modele_rc);
        let profils = self.profils.clone();
        let mut i = self.verrou();
        let anciennes: Vec<String> = i.ombre_fenetre.iter().map(|l| l.donnees.cle.clone()).collect();
        let faites = synchroniser(
            modele,
            &mut i.ombre_fenetre,
            nouvelles,
            |l| l.donnees.cle.clone(),
            |l| ligne_slint(l, &profils),
        );
        if !faites.is_empty() {
            i.statistiques.conversions += faites
                .iter()
                .filter(|o| !matches!(o, Operation::Retirer(_)))
                .count();
            let ops = cles_des_operations(anciennes, &i.ombre_fenetre, &faites);
            i.operations = ops;
        }
        i.avec_portrait = i
            .ombre_fenetre
            .iter()
            .filter(|l| l.portrait)
            .map(|l| l.donnees.cle.clone())
            .collect();
        let en_bas = i.defilement.en_bas();
        if en_bas && i.fin_atteinte {
            i.nouveaux = 0;
        }
        let proprietes = Proprietes {
            active: i.cle.is_some(),
            haut: fenetre.haut as f32,
            bas: fenetre.bas as f32,
            defilement: -(i.defilement.position() as f32),
            etat: Some(if i.chargee {
                EtatConversation::Prete
            } else if i.erreur {
                EtatConversation::Erreur
            } else {
                EtatConversation::Chargement
            }),
            bouton_bas: i.chargee && (!en_bas || !i.fin_atteinte),
            nouveaux: i.nouveaux.min(i32::MAX as usize) as i32,
            saisie: i.saisie.clone(),
            lecture_seule: i.lecture_seule,
        };
        let ancienne = std::mem::replace(&mut i.proprietes, proprietes.clone());
        let charger_haut = i.chargee
            && !i.debut_atteint
            && !i.chargement_haut
            && !i.echec_page
            && i.defilement.pres_du_haut();
        if charger_haut {
            i.chargement_haut = true;
        }
        let charger_bas = i.chargee
            && !i.fin_atteinte
            && !i.chargement_bas
            && !i.echec_page
            && i.defilement.pres_du_bas();
        if charger_bas {
            i.chargement_bas = true;
        }
        let lecture = i.chargee && i.fin_atteinte && en_bas;
        let signaler = i.cle.is_some() && i.chargee && i.en_bas_signale != Some(lecture);
        if signaler {
            i.en_bas_signale = Some(lecture);
        }
        let ancre = i
            .defilement
            .ancre()
            .and_then(|a| i.index.get(&a.cle).copied())
            .and_then(|n| i.lignes.get(n))
            .and_then(|l| l.message.as_ref())
            .map(|m| m.id.clone());
        let cle = i.cle.clone();
        drop(i);
        if ancienne.active != proprietes.active {
            g.set_active(proprietes.active);
        }
        if ancienne.haut != proprietes.haut {
            g.set_haut(proprietes.haut);
        }
        if ancienne.bas != proprietes.bas {
            g.set_bas(proprietes.bas);
        }
        if (g.get_defilement() - proprietes.defilement).abs() >= 0.5 {
            g.set_defilement(proprietes.defilement);
        }
        if ancienne.etat != proprietes.etat {
            g.set_etat(proprietes.etat.unwrap_or(EtatConversation::Chargement));
        }
        if ancienne.bouton_bas != proprietes.bouton_bas {
            g.set_bouton_bas(proprietes.bouton_bas);
        }
        if ancienne.nouveaux != proprietes.nouveaux {
            g.set_nouveaux(proprietes.nouveaux);
        }
        if ancienne.saisie != proprietes.saisie {
            g.set_saisie(proprietes.saisie.as_str().into());
        }
        if ancienne.lecture_seule != proprietes.lecture_seule {
            g.set_lecture_seule(proprietes.lecture_seule);
            g.set_lecture_seule_texte("Conversation en lecture seule : Estompe y publie ses nouveautés.".into());
        }
        let Some(cle) = cle else {
            return;
        };
        if charger_haut {
            self.charger_page(cle.clone(), true);
        }
        if charger_bas {
            self.charger_page(cle.clone(), false);
        }
        if signaler {
            let store = self.store.clone();
            self.runtime
                .spawn(async move { store.signaler_lecture(lecture, ancre).await });
        }
        for avatar in avatars {
            self.telecharger_avatar(avatar);
        }
        for utilisateur in badges {
            self.telecharger_badges(utilisateur);
        }
    }

    fn charger_page(&self, cle: CleConversation, haut: bool) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            let resultat = if haut {
                pont.store.charger_plus_anciens(&cle).await
            } else {
                pont.store.charger_plus_recents(&cle).await
            };
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                {
                    let mut i = p.verrou();
                    if i.cle.as_ref() != Some(&cle) {
                        return;
                    }
                    if haut {
                        i.chargement_haut = false;
                    } else {
                        i.chargement_bas = false;
                    }
                    i.echec_page = resultat.is_err();
                }
                p.reprojeter(&f);
            });
        });
    }

    fn telecharger_avatar(&self, cle: CleAvatar) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            let recu = pont.profils.charger_avatar(cle.clone()).await.is_some();
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                {
                    let mut i = p.verrou();
                    i.demandes_avatars.remove(&cle);
                    if !recu {
                        i.echecs_avatars.insert(cle);
                    }
                }
                p.rafraichir(&f);
            });
        });
    }

    fn telecharger_badges(&self, utilisateur: String) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            pont.profils.charger_badges(utilisateur.clone()).await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                p.verrou().demandes_badges.remove(&utilisateur);
                p.rafraichir(&f);
            });
        });
    }

    pub fn defilee(&self, f: &Principale, position: f32, vue: f32, largeur: f32) {
        {
            let mut i = self.verrou();
            let position = f64::from(position);
            if (position - i.defilement.position()).abs() >= 0.5 {
                i.defilement.defiler(position);
                i.echec_page = false;
            }
            if vue > 0.0 && largeur > 0.0 {
                i.defilement
                    .redimensionner(f64::from(largeur), f64::from(vue));
            }
        }
        self.rafraichir(f);
    }

    pub fn ligne_mesuree(&self, f: &Principale, cle: &str, hauteur: f32) {
        let change = self.verrou().defilement.mesurer(cle, f64::from(hauteur));
        if change {
            self.rafraichir(f);
        }
    }

    pub fn lien_clique(&self, f: &Principale, id: &str, cible: &str) -> Option<String> {
        match analyser_cible(cible)? {
            Cible::Lien(url) => {
                let navigateur = self
                    .navigateur
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone();
                let _ = navigateur.ouvrir(&url);
                None
            }
            Cible::Mention(nom) => {
                let cle = self.conversation()?;
                self.store
                    .lire(|etat| utilisateur_mentionne(etat, &cle, &nom))
            }
            Cible::Masque(n) => {
                self.verrou()
                    .reveles
                    .entry(id.to_string())
                    .or_default()
                    .insert(n);
                self.rafraichir(f);
                None
            }
        }
    }

    pub fn reponse_cliquee(&self, id: &str) {
        let Some(cle) = self.conversation() else {
            return;
        };
        let store = self.store.clone();
        let id = id.to_string();
        self.runtime.spawn(async move {
            let _ = store.sauter_a(&cle, &id).await;
        });
    }

    fn mettre_en_evidence(&self, f: &Principale, id: &str) {
        let Some(cle) = self.conversation() else {
            return;
        };
        let precedent = {
            let mut i = self.verrou();
            i.defilement.aller_a(id, Alignement::Centre);
            i.mis_en_evidence.replace(id.to_string())
        };
        if let Some(precedent) = precedent.filter(|p| p != id) {
            self.modifier_ligne(&cle, &precedent);
        }
        self.modifier_ligne(&cle, id);
        self.rafraichir(f);
        let pont = self.clone();
        let id = id.to_string();
        let minuterie = slint::Timer::default();
        minuterie.start(
            slint::TimerMode::SingleShot,
            DUREE_MISE_EN_EVIDENCE,
            move || {
                if let Some(f) = pont.fenetre.upgrade() {
                    pont.fin_mise_en_evidence(&f, &id);
                }
            },
        );
        MINUTERIE.with(|m| *m.borrow_mut() = Some(minuterie));
    }

    pub fn fin_mise_en_evidence(&self, f: &Principale, id: &str) {
        let Some(cle) = self.conversation() else {
            return;
        };
        {
            let mut i = self.verrou();
            if i.mis_en_evidence.as_deref() != Some(id) {
                return;
            }
            i.mis_en_evidence = None;
        }
        self.modifier_ligne(&cle, id);
        self.rafraichir(f);
    }

    pub fn mis_en_evidence(&self) -> Option<String> {
        self.verrou().mis_en_evidence.clone()
    }

    pub fn aller_en_bas(&self, f: &Principale) {
        {
            let mut i = self.verrou();
            i.nouveaux = 0;
            i.nouveau_depuis = None;
            i.echec_page = false;
            i.defilement.aller_en_bas();
        }
        self.reprojeter(f);
    }

    pub fn reessayer(&self, f: &Principale) {
        let Some(cle) = self.conversation() else {
            return;
        };
        self.verrou().erreur = false;
        self.rafraichir(f);
        let pont = self.clone();
        self.runtime.spawn(async move {
            let reussi = pont.store.charger_conversation(&cle).await.is_ok();
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.etat_chargement(&f, &cle, reussi));
        });
    }

    pub fn theme_change(&self, f: &Principale) {
        let palette = palette_du_theme(f);
        {
            let mut i = self.verrou();
            if i.palette == palette {
                return;
            }
            i.palette = palette;
        }
        self.rafraichir(f);
    }

    pub fn fenetre_active(&self, actif: bool) {
        self.verrou().focus = actif;
        let store = self.store.clone();
        self.runtime
            .spawn(async move { store.definir_focus(actif).await });
    }

    pub fn expirer_saisies(&self, f: &Principale) {
        let Some(cle) = self.conversation() else {
            return;
        };
        let texte = texte_de_saisie(&self.store.saisies(&cle));
        let change = {
            let mut i = self.verrou();
            let change = i.saisie != texte;
            i.saisie = texte;
            change
        };
        if change {
            self.rafraichir(f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(contenu: &str, suite: bool) -> DonneesLigne {
        DonneesLigne {
            cle: "1".into(),
            genre: GenreLigne::Message,
            libelle: String::new(),
            message: Some(DonneesMessage {
                id: "1".into(),
                utilisateur: "u".into(),
                auteur: "u".into(),
                initiales: "U".into(),
                teinte: 1,
                couleur: None,
                avatar: None,
                bot: false,
                heure: String::new(),
                date_complete: String::new(),
                suite,
                systeme: false,
                epingle: false,
                modifie: false,
                date_modification: String::new(),
                mention: false,
                mis_en_evidence: false,
                reponse: None,
                contenu: contenu.into(),
                annexes: DonneesAnnexes::default(),
            }),
        }
    }

    #[test]
    fn l_estimation_croit_avec_le_texte_et_tient_compte_du_regroupement() {
        let court = estimer_hauteur(&message("salut", false), 800.0);
        let suite = estimer_hauteur(&message("salut", true), 800.0);
        let long = estimer_hauteur(&message(&"mot ".repeat(400), false), 800.0);
        let etroit = estimer_hauteur(&message(&"mot ".repeat(400), false), 400.0);
        assert!(suite < court, "{suite} {court}");
        assert!(long > 5.0 * court, "{long} {court}");
        assert!(etroit > long);
        assert_eq!(estimer_hauteur(&pseudo_ligne("x"), 800.0), 40.0);
    }

    #[test]
    fn le_texte_de_saisie_suit_le_nombre_de_personnes() {
        assert_eq!(texte_de_saisie(&[]), "");
        assert_eq!(
            texte_de_saisie(&["carol".into()]),
            "carol est en train d'écrire…"
        );
        assert_eq!(
            texte_de_saisie(&["carol".into(), "dave".into()]),
            "carol et dave écrivent…"
        );
        assert_eq!(
            texte_de_saisie(&["a".into(), "b".into(), "c".into()]),
            "Plusieurs personnes écrivent…"
        );
    }
}
