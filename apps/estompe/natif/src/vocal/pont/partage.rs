use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use estompe_client::vocal::{
    note_resolution, resume_partage, CleVideo, ImagePartage, PlafondPartage, ReglagesPartage,
    ResolutionPartage, CADENCES_PARTAGE, PREREGLAGES,
};
use estompe_plateforme::capture::HAUTEUR_MINIATURE;
use estompe_plateforme::{
    cible_du_son, EvenementCapture, FluxCapture, Image, OptionsCapture, SourceCapture, TypeSource,
};
use slint::{ComponentHandle, ModelRc, SharedPixelBuffer, VecModel};

use super::{Interne, PontVocal};
use crate::coquille::pont::vec_de;
use crate::{
    ChoixPartageVocal, DonneesVocal, EtapePartage, OptionPartage, Principale, SourcePartageVocal,
};

pub const PERIODE_MINIATURES: Duration = Duration::from_secs(2);
pub const IPS_APERCU_MAX: u32 = 30;
const MESSAGE_LISTE: &str = "Impossible de lister les écrans et fenêtres.";
const MESSAGE_APERCU: &str = "Aperçu impossible : la source n'est plus disponible.";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Etape {
    #[default]
    Source,
    Apercu,
    Qualite,
}

#[derive(Default)]
pub struct ChoixPartage {
    pub ouvert: bool,
    pub etape: Etape,
    pub fenetres: bool,
    pub chargement: bool,
    pub indisponible: bool,
    pub erreur: String,
    pub sources: Vec<SourceCapture>,
    pub miniatures: HashMap<String, Image>,
    pub version: u64,
    pub choisie: Option<SourceCapture>,
    pub apercu: Option<FluxCapture>,
    pub generation: u64,
    pub generation_apercu: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmbrePartage {
    ouvert: bool,
    etape: Etape,
    fenetres: bool,
    chargement: bool,
    indisponible: bool,
    erreur: String,
    sources: Vec<String>,
    version: u64,
    choisie: Option<String>,
    reglages: ReglagesPartage,
    plafond: PlafondPartage,
    changer: bool,
    apercu: i32,
    son: Option<&'static str>,
}

pub fn cle_apercu_partage() -> CleVideo {
    CleVideo::partage("#apercu")
}

fn proposee(source: &SourceCapture, fenetres: bool) -> bool {
    match source.genre {
        TypeSource::Ecran => !fenetres,
        TypeSource::Fenetre => fenetres && !source.estompe,
    }
}

fn detail(source: &SourceCapture) -> String {
    if source.application.is_empty() {
        format!("{} × {}", source.largeur, source.hauteur)
    } else {
        source.application.clone()
    }
}

pub fn option_son(disponible: bool, source: Option<&SourceCapture>) -> Option<&'static str> {
    let source = source?;
    if !disponible || cible_du_son(source, 0).is_none() {
        return None;
    }
    Some(match source.genre {
        TypeSource::Fenetre => "Partager le son de cette fenêtre",
        TypeSource::Ecran => "Partager le son de l'ordinateur (sans la voix des participants)",
    })
}

fn image_slint(image: &Image) -> slint::Image {
    slint::Image::from_rgba8(SharedPixelBuffer::clone_from_slice(
        &image.rgba,
        image.largeur,
        image.hauteur,
    ))
}

fn option(libelle: String, indice: &str, choisi: bool, actif: bool) -> OptionPartage {
    OptionPartage {
        libelle: libelle.into(),
        indice: indice.into(),
        choisi,
        actif,
    }
}

pub fn options_de_qualite(
    reglages: ReglagesPartage,
    serveur: PlafondPartage,
    hauteur_source: Option<u32>,
) -> (
    Vec<OptionPartage>,
    Vec<OptionPartage>,
    Vec<OptionPartage>,
    String,
) {
    let effectif = serveur.avec_source(hauteur_source);
    let q = reglages.qualite(effectif);
    let prereglage = reglages.prereglage(effectif);
    let prereglages = PREREGLAGES
        .iter()
        .enumerate()
        .map(|(i, p)| option(p.nom.to_string(), p.indice, prereglage == Some(i), true))
        .collect();
    let resolutions = ResolutionPartage::TOUTES
        .iter()
        .map(|r| {
            option(
                format!("{}{}", r.libelle(), note_resolution(*r, serveur, effectif)),
                "",
                *r == q.resolution,
                r.hauteur() <= effectif.hauteur,
            )
        })
        .collect();
    let cadences = CADENCES_PARTAGE
        .iter()
        .map(|f| {
            let refuse = *f > serveur.ips;
            option(
                format!(
                    "{f} i/s{}",
                    if refuse {
                        " (non proposé par le serveur)"
                    } else {
                        ""
                    }
                ),
                "",
                *f == q.ips,
                !refuse,
            )
        })
        .collect();
    (
        prereglages,
        resolutions,
        cadences,
        resume_partage(reglages, effectif),
    )
}

impl PontVocal {
    pub fn ouvrir_partage(&self, f: &Principale) {
        let apercu = {
            let mut i = self.verrou();
            if !i.etat.actif() {
                return;
            }
            let apercu = i.partage.apercu.take();
            i.partage = ChoixPartage {
                ouvert: true,
                chargement: true,
                generation: i.partage.generation + 1,
                generation_apercu: i.partage.generation_apercu + 1,
                ..ChoixPartage::default()
            };
            apercu
        };
        drop(apercu);
        self.videos_hors_session
            .recepteur(&cle_apercu_partage())
            .effacer();
        self.charger_sources();
        self.projeter(f);
    }

    pub fn ouvrir_qualite_partage(&self, f: &Principale) {
        {
            let mut i = self.verrou();
            if i.etat.partage.is_none() {
                return;
            }
            i.partage.ouvert = true;
            i.partage.etape = Etape::Qualite;
            i.partage.erreur.clear();
        }
        self.projeter(f);
    }

    pub fn fermer_partage(&self, f: &Principale) {
        let apercu = {
            let mut i = self.verrou();
            i.partage.ouvert = false;
            i.partage.generation += 1;
            i.partage.generation_apercu += 1;
            i.partage.choisie = None;
            i.partage.miniatures.clear();
            i.partage.apercu.take()
        };
        drop(apercu);
        self.videos_hors_session
            .recepteur(&cle_apercu_partage())
            .effacer();
        self.projeter(f);
    }

    pub fn partage_ouvert(&self) -> bool {
        self.verrou().partage.ouvert
    }

    pub fn onglet_partage(&self, f: &Principale, fenetres: bool) {
        self.verrou().partage.fenetres = fenetres;
        self.projeter(f);
    }

    pub fn actualiser_partage(&self, f: &Principale) {
        {
            let mut i = self.verrou();
            i.partage.generation += 1;
            i.partage.chargement = true;
            i.partage.erreur.clear();
        }
        self.charger_sources();
        self.projeter(f);
    }

    pub fn presenter_partage(
        &self,
        f: &Principale,
        sources: Vec<SourceCapture>,
        miniatures: Vec<(String, Image)>,
        erreur: Option<&str>,
    ) {
        {
            let mut i = self.verrou();
            i.partage.sources = sources;
            i.partage.miniatures = miniatures.into_iter().collect();
            i.partage.version += 1;
            i.partage.chargement = false;
            i.partage.erreur = erreur.unwrap_or_default().to_string();
        }
        self.projeter(f);
    }

    fn charger_sources(&self) {
        let (outils, generation) = {
            let mut i = self.verrou();
            let Some(outils) = i.outils.clone() else {
                return;
            };
            if !outils.ecrans.disponible() {
                i.partage.indisponible = true;
                i.partage.chargement = false;
                return;
            }
            (outils, i.partage.generation)
        };
        let pont = self.clone();
        self.runtime.spawn_blocking(move || {
            let liste = outils.ecrans.sources();
            let sources = {
                let mut i = pont.verrou();
                if i.partage.generation != generation {
                    return;
                }
                i.partage.chargement = false;
                match liste {
                    Ok(sources) => {
                        i.partage
                            .miniatures
                            .retain(|id, _| sources.iter().any(|s| &s.id == id));
                        i.partage.sources = sources.clone();
                        sources
                    }
                    Err(_) => {
                        i.partage.erreur = MESSAGE_LISTE.to_string();
                        Vec::new()
                    }
                }
            };
            pont.reprojeter();
            pont.rafraichir_miniatures(&outils.ecrans, generation, &sources);
        });
    }

    fn sur_la_liste(&self, generation: u64) -> bool {
        let i = self.verrou();
        i.partage.ouvert && i.partage.generation == generation && i.partage.etape == Etape::Source
    }

    fn rafraichir_miniatures(
        &self,
        ecrans: &Arc<dyn estompe_plateforme::CaptureEcran>,
        generation: u64,
        sources: &[SourceCapture],
    ) {
        while self.sur_la_liste(generation) {
            for source in sources {
                if !self.sur_la_liste(generation) {
                    return;
                }
                if let Ok(miniature) = ecrans.miniature(&source.id, HAUTEUR_MINIATURE) {
                    let mut i = self.verrou();
                    if i.partage.generation != generation {
                        return;
                    }
                    i.partage.miniatures.insert(source.id.clone(), miniature);
                    i.partage.version += 1;
                }
                self.reprojeter();
            }
            let debut = Instant::now();
            while debut.elapsed() < PERIODE_MINIATURES {
                if !self.sur_la_liste(generation) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }

    fn reprojeter(&self) {
        let pont = self.clone();
        let _ = self
            .fenetre
            .upgrade_in_event_loop(move |f| pont.projeter(&f));
    }

    pub fn choisir_source(&self, f: &Principale, id: &str) {
        {
            let mut i = self.verrou();
            let Some(source) = i.partage.sources.iter().find(|s| s.id == id).cloned() else {
                return;
            };
            i.partage.choisie = Some(source);
            i.partage.etape = Etape::Apercu;
            i.partage.erreur.clear();
        }
        self.demarrer_apercu_partage();
        self.projeter(f);
    }

    pub fn revenir_aux_sources(&self, f: &Principale) {
        let apercu = {
            let mut i = self.verrou();
            i.partage.etape = Etape::Source;
            i.partage.choisie = None;
            i.partage.generation += 1;
            i.partage.generation_apercu += 1;
            i.partage.chargement = i.partage.sources.is_empty();
            i.partage.apercu.take()
        };
        drop(apercu);
        self.videos_hors_session
            .recepteur(&cle_apercu_partage())
            .effacer();
        self.charger_sources();
        self.projeter(f);
    }

    fn demarrer_apercu_partage(&self) {
        let (outils, source, generation, ips, ancien) = {
            let mut i = self.verrou();
            i.partage.generation_apercu += 1;
            let ancien = i.partage.apercu.take();
            (
                i.outils.clone(),
                i.partage.choisie.clone(),
                i.partage.generation_apercu,
                i.etat.reglages_partage.ips.min(IPS_APERCU_MAX),
                ancien,
            )
        };
        drop(ancien);
        let recepteur = self.videos_hors_session.recepteur(&cle_apercu_partage());
        recepteur.effacer();
        let (Some(outils), Some(source)) = (outils, source) else {
            return;
        };
        let pont = self.clone();
        self.runtime.spawn_blocking(move || {
            let perte = pont.clone();
            let resultat = outils.ecrans.demarrer(
                &source.id,
                OptionsCapture { ips, curseur: true },
                Box::new(move |evenement| match evenement {
                    EvenementCapture::Trame(image) => {
                        recepteur.pousser(Arc::new(ImagePartage(image)));
                        true
                    }
                    EvenementCapture::Fermee | EvenementCapture::Echec(_) => {
                        perte.apercu_perdu(generation);
                        false
                    }
                    _ => true,
                }),
            );
            let mut i = pont.verrou();
            if i.partage.generation_apercu != generation {
                return;
            }
            match resultat {
                Ok(flux) => i.partage.apercu = Some(flux),
                Err(_) => {
                    drop(i);
                    pont.apercu_perdu(generation);
                }
            }
        });
    }

    fn apercu_perdu(&self, generation: u64) {
        let pont = self.clone();
        let _ = self.fenetre.upgrade_in_event_loop(move |f| {
            let apercu = {
                let mut i = pont.verrou();
                if i.partage.generation_apercu != generation || !i.partage.ouvert {
                    return;
                }
                i.partage.erreur = MESSAGE_APERCU.to_string();
                i.partage.apercu.take()
            };
            drop(apercu);
            pont.projeter(&f);
        });
    }

    fn reglages_partage(&self) -> ReglagesPartage {
        self.verrou().etat.reglages_partage
    }

    fn appliquer_reglages_partage(&self, f: &Principale, reglages: ReglagesPartage) {
        let reglages = reglages.borne();
        self.retenir(|p| p.partage = reglages);
        {
            let mut i = self.verrou();
            i.etat.reglages_partage = reglages;
            if let Some(flux) = &i.partage.apercu {
                flux.definir_cadence(reglages.ips.min(IPS_APERCU_MAX));
            }
        }
        if let Some(session) = self.session() {
            session.regler_partage(reglages);
        }
        self.projeter(f);
    }

    pub fn prereglage_partage(&self, f: &Principale, rang: i32) {
        let Some(p) = PREREGLAGES.get(rang.max(0) as usize) else {
            return;
        };
        let mut r = self.reglages_partage();
        r.appliquer(p);
        self.appliquer_reglages_partage(f, r);
    }

    pub fn resolution_partage(&self, f: &Principale, rang: i32) {
        let Some(resolution) = ResolutionPartage::TOUTES.get(rang.max(0) as usize) else {
            return;
        };
        let r = ReglagesPartage {
            resolution: *resolution,
            ..self.reglages_partage()
        };
        self.appliquer_reglages_partage(f, r);
    }

    pub fn cadence_partage(&self, f: &Principale, rang: i32) {
        let Some(ips) = CADENCES_PARTAGE.get(rang.max(0) as usize) else {
            return;
        };
        let r = ReglagesPartage {
            ips: *ips,
            ..self.reglages_partage()
        };
        self.appliquer_reglages_partage(f, r);
    }

    pub fn fluidite_partage(&self, f: &Principale, fluidite: bool) {
        let r = ReglagesPartage {
            fluidite,
            ..self.reglages_partage()
        };
        self.appliquer_reglages_partage(f, r);
    }

    pub fn auto_partage(&self, f: &Principale, auto: bool) {
        let r = ReglagesPartage {
            auto,
            ..self.reglages_partage()
        };
        self.appliquer_reglages_partage(f, r);
    }

    pub fn son_partage(&self, f: &Principale, son: bool) {
        let r = ReglagesPartage {
            son,
            ..self.reglages_partage()
        };
        self.appliquer_reglages_partage(f, r);
    }

    pub fn partager(&self, f: &Principale) {
        let (source, en_cours) = {
            let i = self.verrou();
            (i.partage.choisie.clone(), i.etat.partage.is_some())
        };
        let Some(source) = source else {
            return;
        };
        self.fermer_partage(f);
        if let Some(session) = self.session() {
            if en_cours {
                session.changer_source(source);
            } else {
                session.partager(source);
            }
        }
    }

    pub fn arreter_partage(&self) {
        if let Some(session) = self.session() {
            session.arreter_partage();
        }
    }

    pub(super) fn projeter_partage(&self, f: &Principale, i: &mut Interne) {
        let v = f.global::<DonneesVocal>();
        let p = &i.partage;
        let visibles: Vec<&SourceCapture> = p
            .sources
            .iter()
            .filter(|s| proposee(s, p.fenetres))
            .collect();
        let apercu = crate::video::indice(f, &cle_apercu_partage());
        let source_son = p
            .choisie
            .as_ref()
            .or(i.etat.partage.as_ref().map(|l| &l.source));
        let son = option_son(
            i.outils.as_ref().is_some_and(|o| o.sons.disponible()),
            source_son,
        );
        let ombre = OmbrePartage {
            ouvert: p.ouvert,
            etape: p.etape,
            fenetres: p.fenetres,
            chargement: p.chargement,
            indisponible: p.indisponible,
            erreur: p.erreur.clone(),
            sources: visibles.iter().map(|s| s.id.clone()).collect(),
            version: p.version,
            choisie: p.choisie.as_ref().map(|s| s.id.clone()),
            reglages: i.etat.reglages_partage,
            plafond: i.etat.plafond_partage,
            changer: i.etat.partage.is_some(),
            apercu,
            son,
        };
        if i.ombres.partage.as_ref() == Some(&ombre) {
            return;
        }
        let hauteur_source = p
            .choisie
            .as_ref()
            .or(i.etat.partage.as_ref().map(|l| &l.source))
            .map(|s| s.hauteur);
        let (prereglages, resolutions, cadences, resume) = options_de_qualite(
            i.etat.reglages_partage,
            i.etat.plafond_partage,
            hauteur_source,
        );
        v.set_partage(ChoixPartageVocal {
            ouvert: p.ouvert,
            etape: match p.etape {
                Etape::Source => EtapePartage::Source,
                Etape::Apercu => EtapePartage::Apercu,
                Etape::Qualite => EtapePartage::Qualite,
            },
            fenetres: p.fenetres,
            chargement: p.chargement,
            erreur: p.erreur.as_str().into(),
            indisponible: p.indisponible,
            vide: if p.fenetres {
                "Aucune fenêtre trouvée."
            } else {
                "Aucun écran trouvé."
            }
            .into(),
            source: p
                .choisie
                .as_ref()
                .map(|s| s.nom.as_str())
                .unwrap_or_default()
                .into(),
            detail: p
                .choisie
                .as_ref()
                .map(|s| format!("{} × {}", s.largeur, s.hauteur))
                .unwrap_or_default()
                .into(),
            apercu,
            prereglages: ModelRc::new(VecModel::from(prereglages)),
            resolutions: ModelRc::new(VecModel::from(resolutions)),
            cadences: ModelRc::new(VecModel::from(cadences)),
            fluidite: i.etat.reglages_partage.fluidite,
            auto: i.etat.reglages_partage.auto,
            resume: resume.into(),
            changer: i.etat.partage.is_some(),
            son_disponible: son.is_some(),
            son: i.etat.reglages_partage.son,
            libelle_son: son.unwrap_or_default().into(),
        });
        let changement_de_sources = i
            .ombres
            .partage
            .as_ref()
            .is_none_or(|o| o.sources != ombre.sources || o.version != ombre.version);
        if changement_de_sources {
            vec_de(&v.get_sources_partage()).set_vec(
                visibles
                    .iter()
                    .map(|s| {
                        let miniature = p.miniatures.get(&s.id);
                        SourcePartageVocal {
                            id: s.id.as_str().into(),
                            nom: s.nom.as_str().into(),
                            detail: detail(s).into(),
                            miniature: miniature.map(image_slint).unwrap_or_default(),
                            prete: miniature.is_some(),
                        }
                    })
                    .collect::<Vec<_>>(),
            );
        }
        i.ombres.partage = Some(ombre);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(id: &str, genre: TypeSource, estompe: bool) -> SourceCapture {
        SourceCapture {
            id: id.into(),
            nom: id.into(),
            application: String::new(),
            genre,
            largeur: 1920,
            hauteur: 1080,
            principale: false,
            processus: None,
            estompe,
        }
    }

    #[test]
    fn les_fenetres_d_estompe_ne_sont_pas_proposees() {
        assert!(proposee(
            &source("ecran:0", TypeSource::Ecran, false),
            false
        ));
        assert!(!proposee(
            &source("ecran:0", TypeSource::Ecran, false),
            true
        ));
        assert!(proposee(
            &source("fenetre:1", TypeSource::Fenetre, false),
            true
        ));
        assert!(!proposee(
            &source("fenetre:2", TypeSource::Fenetre, true),
            true
        ));
        assert_eq!(
            detail(&source("ecran:0", TypeSource::Ecran, false)),
            "1920 × 1080"
        );
    }

    #[test]
    fn l_option_du_son_est_masquee_sans_capture_du_son() {
        let fenetre = source("fenetre:1", TypeSource::Fenetre, false);
        let avec_processus = SourceCapture {
            processus: Some(12),
            ..fenetre.clone()
        };
        let ecran = source("ecran:0", TypeSource::Ecran, false);
        assert_eq!(option_son(false, Some(&ecran)), None);
        assert_eq!(option_son(false, Some(&avec_processus)), None);
        assert_eq!(option_son(true, Some(&fenetre)), None);
        assert_eq!(option_son(true, None), None);
        assert_eq!(
            option_son(true, Some(&avec_processus)),
            Some("Partager le son de cette fenêtre")
        );
        assert_eq!(
            option_son(true, Some(&ecran)),
            Some("Partager le son de l'ordinateur (sans la voix des participants)")
        );
    }

    #[test]
    fn les_options_grisent_ce_que_le_serveur_ou_l_ecran_refusent() {
        let serveur = PlafondPartage {
            ips: 60,
            hauteur: 2160,
        };
        let (prereglages, resolutions, cadences, resume) =
            options_de_qualite(ReglagesPartage::default(), serveur, Some(1080));
        assert_eq!(prereglages.len(), 3);
        assert!(prereglages.iter().all(|p| !p.choisi));
        let libelles: Vec<String> = resolutions.iter().map(|r| r.libelle.to_string()).collect();
        assert_eq!(
            libelles,
            [
                "720p",
                "1080p",
                "1440p (au-delà de ton écran)",
                "4K (au-delà de ton écran)"
            ]
        );
        assert!(resolutions[1].choisi && resolutions[1].actif && !resolutions[2].actif);
        assert_eq!(cadences[2].libelle, "120 i/s (non proposé par le serveur)");
        assert!(!cadences[2].actif && cadences[0].choisi);
        assert!(resume.starts_with("Partage : 1080p · 30 i/s"));
    }
}
