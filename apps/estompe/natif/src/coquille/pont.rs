use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{NaiveDateTime, TimeZone};
use estompe_client::appels::Badge;
use estompe_client::ordre_serveurs::Sens;
use estompe_client::route::Route;
use estompe_client::sections::Section;
use estompe_client::store::Presence;
use estompe_client::store::{Changement, CleConversation, Store};
use estompe_client::temps_reel::EtatReseau;
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

use super::avatars::{self, decoder_image, CleAvatar, Profils};
use super::membres::{
    analyser_couleur, donnees_carte, donnees_marge, fenetre_des_membres, libelle_presence,
    lignes_membres, DonneesCarte, DonneesMarge, DonneesMembre, Fenetre,
};
use super::modeles::{cibles, remplacer_ligne, synchroniser, Cibles, Ensemble, Operation};
use super::navigation::Navigation;
use super::projection::*;
use super::recherche::EtatRecherche;
use crate::messagerie::pont::PontMessagerie;
use crate::session::Memoire;
use crate::vocal::PontVocal;
use crate::{
    Accueil, CarteProfil, DonneesCoquille, DonneesMessagerie, EnTeteMarge, EnTetePanneau,
    EtatConversation, EtatPresence, LigneBadge, LigneContact, LigneMembre, LigneMessage,
    LignePrive, LigneRattrapage, LigneSalon, LigneSection, LigneServeur, LigneVocal, OngletMarge,
    Principale, RoleProfil, VueContenu,
};

pub type Horloge = Arc<dyn Fn() -> NaiveDateTime + Send + Sync>;
pub type Icone = SharedPixelBuffer<Rgba8Pixel>;

const COTE_ICONE: u32 = 128;

fn millisecondes(maintenant: NaiveDateTime) -> i64 {
    chrono::Local
        .from_local_datetime(&maintenant)
        .earliest()
        .map(|d| d.timestamp_millis())
        .unwrap_or_else(|| maintenant.and_utc().timestamp_millis())
}

mod creation;
mod menus;
mod nouveau_prive;
mod parametres;
mod profil;
mod recherche;
mod reglages;

pub use creation::{EtatCreationSalon, EtatCreerRejoindre};
pub use nouveau_prive::EtatNouveauPrive;
pub use reglages::{emoji_depuis, EmojiEnAttente, EtatReglages};

pub fn horloge_locale() -> Horloge {
    Arc::new(|| chrono::Local::now().naive_local())
}

pub fn decoder_icone(octets: &[u8]) -> Option<Icone> {
    decoder_image(octets, COTE_ICONE)
}

const VUE_PAR_DEFAUT: f32 = 800.0;

struct EtatMembres {
    serveur: Option<String>,
    filtre: String,
    ombre: Vec<DonneesMembre>,
    index: HashMap<String, usize>,
    defilement: f32,
    vue: Option<f32>,
    fenetre: Fenetre,
    avec_portrait: HashSet<String>,
    demandes_avatars: HashSet<CleAvatar>,
    echecs_avatars: HashSet<CleAvatar>,
    demandes_badges: HashSet<String>,
    operations: Vec<(Operation, String)>,
    marge: Option<DonneesMarge>,
    carte: Option<String>,
    ombre_carte: Option<DonneesCarte>,
}

impl Default for EtatMembres {
    fn default() -> Self {
        Self {
            serveur: None,
            filtre: String::new(),
            ombre: Vec::new(),
            index: HashMap::new(),
            defilement: 0.0,
            vue: None,
            fenetre: Fenetre {
                debut: 0,
                fin: 0,
                haut: 0.0,
                bas: 0.0,
            },
            avec_portrait: HashSet::new(),
            demandes_avatars: HashSet::new(),
            echecs_avatars: HashSet::new(),
            demandes_badges: HashSet::new(),
            operations: Vec::new(),
            marge: None,
            carte: None,
            ombre_carte: None,
        }
    }
}

pub(crate) fn couleur_slint(couleur: Option<&str>) -> Option<slint::Color> {
    couleur
        .and_then(analyser_couleur)
        .map(|(r, v, b)| slint::Color::from_rgb_u8(r, v, b))
}

pub(crate) fn presence_slint(presence: Presence) -> EtatPresence {
    match presence {
        Presence::EnLigne => EtatPresence::EnLigne,
        Presence::Absent => EtatPresence::Absent,
        Presence::NePasDeranger => EtatPresence::NePasDeranger,
        Presence::HorsLigne => EtatPresence::HorsLigne,
    }
}

pub(crate) fn badge_slint(badge: &Badge, profils: &Profils) -> LigneBadge {
    let image = avatars::image_de_badge(profils, badge);
    LigneBadge {
        nom: badge.name.as_str().into(),
        description: badge.description.as_str().into(),
        couleur: couleur_slint(Some(&badge.color))
            .unwrap_or_else(|| slint::Color::from_rgb_u8(0xd0, 0x68, 0x8a)),
        a_image: image.is_some(),
        image: image.unwrap_or_default(),
    }
}

fn ligne_membre_slint(
    d: &DonneesMembre,
    portrait: Option<Icone>,
    profils: &Profils,
) -> LigneMembre {
    let badges = profils.badges(&d.utilisateur).unwrap_or_default();
    let couleur_titre = couleur_slint(d.couleur_titre.as_deref());
    let couleur = couleur_slint(d.couleur.as_deref());
    LigneMembre {
        cle: d.cle.as_str().into(),
        en_tete: d.en_tete,
        titre: d.titre.as_str().into(),
        a_couleur_titre: couleur_titre.is_some(),
        couleur_titre: couleur_titre.unwrap_or_default(),
        utilisateur: d.utilisateur.as_str().into(),
        nom: d.nom.as_str().into(),
        pseudo: d.pseudo.as_str().into(),
        initiales: d.initiales.as_str().into(),
        teinte: d.teinte,
        portrait: portrait.map(Image::from_rgba8).unwrap_or_default(),
        presence: if d.en_tete {
            EtatPresence::Aucune
        } else {
            presence_slint(d.presence)
        },
        libelle_presence: libelle_presence(d.presence).into(),
        activite: d.activite.as_str().into(),
        a_couleur: couleur.is_some(),
        couleur: couleur.unwrap_or_default(),
        createur: d.createur,
        bot: d.bot,
        exclu: d.exclu,
        badge_1: badges
            .first()
            .map(|b| badge_slint(b, profils))
            .unwrap_or_default(),
        badge_2: badges
            .get(1)
            .map(|b| badge_slint(b, profils))
            .unwrap_or_default(),
        nombre_badges: badges.len().min(i32::MAX as usize) as i32,
    }
}

fn carte_slint(
    d: &DonneesCarte,
    portrait: Option<Icone>,
    badges: Option<&[Badge]>,
    profils: &Profils,
) -> CarteProfil {
    let couleur = couleur_slint(d.couleur.as_deref());
    let roles: Vec<RoleProfil> = d
        .roles
        .iter()
        .map(|r| {
            let couleur = couleur_slint(r.couleur.as_deref());
            RoleProfil {
                nom: r.nom.as_str().into(),
                a_couleur: couleur.is_some(),
                couleur: couleur.unwrap_or_default(),
            }
        })
        .collect();
    let badges: Vec<LigneBadge> = badges
        .unwrap_or_default()
        .iter()
        .map(|b| badge_slint(b, profils))
        .collect();
    CarteProfil {
        utilisateur: d.utilisateur.as_str().into(),
        nom: d.nom.as_str().into(),
        pseudo_origine: d.pseudo_origine.as_str().into(),
        initiales: d.initiales.as_str().into(),
        teinte: d.teinte,
        portrait: portrait.map(Image::from_rgba8).unwrap_or_default(),
        a_couleur: couleur.is_some(),
        couleur: couleur.unwrap_or_default(),
        banniere: couleur_slint(d.banniere.as_deref())
            .unwrap_or_else(|| slint::Color::from_rgb_u8(0xf6, 0xc6, 0xd6)),
        presence: presence_slint(d.presence),
        statut: d.statut.as_str().into(),
        exclu_jusqu_au: d.exclu_jusqu_au.as_str().into(),
        rang: d.rang.into(),
        roles: ModelRc::new(VecModel::from(roles)),
        badges: ModelRc::new(VecModel::from(badges)),
        serveur: d.serveur.as_str().into(),
        arrivee: d.arrivee.as_str().into(),
        creation: d.creation.as_str().into(),
        bot: d.bot,
        moi: d.moi,
    }
}

fn cles_des_operations(
    anciennes: Vec<String>,
    nouvelles: &[DonneesMembre],
    operations: &[Operation],
) -> Vec<(Operation, String)> {
    let mut cles = anciennes;
    operations
        .iter()
        .map(|operation| {
            let cle = match *operation {
                Operation::Retirer(i) => cles.remove(i),
                Operation::Inserer(i) => {
                    cles.insert(i, nouvelles[i].cle.clone());
                    nouvelles[i].cle.clone()
                }
                Operation::Remplacer(i) => nouvelles[i].cle.clone(),
            };
            (*operation, cle)
        })
        .collect()
}

#[derive(Default)]
struct Ombres {
    serveurs: Vec<DonneesServeur>,
    salons: Vec<DonneesSalon>,
    prives: Vec<DonneesPrive>,
    sections: Vec<DonneesSection>,
    rattrapage: Vec<DonneesRattrapage>,
    vocaux: Vec<DonneesVocal>,
    contacts: Vec<DonneesContact>,
    panneau: Option<DonneesPanneau>,
    vue: Option<DonneesVue>,
    accueil: Option<DonneesAccueil>,
    compte: Option<DonneesCompte>,
}

struct Interne {
    navigation: Navigation,
    ombres: Ombres,
    sections: Vec<Section>,
    reseau: EtatReseau,
    deja_connecte: bool,
    icones: BTreeMap<String, (i64, Icone)>,
    demandes: BTreeSet<(String, i64)>,
    membres: EtatMembres,
    onglet_voulu: OngletMarge,
    avant_epingles: Option<(bool, OngletMarge)>,
    recherche: Option<EtatRecherche>,
    nouveau_prive: Option<EtatNouveauPrive>,
    profil: profil::EtatProfil,
    creation: creation::EtatCreation,
    reglages: Option<reglages::EtatReglages>,
}

#[derive(Clone)]
pub struct Pont {
    interne: Arc<Mutex<Interne>>,
    store: Store,
    fenetre: slint::Weak<Principale>,
    runtime: tokio::runtime::Handle,
    memoire: Option<Memoire>,
    horloge: Horloge,
    profils: Arc<Profils>,
    messagerie: PontMessagerie,
    vocal: PontVocal,
}

pub(crate) fn vec_de<S: Clone + 'static>(modele: &ModelRc<S>) -> &VecModel<S> {
    modele
        .as_any()
        .downcast_ref::<VecModel<S>>()
        .expect("modèle installé par installer_modeles")
}

pub fn installer_modeles(f: &Principale) {
    let g = f.global::<DonneesCoquille>();
    g.set_serveurs(ModelRc::new(VecModel::<LigneServeur>::default()));
    g.set_salons(ModelRc::new(VecModel::<LigneSalon>::default()));
    g.set_prives(ModelRc::new(VecModel::<LignePrive>::default()));
    g.set_sections(ModelRc::new(VecModel::<LigneSection>::default()));
    g.set_rattrapage(ModelRc::new(VecModel::<LigneRattrapage>::default()));
    g.set_vocaux(ModelRc::new(VecModel::<LigneVocal>::default()));
    g.set_contacts(ModelRc::new(VecModel::<LigneContact>::default()));
    g.set_membres(ModelRc::new(VecModel::<LigneMembre>::default()));
    f.global::<DonneesMessagerie>()
        .set_lignes(ModelRc::new(VecModel::<LigneMessage>::default()));
    crate::vocal::pont::installer_modeles(f);
}

pub fn onglet_effectif(voulu: OngletMarge, membres: bool, epingles: bool) -> Option<OngletMarge> {
    if voulu == OngletMarge::Epingles && epingles {
        Some(OngletMarge::Epingles)
    } else if membres {
        Some(OngletMarge::Membres)
    } else {
        None
    }
}

pub fn vider_modeles(f: &Principale) {
    f.global::<crate::DonneesParametres>().set_ouverte(false);
    let g = f.global::<DonneesCoquille>();
    vec_de(&g.get_serveurs()).set_vec(Vec::new());
    vec_de(&g.get_salons()).set_vec(Vec::new());
    vec_de(&g.get_prives()).set_vec(Vec::new());
    vec_de(&g.get_sections()).set_vec(Vec::new());
    vec_de(&g.get_rattrapage()).set_vec(Vec::new());
    vec_de(&g.get_vocaux()).set_vec(Vec::new());
    vec_de(&g.get_contacts()).set_vec(Vec::new());
    vec_de(&g.get_membres()).set_vec(Vec::new());
    g.set_marge(EnTeteMarge::default());
    g.set_marge_disponible(false);
    g.set_membres_disponibles(false);
    g.set_epingles_disponibles(false);
    g.set_carte_ouverte(false);
    g.set_filtre_membres("".into());
    g.set_defilement_membres(0.0);
    g.set_panneau(EnTetePanneau::default());
    g.set_vue(VueContenu::default());
    g.set_accueil(Accueil::default());
    g.set_bandeau(crate::EtatBandeau::Aucun);
    g.set_pret(false);
    g.set_pseudo("".into());
    g.set_initiales("".into());
    g.set_portrait(Image::default());
    let profil = f.global::<crate::DonneesProfil>();
    profil.set_recadrage_ouvert(false);
    profil.set_source(Image::default());
    profil.set_portrait(Image::default());
    profil.set_a_avatar(false);
    profil.set_occupe("".into());
    profil.set_erreur("".into());
    profil.set_serveurs(ModelRc::new(VecModel::from(Vec::new())));
    g.set_presence(EtatPresence::EnLigne);
    g.set_statut("En ligne".into());
    crate::messagerie::pont::liberer_modele();
    let m = f.global::<DonneesMessagerie>();
    vec_de(&m.get_lignes()).set_vec(Vec::new());
    m.set_active(false);
    m.set_haut(0.0);
    m.set_bas(0.0);
    m.set_defilement(0.0);
    m.set_etat(EtatConversation::Chargement);
    m.set_bouton_bas(false);
    m.set_nouveaux(0);
    m.set_saisie("".into());
    m.set_lecture_seule(false);
    crate::vocal::pont::vider_modeles(f);
    f.global::<crate::DonneesRecherche>().set_ouvert(false);
    f.global::<crate::DonneesNouveauPrive>().set_ouvert(false);
    f.global::<crate::DonneesCreerRejoindre>().set_ouvert(false);
    f.global::<crate::DonneesCreationSalon>().set_ouvert(false);
    f.global::<crate::DonneesReglagesServeur>()
        .set_ouverte(false);
}

fn image_de(icones: &BTreeMap<String, (i64, Icone)>, serveur: &str, version: i64) -> Option<Image> {
    icones
        .get(serveur)
        .filter(|(v, _)| *v == version && version > 0)
        .map(|(_, tampon)| Image::from_rgba8(tampon.clone()))
}

fn ligne_serveur_slint(
    d: &DonneesServeur,
    icones: &BTreeMap<String, (i64, Icone)>,
) -> LigneServeur {
    let image = image_de(icones, &d.id, d.version_icone);
    LigneServeur {
        id: d.id.as_str().into(),
        nom: d.nom.as_str().into(),
        initiales: d.initiales.as_str().into(),
        a_image: image.is_some(),
        image: image.unwrap_or_default(),
        actif: d.actif,
        non_lus: d.non_lus.min(i64::from(i32::MAX)) as i32,
        mentions: d.mentions.min(i32::MAX as usize) as i32,
    }
}

fn ligne_salon_slint(d: &DonneesSalon) -> LigneSalon {
    LigneSalon {
        id: d.id.as_str().into(),
        nom: d.nom.as_str().into(),
        icone: d.icone.into(),
        titre_groupe: d.titre_groupe.into(),
        genre_groupe: d.genre_groupe.into(),
        actif: d.actif,
        non_lus: d.non_lus.min(i64::from(i32::MAX)) as i32,
        mentions: d.mentions.min(i32::MAX as usize) as i32,
        bientot: d.bientot,
        occupants: d.occupants.min(i32::MAX as usize) as i32,
        brouillon: d.brouillon,
        duree: d.duree.as_str().into(),
        vocal: d.vocal,
    }
}

fn ligne_prive_slint(d: &DonneesPrive) -> LignePrive {
    LignePrive {
        id: d.id.as_str().into(),
        pseudo: d.pseudo.as_str().into(),
        initiales: d.initiales.as_str().into(),
        teinte: d.teinte,
        actif: d.actif,
        non_lus: d.non_lus.min(i64::from(i32::MAX)) as i32,
        mentions: d.mentions.min(i32::MAX as usize) as i32,
        presence: presence_slint(d.presence),
        libelle_presence: super::membres::libelle_presence(d.presence).into(),
        brouillon: d.brouillon,
    }
}

fn ligne_section_slint(d: &DonneesSection) -> LigneSection {
    LigneSection {
        id: identifiant_de_section(d.section).into(),
        nom: nom_de_section(d.section).into(),
        icone: icone_de_section(d.section).into(),
        actif: d.actif,
    }
}

fn ligne_rattrapage_slint(d: &DonneesRattrapage) -> LigneRattrapage {
    LigneRattrapage {
        cle: d.cle.as_str().into(),
        titre: d.titre.as_str().into(),
        detail: d.detail.as_str().into(),
        icone: d.icone.into(),
        nombre: d.nombre.min(i64::from(i32::MAX)) as i32,
        mention: d.mention,
    }
}

fn ligne_vocal_slint(d: &DonneesVocal) -> LigneVocal {
    LigneVocal {
        cle: d.cle.as_str().into(),
        nom: d.nom.as_str().into(),
        lieu: d.lieu.as_str().into(),
        icone: d.icone.into(),
        occupants: d.occupants.min(i32::MAX as usize) as i32,
    }
}

fn panneau_slint(d: &DonneesPanneau, icones: &BTreeMap<String, (i64, Icone)>) -> EnTetePanneau {
    let image = d
        .serveur
        .as_deref()
        .and_then(|s| image_de(icones, s, d.version_icone));
    EnTetePanneau {
        genre: d.genre,
        titre: d.titre.as_str().into(),
        initiales: d.initiales.as_str().into(),
        a_image: image.is_some(),
        image: image.unwrap_or_default(),
        peut_creer_salon: d.peut_creer_salon,
    }
}

fn vue_slint(d: &DonneesVue) -> VueContenu {
    VueContenu {
        genre: d.genre,
        titre: d.titre.as_str().into(),
        sujet: d.sujet.as_str().into(),
        icone: d.icone.into(),
        message: d.message.as_str().into(),
        detail: d.detail.as_str().into(),
    }
}

fn ligne_contact_slint(d: &DonneesContact) -> LigneContact {
    LigneContact {
        id: d.id.as_str().into(),
        pseudo: d.pseudo.as_str().into(),
        initiales: d.initiales.as_str().into(),
        teinte: d.teinte,
        presence: presence_slint(d.presence),
        detail: d.detail.as_str().into(),
    }
}

fn accueil_slint(d: &DonneesAccueil) -> Accueil {
    Accueil {
        actif: d.actif,
        non_lus: d.non_lus.min(i64::from(i32::MAX)) as i32,
        mentions: d.mentions.min(i32::MAX as usize) as i32,
        date: d.date.as_str().into(),
        salut: d.salut.as_str().into(),
        resume: d.resume.as_str().into(),
        sans_serveur: d.sans_serveur,
        contacts: d.contacts.as_str().into(),
    }
}

fn tout() -> Cibles {
    Cibles {
        serveurs: Ensemble::Tous,
        salons: Ensemble::Tous,
        prives: Ensemble::Tous,
        accueil: true,
        panneau: true,
        vue: true,
        compte: true,
        corriger: true,
        membres: true,
    }
}

impl Pont {
    pub fn nouveau(
        store: Store,
        fenetre: slint::Weak<Principale>,
        runtime: tokio::runtime::Handle,
        memoire: Option<Memoire>,
        horloge: Horloge,
        navigation: Navigation,
        profils: Arc<Profils>,
    ) -> Self {
        let messagerie = PontMessagerie::nouveau(
            store.clone(),
            fenetre.clone(),
            runtime.clone(),
            profils.clone(),
        );
        let horloge_vocal = horloge.clone();
        let vocal = PontVocal::nouveau(
            store.clone(),
            fenetre.clone(),
            runtime.clone(),
            memoire.clone(),
            Arc::new(move || millisecondes(horloge_vocal())),
            profils.clone(),
        );
        Self {
            interne: Arc::new(Mutex::new(Interne {
                navigation,
                ombres: Ombres::default(),
                sections: Vec::new(),
                reseau: EtatReseau::Connecte,
                deja_connecte: false,
                icones: BTreeMap::new(),
                demandes: BTreeSet::new(),
                membres: EtatMembres::default(),
                onglet_voulu: OngletMarge::Membres,
                avant_epingles: None,
                recherche: None,
                nouveau_prive: None,
                profil: profil::EtatProfil::default(),
                creation: creation::EtatCreation::default(),
                reglages: None,
            })),
            store,
            fenetre,
            runtime,
            memoire,
            horloge,
            profils,
            messagerie,
            vocal,
        }
    }

    pub fn messagerie(&self) -> &PontMessagerie {
        &self.messagerie
    }

    pub fn vocal(&self) -> &PontVocal {
        &self.vocal
    }

    fn verrou(&self) -> MutexGuard<'_, Interne> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn route(&self) -> Route {
        self.verrou().navigation.route().clone()
    }

    pub fn demarrer(&self, f: &Principale) {
        let route = self.route();
        self.naviguer(f, route);
    }

    pub fn tout_projeter(&self, f: &Principale) {
        self.projeter(f, &tout(), true);
        self.projeter_parametres(f);
        self.projeter_profil(f);
        f.global::<DonneesCoquille>().set_pret(true);
    }

    pub fn appliquer(&self, f: &Principale, changements: &[Changement]) {
        let mut c = {
            let interne = self.verrou();
            cibles(changements, interne.navigation.route())
        };
        let mut corrigee = false;
        if c.corriger {
            corrigee = self
                .store
                .lire(|etat| self.verrou().navigation.corriger(etat));
            if corrigee {
                c = tout();
            }
        }
        self.projeter(f, &c, false);
        let moi = self.store.lire(|e| e.moi().to_string());
        if parametres::touche_les_notifications(changements, &moi) {
            self.projeter_parametres(f);
        }
        if profil::touche_le_profil(changements, &moi) {
            self.projeter_profil(f);
        }
        self.changements_des_reglages(f, changements);
        if corrigee {
            self.suivre_conversation(f);
            self.ouvrir_conversation_courante(None);
        }
        self.messagerie.appliquer(f, changements);
        self.vocal.changements(f, changements);
    }

    pub fn rafraichir_brouillons(&self, f: &Principale) {
        let c = Cibles {
            salons: Ensemble::Tous,
            prives: Ensemble::Tous,
            ..Cibles::default()
        };
        self.projeter(f, &c, false);
    }

    fn suivre_conversation(&self, f: &Principale) {
        let route = self.route();
        let (cle, non_lus) = self.store.lire(|etat| {
            let cle = conversation_de_la_route(etat, &route);
            let non_lus = cle.as_ref().map_or(0, |c| etat.non_lus_de(c));
            (cle, non_lus)
        });
        let serveur = serveur_courant(&route).map(str::to_string);
        let avant = self.messagerie.conversations_en_brouillon();
        self.messagerie.ouvrir(f, cle, serveur, non_lus);
        if self.messagerie.conversations_en_brouillon() != avant {
            self.rafraichir_brouillons(f);
        }
        self.actualiser_marge(f);
    }

    fn projeter(&self, f: &Principale, c: &Cibles, sections: bool) {
        let g = f.global::<DonneesCoquille>();
        let maintenant = (self.horloge)();
        let mut guard = self.verrou();
        let interne = &mut *guard;
        let route = interne.navigation.route().clone();
        let route_vocal = route.clone();
        let Interne {
            ombres,
            icones,
            sections: liste_sections,
            ..
        } = interne;
        let brouillons = self.messagerie.conversations_en_brouillon();
        let privees = if c.prives.est_vide() {
            Vec::new()
        } else {
            self.store.lignes_privees()
        };
        self.store.lire(|etat| {
            let serveurs = g.get_serveurs();
            let modele = vec_de(&serveurs);
            let convertir = |d: &DonneesServeur| ligne_serveur_slint(d, icones);
            let mut complet = c.serveurs == Ensemble::Tous;
            if let Ensemble::Certains(ids) = &c.serveurs {
                for id in ids {
                    match (
                        ombres.serveurs.iter().position(|d| d.id == *id),
                        ligne_serveur(etat, id, &route),
                    ) {
                        (Some(i), Some(ligne)) => {
                            remplacer_ligne(modele, &mut ombres.serveurs, i, ligne, convertir);
                        }
                        _ => complet = true,
                    }
                }
            }
            if complet {
                synchroniser(
                    modele,
                    &mut ombres.serveurs,
                    lignes_serveurs(etat, &route),
                    |d| d.id.clone(),
                    convertir,
                );
            }
            if !c.salons.est_vide() {
                let mut salons = lignes_salons(etat, &route, millisecondes(maintenant));
                marquer_brouillons(&mut salons, &mut [], |c| brouillons.contains(c));
                synchroniser(
                    vec_de(&g.get_salons()),
                    &mut ombres.salons,
                    salons,
                    |d| d.id.clone(),
                    ligne_salon_slint,
                );
            }
            if !c.prives.est_vide() {
                let mut prives = lignes_prives(&privees, &route);
                marquer_brouillons(&mut [], &mut prives, |c| brouillons.contains(c));
                synchroniser(
                    vec_de(&g.get_prives()),
                    &mut ombres.prives,
                    prives,
                    |d| d.id.clone(),
                    ligne_prive_slint,
                );
            }
            if sections {
                synchroniser(
                    vec_de(&g.get_sections()),
                    &mut ombres.sections,
                    lignes_sections(liste_sections, &route),
                    |d| d.section,
                    ligne_section_slint,
                );
            }
            if c.accueil {
                let accueil = donnees_accueil(etat, &route, maintenant);
                if ombres.accueil.as_ref() != Some(&accueil) {
                    g.set_accueil(accueil_slint(&accueil));
                    ombres.accueil = Some(accueil);
                }
                synchroniser(
                    vec_de(&g.get_rattrapage()),
                    &mut ombres.rattrapage,
                    lignes_rattrapage(etat),
                    |d| d.cle.clone(),
                    ligne_rattrapage_slint,
                );
                synchroniser(
                    vec_de(&g.get_vocaux()),
                    &mut ombres.vocaux,
                    lignes_vocaux(etat),
                    |d| d.cle.clone(),
                    ligne_vocal_slint,
                );
                synchroniser(
                    vec_de(&g.get_contacts()),
                    &mut ombres.contacts,
                    lignes_contacts(etat),
                    |d| d.id.clone(),
                    ligne_contact_slint,
                );
            }
            if c.panneau {
                let panneau = donnees_panneau(etat, &route);
                if ombres.panneau.as_ref() != Some(&panneau) {
                    g.set_panneau(panneau_slint(&panneau, icones));
                    ombres.panneau = Some(panneau);
                }
            }
            if c.vue {
                let vue = donnees_vue(etat, &route);
                if ombres.vue.as_ref() != Some(&vue) {
                    g.set_vue(vue_slint(&vue));
                    ombres.vue = Some(vue);
                }
            }
            if c.compte {
                let compte = donnees_compte(etat);
                if ombres.compte.as_ref() != Some(&compte) {
                    g.set_pseudo(compte.pseudo.as_str().into());
                    g.set_initiales(initiales(&compte.pseudo).into());
                    g.set_presence(presence_slint(compte.presence));
                    g.set_statut(compte.statut.as_str().into());
                    ombres.compte = Some(compte);
                }
            }
        });
        let a_demander: Vec<(String, i64)> = interne
            .ombres
            .serveurs
            .iter()
            .filter(|d| d.version_icone > 0)
            .filter(|d| {
                interne
                    .icones
                    .get(&d.id)
                    .is_none_or(|(v, _)| *v != d.version_icone)
            })
            .map(|d| (d.id.clone(), d.version_icone))
            .filter(|cle| interne.demandes.insert(cle.clone()))
            .collect();
        drop(guard);
        for (serveur, version) in a_demander {
            self.telecharger_icone(serveur, version);
        }
        if c.membres {
            self.projeter_membres(f);
        }
        self.vocal.definir_route(&route_vocal);
        self.vocal.projeter(f);
    }

    fn telecharger_icone(&self, serveur: String, version: i64) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            let Ok(octets) = pont.store.client().icone_de_serveur(&serveur).await else {
                return;
            };
            if let Some(tampon) = decoder_icone(&octets) {
                pont.installer_icone_recue(serveur, version, tampon);
            }
        });
    }

    fn installer_icone_recue(&self, serveur: String, version: i64, tampon: Icone) {
        self.verrou()
            .icones
            .insert(serveur.clone(), (version, tampon));
        let pont = self.clone();
        let _ = self
            .fenetre
            .upgrade_in_event_loop(move |f| pont.icone_recue(&f, &serveur));
    }

    pub fn installer_icone(&self, f: &Principale, serveur: &str, version: i64, tampon: Icone) {
        self.verrou()
            .icones
            .insert(serveur.to_string(), (version, tampon));
        self.icone_recue(f, serveur);
    }

    fn icone_recue(&self, f: &Principale, serveur: &str) {
        let g = f.global::<DonneesCoquille>();
        let interne = self.verrou();
        if let Some(i) = interne.ombres.serveurs.iter().position(|d| d.id == serveur) {
            vec_de(&g.get_serveurs()).set_row_data(
                i,
                ligne_serveur_slint(&interne.ombres.serveurs[i], &interne.icones),
            );
        }
        if let Some(panneau) = interne
            .ombres
            .panneau
            .as_ref()
            .filter(|p| p.serveur.as_deref() == Some(serveur))
        {
            g.set_panneau(panneau_slint(panneau, &interne.icones));
        }
        let reglages = interne
            .reglages
            .as_ref()
            .is_some_and(|r| r.serveur == serveur);
        drop(interne);
        if reglages {
            self.projeter_reglages(f);
        }
    }

    pub fn naviguer(&self, f: &Principale, route: Route) {
        self.aller(f, route, None);
    }

    fn aller(&self, f: &Principale, route: Route, message: Option<String>) {
        f.global::<crate::DonneesParametres>().set_ouverte(false);
        self.oublier_reglages_serveur(f);
        let adresse = self.store.lire(|etat| {
            let mut interne = self.verrou();
            interne.navigation.aller(route, etat);
            interne.navigation.corriger(etat);
            interne.navigation.adresse_a_retenir(etat)
        });
        self.tout_projeter(f);
        self.suivre_conversation(f);
        if let (Some(adresse), Some(memoire)) = (adresse, &self.memoire) {
            let vue = self.store.memoriser_vue(&adresse);
            memoire.modifier(|p| p.derniere_vue = Some(vue));
        }
        self.ouvrir_conversation_courante(message);
    }

    fn ouvrir_conversation_courante(&self, message: Option<String>) {
        let route = self.route();
        let cle = self
            .store
            .lire(|etat| conversation_de_la_route(etat, &route));
        let store = self.store.clone();
        let deja = store.lire(|etat| etat.courante == cle);
        if deja && message.is_none() {
            return;
        }
        let messagerie = self.messagerie.clone();
        let fenetre = self.fenetre.clone();
        self.runtime.spawn(async move {
            if !deja {
                store.ouvrir(cle.clone()).await;
            }
            let Some(cle) = cle else {
                return;
            };
            if !deja {
                let reussi = store.conversation(&cle).is_some_and(|c| c.chargee);
                let c = cle.clone();
                let _ = fenetre
                    .upgrade_in_event_loop(move |f| messagerie.etat_chargement(&f, &c, reussi));
            }
            if let Some(message) = message {
                let _ = store.sauter_a(&cle, &message).await;
            }
        });
    }

    pub fn ouvrir_accueil(&self, f: &Principale) {
        self.naviguer(f, Route::Accueil);
    }

    pub fn ouvrir_serveur(&self, f: &Principale, serveur: &str) {
        let route = self
            .store
            .lire(|etat| self.verrou().navigation.route_du_serveur(serveur, etat));
        self.naviguer(f, route);
    }

    pub fn ouvrir_salon(&self, f: &Principale, salon: &str) {
        let Some(serveur) = serveur_courant(&self.route()).map(str::to_string) else {
            return;
        };
        self.naviguer(
            f,
            Route::Salon {
                serveur,
                salon: Some(salon.to_string()),
                message: None,
            },
        );
        let vocal = self.store.lire(|etat| {
            etat.salon(salon)
                .is_some_and(|(_, s)| estompe_client::store::est_vocal(s.kind))
        });
        if vocal {
            self.vocal.rejoindre(salon);
        }
    }

    pub fn ouvrir_salon_de_son_serveur(&self, f: &Principale, salon: &str) {
        let Some(serveur) = self.store.lire(|etat| etat.serveur_du_salon(salon)) else {
            return;
        };
        if matches!(self.route(), Route::Salon { salon: Some(courant), .. } if courant == salon) {
            return;
        }
        self.naviguer(
            f,
            Route::Salon {
                serveur,
                salon: Some(salon.to_string()),
                message: None,
            },
        );
    }

    pub fn ouvrir_prive(&self, f: &Principale, prive: &str) {
        self.naviguer(
            f,
            Route::MessagePrive {
                id: prive.to_string(),
                message: None,
            },
        );
    }

    pub fn ouvrir_section(&self, f: &Principale, identifiant: &str) {
        if let Some(section) = section_de_l_identifiant(identifiant) {
            self.naviguer(f, route_de_section(section));
        }
    }

    pub fn ouvrir_conversation(&self, f: &Principale, cle: &str) {
        let cle = cle.split('#').next().unwrap_or_default();
        let route = CleConversation::analyser(cle).and_then(|cle| {
            self.store
                .lire(|etat| Navigation::route_de_la_conversation(etat, &cle))
        });
        if let Some(route) = route {
            self.naviguer(f, route);
        }
    }

    pub fn deplacer_serveur(&self, de: &str, vers: &str) {
        let ordre = self.store.deplacer_serveur(de, vers);
        if let Some(memoire) = &self.memoire {
            memoire.modifier(|p| p.ordre_serveurs = ordre);
        }
    }

    pub fn serveur_voisin(&self, f: &Principale, suivant: bool) {
        let route = self.route();
        let sens = if suivant {
            Sens::Suivant
        } else {
            Sens::Precedent
        };
        let courant = match &route {
            Route::Salon { serveur, .. } => Some(serveur.as_str()),
            Route::Accueil | Route::MessagePrive { .. } => Some(""),
            _ => None,
        };
        match self.store.serveur_voisin(courant, sens).as_deref() {
            Some("") => self.ouvrir_accueil(f),
            Some(serveur) => self.ouvrir_serveur(f, serveur),
            None => {}
        }
    }

    pub fn definir_reseau(&self, f: &Principale, reseau: EtatReseau) {
        let etat = {
            let mut interne = self.verrou();
            interne.reseau = reseau;
            interne.deja_connecte |= reseau == EtatReseau::Connecte;
            bandeau(interne.reseau, interne.deja_connecte)
        };
        f.global::<DonneesCoquille>().set_bandeau(etat);
    }

    pub fn definir_sections(&self, f: &Principale, sections: Vec<Section>) {
        self.verrou().sections = sections;
        self.projeter(f, &Cibles::default(), true);
    }

    pub fn rafraichir_accueil(&self, f: &Principale) {
        self.projeter(
            f,
            &Cibles {
                accueil: true,
                ..Cibles::default()
            },
            false,
        );
    }

    pub fn durees_vocales(&self) -> bool {
        self.verrou()
            .ombres
            .salons
            .iter()
            .any(|d| !d.duree.is_empty())
    }

    pub fn rafraichir_durees(&self, f: &Principale) {
        self.projeter(
            f,
            &Cibles {
                salons: Ensemble::Tous,
                ..Cibles::default()
            },
            false,
        );
    }

    pub fn profils(&self) -> &Arc<Profils> {
        &self.profils
    }

    pub fn operations_membres(&self) -> Vec<(Operation, String)> {
        self.verrou().membres.operations.clone()
    }

    pub fn fenetre_membres(&self) -> Fenetre {
        self.verrou().membres.fenetre
    }

    fn maintenant_ms(&self) -> i64 {
        millisecondes((self.horloge)())
    }

    fn projeter_membres(&self, f: &Principale) {
        let g = f.global::<DonneesCoquille>();
        let route = self.route();
        let maintenant = self.maintenant_ms();
        let mut guard = self.verrou();
        let m = &mut guard.membres;
        let serveur = self.store.lire(|etat| {
            serveur_courant(&route)
                .filter(|s| etat.serveur(s).is_some())
                .map(str::to_string)
        });
        if m.serveur != serveur {
            m.serveur = serveur.clone();
            m.filtre.clear();
            m.defilement = 0.0;
            m.carte = None;
            m.ombre_carte = None;
            g.set_filtre_membres("".into());
            g.set_defilement_membres(0.0);
            g.set_carte_ouverte(false);
        }
        let (lignes, marge) = self.store.lire(|etat| match serveur.as_deref() {
            Some(serveur) => {
                let lignes = lignes_membres(etat, serveur, &m.filtre, maintenant);
                let marge = donnees_marge(etat, serveur, &lignes);
                (lignes, marge)
            }
            None => (Vec::new(), DonneesMarge::default()),
        });
        if m.marge.as_ref() != Some(&marge) {
            g.set_marge(EnTeteMarge {
                titre: marge.titre.as_str().into(),
                filtrable: marge.filtrable,
                vide: marge.vide && serveur.is_some(),
            });
            m.marge = Some(marge);
        }
        let modele_rc = g.get_membres();
        let modele = vec_de(&modele_rc);
        let profils = &self.profils;
        let EtatMembres {
            ombre,
            index,
            avec_portrait,
            operations,
            ..
        } = m;
        let anciennes: Vec<String> = ombre.iter().map(|d| d.cle.clone()).collect();
        let faites = synchroniser(
            modele,
            ombre,
            lignes,
            |d| d.cle.clone(),
            |d| {
                let portrait = if avec_portrait.contains(&d.cle) {
                    d.avatar.as_ref().and_then(|c| profils.avatar(c))
                } else {
                    None
                };
                ligne_membre_slint(d, portrait, profils)
            },
        );
        if faites.iter().any(|o| !matches!(o, Operation::Remplacer(_)))
            || index.len() != ombre.len()
        {
            *index = ombre
                .iter()
                .enumerate()
                .map(|(i, d)| (d.cle.clone(), i))
                .collect();
        }
        for operation in &faites {
            if let Operation::Inserer(i) | Operation::Remplacer(i) = *operation {
                let porte = modele
                    .row_data(i)
                    .is_some_and(|l| l.portrait.size().width > 0);
                if porte {
                    avec_portrait.insert(ombre[i].cle.clone());
                } else {
                    avec_portrait.remove(&ombre[i].cle);
                }
            }
        }
        if !faites.is_empty() {
            *operations = cles_des_operations(anciennes, ombre, &faites);
        }
        drop(guard);
        self.actualiser_marge(f);
        self.rafraichir_fenetre(f);
        self.projeter_carte(f);
    }

    fn actualiser_marge(&self, f: &Principale) {
        let g = f.global::<DonneesCoquille>();
        let (membres, voulu) = {
            let i = self.verrou();
            (i.membres.serveur.is_some(), i.onglet_voulu)
        };
        let epingles = self.messagerie.conversation().is_some();
        let onglet = onglet_effectif(voulu, membres, epingles);
        if g.get_membres_disponibles() != membres {
            g.set_membres_disponibles(membres);
        }
        if g.get_epingles_disponibles() != epingles {
            g.set_epingles_disponibles(epingles);
        }
        if g.get_marge_disponible() != onglet.is_some() {
            g.set_marge_disponible(onglet.is_some());
        }
        if let Some(onglet) = onglet.filter(|o| *o != g.get_onglet_marge()) {
            g.set_onglet_marge(onglet);
        }
        let visibles = g.get_marge_ouverte() && onglet == Some(OngletMarge::Epingles);
        self.messagerie.montrer_epingles(f, visibles);
    }

    fn regler_marge(&self, f: &Principale, ouverte: bool) {
        f.global::<DonneesCoquille>().set_marge_ouverte(ouverte);
        if let Some(memoire) = &self.memoire {
            memoire.modifier(|p| p.marge_ouverte = ouverte);
        }
    }

    pub fn choisir_onglet_marge(&self, f: &Principale, onglet: OngletMarge) {
        {
            let mut i = self.verrou();
            i.onglet_voulu = onglet;
            i.avant_epingles = None;
        }
        self.regler_marge(f, true);
        self.actualiser_marge(f);
        self.rafraichir_fenetre(f);
    }

    pub fn basculer_epingles(&self, f: &Principale) {
        let g = f.global::<DonneesCoquille>();
        let visible = g.get_marge_disponible() && g.get_marge_ouverte();
        let actuel = g.get_onglet_marge();
        let ouvrir = {
            let mut i = self.verrou();
            if visible && actuel == OngletMarge::Epingles {
                let avant = i.avant_epingles.take();
                i.onglet_voulu = avant.map_or(OngletMarge::Membres, |(_, o)| o);
                avant.is_some_and(|(v, _)| v)
            } else {
                i.avant_epingles = Some((visible, actuel));
                i.onglet_voulu = OngletMarge::Epingles;
                true
            }
        };
        self.regler_marge(f, ouvrir);
        self.actualiser_marge(f);
        self.rafraichir_fenetre(f);
    }

    fn rafraichir_fenetre(&self, f: &Principale) {
        let g = f.global::<DonneesCoquille>();
        let ouverte = g.get_marge_disponible() && g.get_marge_ouverte();
        let modele_rc = g.get_membres();
        let modele = vec_de(&modele_rc);
        let mut guard = self.verrou();
        let m = &mut guard.membres;
        let fenetre =
            fenetre_des_membres(m.ombre.len(), m.defilement, m.vue.unwrap_or(VUE_PAR_DEFAUT));
        m.fenetre = fenetre;
        let peut_demander = m.vue.is_some();
        let dedans = |i: usize| i >= fenetre.debut && i < fenetre.fin;
        let a_vider: Vec<String> = m
            .avec_portrait
            .iter()
            .filter(|cle| m.index.get(*cle).is_none_or(|&i| !dedans(i)))
            .cloned()
            .collect();
        for cle in a_vider {
            m.avec_portrait.remove(&cle);
            if let Some(d) = m
                .index
                .get(&cle)
                .and_then(|&i| m.ombre.get(i).map(|d| (i, d)))
            {
                let (i, d) = d;
                modele.set_row_data(i, ligne_membre_slint(d, None, &self.profils));
            }
        }
        if !ouverte {
            return;
        }
        let mut avatars = Vec::new();
        let mut badges = Vec::new();
        for i in fenetre.debut..fenetre.fin.min(m.ombre.len()) {
            let d = &m.ombre[i];
            if d.en_tete {
                continue;
            }
            if let Some(cle) = &d.avatar {
                if !m.avec_portrait.contains(&d.cle) {
                    match self.profils.avatar(cle) {
                        Some(image) => {
                            modele
                                .set_row_data(i, ligne_membre_slint(d, Some(image), &self.profils));
                            m.avec_portrait.insert(d.cle.clone());
                        }
                        None => {
                            if peut_demander
                                && !m.echecs_avatars.contains(cle)
                                && m.demandes_avatars.insert(cle.clone())
                            {
                                avatars.push(cle.clone());
                            }
                        }
                    }
                }
            }
            if peut_demander
                && self.profils.badges(&d.utilisateur).is_none()
                && m.demandes_badges.insert(d.utilisateur.clone())
            {
                badges.push(d.utilisateur.clone());
            }
        }
        drop(guard);
        for cle in avatars {
            self.telecharger_avatar(cle);
        }
        for utilisateur in badges {
            self.telecharger_badges(utilisateur);
        }
    }

    fn telecharger_avatar(&self, cle: CleAvatar) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            let recu = pont.profils.charger_avatar(cle.clone()).await.is_some();
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.avatar_recu(&f, cle, recu));
        });
    }

    fn avatar_recu(&self, f: &Principale, cle: CleAvatar, recu: bool) {
        {
            let mut guard = self.verrou();
            let m = &mut guard.membres;
            m.demandes_avatars.remove(&cle);
            if !recu {
                m.echecs_avatars.insert(cle);
            }
        }
        self.rafraichir_fenetre(f);
        self.projeter_carte(f);
    }

    fn telecharger_badges(&self, utilisateur: String) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            pont.profils.charger_badges(utilisateur.clone()).await;
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.badges_recus(&f, &utilisateur));
        });
    }

    fn badges_recus(&self, f: &Principale, utilisateur: &str) {
        {
            let g = f.global::<DonneesCoquille>();
            let mut guard = self.verrou();
            let m = &mut guard.membres;
            m.demandes_badges.remove(utilisateur);
            let cle = format!("m:{utilisateur}");
            if let Some(&i) = m.index.get(&cle) {
                if let Some(d) = m.ombre.get(i) {
                    let portrait = if m.avec_portrait.contains(&cle) {
                        d.avatar.as_ref().and_then(|c| self.profils.avatar(c))
                    } else {
                        None
                    };
                    vec_de(&g.get_membres())
                        .set_row_data(i, ligne_membre_slint(d, portrait, &self.profils));
                }
            }
        }
        self.projeter_carte(f);
    }

    pub fn membres_defiles(&self, f: &Principale, defilement: f32, vue: f32) {
        {
            let mut guard = self.verrou();
            let m = &mut guard.membres;
            m.defilement = defilement.max(0.0);
            if vue > 0.0 {
                m.vue = Some(vue);
            }
        }
        self.rafraichir_fenetre(f);
    }

    pub fn marge_basculee(&self, f: &Principale) {
        self.verrou().avant_epingles = None;
        self.actualiser_marge(f);
        self.rafraichir_fenetre(f);
    }

    pub fn filtrer_membres(&self, f: &Principale, filtre: &str) {
        self.verrou().membres.filtre = filtre.to_string();
        self.projeter_membres(f);
    }

    pub fn ouvrir_membre(&self, f: &Principale, utilisateur: &str) {
        self.verrou().membres.carte = Some(utilisateur.to_string());
        self.projeter_carte(f);
    }

    pub fn fermer_carte(&self, f: &Principale) {
        {
            let mut guard = self.verrou();
            guard.membres.carte = None;
            guard.membres.ombre_carte = None;
        }
        f.global::<DonneesCoquille>().set_carte_ouverte(false);
    }

    fn projeter_carte(&self, f: &Principale) {
        let g = f.global::<DonneesCoquille>();
        let maintenant = self.maintenant_ms();
        let mut guard = self.verrou();
        let m = &mut guard.membres;
        let (Some(utilisateur), Some(serveur)) = (m.carte.clone(), m.serveur.clone()) else {
            return;
        };
        let Some(donnees) = self
            .store
            .lire(|etat| donnees_carte(etat, &serveur, &utilisateur, maintenant))
        else {
            m.carte = None;
            m.ombre_carte = None;
            drop(guard);
            g.set_carte_ouverte(false);
            return;
        };
        let portrait = donnees.avatar.as_ref().and_then(|c| self.profils.avatar(c));
        let badges = self.profils.badges(&utilisateur);
        let mut avatar = None;
        if let (None, Some(cle)) = (&portrait, &donnees.avatar) {
            if !m.echecs_avatars.contains(cle) && m.demandes_avatars.insert(cle.clone()) {
                avatar = Some(cle.clone());
            }
        }
        let demander_badges = badges.is_none() && m.demandes_badges.insert(utilisateur.clone());
        g.set_carte(carte_slint(
            &donnees,
            portrait,
            badges.as_deref(),
            &self.profils,
        ));
        m.ombre_carte = Some(donnees);
        drop(guard);
        if !g.get_carte_ouverte() {
            g.set_carte_ouverte(true);
        }
        if let Some(cle) = avatar {
            self.telecharger_avatar(cle);
        }
        if demander_badges {
            self.telecharger_badges(utilisateur);
        }
    }

    pub fn nombre_de_lignes(f: &Principale) -> (usize, usize, usize) {
        let g = f.global::<DonneesCoquille>();
        (
            g.get_serveurs().row_count(),
            g.get_salons().row_count(),
            g.get_prives().row_count(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_onglet_de_la_marge_suit_le_web() {
        use OngletMarge::{Epingles, Membres};
        assert_eq!(onglet_effectif(Epingles, true, true), Some(Epingles));
        assert_eq!(onglet_effectif(Epingles, true, false), Some(Membres));
        assert_eq!(onglet_effectif(Membres, true, true), Some(Membres));
        assert_eq!(onglet_effectif(Epingles, false, true), Some(Epingles));
        assert_eq!(onglet_effectif(Membres, false, true), None);
        assert_eq!(onglet_effectif(Membres, false, false), None);
    }

    fn png(largeur: u32, hauteur: u32) -> Vec<u8> {
        let image =
            image::RgbaImage::from_pixel(largeur, hauteur, image::Rgba([208, 104, 138, 255]));
        let mut octets = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut octets, image::ImageFormat::Png)
            .unwrap();
        octets.into_inner()
    }

    #[test]
    fn decode_l_icone_du_serveur_et_reduit_les_grandes_images() {
        let petite = decoder_icone(&png(64, 32)).unwrap();
        assert_eq!((petite.width(), petite.height()), (64, 32));
        assert_eq!(petite.as_slice()[0], Rgba8Pixel::new(208, 104, 138, 255));
        let grande = decoder_icone(&png(512, 512)).unwrap();
        assert_eq!((grande.width(), grande.height()), (COTE_ICONE, COTE_ICONE));
        assert!(decoder_icone(b"pas une image").is_none());
    }
}
