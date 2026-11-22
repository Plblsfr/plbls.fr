pub mod lecture;
pub mod session;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::replay::clips::date_clip;
use estompe_client::replay::galerie::nettoyer;
use estompe_client::replay::montage::{
    self, action_clavier, arreter, clips_de, encodeurs_export, extrait, formater_temps,
    image_seule, nom_travail, plan_apercu, Action, Demande, Echec, Enfant, Montage, Resume, Source,
    Touche, RAPPORTS, VITESSES,
};
use estompe_client::replay::types::Clip;
use estompe_plateforme::commande_d_arriere_plan;
use slint::platform::Key;
use slint::{ComponentHandle, Image, ModelRc, VecModel};

use self::lecture::{ImageApercu, Lecteur, SortiePartagee};
use self::session::{libelle_vitesse, Operation, SessionMontage, BARRES_ONDE};
use super::clips::{Galerie, Protection};
use super::{heure_locale, Horloge, ServiceReplay};
use crate::confirmation::{self, Demande as Question, Saisie};
use crate::{
    ChoixEditeur, ClipAjoutable, DonneesClips, DonneesEditeur, PisteEditeur, Principale,
    ProjetMontage, SegmentEditeur,
};

pub const HAUTEUR_APERCU: u32 = 360;
pub const IPS_APERCU: u32 = 30;
const SCRUTATION: Duration = Duration::from_millis(100);

#[derive(Clone)]
pub struct OutilsEditeur {
    pub service: Arc<ServiceReplay>,
    pub galerie: Arc<Galerie>,
    pub ffmpeg: PathBuf,
    pub sortie_son: Option<SortiePartagee>,
    pub encodeurs: Option<Vec<String>>,
    pub horloge: Option<Horloge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EtatExport {
    pub id: u64,
    pub nom: String,
    pub pct: u8,
    pub logiciel: bool,
}

struct TacheExport {
    etat: EtatExport,
    annule: Arc<AtomicBool>,
    enfant: Enfant,
}

#[derive(Default)]
pub struct Exports {
    taches: Mutex<BTreeMap<u64, TacheExport>>,
}

fn verrou<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Exports {
    pub fn suivre(&self, etat: EtatExport, annule: Arc<AtomicBool>, enfant: Enfant) {
        verrou(&self.taches).insert(
            etat.id,
            TacheExport {
                etat,
                annule,
                enfant,
            },
        );
    }

    pub fn en_cours(&self) -> Vec<EtatExport> {
        verrou(&self.taches)
            .values()
            .map(|t| t.etat.clone())
            .collect()
    }

    pub fn avancer(&self, id: u64, pct: u8, logiciel: bool) -> Option<EtatExport> {
        let mut taches = verrou(&self.taches);
        let etat = &mut taches.get_mut(&id)?.etat;
        if (etat.pct, etat.logiciel) == (pct, logiciel) {
            return None;
        }
        (etat.pct, etat.logiciel) = (pct, logiciel);
        Some(etat.clone())
    }

    pub fn annuler(&self, id: u64) -> Result<(), String> {
        let taches = verrou(&self.taches);
        let tache = taches
            .get(&id)
            .ok_or("export introuvable ou déjà terminé")?;
        tache.annule.store(true, Ordering::SeqCst);
        arreter(&tache.enfant);
        Ok(())
    }

    pub fn terminer(&self, id: u64) {
        verrou(&self.taches).remove(&id);
    }
}

pub fn nom_touche(texte: &str) -> String {
    let speciales = [
        (Key::LeftArrow, "ArrowLeft"),
        (Key::RightArrow, "ArrowRight"),
        (Key::Delete, "Delete"),
        (Key::Backspace, "Backspace"),
        (Key::Return, "Enter"),
    ];
    let mut caracteres = texte.chars();
    match (caracteres.next(), caracteres.next()) {
        (Some(c), None) => speciales
            .iter()
            .find(|(k, _)| char::from(*k) == c)
            .map_or_else(|| texte.to_string(), |(_, n)| n.to_string()),
        _ => texte.to_string(),
    }
}

pub fn infos_projet(r: &Resume) -> (String, String) {
    let n = r.clips.len();
    let clips = if n > 1 {
        format!("{n} clips")
    } else {
        "1 clip".into()
    };
    let date = date_clip(&r.date);
    let infos = [formater_temps(r.duree_s), clips, date]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    let manquants = if r.manquants.is_empty() {
        String::new()
    } else {
        format!("Clip introuvable : {}", r.manquants.join(", "))
    };
    (infos, manquants)
}

enum Cible {
    Clip(PathBuf),
    Projet(String),
}

#[derive(Default)]
struct EtatEditeur {
    session: Option<SessionMontage>,
    dossier: Option<PathBuf>,
    protections: Vec<Protection>,
    lecture: Option<f64>,
    ouverture: u64,
    export: Option<u64>,
}

pub struct Editeur {
    outils: OutilsEditeur,
    fenetre: slint::Weak<Principale>,
    etat: Mutex<EtatEditeur>,
    lecteur: Lecteur,
    exports: Exports,
    ecriture: Mutex<Sender<(PathBuf, String, Montage)>>,
    compteur: AtomicU64,
}

fn choix(liste: impl IntoIterator<Item = (String, bool)>) -> ModelRc<ChoixEditeur> {
    ModelRc::new(VecModel::from(
        liste
            .into_iter()
            .map(|(libelle, choisi)| ChoixEditeur {
                libelle: libelle.into(),
                choisi,
            })
            .collect::<Vec<_>>(),
    ))
}

impl Editeur {
    fn verrou(&self) -> MutexGuard<'_, EtatEditeur> {
        verrou(&self.etat)
    }

    pub fn exports(&self) -> &Exports {
        &self.exports
    }

    pub fn session<R>(&self, lire: impl FnOnce(Option<&SessionMontage>) -> R) -> R {
        lire(self.verrou().session.as_ref())
    }

    fn commande(&self) -> impl Fn() -> Result<Command, String> + Send + Sync + 'static {
        let ffmpeg = self.outils.ffmpeg.clone();
        move || {
            let mut c = commande_d_arriere_plan(&ffmpeg);
            c.args(["-hide_banner", "-nostdin"]);
            Ok(c)
        }
    }

    fn plus_tard(self: &Arc<Self>, action: impl FnOnce(&Arc<Self>, &Principale) + Send + 'static) {
        let editeur = self.clone();
        let _ = self
            .fenetre
            .upgrade_in_event_loop(move |f| action(&editeur, &f));
    }

    fn sur_un_fil(self: &Arc<Self>, nom: &str, action: impl FnOnce(&Arc<Self>) + Send + 'static) {
        let editeur = self.clone();
        let _ = std::thread::Builder::new()
            .name(format!("estompe-montage-{nom}"))
            .spawn(move || action(&editeur));
    }

    fn signaler(&self, f: &Principale, message: impl Into<String>, erreur: bool) {
        let d = f.global::<DonneesEditeur>();
        d.set_message(message.into().into());
        d.set_message_erreur(erreur);
    }

    fn preparer(&self, f: &Principale, texte: String) -> u64 {
        self.arreter_lecture(f);
        let ouverture = {
            let mut e = self.verrou();
            e.session = None;
            e.protections.clear();
            e.ouverture += 1;
            e.ouverture
        };
        f.global::<DonneesClips>().set_ouvert(false);
        let d = f.global::<DonneesEditeur>();
        d.set_pret(false);
        d.set_a_image(false);
        d.set_preparation(texte.into());
        d.set_message("".into());
        d.set_ouvert(true);
        ouverture
    }

    pub fn ouvrir_clip(self: &Arc<Self>, f: &Principale, clip: &Clip) {
        let ouverture = self.preparer(f, format!("Ouverture de « {} »…", clip.nom));
        let chemin = clip.chemin.clone();
        self.sur_un_fil("ouverture", move |e| {
            e.charger(ouverture, Cible::Clip(chemin))
        });
    }

    pub fn ouvrir_projet(self: &Arc<Self>, f: &Principale, id: &str) {
        let ouverture = self.preparer(f, "Ouverture du montage…".into());
        let id = id.to_string();
        self.sur_un_fil("ouverture", move |e| {
            e.charger(ouverture, Cible::Projet(id))
        });
    }

    fn charger(self: &Arc<Self>, ouverture: u64, cible: Cible) {
        let resultat = self.lire_cible(&cible);
        self.plus_tard(move |e, f| {
            if e.verrou().ouverture != ouverture {
                return;
            }
            match resultat {
                Ok((dossier, session)) => e.installer(f, dossier, session),
                Err(message) => e.signaler(f, message, true),
            }
        });
    }

    fn lire_cible(&self, cible: &Cible) -> Result<(PathBuf, SessionMontage), String> {
        let dossier = self.outils.service.dossier()?;
        let commande = self.commande();
        let (id, projet, noms) = match cible {
            Cible::Clip(chemin) => {
                let source = chemin
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                match montage::dernier_pour(&dossier, &source) {
                    Some((id, m)) => (id, Some(m), vec![source]),
                    None => (montage::nouvel_id(&dossier), None, vec![source]),
                }
            }
            Cible::Projet(id) => {
                let m =
                    montage::lire(&dossier, id).ok_or("Ce projet est introuvable ou illisible.")?;
                let noms = clips_de(&m);
                (id.clone(), Some(m), noms)
            }
        };
        let (mut clips, mut manquants) = (Vec::new(), Vec::new());
        for nom in noms {
            match montage::sonder(&commande, &dossier.join(&nom)) {
                Ok(infos) => clips.push((nom, infos)),
                Err(e) if e.starts_with("Clip illisible") && projet.is_some() => {
                    manquants.push(nom)
                }
                Err(e) => return Err(e),
            }
        }
        SessionMontage::nouvelle(id, clips, manquants, projet.as_ref())
            .map(|s| (dossier, s))
            .ok_or_else(|| "Tous les clips de ce projet sont introuvables : ils ont été déplacés ou supprimés hors d'Estompe.".into())
    }

    fn installer(self: &Arc<Self>, f: &Principale, dossier: PathBuf, session: SessionMontage) {
        let pistes: Vec<(String, usize)> = session
            .sources
            .values()
            .flat_map(|s| (0..s.pistes).map(move |p| (s.nom.clone(), p)))
            .collect();
        {
            let mut e = self.verrou();
            e.protections = session
                .sources
                .keys()
                .map(|nom| self.outils.galerie.proteger(dossier.join(nom)))
                .collect();
            e.session = Some(session);
            e.dossier = Some(dossier.clone());
        }
        let d = f.global::<DonneesEditeur>();
        d.set_pret(true);
        d.set_message("".into());
        self.afficher(f);
        self.montrer_image();
        self.charger_ondes(dossier, pistes);
    }

    fn charger_ondes(self: &Arc<Self>, dossier: PathBuf, pistes: Vec<(String, usize)>) {
        self.sur_un_fil("ondes", move |e| {
            let commande = e.commande();
            for (nom, piste) in pistes {
                let Ok(onde) = montage::onde(&commande, &dossier.join(&nom), piste, BARRES_ONDE)
                else {
                    continue;
                };
                if let Some(s) = e.verrou().session.as_mut() {
                    s.ondes.insert((nom, piste), onde);
                }
                e.plus_tard(|e, f| e.afficher(f));
            }
        });
    }

    pub fn afficher(&self, f: &Principale) {
        let e = self.verrou();
        let Some(s) = e.session.as_ref() else {
            return;
        };
        let v = s.vue();
        let d = f.global::<DonneesEditeur>();
        d.set_nom(v.nom.as_str().into());
        d.set_duree(v.duree.as_str().into());
        d.set_temps(v.temps.as_str().into());
        d.set_segments(ModelRc::new(VecModel::from(
            v.segments
                .iter()
                .map(|s| SegmentEditeur {
                    debut: s.debut,
                    largeur: s.largeur,
                    garde: s.garde,
                    choisi: s.choisi,
                    libelle: s.libelle.as_str().into(),
                    clip: s.clip.as_str().into(),
                    muet: s.muet,
                })
                .collect::<Vec<_>>(),
        )));
        d.set_tete(v.tete);
        d.set_entree(v.entree);
        d.set_sortie(v.sortie);
        d.set_pistes(ModelRc::new(VecModel::from(
            v.pistes
                .iter()
                .map(|p| PisteEditeur {
                    nom: p.nom.as_str().into(),
                    volume: p.volume,
                    muet: p.muet,
                    onde: p.onde.as_str().into(),
                })
                .collect::<Vec<_>>(),
        )));
        d.set_clip_choisi(v.clip_choisi.as_str().into());
        d.set_garde(v.garde);
        d.set_muet(v.muet);
        d.set_vitesses(choix(
            VITESSES
                .iter()
                .enumerate()
                .map(|(i, x)| (libelle_vitesse(*x), i == v.vitesse)),
        ));
        d.set_saccade(v.saccade);
        d.set_peut_annuler(v.peut_annuler);
        d.set_peut_retablir(v.peut_retablir);
        d.set_rapports(choix(RAPPORTS.iter().enumerate().map(|(i, r)| {
            (
                if *r == "libre" {
                    "Libre".to_string()
                } else {
                    r.to_string()
                },
                i == v.rapport,
            )
        })));
        d.set_plusieurs(v.plusieurs);
        d.set_peut_reculer(v.peut_reculer);
        d.set_peut_avancer(v.peut_avancer);
        d.set_manquants(v.manquants.as_str().into());
        d.set_textes(
            if v.textes > 0 {
                format!("Ce montage contient {} bloc(s) de texte, gardés dans le projet mais ni affichés ni exportés par cette app.", v.textes)
            } else {
                String::new()
            }
            .into(),
        );
        d.set_lecture(e.lecture.is_some());
    }

    fn afficher_temps(&self, f: &Principale) {
        let e = self.verrou();
        let Some(s) = e.session.as_ref() else {
            return;
        };
        let v = s.vue();
        let d = f.global::<DonneesEditeur>();
        d.set_temps(v.temps.as_str().into());
        d.set_tete(v.tete);
    }

    fn poser_image(f: &Principale, image: Option<ImageApercu>) {
        let d = f.global::<DonneesEditeur>();
        d.set_a_image(image.is_some());
        if let Some(i) = image {
            d.set_image(Image::from_rgba8(i));
        }
    }

    pub fn montrer_image(self: &Arc<Self>) {
        let plan = {
            let e = self.verrou();
            let (Some(s), Some(dossier)) = (e.session.as_ref(), e.dossier.clone()) else {
                return;
            };
            image_seule(s.montage(), s.lieu(), &s.sources)
                .and_then(|m| plan_apercu(&m, &s.infos, HAUTEUR_APERCU, IPS_APERCU).ok())
                .map(|p| (dossier, p))
        };
        let Some((dossier, plan)) = plan else {
            return;
        };
        let editeur = self.clone();
        self.lecteur.image(dossier, plan, move |image| {
            editeur.plus_tard(move |_, f| Self::poser_image(f, image));
        });
    }

    fn arreter_lecture(&self, f: &Principale) {
        if self.verrou().lecture.take().is_some() {
            self.lecteur.arreter();
            f.global::<DonneesEditeur>().set_lecture(false);
        }
    }

    pub fn basculer_lecture(self: &Arc<Self>, f: &Principale) {
        if self.verrou().lecture.is_some() {
            self.arreter_lecture(f);
            self.montrer_image();
            return;
        }
        let depart = {
            let e = self.verrou();
            let (Some(s), Some(dossier)) = (e.session.as_ref(), e.dossier.clone()) else {
                return;
            };
            extrait(s.montage(), s.lieu(), &s.sources)
                .and_then(|m| plan_apercu(&m, &s.infos, HAUTEUR_APERCU, IPS_APERCU).ok())
                .map(|p| (dossier, p, s.temps_monte()))
        };
        let Some((dossier, plan, debut)) = depart else {
            self.signaler(
                f,
                "Rien à lire après la tête de lecture : place-la sur un passage gardé.",
                false,
            );
            return;
        };
        self.verrou().lecture = Some(debut);
        f.global::<DonneesEditeur>().set_lecture(true);
        let ips = f64::from(plan.ips);
        let suivi = self.clone();
        let fin = self.clone();
        self.lecteur.lire(
            dossier,
            plan,
            self.outils.sortie_son.clone(),
            move |k, image| {
                suivi.plus_tard(move |e, f| {
                    let Some(debut) = e.verrou().lecture else {
                        return;
                    };
                    if let Some(s) = e.verrou().session.as_mut() {
                        s.placer_au_temps(debut + k as f64 / ips);
                    }
                    Self::poser_image(f, Some(image));
                    e.afficher_temps(f);
                });
            },
            move || {
                fin.plus_tard(|e, f| {
                    e.verrou().lecture = None;
                    f.global::<DonneesEditeur>().set_lecture(false);
                    e.afficher(f);
                });
            },
        );
    }

    fn enregistrer(&self) {
        let e = self.verrou();
        let (Some(s), Some(dossier)) = (e.session.as_ref(), e.dossier.clone()) else {
            return;
        };
        if s.modifie {
            let _ = verrou(&self.ecriture).send((dossier, s.id.clone(), s.montage().clone()));
        }
    }

    pub fn operer(self: &Arc<Self>, f: &Principale, op: Operation) {
        self.arreter_lecture(f);
        let resultat = match self.verrou().session.as_mut() {
            Some(s) => s.executer(op),
            None => return,
        };
        match resultat {
            Ok(true) => {
                self.signaler(f, "", false);
                self.enregistrer();
                self.afficher(f);
                self.montrer_image();
            }
            Ok(false) => self.afficher(f),
            Err(message) => self.signaler(f, message, true),
        }
    }

    fn deplacer_tete(self: &Arc<Self>, f: &Principale, action: impl FnOnce(&mut SessionMontage)) {
        self.arreter_lecture(f);
        if let Some(s) = self.verrou().session.as_mut() {
            action(s);
        }
        self.afficher(f);
        self.montrer_image();
    }

    pub fn placer(self: &Arc<Self>, f: &Principale, fraction: f64) {
        self.deplacer_tete(f, |s| s.placer(fraction));
    }

    pub fn choisir(self: &Arc<Self>, f: &Principale, i: usize) {
        self.deplacer_tete(f, |s| s.choisir(i));
    }

    pub fn image_voisine(self: &Arc<Self>, f: &Principale, n: i64) {
        self.deplacer_tete(f, |s| s.image_voisine(n));
    }

    pub fn fermer(self: &Arc<Self>, f: &Principale) {
        self.arreter_lecture(f);
        self.lecteur.arreter();
        self.enregistrer();
        {
            let mut e = self.verrou();
            e.session = None;
            e.protections.clear();
            e.ouverture += 1;
        }
        let d = f.global::<DonneesEditeur>();
        d.set_ouvert(false);
        d.set_export_ouvert(false);
        d.set_choix_clip(false);
        self.outils.galerie.ouvrir(f);
    }

    pub fn touche(
        self: &Arc<Self>,
        f: &Principale,
        texte: &str,
        ctrl: bool,
        maj: bool,
        alt: bool,
    ) -> bool {
        let nom = nom_touche(texte);
        let action = action_clavier(
            Touche {
                touche: &nom,
                ctrl,
                meta: false,
                maj,
                alt,
            },
            false,
        );
        let Some(action) = action else {
            return false;
        };
        match action {
            Action::Lecture => self.basculer_lecture(f),
            Action::Precedente => self.image_voisine(f, -1),
            Action::Suivante => self.image_voisine(f, 1),
            Action::Couper => self.operer(f, Operation::Couper),
            Action::Basculer => self.operer(f, Operation::Basculer),
            Action::Annuler => self.operer(f, Operation::Annuler),
            Action::Retablir => self.operer(f, Operation::Retablir),
            Action::Entree => self.operer(f, Operation::Entree),
            Action::Sortie => self.operer(f, Operation::Sortie),
        }
        true
    }

    pub fn proposer_clips(&self, f: &Principale) {
        let presents: Vec<String> =
            self.session(|s| s.map(|s| clips_de(s.montage())).unwrap_or_default());
        let ajoutables: Vec<ClipAjoutable> = self
            .outils
            .galerie
            .clips()
            .into_iter()
            .filter(|c| {
                c.chemin
                    .file_name()
                    .is_some_and(|n| !presents.iter().any(|p| p.as_str() == n.to_string_lossy()))
            })
            .map(|c| ClipAjoutable {
                chemin: c.chemin.to_string_lossy().as_ref().into(),
                nom: c.nom.as_str().into(),
            })
            .collect();
        let d = f.global::<DonneesEditeur>();
        d.set_ajoutables(ModelRc::new(VecModel::from(ajoutables)));
        d.set_choix_clip(true);
    }

    pub fn ajouter_clip(self: &Arc<Self>, chemin: PathBuf) {
        self.sur_un_fil("ajout", move |e| {
            let commande = e.commande();
            let nom = chemin
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let resultat = montage::sonder(&commande, &chemin);
            e.plus_tard(move |e, f| match resultat {
                Ok(infos) => {
                    let dossier = e.verrou().dossier.clone();
                    let source = Source::de(&nom, &infos);
                    let pistes: Vec<(String, usize)> =
                        (0..source.pistes).map(|p| (nom.clone(), p)).collect();
                    e.operer(f, Operation::AjouterClip(source, infos));
                    if let Some(d) = dossier {
                        let protection = e.outils.galerie.proteger(d.join(&nom));
                        e.verrou().protections.push(protection);
                        e.charger_ondes(d, pistes);
                    }
                }
                Err(message) => e.signaler(f, message, true),
            });
        });
    }

    fn afficher_export(&self, f: &Principale) {
        let Some(v) = self.session(|s| s.map(SessionMontage::vue_export)) else {
            return;
        };
        let d = f.global::<DonneesEditeur>();
        d.set_hauteurs(choix(v.hauteurs.iter().map(|h| (h.0.clone(), h.2))));
        d.set_cadences(choix(v.cadences.iter().map(|c| (c.0.clone(), c.2))));
        d.set_export_format(v.format.as_str().into());
    }

    pub fn demander_export(&self, f: &Principale) {
        self.afficher_export(f);
        f.global::<DonneesEditeur>().set_export_ouvert(true);
    }

    pub fn choisir_format(
        self: &Arc<Self>,
        f: &Principale,
        hauteur: Option<usize>,
        cadence: Option<usize>,
    ) {
        let Some((v, sortie)) = self.session(|s| s.map(|s| (s.vue_export(), s.montage().sortie)))
        else {
            return;
        };
        let hauteur = hauteur
            .and_then(|i| v.hauteurs.get(i))
            .map_or(sortie.hauteur, |h| h.1);
        let ips = cadence
            .and_then(|i| v.cadences.get(i))
            .map_or(sortie.ips, |c| Some(c.1));
        self.operer(f, Operation::Format { hauteur, ips });
        self.afficher_export(f);
    }

    pub fn exporter(self: &Arc<Self>, f: &Principale) {
        self.arreter_lecture(f);
        let preparation = {
            let e = self.verrou();
            let (Some(s), Some(dossier)) = (e.session.as_ref(), e.dossier.clone()) else {
                return;
            };
            (dossier, s.montage().clone())
        };
        let (dossier, projet) = preparation;
        if let Some(s) = self.verrou().session.as_mut() {
            s.modifie = true;
        }
        self.enregistrer();
        f.global::<DonneesEditeur>().set_export_ouvert(false);
        let nom = projet.nom.clone();
        let textes = projet.textes.len();
        let a_exporter = Montage {
            textes: Vec::new(),
            ..projet
        };
        let id = self.compteur.fetch_add(1, Ordering::Relaxed) + 1;
        let (annule, enfant) = (Arc::new(AtomicBool::new(false)), Enfant::default());
        let chemins: Vec<PathBuf> = clips_de(&a_exporter)
            .iter()
            .map(|c| dossier.join(c))
            .collect();
        let protections: Vec<Protection> = chemins
            .iter()
            .map(|c| self.outils.galerie.proteger(c.clone()))
            .collect();
        let travail = self.outils.galerie.commencer_travail(
            chemins[0].clone(),
            nom_travail(&nom, false),
            annule.clone(),
        );
        self.exports.suivre(
            EtatExport {
                id,
                nom: nom.clone(),
                pct: 0,
                logiciel: false,
            },
            annule.clone(),
            enfant.clone(),
        );
        self.verrou().export = Some(id);
        let d = f.global::<DonneesEditeur>();
        d.set_export_en_cours(true);
        d.set_export_libelle(nom_travail(&nom, false).into());
        d.set_export_progression(0.0);
        self.signaler(f, "", false);
        self.sur_un_fil("export", move |e| {
            let fini = Arc::new(AtomicBool::new(false));
            let veille = (fini.clone(), annule.clone(), enfant.clone());
            let _ = std::thread::Builder::new()
                .name("estompe-montage-veille".into())
                .spawn(move || {
                    let (fini, annule, enfant) = veille;
                    while !fini.load(Ordering::SeqCst) {
                        if annule.load(Ordering::SeqCst) {
                            arreter(&enfant);
                        }
                        std::thread::sleep(SCRUTATION);
                    }
                });
            let commande = e.commande();
            let encodeurs = e.outils.encodeurs.clone().unwrap_or_else(|| {
                commande()
                    .ok()
                    .and_then(|mut c| c.arg("-encoders").stdin(Stdio::null()).output().ok())
                    .map(|o| encodeurs_export(&String::from_utf8_lossy(&o.stdout)))
                    .unwrap_or_default()
            });
            let demande = Demande {
                id,
                dossier: &dossier,
                montage: &a_exporter,
                images: &[],
                quota: e.outils.service.reglages().replay.quota_octets,
                encodeurs: &encodeurs,
                date: e.outils.horloge.as_ref().map_or_else(heure_locale, |h| h()),
            };
            let resultat = montage::exporter(&commande, &demande, &annule, &enfant, &mut |pct, logiciel| {
                if e.exports.avancer(id, pct, logiciel).is_some() {
                    let libelle = nom_travail(&nom, logiciel);
                    e.outils.galerie.avancer_travail(travail, Some(libelle.clone()), f32::from(pct) / 100.0);
                    e.plus_tard(move |e, f| {
                        if e.verrou().export == Some(id) {
                            let d = f.global::<DonneesEditeur>();
                            d.set_export_libelle(libelle.into());
                            d.set_export_progression(f32::from(pct) / 100.0);
                        }
                    });
                }
            });
            fini.store(true, Ordering::SeqCst);
            e.exports.terminer(id);
            drop(protections);
            let (message, erreur) = match &resultat {
                Ok(fait) => {
                    let fichier = fait.chemin.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let mut m = format!("Montage exporté : « {fichier} » est dans la galerie.");
                    if fait.logiciel {
                        m += " Encodeur matériel indisponible ou en échec : l'export a été fait par l'encodeur logiciel.";
                    }
                    if fait.quota_atteint {
                        m += " Quota du dossier des clips atteint : supprime d'anciens clips pour continuer à enregistrer.";
                    }
                    if textes > 0 {
                        m += " Les blocs de texte n'ont pas été incrustés.";
                    }
                    (m, false)
                }
                Err(Echec::Annule) => (format!("Export de « {nom} » annulé."), false),
                Err(Echec::Erreur(m)) => (m.clone(), true),
            };
            e.outils.galerie.finir_travail(travail, if erreur { message.clone() } else { String::new() });
            e.plus_tard(move |e, f| {
                let mien = {
                    let mut etat = e.verrou();
                    let mien = etat.export == Some(id);
                    if mien {
                        etat.export = None;
                    }
                    mien
                };
                if mien {
                    f.global::<DonneesEditeur>().set_export_en_cours(false);
                    e.signaler(f, message, erreur);
                }
                e.lister_projets();
            });
        });
    }

    pub fn annuler_export(&self) {
        if let Some(id) = self.verrou().export {
            let _ = self.exports.annuler(id);
        }
    }

    pub fn lister_projets(self: &Arc<Self>) {
        self.sur_un_fil("projets", |e| {
            let liste = e
                .outils
                .service
                .dossier()
                .map(|d| montage::projets(&d))
                .unwrap_or_default();
            e.plus_tard(move |_, f| {
                let lignes: Vec<ProjetMontage> = liste
                    .iter()
                    .map(|r| {
                        let (infos, manquants) = infos_projet(r);
                        ProjetMontage {
                            id: r.id.as_str().into(),
                            nom: r.nom.as_str().into(),
                            infos: infos.into(),
                            manquants: manquants.into(),
                        }
                    })
                    .collect();
                f.global::<DonneesEditeur>()
                    .set_projets(ModelRc::new(VecModel::from(lignes)));
            });
        });
    }

    fn nom_projet(&self, f: &Principale, id: &str) -> Option<String> {
        let projets = f.global::<DonneesEditeur>().get_projets();
        let n = slint::Model::row_count(&projets);
        (0..n)
            .filter_map(|i| slint::Model::row_data(&projets, i))
            .find(|p| p.id.as_str() == id)
            .map(|p| p.nom.to_string())
    }

    pub fn demander_renommage(self: &Arc<Self>, f: &Principale, id: &str) {
        let Some(nom) = self.nom_projet(f, id) else {
            return;
        };
        let editeur = self.clone();
        let id = id.to_string();
        confirmation::saisir(
            f,
            Question {
                titre: "Renommer le montage".into(),
                action: "Renommer".into(),
                saisie: Some(Saisie {
                    libelle: "Nom du montage".into(),
                    valeur: nom,
                    exiger: None,
                }),
                ..Question::default()
            },
            move |_, saisi| {
                let Some(saisi) = saisi.filter(|n| !nettoyer(n).is_empty()) else {
                    return;
                };
                editeur.sur_un_fil("renommage", move |e| {
                    let resultat = e
                        .outils
                        .service
                        .dossier()
                        .and_then(|d| montage::renommer_projet(&d, &id, &saisi));
                    e.apres_projet(resultat.map_err(|m| format!("Renommage impossible : {m}.")));
                });
            },
        );
    }

    pub fn demander_suppression(self: &Arc<Self>, f: &Principale, id: &str) {
        let Some(nom) = self.nom_projet(f, id) else {
            return;
        };
        let editeur = self.clone();
        let id = id.to_string();
        confirmation::confirmer(
            f,
            Question {
                titre: "Supprimer le montage".into(),
                message: format!("Supprimer le montage « {nom} » ? Les clips et les fichiers déjà exportés restent."),
                action: "Supprimer".into(),
                destructif: true,
                ..Question::default()
            },
            move |_, ok| {
                if !ok {
                    return;
                }
                editeur.sur_un_fil("suppression", move |e| {
                    let resultat = e.outils.service.dossier().and_then(|d| montage::supprimer_projet(&d, &id));
                    e.apres_projet(resultat.map_err(|m| format!("Suppression impossible : {m}.")));
                });
            },
        );
    }

    fn apres_projet(self: &Arc<Self>, resultat: Result<(), String>) {
        if let Err(message) = resultat {
            self.outils.galerie.signaler_plus_tard(message);
        }
        self.lister_projets();
    }
}

fn ecrivain() -> Sender<(PathBuf, String, Montage)> {
    let (envoi, reception) = mpsc::channel::<(PathBuf, String, Montage)>();
    let _ = std::thread::Builder::new()
        .name("estompe-montage-ecriture".into())
        .spawn(move || {
            while let Ok(premier) = reception.recv() {
                let mut derniers: BTreeMap<String, (PathBuf, Montage)> = BTreeMap::new();
                derniers.insert(premier.1, (premier.0, premier.2));
                while let Ok((dossier, id, m)) = reception.try_recv() {
                    derniers.insert(id, (dossier, m));
                }
                for (id, (dossier, m)) in derniers {
                    let _ = montage::enregistrer(&dossier, &id, &m);
                }
            }
        });
    envoi
}

pub fn brancher(f: &Principale, outils: OutilsEditeur) -> Arc<Editeur> {
    if let Ok(dossier) = outils.service.dossier() {
        montage::purger_partiels(&dossier);
    }
    let editeur = Arc::new(Editeur {
        lecteur: Lecteur::nouveau(outils.ffmpeg.clone()),
        outils,
        fenetre: f.as_weak(),
        etat: Mutex::new(EtatEditeur::default()),
        exports: Exports::default(),
        ecriture: Mutex::new(ecrivain()),
        compteur: AtomicU64::new(0),
    });
    let ouvrir = editeur.clone();
    editeur.outils.galerie.definir_editeur(
        f,
        Some(Arc::new(move |f: &Principale, clip: &Clip| {
            ouvrir.ouvrir_clip(f, clip)
        })),
    );
    let d = f.global::<DonneesEditeur>();
    macro_rules! rappel {
        ($methode:ident, |$e:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let $e = editeur.clone();
            let faible = f.as_weak();
            d.$methode(move |$($arg),*| {
                if let Some($f) = faible.upgrade() {
                    $corps;
                }
            });
        }};
    }
    rappel!(on_fermer, |e, f| e.fermer(&f));
    rappel!(on_placer, |e, f, fraction| e
        .placer(&f, f64::from(fraction)));
    rappel!(on_choisir, |e, f, i| e.choisir(&f, i.max(0) as usize));
    rappel!(on_poignee, |e, f, debut, fraction| e.operer(
        &f,
        Operation::Poignee {
            debut,
            fraction: f64::from(fraction)
        }
    ));
    rappel!(on_couper, |e, f| e.operer(&f, Operation::Couper));
    rappel!(on_basculer, |e, f| e.operer(&f, Operation::Basculer));
    rappel!(on_entree_ici, |e, f| e.operer(&f, Operation::Entree));
    rappel!(on_sortie_ici, |e, f| e.operer(&f, Operation::Sortie));
    rappel!(on_vitesse, |e, f, i| e
        .operer(&f, Operation::Vitesse(i.max(0) as usize)));
    rappel!(on_muet_segment, |e, f, muet| e
        .operer(&f, Operation::MuetSegment(muet)));
    rappel!(on_volume, |e, f, piste, volume| e.operer(
        &f,
        Operation::Volume(piste.max(0) as usize, f64::from(volume))
    ));
    rappel!(on_muet_piste, |e, f, piste, muet| e
        .operer(&f, Operation::MuetPiste(piste.max(0) as usize, muet)));
    rappel!(on_annuler, |e, f| e.operer(&f, Operation::Annuler));
    rappel!(on_retablir, |e, f| e.operer(&f, Operation::Retablir));
    rappel!(on_lire, |e, f| e.basculer_lecture(&f));
    rappel!(on_precedente, |e, f| e.image_voisine(&f, -1));
    rappel!(on_suivante, |e, f| e.image_voisine(&f, 1));
    rappel!(on_renommer, |e, f, nom| e
        .operer(&f, Operation::Renommer(nom.to_string())));
    rappel!(on_rapport, |e, f, i| e
        .operer(&f, Operation::Rapport(i.max(0) as usize)));
    rappel!(on_deplacer, |e, f, sens| e
        .operer(&f, Operation::Deplacer(sens as isize)));
    rappel!(on_retirer_clip, |e, f| e.operer(&f, Operation::RetirerClip));
    rappel!(on_proposer_clips, |e, f| e.proposer_clips(&f));
    rappel!(on_ajouter_clip, |e, _f, chemin| e
        .ajouter_clip(PathBuf::from(chemin.as_str())));
    rappel!(on_demander_export, |e, f| e.demander_export(&f));
    rappel!(on_choisir_hauteur, |e, f, i| e.choisir_format(
        &f,
        usize::try_from(i).ok(),
        None
    ));
    rappel!(on_choisir_cadence, |e, f, i| e.choisir_format(
        &f,
        None,
        usize::try_from(i).ok()
    ));
    rappel!(on_exporter, |e, f| e.exporter(&f));
    rappel!(on_annuler_export, |e, _f| e.annuler_export());
    rappel!(on_lister_projets, |e, _f| e.lister_projets());
    rappel!(on_ouvrir_projet, |e, f, id| e.ouvrir_projet(&f, &id));
    rappel!(on_renommer_projet, |e, f, id| e.demander_renommage(&f, &id));
    rappel!(on_supprimer_projet, |e, f, id| e
        .demander_suppression(&f, &id));
    {
        let e = editeur.clone();
        let faible = f.as_weak();
        d.on_touche(move |texte, ctrl, maj, alt| {
            faible
                .upgrade()
                .is_some_and(|f| e.touche(&f, &texte, ctrl, maj, alt))
        });
    }
    editeur
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suivi_des_exports() {
        let exports = Exports::default();
        let (annule, enfant) = (Arc::new(AtomicBool::new(false)), Enfant::default());
        exports.suivre(
            EtatExport {
                id: 1,
                nom: "n".into(),
                pct: 0,
                logiciel: false,
            },
            annule.clone(),
            enfant,
        );
        assert_eq!(exports.avancer(1, 0, false), None);
        assert_eq!(exports.avancer(1, 40, false).unwrap().pct, 40);
        assert_eq!(exports.avancer(1, 40, false), None);
        assert!(exports.avancer(1, 0, true).unwrap().logiciel);
        assert_eq!(exports.en_cours()[0].pct, 0);
        assert!(exports.annuler(2).is_err());
        exports.annuler(1).unwrap();
        assert!(annule.load(Ordering::SeqCst));
        exports.terminer(1);
        assert!(exports.en_cours().is_empty() && exports.avancer(1, 50, false).is_none());
    }

    #[test]
    fn touches_de_slint_vers_les_noms_du_clavier() {
        assert_eq!(
            nom_touche(&char::from(Key::LeftArrow).to_string()),
            "ArrowLeft"
        );
        assert_eq!(
            nom_touche(&char::from(Key::RightArrow).to_string()),
            "ArrowRight"
        );
        assert_eq!(nom_touche(&char::from(Key::Delete).to_string()), "Delete");
        assert_eq!(
            (nom_touche(" "), nom_touche("c"), nom_touche("ab")),
            (" ".into(), "c".into(), "ab".into())
        );
    }

    #[test]
    fn resume_d_un_projet_dans_la_galerie() {
        let r = Resume {
            id: "p".into(),
            nom: "Soirée".into(),
            date: "2026-03-08T21:14:00Z".into(),
            modifie: 0,
            duree_s: 6.0,
            clips: vec!["a.mp4".into(), "b.mp4".into()],
            manquants: vec!["b.mp4".into()],
        };
        assert_eq!(
            infos_projet(&r),
            (
                "0:06,00 · 2 clips · 8 mars 2026 à 21:14".into(),
                "Clip introuvable : b.mp4".into()
            )
        );
        let seul = Resume {
            clips: vec!["a.mp4".into()],
            manquants: vec![],
            ..r
        };
        assert_eq!(
            infos_projet(&seul).0,
            "0:06,00 · 1 clip · 8 mars 2026 à 21:14"
        );
    }
}
