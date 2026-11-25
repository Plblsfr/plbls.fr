use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::appels::NouveauMessage;
use estompe_client::http::ErreurApi;
use estompe_client::journal::{Journal, Niveau};
use estompe_client::lecteur::Origine;
use estompe_client::regarder_ensemble::{
    GenreSeance, LecteurFichier, LecteurSeance, LecteurYoutube, LienVue,
};
use estompe_client::route::Route;
use estompe_client::store::{Changement, CleConversation, GenreSalon, Store};
use estompe_client::videos::solo::mode_solo;
use estompe_client::videos::{
    self as v, ElementSolo, FabriqueSolo, FileSeance, Filtre, FiltreVues, Lancement, ModeSolo,
    PlaceSolo, PostVideo, Rsvp, Solo, TableauVideos, Tri,
};
use slint::{ComponentHandle, Image, Model, ModelRc, VecModel};

use crate::confirmation::{self, Demande, Saisie};
use crate::coquille::avatars::{decoder_image, Image as Pixels};
use crate::menu::{self, Ancre, Entree};
use crate::messagerie::pont::Navigateur;
use crate::regarder_ensemble::PontSeance;
use crate::video::lecteur::GestionLecteur;
use crate::video::youtube;
use crate::{
    CarteVideo, ChoixPlanVideo, DonneesCoquille, DonneesVideos, DonneesVocal, ElementListeVideo,
    ListeVideoVue, OngletVideos, PlanSeanceVideo, Principale, ProchaineSeanceVideo, SeanceVideoVue,
    VueSoloVideo, VueVideos,
};

pub const ORIGINE: &str = "solo";
const SOURCE_JOURNAL: &str = "videos";
const HAUTEUR_BOUTON: f32 = 28.0;
const BATTEMENT: Duration = Duration::from_millis(250);
const COTE_MINIATURE: u32 = 480;
const RAPPEL_DEFAUT: i64 = 10;
const LIENS_MAX: usize = 50;

pub type Horloge = Arc<dyn Fn() -> i64 + Send + Sync>;
pub type Decalage = Arc<dyn Fn() -> i32 + Send + Sync>;

#[derive(Clone)]
pub struct OutilsVideos {
    pub lecteur: Option<Arc<GestionLecteur>>,
    pub navigateur: Navigateur,
    pub journal: Option<Arc<Journal>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Lieu {
    serveur: String,
    salon: String,
}

#[derive(Debug, Clone, Default)]
struct Plan {
    titre: String,
    date: String,
    salon_vocal: String,
    rappel: i64,
    liste: Option<String>,
    choisis: Vec<String>,
    erreur: String,
    occupe: bool,
}

struct Interne {
    outils: Option<OutilsVideos>,
    lieu: Option<Lieu>,
    onglet: OngletVideos,
    grille: bool,
    filtre: Filtre,
    tri: Tri,
    demandees: HashSet<String>,
    avis: String,
    plan: Option<Plan>,
    place: PlaceSolo,
    solo_id: String,
}

impl Default for Interne {
    fn default() -> Self {
        Self {
            outils: None,
            lieu: None,
            onglet: OngletVideos::Videos,
            grille: true,
            filtre: Filtre::default(),
            tri: Tri::Recentes,
            demandees: HashSet::new(),
            avis: String::new(),
            plan: None,
            place: PlaceSolo::Aucune,
            solo_id: String::new(),
        }
    }
}

thread_local! {
    static MINIATURES: RefCell<HashMap<String, Image>> = RefCell::new(HashMap::new());
    static MINUTEUR: RefCell<Option<(usize, slint::Timer)>> = const { RefCell::new(None) };
}

pub fn installer_miniature(id: &str, image: Image) {
    MINIATURES.with(|m| m.borrow_mut().insert(id.to_string(), image));
}

fn miniature(id: &str) -> Option<Image> {
    MINIATURES.with(|m| m.borrow().get(id).cloned())
}

fn origine() -> Origine {
    Origine::Autre(ORIGINE.to_string())
}

fn ancre(x: f32, y: f32) -> Ancre {
    Ancre::Element {
        x,
        haut: y - HAUTEUR_BOUTON,
        bas: y,
    }
}

fn poser<T: Clone + PartialEq + 'static>(
    actuel: ModelRc<T>,
    nouveau: Vec<T>,
    poser: impl FnOnce(ModelRc<T>),
) {
    let identique = actuel.row_count() == nouveau.len()
        && actuel.iter().zip(nouveau.iter()).all(|(a, b)| &a == b);
    if !identique {
        poser(ModelRc::new(VecModel::from(nouveau)));
    }
}

fn libelle_vues(f: FiltreVues, musique: bool) -> &'static str {
    match (f, musique) {
        (FiltreVues::Toutes, false) => "Vues ou non",
        (FiltreVues::NonVues, false) => "Pas encore vues",
        (FiltreVues::Vues, false) => "Déjà vues",
        (FiltreVues::Toutes, true) => "Écoutés ou non",
        (FiltreVues::NonVues, true) => "Pas encore écoutés",
        (FiltreVues::Vues, true) => "Déjà écoutés (historique)",
    }
}

fn libelle_tri(t: Tri) -> &'static str {
    match t {
        Tri::Recentes => "Plus récentes",
        Tri::Reactions => "Plus de réactions",
        Tri::Titre => "Titre (A→Z)",
    }
}

fn entree(id: &str, libelle: &str, coche: bool) -> Entree {
    Entree::nouvelle(id, libelle, "").cochee(coche)
}

fn compte(n: i64) -> String {
    if n > 0 {
        n.to_string()
    } else {
        String::new()
    }
}

struct Contexte {
    moi: String,
    musique: bool,
    publier: bool,
    organiser: bool,
    gerer: bool,
    session_regarder: bool,
    session_musique: bool,
}

#[derive(Clone)]
pub struct PontVideos {
    interne: Arc<Mutex<Interne>>,
    solo: Arc<Mutex<Solo>>,
    store: Store,
    fenetre: slint::Weak<Principale>,
    runtime: tokio::runtime::Handle,
    seance: PontSeance,
    horloge: Horloge,
    decalage: Decalage,
    en_vol: Arc<AtomicBool>,
    lien_youtube: Arc<Mutex<Option<LienVue>>>,
}

impl PontVideos {
    pub fn nouveau(
        store: Store,
        fenetre: slint::Weak<Principale>,
        runtime: tokio::runtime::Handle,
        seance: PontSeance,
        horloge: Horloge,
        decalage: Decalage,
    ) -> Self {
        let interne = Arc::new(Mutex::new(Interne::default()));
        let lien_youtube: Arc<Mutex<Option<LienVue>>> = Arc::new(Mutex::new(None));
        let fabrique = fabrique(interne.clone(), store.clone(), lien_youtube.clone());
        Self {
            interne,
            solo: Arc::new(Mutex::new(Solo::nouveau(fabrique))),
            store,
            fenetre,
            runtime,
            seance,
            horloge,
            decalage,
            en_vol: Arc::new(AtomicBool::new(false)),
            lien_youtube,
        }
    }

    fn verrou(&self) -> MutexGuard<'_, Interne> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn solo(&self) -> MutexGuard<'_, Solo> {
        self.solo.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn gestion(&self) -> Option<Arc<GestionLecteur>> {
        self.verrou()
            .outils
            .as_ref()
            .and_then(|o| o.lecteur.clone())
    }

    fn journal(&self, message: &str) {
        let journal = self
            .verrou()
            .outils
            .as_ref()
            .and_then(|o| o.journal.clone());
        if let Some(j) = journal {
            j.ecrire(Niveau::Info, SOURCE_JOURNAL, message);
        }
    }

    fn ouvrir_adresse(&self, adresse: &str) {
        let navigateur = self.verrou().outils.as_ref().map(|o| o.navigateur.clone());
        if let Some(n) = navigateur {
            if let Err(e) = n.ouvrir(adresse) {
                self.journal(&format!("navigateur : {e}"));
            }
        }
    }

    pub fn definir_outils(&self, outils: OutilsVideos) {
        self.verrou().outils = Some(outils);
    }

    fn lieu(&self) -> Option<Lieu> {
        self.verrou().lieu.clone()
    }

    fn tableau(&self, salon: &str) -> Option<TableauVideos> {
        self.store.lire(|e| e.videos.get(salon).cloned())
    }

    fn post(&self, id: &str) -> Option<PostVideo> {
        let salon = self.lieu()?.salon;
        self.tableau(&salon)?
            .posts
            .into_iter()
            .find(|p| p.message_id == id)
    }

    fn planifier(&self) {
        if self.en_vol.swap(true, Ordering::AcqRel) {
            return;
        }
        let moi = self.clone();
        let resultat = self.fenetre.upgrade_in_event_loop(move |f| {
            moi.en_vol.store(false, Ordering::Release);
            moi.projeter(&f);
        });
        if resultat.is_err() {
            self.en_vol.store(false, Ordering::Release);
        }
    }

    fn executer(&self, futur: impl Future<Output = Result<(), ErreurApi>> + Send + 'static) {
        let moi = self.clone();
        self.runtime.spawn(async move {
            let resultat = futur.await;
            moi.verrou().avis = match resultat {
                Ok(()) => String::new(),
                Err(e) => e.message().to_string(),
            };
            moi.planifier();
        });
    }

    fn recharger(&self, salon: String) {
        let moi = self.clone();
        self.runtime.spawn(async move {
            if let Err(e) = moi.store.charger_videos(&salon).await {
                moi.verrou().avis = e.message().to_string();
                moi.planifier();
            }
        });
    }

    pub fn suivre(&self, f: &Principale, route: &Route) {
        let lieu = match route {
            Route::Salon {
                serveur,
                salon: Some(salon),
                ..
            } => self
                .store
                .lire(|e| {
                    e.salon(salon).is_some_and(|(_, s)| {
                        matches!(s.kind, Some(GenreSalon::Videos | GenreSalon::Music))
                    })
                })
                .then(|| Lieu {
                    serveur: serveur.clone(),
                    salon: salon.clone(),
                }),
            _ => None,
        };
        let change = {
            let mut i = self.verrou();
            if i.lieu == lieu {
                false
            } else {
                if i.lieu.as_ref().map(|l| &l.salon) != lieu.as_ref().map(|l| &l.salon) {
                    i.filtre = Filtre::default();
                    i.tri = Tri::Recentes;
                    i.onglet = OngletVideos::Videos;
                    i.avis.clear();
                }
                i.lieu = lieu.clone();
                true
            }
        };
        if !change {
            return;
        }
        if let Some(l) = lieu {
            self.recharger(l.salon);
        }
        self.projeter(f);
    }

    pub fn reconnecte(&self) {
        if let Some(l) = self.lieu() {
            self.recharger(l.salon);
        }
    }

    pub fn changements(&self, f: &Principale, changements: &[Changement]) {
        let Some(l) = self.lieu() else {
            return;
        };
        let concerne = changements.iter().any(|c| match c {
            Changement::Videos { salon } => *salon == l.salon,
            Changement::Salon { serveur, .. }
            | Changement::Roles { serveur }
            | Changement::Membre { serveur, .. }
            | Changement::MembreAjoute { serveur, .. } => *serveur == l.serveur,
            Changement::Seance { .. } | Changement::Vocal { .. } => true,
            _ => false,
        });
        if concerne {
            self.projeter(f);
        }
    }

    fn contexte(&self, l: &Lieu, f: &Principale) -> Contexte {
        let barre = f.global::<DonneesVocal>().get_barre();
        let vocal = (barre.visible && !barre.salon.is_empty()).then(|| barre.salon.to_string());
        self.store.lire(|e| {
            let peut = |p: &str| e.peut(Some(l.serveur.as_str()), p, Some(l.salon.as_str()));
            let gerer = peut("manage_messages");
            Contexte {
                moi: e.moi().to_string(),
                musique: e
                    .salon(&l.salon)
                    .is_some_and(|(_, s)| s.kind == Some(GenreSalon::Music)),
                publier: peut("send_messages"),
                organiser: peut("organize_watch") || gerer,
                gerer,
                session_regarder: vocal
                    .as_deref()
                    .is_some_and(|s| e.seances.seance(GenreSeance::Regarder, s).is_some()),
                session_musique: vocal
                    .as_deref()
                    .is_some_and(|s| e.seances.seance(GenreSeance::Musique, s).is_some()),
            }
        })
    }

    fn carte(&self, p: &PostVideo, c: &Contexte, solo: &v::VueSolo) -> CarteVideo {
        let musique = v::musique(&p.provider);
        let image = miniature(&p.message_id);
        let session = if musique {
            c.session_musique
        } else {
            c.session_regarder
        };
        CarteVideo {
            id: p.message_id.as_str().into(),
            titre: v::titre_video(p).into(),
            sous_titre: if p.site.is_empty() {
                p.author.pseudo.clone()
            } else {
                format!("{} · {}", p.site, p.author.pseudo)
            }
            .into(),
            source: v::libelle_etiquette(&p.provider).into(),
            a_miniature: image.is_some(),
            miniature: image.unwrap_or_default(),
            musique,
            vue: p.watched_at.is_some(),
            ensemble: v::jouable(&p.provider),
            solo: v::jouable(&p.provider),
            en_cours: solo.actif && solo.id == p.message_id,
            reactions: compte(p.reactions).into(),
            aime: p.liked,
            reponses: compte(p.replies).into(),
            peut_lister: c.publier,
            peut_organiser: c.organiser,
            peut_supprimer: p.author.id == c.moi || c.gerer,
            ajout_file: v::jouable(&p.provider) && session,
            ajout_solo: solo.actif && solo.id != p.message_id && mode_solo(&p.provider).is_some(),
        }
    }

    fn demander_miniatures(&self, salon: &str, posts: &[PostVideo]) {
        let manquantes: Vec<String> = {
            let mut i = self.verrou();
            posts
                .iter()
                .filter(|p| p.has_thumb && miniature(&p.message_id).is_none())
                .filter(|p| i.demandees.insert(p.message_id.clone()))
                .map(|p| p.message_id.clone())
                .collect()
        };
        for id in manquantes {
            let (moi, salon) = (self.clone(), salon.to_string());
            self.runtime.spawn(async move {
                let Ok(octets) = moi.store.client().miniature_video(&salon, &id).await else {
                    return;
                };
                let Some(pixels): Option<Pixels> = decoder_image(&octets, COTE_MINIATURE) else {
                    return;
                };
                let p = moi.clone();
                let _ = moi.fenetre.upgrade_in_event_loop(move |_| {
                    installer_miniature(&id, Image::from_rgba8(pixels));
                    p.planifier();
                });
            });
        }
    }

    pub fn projeter(&self, f: &Principale) {
        self.projeter_solo(f);
        let g = f.global::<DonneesVideos>();
        let Some(l) = self.lieu() else {
            return;
        };
        let c = self.contexte(&l, f);
        let tableau = self.tableau(&l.salon);
        let solo = self.solo().vue();
        let (onglet, grille, filtre, tri, avis) = {
            let i = self.verrou();
            (i.onglet, i.grille, i.filtre.clone(), i.tri, i.avis.clone())
        };
        let (maintenant, decalage) = ((self.horloge)(), (self.decalage)());
        let t = tableau.clone().unwrap_or_default();
        let nom_de = |id: &str| -> String {
            self.store.lire(|e| {
                e.membre(&l.serveur, id)
                    .and_then(|m| m.pseudo.clone())
                    .unwrap_or_else(|| "…".to_string())
            })
        };
        let auteurs: BTreeMap<String, String> = t
            .posts
            .iter()
            .map(|p| (p.author.id.clone(), p.author.pseudo.clone()))
            .collect();
        let visibles = v::filtrer(&t.posts, &filtre, tri, &t.lists);
        let vue = VueVideos {
            musique: c.musique,
            charge: tableau.is_some(),
            onglet,
            peut_publier: c.publier,
            grille,
            filtre_source: if filtre.source.is_empty() {
                "Toutes les sources".to_string()
            } else {
                v::libelle_source(&filtre.source).to_string()
            }
            .into(),
            filtre_vues: libelle_vues(filtre.vues, c.musique).into(),
            filtre_auteur: auteurs
                .get(&filtre.auteur)
                .cloned()
                .unwrap_or_else(|| "Tous les auteurs".to_string())
                .into(),
            filtre_liste: t
                .lists
                .iter()
                .find(|x| x.id == filtre.liste)
                .map_or_else(|| "Toutes les listes".to_string(), |x| x.name.clone())
                .into(),
            filtre_artiste: if filtre.artiste.is_empty() {
                "Tous les artistes".to_string()
            } else {
                filtre.artiste.clone()
            }
            .into(),
            tri: libelle_tri(tri).into(),
            listes_existent: !t.lists.is_empty(),
            vide: v::texte_vide(c.musique, t.posts.is_empty()).into(),
            peut_creer_liste: c.publier,
            peut_planifier: c.organiser,
            avis: avis.into(),
        };
        if g.get_vue() != vue {
            g.set_vue(vue);
        }
        self.demander_miniatures(&l.salon, &t.posts);
        let cartes: Vec<CarteVideo> = visibles.iter().map(|p| self.carte(p, &c, &solo)).collect();
        poser(g.get_cartes(), cartes, |m| g.set_cartes(m));
        let listes: Vec<ListeVideoVue> = t
            .lists
            .iter()
            .map(|x| {
                let elements = v::elements_de_liste(x, &t.posts);
                let supprimable = x.created_by == c.moi || c.gerer;
                let modifiable = c.publier && (x.collaborative || supprimable);
                let n = elements.len();
                ListeVideoVue {
                    id: x.id.as_str().into(),
                    nom: x.name.as_str().into(),
                    meta: format!(
                        "{} · {}",
                        if x.collaborative {
                            "Collaborative"
                        } else {
                            "Réservée à son créateur"
                        },
                        v::texte_elements(n)
                    )
                    .into(),
                    collaborative: x.collaborative,
                    modifiable,
                    supprimable,
                    regarder: elements
                        .iter()
                        .any(|p| v::jouable(&p.provider) && !v::musique(&p.provider)),
                    ecouter: elements.iter().any(|p| v::musique(&p.provider)),
                    planifiable: c.organiser && n > 0,
                    elements: ModelRc::new(VecModel::from(
                        elements
                            .iter()
                            .enumerate()
                            .map(|(k, p)| ElementListeVideo {
                                id: p.message_id.as_str().into(),
                                rang: (k + 1).to_string().into(),
                                source: v::libelle_etiquette(&p.provider).into(),
                                titre: if p.title.is_empty() {
                                    p.url.as_str()
                                } else {
                                    p.title.as_str()
                                }
                                .into(),
                                monter: k > 0,
                                descendre: k + 1 < n,
                            })
                            .collect::<Vec<_>>(),
                    )),
                }
            })
            .collect();
        poser(g.get_listes(), listes, |m| g.set_listes(m));
        let vocaux: HashMap<String, String> = self.store.lire(|e| {
            e.salons(&l.serveur)
                .iter()
                .filter(|s| s.kind == Some(GenreSalon::Voice))
                .map(|s| {
                    (
                        s.id.clone().unwrap_or_default(),
                        s.name.clone().unwrap_or_default(),
                    )
                })
                .collect()
        });
        let par_id: HashMap<&str, &PostVideo> =
            t.posts.iter().map(|p| (p.message_id.as_str(), p)).collect();
        let seances: Vec<SeanceVideoVue> = t
            .sessions
            .iter()
            .map(|s| {
                let en_cours = v::phase_seance(s, maintenant) == v::PhaseSeance::EnCours;
                let comptes = v::compte_rsvp(s);
                let libelle = |r: Rsvp| match comptes.de(r) {
                    0 => r.libelle().to_string(),
                    n => format!("{} ({n})", r.libelle()),
                };
                let (mois, jour) = v::pastille_date(&s.starts_at, decalage);
                let salon = vocaux.get(&s.voice_channel_id).cloned().unwrap_or_default();
                SeanceVideoVue {
                    id: s.id.as_str().into(),
                    titre: s.title.as_str().into(),
                    mois: mois.into(),
                    jour: jour.into(),
                    detail: v::detail_seance(s, &salon, decalage).into(),
                    statut: v::statut_seance(s, maintenant, decalage).into(),
                    en_cours,
                    annulable: s.created_by == c.moi || c.gerer,
                    videos: ModelRc::new(VecModel::from(
                        s.videos
                            .iter()
                            .map(|id| {
                                par_id
                                    .get(id.as_str())
                                    .map(|p| {
                                        if p.title.is_empty() {
                                            p.url.clone()
                                        } else {
                                            p.title.clone()
                                        }
                                    })
                                    .unwrap_or_else(|| "Vidéo supprimée".to_string())
                                    .into()
                            })
                            .collect::<Vec<slint::SharedString>>(),
                    )),
                    oui: libelle(Rsvp::Oui).into(),
                    peut_etre: libelle(Rsvp::PeutEtre).into(),
                    non: libelle(Rsvp::Non).into(),
                    reponse: match s.rsvps.get(&c.moi) {
                        Some(Rsvp::Oui) => 1,
                        Some(Rsvp::PeutEtre) => 2,
                        Some(Rsvp::Non) => 3,
                        None => 0,
                    },
                    rejoindre: en_cours,
                    presents: v::reponses_par_groupe(s, nom_de)
                        .into_iter()
                        .map(|(r, noms)| format!("{} : {}", r.groupe(), noms.join(", ")))
                        .collect::<Vec<_>>()
                        .join("\n")
                        .into(),
                }
            })
            .collect();
        poser(g.get_seances(), seances, |m| g.set_seances(m));
        let prochaines: Vec<ProchaineSeanceVideo> = v::seances_proches(&t.sessions, maintenant)
            .into_iter()
            .map(|s| ProchaineSeanceVideo {
                id: s.id.as_str().into(),
                titre: s.title.as_str().into(),
                quand: if s.started_at.is_some() {
                    "en cours".to_string()
                } else {
                    v::echeance(&s.starts_at, maintenant, decalage)
                }
                .into(),
            })
            .collect();
        poser(g.get_prochaines(), prochaines, |m| g.set_prochaines(m));
        self.projeter_plan(f, &t, &vocaux);
    }

    fn projeter_solo(&self, f: &Principale) {
        let salon_affiche = self.lieu().map(|l| l.salon);
        let vs = self.solo().vue();
        let place = vs.place(salon_affiche.as_deref());
        let (texte_position, texte_duree, texte_restant) = v::textes_temps(vs.position, vs.duree);
        let vue = VueSoloVideo {
            actif: vs.actif,
            ancre: place == PlaceSolo::Ancree,
            youtube: vs.mode == Some(ModeSolo::Youtube),
            titre: vs.titre.as_str().into(),
            sous_titre: vs.sous_titre.as_str().into(),
            a_suivre: v::texte_a_suivre(vs.a_suivre).into(),
            lecture: vs.lecture,
            position: vs.position as f32,
            duree: vs.duree as f32,
            texte_position: texte_position.into(),
            texte_duree: texte_duree.into(),
            texte_restant: texte_restant.into(),
            volume: i32::from(vs.volume),
            erreur: vs.erreur.as_str().into(),
            cede: vs.cede,
            partage: vs.actif
                && self
                    .solo()
                    .courant()
                    .is_some_and(|e| v::musique(&e.provider)),
        };
        let g = f.global::<DonneesVideos>();
        if g.get_solo() != vue {
            g.set_solo(vue);
        }
        let (place_changee, id_change) = {
            let mut i = self.verrou();
            let p = std::mem::replace(&mut i.place, place) != place;
            let id = std::mem::replace(&mut i.solo_id, vs.id.clone()) != vs.id;
            (p, id)
        };
        if place_changee && vs.mode == Some(ModeSolo::Lecteur) && !vs.cede {
            if let Some(g) = self.gestion() {
                if g.reservee_par(&origine()) {
                    g.lecteur().definir_visible(place == PlaceSolo::Ancree);
                }
            }
        }
        if id_change {
            self.planifier();
        }
        self.minuteur(f, vs.actif);
    }

    fn minuteur(&self, f: &Principale, actif: bool) {
        let cle = Arc::as_ptr(&self.interne) as usize;
        let etat = MINUTEUR.with(|m| m.borrow().as_ref().map(|(c, t)| (*c == cle, t.running())));
        match (actif, etat) {
            (false, Some((_, true))) => MINUTEUR.with(|m| {
                if let Some((_, t)) = m.borrow().as_ref() {
                    t.stop();
                }
            }),
            (false, _) | (true, Some((true, true))) => {}
            (true, Some((true, false))) => MINUTEUR.with(|m| {
                if let Some((_, t)) = m.borrow().as_ref() {
                    t.restart();
                }
            }),
            (true, _) => {
                let (moi, faible) = (self.clone(), f.as_weak());
                let t = slint::Timer::default();
                t.start(slint::TimerMode::Repeated, BATTEMENT, move || {
                    if let Some(f) = faible.upgrade() {
                        moi.actualiser_solo(&f);
                    }
                });
                MINUTEUR.with(|m| *m.borrow_mut() = Some((cle, t)));
            }
        }
    }

    pub fn actualiser_solo(&self, f: &Principale) {
        let cede = {
            let mode = self.solo().mode();
            mode == Some(ModeSolo::Lecteur)
                && self.gestion().is_some_and(|g| !g.reservee_par(&origine()))
        };
        if cede {
            self.solo().ceder();
        }
        let lancement = self.solo().actualiser((self.horloge)());
        self.apres(f, lancement);
    }

    fn pomper_youtube(&self, f: &Principale) {
        let lien = if self.solo().mode() == Some(ModeSolo::Youtube) {
            self.lien_youtube
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        } else {
            None
        };
        youtube::pomper_solo(f, lien.as_ref(), Rc::new(|| {}));
    }

    fn apres(&self, f: &Principale, lancement: Lancement) {
        if let Lancement::Navigateur(adresse) = lancement {
            self.ouvrir_adresse(&adresse);
        }
        let liberer = {
            let s = self.solo();
            s.mode() != Some(ModeSolo::Lecteur)
        };
        if liberer {
            if let Some(g) = self.gestion() {
                g.liberer(&origine());
            }
        }
        self.pomper_youtube(f);
        self.projeter_solo(f);
    }

    fn jouer(&self, f: &Principale, elements: Vec<ElementSolo>, melanger: bool, ajouter: bool) {
        if elements.iter().any(ElementSolo::a_resoudre) {
            let (moi, client) = (self.clone(), self.store.client().clone());
            self.runtime.spawn(async move {
                let mut resolus = Vec::new();
                for e in elements {
                    if !e.a_resoudre() {
                        resolus.push(e);
                        continue;
                    }
                    match client
                        .resoudre_musique(&v::lien_musique(&e.reference))
                        .await
                    {
                        Ok(m) => {
                            resolus.extend(m.tracks.iter().map(|t| {
                                ElementSolo::morceau(&t.id, &t.title, &t.artist, &e.salon)
                            }))
                        }
                        Err(err) => moi.verrou().avis = err.message().to_string(),
                    }
                }
                let p = moi.clone();
                let _ = moi.fenetre.upgrade_in_event_loop(move |f| {
                    p.jouer_resolus(&f, resolus, melanger, ajouter);
                    p.projeter(&f);
                });
            });
            return;
        }
        self.jouer_resolus(f, elements, melanger, ajouter);
    }

    fn jouer_resolus(
        &self,
        f: &Principale,
        elements: Vec<ElementSolo>,
        melanger: bool,
        ajouter: bool,
    ) {
        let elements = if melanger {
            v::melange(&elements, v::hasard)
        } else {
            elements
        };
        let lancement = self.solo().jouer(elements, ajouter);
        self.apres(f, lancement);
    }

    pub fn lire(&self, f: &Principale, id: &str) {
        let (Some(p), Some(l)) = (self.post(id), self.lieu()) else {
            return;
        };
        let courant = self.solo().vue();
        if courant.actif && courant.id == id && !courant.cede {
            self.solo_basculer(f);
            return;
        }
        self.jouer(f, vec![ElementSolo::de_post(&p, &l.salon)], false, false);
    }

    pub fn ajouter_solo(&self, f: &Principale, id: &str) {
        let (Some(p), Some(l)) = (self.post(id), self.lieu()) else {
            return;
        };
        self.jouer(f, vec![ElementSolo::de_post(&p, &l.salon)], false, true);
    }

    pub fn solo_basculer(&self, f: &Principale) {
        let lancement = self.solo().basculer();
        self.apres(f, lancement);
    }

    pub fn solo_suivant(&self, f: &Principale) {
        let lancement = self.solo().suivant();
        self.apres(f, lancement);
    }

    pub fn solo_arreter(&self, f: &Principale) {
        self.solo().arreter();
        self.apres(f, Lancement::Rien);
    }

    pub fn solo_aller(&self, f: &Principale, position: f32) {
        self.solo().aller(f64::from(position));
        self.projeter_solo(f);
    }

    pub fn solo_volume(&self, f: &Principale, volume: i32) {
        self.solo().regler_volume(volume.clamp(0, 100) as u8);
        self.projeter_solo(f);
    }

    pub fn solo_ouvrir(&self, f: &Principale) {
        let salon = self.solo().vue().salon;
        if !salon.is_empty() {
            f.global::<DonneesCoquille>()
                .invoke_ouvrir_conversation(CleConversation::Salon(salon).to_string().into());
        }
    }

    pub fn solo_partager(&self, f: &Principale) {
        let barre = f.global::<DonneesVocal>().get_barre();
        if !barre.visible || barre.salon.is_empty() {
            return;
        }
        let salon = barre.salon.to_string();
        let liens: Vec<String> = {
            let s = self.solo();
            s.courant()
                .into_iter()
                .chain(s.a_venir())
                .filter(|e| v::musique(&e.provider))
                .take(LIENS_MAX)
                .map(|e| v::lien_musique(&e.reference))
                .collect()
        };
        if liens.is_empty() {
            return;
        }
        let existe = self
            .store
            .lire(|e| e.seances.seance(GenreSeance::Musique, &salon).is_some());
        let client = self.store.client().clone();
        self.executer(async move {
            if !existe {
                return client
                    .lancer_seance(&salon, GenreSeance::Musique, &liens[0], &liens[1..])
                    .await;
            }
            for l in &liens {
                client
                    .ajouter_a_la_file(&salon, GenreSeance::Musique, l)
                    .await?;
            }
            Ok(())
        });
        self.solo_arreter(f);
    }

    pub fn onglet(&self, f: &Principale, onglet: OngletVideos) {
        self.verrou().onglet = onglet;
        self.projeter(f);
    }

    pub fn disposition(&self, f: &Principale, grille: bool) {
        self.verrou().grille = grille;
        self.projeter(f);
    }

    pub fn publier(&self, f: &Principale, lien: &str) {
        let lien = lien.trim().to_string();
        let Some(l) = self.lieu() else {
            return;
        };
        if lien.is_empty() {
            return;
        }
        let client = self.store.client().clone();
        self.executer(async move {
            client
                .envoyer_message(
                    &CleConversation::Salon(l.salon),
                    &NouveauMessage {
                        content: lien,
                        file_ids: Vec::new(),
                        reply_to: None,
                        poll: None,
                    },
                )
                .await
                .map(|_| ())
        });
        self.projeter(f);
    }

    pub fn menu_filtre(&self, f: &Principale, nom: &str, x: f32, y: f32) {
        let Some(l) = self.lieu() else {
            return;
        };
        let t = self.tableau(&l.salon).unwrap_or_default();
        let musique = self.contexte(&l, f).musique;
        let filtre = self.verrou().filtre.clone();
        let tri = self.verrou().tri;
        let (titre, entrees): (&str, Vec<Entree>) = match nom {
            "source" => {
                let mut sources: Vec<&str> =
                    t.posts.iter().map(|p| v::source_de(&p.provider)).collect();
                sources.sort_unstable();
                sources.dedup();
                (
                    "Filtrer par source",
                    std::iter::once(entree("", "Toutes les sources", filtre.source.is_empty()))
                        .chain(
                            sources
                                .iter()
                                .map(|s| entree(s, v::libelle_source(s), filtre.source == *s)),
                        )
                        .collect(),
                )
            }
            "vues" => (
                if musique {
                    "Filtrer selon l'écoute"
                } else {
                    "Filtrer selon la vue"
                },
                [FiltreVues::Toutes, FiltreVues::NonVues, FiltreVues::Vues]
                    .iter()
                    .enumerate()
                    .map(|(k, fv)| {
                        entree(
                            &k.to_string(),
                            libelle_vues(*fv, musique),
                            filtre.vues == *fv,
                        )
                    })
                    .collect(),
            ),
            "auteur" => {
                let auteurs: BTreeMap<String, String> = t
                    .posts
                    .iter()
                    .map(|p| (p.author.id.clone(), p.author.pseudo.clone()))
                    .collect();
                (
                    "Filtrer par auteur",
                    std::iter::once(entree("", "Tous les auteurs", filtre.auteur.is_empty()))
                        .chain(
                            auteurs
                                .iter()
                                .map(|(id, pseudo)| entree(id, pseudo, filtre.auteur == *id)),
                        )
                        .collect(),
                )
            }
            "liste" => (
                "Filtrer par liste",
                std::iter::once(entree("", "Toutes les listes", filtre.liste.is_empty()))
                    .chain(
                        t.lists
                            .iter()
                            .map(|x| entree(&x.id, &x.name, filtre.liste == x.id)),
                    )
                    .collect(),
            ),
            "artiste" => {
                let mut artistes: Vec<&str> = t
                    .posts
                    .iter()
                    .map(|p| v::artiste_de(&p.site))
                    .filter(|a| !a.is_empty())
                    .collect();
                artistes.sort_by_key(|a| v::cle_alphabetique(a));
                artistes.dedup();
                (
                    "Filtrer par artiste",
                    std::iter::once(entree("", "Tous les artistes", filtre.artiste.is_empty()))
                        .chain(artistes.iter().map(|a| entree(a, a, filtre.artiste == *a)))
                        .collect(),
                )
            }
            _ => {
                let mut tris = vec![Tri::Recentes, Tri::Reactions];
                if musique {
                    tris.push(Tri::Titre);
                }
                (
                    "Trier",
                    tris.iter()
                        .enumerate()
                        .map(|(k, t)| entree(&k.to_string(), libelle_tri(*t), tri == *t))
                        .collect(),
                )
            }
        };
        let (moi, nom) = (self.clone(), nom.to_string());
        menu::ouvrir(f, titre, ancre(x, y), entrees, move |f, id| {
            {
                let mut i = moi.verrou();
                match nom.as_str() {
                    "source" => i.filtre.source = id.to_string(),
                    "vues" => {
                        i.filtre.vues = match id {
                            "1" => FiltreVues::NonVues,
                            "2" => FiltreVues::Vues,
                            _ => FiltreVues::Toutes,
                        }
                    }
                    "auteur" => i.filtre.auteur = id.to_string(),
                    "liste" => i.filtre.liste = id.to_string(),
                    "artiste" => i.filtre.artiste = id.to_string(),
                    _ => {
                        i.tri = match id {
                            "1" => Tri::Reactions,
                            "2" => Tri::Titre,
                            _ => Tri::Recentes,
                        }
                    }
                }
            }
            moi.projeter(f);
            false
        });
    }

    pub fn ensemble(&self, f: &Principale, id: &str) {
        let (Some(p), Some(l)) = (self.post(id), self.lieu()) else {
            return;
        };
        let genre = if v::musique(&p.provider) {
            GenreSeance::Musique
        } else {
            GenreSeance::Regarder
        };
        self.seance.ouvrir_choix_avec(
            f,
            Some(l.serveur),
            genre,
            vec![v::lien_lecture(&p).to_string()],
        );
    }

    pub fn ajouter_file(&self, f: &Principale, id: &str) {
        let Some(p) = self.post(id) else {
            return;
        };
        let barre = f.global::<DonneesVocal>().get_barre();
        if !barre.visible || barre.salon.is_empty() {
            return;
        }
        let genre = if v::musique(&p.provider) {
            GenreSeance::Musique
        } else {
            GenreSeance::Regarder
        };
        let (client, salon, lien) = (
            self.store.client().clone(),
            barre.salon.to_string(),
            v::lien_lecture(&p).to_string(),
        );
        self.executer(async move { client.ajouter_a_la_file(&salon, genre, &lien).await });
    }

    pub fn aimer(&self, id: &str) {
        let Some(p) = self.post(id) else {
            return;
        };
        let (client, id) = (self.store.client().clone(), id.to_string());
        self.executer(async move {
            if p.liked {
                client.retirer_reaction(&id, "👍").await
            } else {
                client.reagir(&id, "👍").await
            }
        });
    }

    pub fn menu_listes(&self, f: &Principale, id: &str, x: f32, y: f32) {
        let Some(l) = self.lieu() else {
            return;
        };
        let t = self.tableau(&l.salon).unwrap_or_default();
        let c = self.contexte(&l, f);
        let modifiables: Vec<v::ListeVideos> = t
            .lists
            .into_iter()
            .filter(|x| x.collaborative || x.created_by == c.moi || c.gerer)
            .collect();
        let entrees: Vec<Entree> = if modifiables.is_empty() {
            vec![Entree::nouvelle(
                "",
                "Aucune liste : créez-en une dans l’onglet Listes",
                "",
            )]
        } else {
            modifiables
                .iter()
                .map(|x| {
                    Entree::nouvelle(&x.id, &x.name, "").cochee(x.items.iter().any(|i| i == id))
                })
                .collect()
        };
        let (moi, id) = (self.clone(), id.to_string());
        menu::ouvrir(
            f,
            "Ajouter à une liste",
            ancre(x, y),
            entrees,
            move |_, choisie| {
                if let Some(x) = modifiables.iter().find(|x| x.id == choisie) {
                    let ids: Vec<String> = if x.items.contains(&id) {
                        x.items.iter().filter(|i| **i != id).cloned().collect()
                    } else {
                        x.items.iter().cloned().chain([id.clone()]).collect()
                    };
                    let (client, salon, liste) =
                        (moi.store.client().clone(), l.salon.clone(), x.id.clone());
                    moi.executer(async move {
                        client.elements_liste_videos(&salon, &liste, &ids).await
                    });
                }
                false
            },
        );
    }

    pub fn marquer_vue(&self, id: &str) {
        let (Some(p), Some(l)) = (self.post(id), self.lieu()) else {
            return;
        };
        let client = self.store.client().clone();
        self.executer(async move {
            client
                .marquer_video_vue(&l.salon, &p.message_id, p.watched_at.is_none())
                .await
        });
    }

    pub fn menu_carte(&self, f: &Principale, id: &str, x: f32, y: f32) {
        let (moi, id) = (self.clone(), id.to_string());
        menu::ouvrir(
            f,
            "Plus d’actions",
            ancre(x, y),
            vec![Entree::nouvelle("supprimer", "Supprimer", "corbeille").danger()],
            move |_, choisie| {
                if choisie == "supprimer" {
                    if let Some(l) = moi.lieu() {
                        let (client, store) = (moi.store.client().clone(), moi.store.clone());
                        moi.executer(async move {
                            match client.supprimer_message(&id).await {
                                Ok(()) => store.charger_videos(&l.salon).await,
                                Err(e) => Err(e),
                            }
                        });
                    }
                }
                false
            },
        );
    }

    pub fn ouvrir_lien(&self, id: &str) {
        if let Some(p) = self.post(id) {
            self.ouvrir_adresse(&p.url);
        }
    }

    fn liste(&self, id: &str) -> Option<(Lieu, v::ListeVideos, Vec<PostVideo>)> {
        let l = self.lieu()?;
        let t = self.tableau(&l.salon)?;
        let x = t.lists.iter().find(|x| x.id == id)?.clone();
        let elements = v::elements_de_liste(&x, &t.posts)
            .into_iter()
            .cloned()
            .collect();
        Some((l, x, elements))
    }

    pub fn creer_liste(&self, nom: &str, collaborative: bool) {
        let nom: String = nom.trim().chars().take(v::TITRE_MAX).collect();
        let Some(l) = self.lieu() else {
            return;
        };
        if nom.is_empty() {
            return;
        }
        let client = self.store.client().clone();
        self.executer(async move {
            client
                .creer_liste_videos(&l.salon, &nom, collaborative)
                .await
                .map(|_| ())
        });
    }

    pub fn renommer_liste(&self, f: &Principale, id: &str) {
        let Some((l, x, _)) = self.liste(id) else {
            return;
        };
        let moi = self.clone();
        confirmation::saisir(
            f,
            Demande {
                titre: "Renommer la liste".into(),
                message: String::new(),
                action: "Renommer".into(),
                destructif: false,
                saisie: Some(Saisie {
                    libelle: "Nouveau nom de la liste".into(),
                    valeur: x.name.clone(),
                    exiger: None,
                }),
                case: None,
            },
            move |_, nom| {
                let Some(nom) =
                    nom.map(|n| n.trim().chars().take(v::TITRE_MAX).collect::<String>())
                else {
                    return;
                };
                if nom.is_empty() || nom == x.name {
                    return;
                }
                let client = moi.store.client().clone();
                moi.executer(async move {
                    client
                        .modifier_liste_videos(&l.salon, &x.id, &nom, None)
                        .await
                });
            },
        );
    }

    pub fn basculer_collaborative(&self, id: &str) {
        let Some((l, x, _)) = self.liste(id) else {
            return;
        };
        let client = self.store.client().clone();
        self.executer(async move {
            client
                .modifier_liste_videos(&l.salon, &x.id, &x.name, Some(!x.collaborative))
                .await
        });
    }

    pub fn supprimer_liste(&self, f: &Principale, id: &str) {
        let Some((l, x, _)) = self.liste(id) else {
            return;
        };
        let moi = self.clone();
        confirmation::confirmer(
            f,
            Demande {
                titre: "Supprimer la liste".into(),
                message: format!(
                    "Supprimer la liste « {} » ? Les vidéos restent dans le salon.",
                    x.name
                ),
                action: "Supprimer la liste".into(),
                destructif: true,
                saisie: None,
                case: None,
            },
            move |_, ok| {
                if ok {
                    let client = moi.store.client().clone();
                    moi.executer(
                        async move { client.supprimer_liste_videos(&l.salon, &x.id).await },
                    );
                }
            },
        );
    }

    pub fn deplacer_element(&self, liste: &str, rang: i32, monter: bool) {
        let Some((l, x, elements)) = self.liste(liste) else {
            return;
        };
        let ids: Vec<String> = elements.iter().map(|p| p.message_id.clone()).collect();
        let nouveaux = v::deplace(&ids, rang.max(0) as usize, if monter { -1 } else { 1 });
        if nouveaux == ids {
            return;
        }
        let client = self.store.client().clone();
        self.executer(async move {
            client
                .elements_liste_videos(&l.salon, &x.id, &nouveaux)
                .await
        });
    }

    pub fn retirer_element(&self, liste: &str, id: &str) {
        let Some((l, x, elements)) = self.liste(liste) else {
            return;
        };
        let ids: Vec<String> = elements
            .iter()
            .map(|p| p.message_id.clone())
            .filter(|i| i != id)
            .collect();
        let client = self.store.client().clone();
        self.executer(async move { client.elements_liste_videos(&l.salon, &x.id, &ids).await });
    }

    pub fn regarder_liste(&self, f: &Principale, id: &str, musique: bool) {
        let Some((l, _, elements)) = self.liste(id) else {
            return;
        };
        let liens: Vec<String> = elements
            .iter()
            .filter(|p| {
                if musique {
                    v::musique(&p.provider)
                } else {
                    v::jouable(&p.provider) && !v::musique(&p.provider)
                }
            })
            .map(|p| v::lien_lecture(p).to_string())
            .take(LIENS_MAX)
            .collect();
        if liens.is_empty() {
            return;
        }
        let genre = if musique {
            GenreSeance::Musique
        } else {
            GenreSeance::Regarder
        };
        self.seance
            .ouvrir_choix_avec(f, Some(l.serveur), genre, liens);
    }

    pub fn ecouter_liste_solo(&self, f: &Principale, id: &str, melanger: bool) {
        let Some((l, _, elements)) = self.liste(id) else {
            return;
        };
        let morceaux: Vec<ElementSolo> = elements
            .iter()
            .filter(|p| v::musique(&p.provider))
            .map(|p| ElementSolo::de_post(p, &l.salon))
            .collect();
        self.jouer(f, morceaux, melanger, false);
    }

    pub fn planifier_liste(&self, f: &Principale, id: &str) {
        self.verrou().onglet = OngletVideos::Seances;
        self.ouvrir_plan(f, Some(id.to_string()));
    }

    pub fn repondre(&self, seance: &str, reponse: i32) {
        let Some(l) = self.lieu() else {
            return;
        };
        let r = match reponse {
            1 => Rsvp::Oui,
            2 => Rsvp::PeutEtre,
            _ => Rsvp::Non,
        };
        let (client, seance) = (self.store.client().clone(), seance.to_string());
        self.executer(async move { client.repondre_seance_videos(&l.salon, &seance, r).await });
    }

    pub fn annuler_seance(&self, f: &Principale, id: &str) {
        let Some(l) = self.lieu() else {
            return;
        };
        let Some(s) = self
            .tableau(&l.salon)
            .and_then(|t| t.sessions.into_iter().find(|s| s.id == id))
        else {
            return;
        };
        let moi = self.clone();
        confirmation::confirmer(
            f,
            Demande {
                titre: "Annuler la séance".into(),
                message: format!("Annuler la séance « {} » ?", s.title),
                action: "Annuler la séance".into(),
                destructif: true,
                saisie: None,
                case: None,
            },
            move |_, ok| {
                if ok {
                    let client = moi.store.client().clone();
                    moi.executer(
                        async move { client.annuler_seance_videos(&l.salon, &s.id).await },
                    );
                }
            },
        );
    }

    pub fn rejoindre(&self, f: &Principale, id: &str) {
        let Some(l) = self.lieu() else {
            return;
        };
        let Some(vocal) = self.tableau(&l.salon).and_then(|t| {
            t.sessions
                .into_iter()
                .find(|s| s.id == id)
                .map(|s| s.voice_channel_id)
        }) else {
            return;
        };
        let g = f.global::<DonneesCoquille>();
        g.invoke_ouvrir_conversation(CleConversation::Salon(vocal.clone()).to_string().into());
        g.invoke_ouvrir_salon(vocal.into());
    }

    fn vocaux(&self, serveur: &str) -> Vec<(String, String)> {
        self.store.lire(|e| {
            e.salons(serveur)
                .iter()
                .filter(|s| s.kind == Some(GenreSalon::Voice))
                .map(|s| {
                    (
                        s.id.clone().unwrap_or_default(),
                        s.name.clone().unwrap_or_default(),
                    )
                })
                .collect()
        })
    }

    pub fn ouvrir_plan(&self, f: &Principale, liste: Option<String>) {
        let Some(l) = self.lieu() else {
            return;
        };
        let salon_vocal = self
            .vocaux(&l.serveur)
            .into_iter()
            .next()
            .map(|(id, _)| id)
            .unwrap_or_default();
        let date = v::date_du_formulaire((self.horloge)() + 3_600_000, (self.decalage)());
        self.verrou().plan = Some(Plan {
            date,
            salon_vocal,
            rappel: RAPPEL_DEFAUT,
            liste,
            ..Plan::default()
        });
        self.projeter(f);
    }

    pub fn fermer_plan(&self, f: &Principale) {
        self.verrou().plan = None;
        self.projeter(f);
    }

    fn modifier_plan(&self, f: &Principale, action: impl FnOnce(&mut Plan)) {
        if let Some(p) = self.verrou().plan.as_mut() {
            action(p);
            p.erreur.clear();
        }
        self.projeter(f);
    }

    pub fn plan_titre(&self, f: &Principale, titre: &str) {
        let titre = titre.to_string();
        self.modifier_plan(f, |p| p.titre = titre);
    }

    pub fn plan_date(&self, f: &Principale, date: &str) {
        let date = date.to_string();
        self.modifier_plan(f, |p| p.date = date);
    }

    pub fn plan_choix(&self, f: &Principale, id: &str, coche: bool) {
        let id = id.to_string();
        self.modifier_plan(f, |p| {
            p.choisis.retain(|c| *c != id);
            if coche {
                p.choisis.push(id);
            }
        });
    }

    pub fn menu_plan(&self, f: &Principale, nom: &str, x: f32, y: f32) {
        let Some(l) = self.lieu() else {
            return;
        };
        let Some(plan) = self.verrou().plan.clone() else {
            return;
        };
        let (titre, entrees): (&str, Vec<Entree>) = match nom {
            "salon" => (
                "Salon vocal",
                self.vocaux(&l.serveur)
                    .iter()
                    .map(|(id, n)| Entree::nouvelle(id, n, "").cochee(*id == plan.salon_vocal))
                    .collect(),
            ),
            "rappel" => (
                "Rappel",
                v::RAPPELS
                    .iter()
                    .map(|(m, n)| Entree::nouvelle(&m.to_string(), n, "").cochee(*m == plan.rappel))
                    .collect(),
            ),
            _ => {
                let t = self.tableau(&l.salon).unwrap_or_default();
                (
                    "File de la séance",
                    std::iter::once(
                        Entree::nouvelle("", "Choisir des vidéos…", "")
                            .cochee(plan.liste.is_none()),
                    )
                    .chain(t.lists.iter().map(|x| {
                        Entree::nouvelle(
                            &x.id,
                            &format!("Liste « {} » ({})", x.name, x.items.len()),
                            "",
                        )
                        .cochee(plan.liste.as_deref() == Some(x.id.as_str()))
                    }))
                    .collect(),
                )
            }
        };
        let (moi, nom) = (self.clone(), nom.to_string());
        menu::ouvrir(f, titre, ancre(x, y), entrees, move |f, id| {
            let id = id.to_string();
            moi.modifier_plan(f, |p| match nom.as_str() {
                "salon" => p.salon_vocal = id,
                "rappel" => p.rappel = id.parse().unwrap_or(RAPPEL_DEFAUT),
                _ => p.liste = (!id.is_empty()).then_some(id),
            });
            false
        });
    }

    fn seance_du_plan(&self, plan: &Plan) -> Option<v::NouvelleSeance> {
        let file = match &plan.liste {
            Some(x) => FileSeance::Liste(x.clone()),
            None => FileSeance::Videos(plan.choisis.clone()),
        };
        v::nouvelle_seance(
            &plan.titre,
            v::lire_date_du_formulaire(&plan.date, (self.decalage)()),
            &plan.salon_vocal,
            plan.rappel,
            &file,
        )
    }

    pub fn planifier_seance(&self, f: &Principale) {
        let Some(l) = self.lieu() else {
            return;
        };
        let Some(plan) = self.verrou().plan.clone() else {
            return;
        };
        let Some(seance) = self.seance_du_plan(&plan) else {
            if let Some(p) = self.verrou().plan.as_mut() {
                p.erreur =
                    "Indique un titre, une date (JJ/MM/AAAA HH:MM), un salon vocal et une file."
                        .into();
            }
            self.projeter(f);
            return;
        };
        if let Some(p) = self.verrou().plan.as_mut() {
            p.occupe = true;
        }
        self.projeter(f);
        let (moi, client) = (self.clone(), self.store.client().clone());
        self.runtime.spawn(async move {
            let resultat = client.planifier_seance_videos(&l.salon, &seance).await;
            {
                let mut i = moi.verrou();
                match resultat {
                    Ok(_) => {
                        i.plan = None;
                        i.onglet = OngletVideos::Seances;
                    }
                    Err(e) => {
                        if let Some(p) = i.plan.as_mut() {
                            p.occupe = false;
                            p.erreur = e.message().to_string();
                        }
                    }
                }
            }
            moi.planifier();
        });
    }

    fn projeter_plan(&self, f: &Principale, t: &TableauVideos, vocaux: &HashMap<String, String>) {
        let g = f.global::<DonneesVideos>();
        let Some(plan) = self.verrou().plan.clone() else {
            if g.get_plan().ouvert {
                g.set_plan(PlanSeanceVideo::default());
            }
            return;
        };
        let pret = self.seance_du_plan(&plan).is_some();
        let vue = PlanSeanceVideo {
            ouvert: true,
            titre: plan.titre.as_str().into(),
            date: plan.date.as_str().into(),
            salon: vocaux
                .get(&plan.salon_vocal)
                .cloned()
                .unwrap_or_else(|| "Aucun salon vocal".to_string())
                .into(),
            rappel: v::libelle_rappel(plan.rappel).into(),
            file: plan
                .liste
                .as_ref()
                .and_then(|x| t.lists.iter().find(|y| y.id == *x))
                .map_or_else(
                    || "Choisir des vidéos…".to_string(),
                    |x| format!("Liste « {} » ({})", x.name, x.items.len()),
                )
                .into(),
            choisir: plan.liste.is_none(),
            pret,
            erreur: plan.erreur.as_str().into(),
            occupe: plan.occupe,
        };
        if g.get_plan() != vue {
            g.set_plan(vue);
        }
        let choix: Vec<ChoixPlanVideo> = t
            .posts
            .iter()
            .filter(|p| v::jouable(&p.provider))
            .map(|p| ChoixPlanVideo {
                id: p.message_id.as_str().into(),
                titre: if p.title.is_empty() {
                    p.url.as_str()
                } else {
                    p.title.as_str()
                }
                .into(),
                coche: plan.choisis.contains(&p.message_id),
            })
            .collect();
        poser(g.get_choix_plan(), choix, |m| g.set_choix_plan(m));
    }

    pub fn brancher(&self, f: &Principale) {
        let g = f.global::<DonneesVideos>();
        let pont = self.clone();
        let faible = f.as_weak();
        macro_rules! rappel {
            ($on:ident, |$p:ident, $f:ident $(, $a:ident)*| $corps:expr) => {{
                let $p = pont.clone();
                let faible = faible.clone();
                g.$on(move |$($a),*| {
                    if let Some($f) = faible.upgrade() {
                        $corps;
                    }
                });
            }};
        }
        macro_rules! seul {
            ($on:ident, |$p:ident $(, $a:ident)*| $corps:expr) => {{
                let $p = pont.clone();
                g.$on(move |$($a),*| {
                    $corps;
                });
            }};
        }
        rappel!(on_onglet, |p, f, o| p.onglet(&f, o));
        rappel!(on_publier, |p, f, lien| p.publier(&f, &lien));
        rappel!(on_disposition, |p, f, grille| p.disposition(&f, grille));
        rappel!(on_menu_filtre, |p, f, nom, x, y| p
            .menu_filtre(&f, &nom, x, y));
        rappel!(on_ensemble, |p, f, id| p.ensemble(&f, &id));
        rappel!(on_lire, |p, f, id| p.lire(&f, &id));
        rappel!(on_ajouter_solo, |p, f, id| p.ajouter_solo(&f, &id));
        rappel!(on_ajouter_file, |p, f, id| p.ajouter_file(&f, &id));
        seul!(on_aimer, |p, id| p.aimer(&id));
        rappel!(on_menu_listes, |p, f, id, x, y| p
            .menu_listes(&f, &id, x, y));
        seul!(on_marquer_vue, |p, id| p.marquer_vue(&id));
        rappel!(on_menu_carte, |p, f, id, x, y| p.menu_carte(&f, &id, x, y));
        seul!(on_ouvrir_lien, |p, id| p.ouvrir_lien(&id));
        seul!(on_creer_liste, |p, nom, collaborative| p
            .creer_liste(&nom, collaborative));
        rappel!(on_renommer_liste, |p, f, id| p.renommer_liste(&f, &id));
        seul!(on_basculer_collaborative, |p, id| p
            .basculer_collaborative(&id));
        rappel!(on_supprimer_liste, |p, f, id| p.supprimer_liste(&f, &id));
        seul!(on_deplacer_element, |p, liste, rang, monter| p
            .deplacer_element(&liste, rang, monter));
        seul!(on_retirer_element, |p, liste, id| p
            .retirer_element(&liste, &id));
        rappel!(on_regarder_liste, |p, f, id| p
            .regarder_liste(&f, &id, false));
        rappel!(on_ecouter_liste, |p, f, id| p.regarder_liste(&f, &id, true));
        rappel!(on_ecouter_liste_solo, |p, f, id, melanger| p
            .ecouter_liste_solo(&f, &id, melanger));
        rappel!(on_planifier_liste, |p, f, id| p.planifier_liste(&f, &id));
        seul!(on_repondre, |p, id, reponse| p.repondre(&id, reponse));
        rappel!(on_annuler_seance, |p, f, id| p.annuler_seance(&f, &id));
        rappel!(on_rejoindre, |p, f, id| p.rejoindre(&f, &id));
        rappel!(on_ouvrir_plan, |p, f| p.ouvrir_plan(&f, None));
        rappel!(on_fermer_plan, |p, f| p.fermer_plan(&f));
        rappel!(on_plan_titre, |p, f, titre| p.plan_titre(&f, &titre));
        rappel!(on_plan_date, |p, f, date| p.plan_date(&f, &date));
        rappel!(on_menu_plan, |p, f, nom, x, y| p.menu_plan(&f, &nom, x, y));
        rappel!(on_plan_choix, |p, f, id, coche| p
            .plan_choix(&f, &id, coche));
        rappel!(on_planifier, |p, f| p.planifier_seance(&f));
        rappel!(on_solo_basculer, |p, f| p.solo_basculer(&f));
        rappel!(on_solo_suivant, |p, f| p.solo_suivant(&f));
        rappel!(on_solo_arreter, |p, f| p.solo_arreter(&f));
        rappel!(on_solo_aller, |p, f, position| p.solo_aller(&f, position));
        rappel!(on_solo_volume, |p, f, volume| p.solo_volume(&f, volume));
        rappel!(on_solo_ouvrir, |p, f| p.solo_ouvrir(&f));
        rappel!(on_solo_partager, |p, f| p.solo_partager(&f));
    }
}

fn fabrique(
    interne: Arc<Mutex<Interne>>,
    store: Store,
    lien_youtube: Arc<Mutex<Option<LienVue>>>,
) -> FabriqueSolo {
    Box::new(move |mode: ModeSolo| -> Option<Box<dyn LecteurSeance>> {
        match mode {
            ModeSolo::Lecteur => {
                let gestion = interne
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .outils
                    .as_ref()
                    .and_then(|o| o.lecteur.clone())?;
                gestion.reserver(origine());
                gestion.lecteur().definir_visible(true);
                let client = store.client();
                let mut lecteur = LecteurFichier::nouveau(gestion.lecteur().clone());
                if let Some(jeton) = client.jeton() {
                    lecteur = lecteur.avec_jeton_pour(client.adresse().to_string(), jeton);
                }
                Some(Box::new(lecteur) as Box<dyn LecteurSeance>)
            }
            ModeSolo::Youtube => {
                if !youtube::disponible() {
                    return None;
                }
                let lien = LienVue::nouveau();
                *lien_youtube.lock().unwrap_or_else(|e| e.into_inner()) = Some(lien.clone());
                Some(Box::new(LecteurYoutube::nouveau(lien)) as Box<dyn LecteurSeance>)
            }
        }
    })
}
