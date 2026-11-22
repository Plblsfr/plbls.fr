use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use estompe_client::jeux::ReglagesJeux;
use estompe_client::preferences::UsageTouche;
use estompe_client::replay::affichage::{
    duree_choisie, duree_max, duree_tampon, nom_codec, quota_choisi, taille, GO,
};
use estompe_client::replay::automate::Etat;
use estompe_client::replay::reglages::{Options, RESOLUTIONS};
use estompe_client::replay::types::{
    Codec, Disponibilite, EtatReplay, Qualite, ReglagesReplay, Resolution, CADENCES, DUREE_MAX_S,
};
use estompe_plateforme::libelle_touche;
use slint::{ComponentHandle, ModelRc, VecModel};

use super::ServiceReplay;
use crate::messagerie::pont::SystemeFichiers;
use crate::overlay::reglages::{choisir_touche, ChoixTouche};
use crate::overlay::replay::ServiceOverlayReplay;
use crate::session::Memoire;
use crate::{reglages_replay, ChoixReplay, DonneesReplay, Principale};

pub const QUALITES: [Qualite; 3] = [Qualite::Basse, Qualite::Moyenne, Qualite::Haute];
pub const QUOTA_MAX_GO: u64 = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choix {
    pub libelle: String,
    pub disponible: bool,
    pub choisi: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VueReplay {
    pub materiel_lu: bool,
    pub disponible: bool,
    pub raison: String,
    pub actif: bool,
    pub etat: String,
    pub resume: String,
    pub tourne: bool,
    pub touche: String,
    pub resolutions: Vec<Choix>,
    pub cadences: Vec<Choix>,
    pub hors_portee: String,
    pub qualite: i32,
    pub duree: i32,
    pub duree_max: i32,
    pub plafond: String,
    pub curseur: bool,
    pub micro: bool,
    pub codec: String,
    pub jeux: String,
    pub dossier: String,
    pub dossier_defaut: bool,
    pub quota: i32,
    pub quota_max: i32,
}

#[derive(Clone, Debug, Default)]
pub struct Materiel {
    pub disponibilite: Option<Disponibilite>,
    pub options: Option<Result<Options, String>>,
}

pub fn libelle_resolution(r: Resolution) -> &'static str {
    match r {
        Resolution::R720 => "720p",
        Resolution::R1080 => "1080p",
        Resolution::R1440 => "1440p",
        Resolution::R4k => "4K",
        Resolution::Native => "Native (celle du jeu)",
    }
}

pub fn texte_etat(r: &ReglagesReplay, etat: &EtatReplay) -> String {
    if !r.actif {
        return "Désactivé.".into();
    }
    let raison = |e: &Etat| e.raison().map(|r| format!(" : {r}")).unwrap_or_default();
    match &etat.etat {
        Etat::Arme => {
            let jeu = etat
                .jeu
                .as_ref()
                .map(|j| format!(" sur {j}"))
                .unwrap_or_default();
            let format = etat
                .format
                .map(|f| {
                    format!(
                        " : {}, {} × {}, {} i/s",
                        nom_codec(f.codec),
                        f.largeur,
                        f.hauteur,
                        f.ips
                    )
                })
                .unwrap_or_default();
            format!("Le tampon tourne{jeu}{format}.")
        }
        e @ Etat::Pause(_) => format!("En pause{}. Le tampon est gardé.", raison(e)),
        e @ Etat::Indisponible(_) => format!("Indisponible{}.", raison(e)),
        Etat::Inactif => {
            "En attente : le tampon s'arme dès qu'un jeu détecté passe au premier plan.".into()
        }
    }
}

fn mbits(kbps: u32) -> String {
    let dixiemes = (f64::from(kbps) / 100.0).round() / 10.0;
    format!("{dixiemes}").replace('.', ",")
}

pub fn texte_jeux(jeux: &ReglagesJeux) -> String {
    let pluriel = |n: usize, mot: &str| {
        if n > 1 {
            format!("{n} {mot}s")
        } else {
            format!("{n} {mot}")
        }
    };
    format!(
        "Jeux ajoutés à la main : {}, exclus : {}. Ils se règlent dans « Jeux et activité », pour la détection comme pour l'instant replay.",
        pluriel(jeux.ajoutes.len(), "jeu").replace("jeus", "jeux"),
        pluriel(jeux.exclus.len(), "jeu").replace("jeus", "jeux"),
    )
}

pub fn vue(
    r: &ReglagesReplay,
    etat: &EtatReplay,
    materiel: &Materiel,
    dossier: &Result<PathBuf, String>,
    jeux: &ReglagesJeux,
) -> VueReplay {
    let options = materiel.options.as_ref().and_then(|o| o.as_ref().ok());
    let disponible = materiel
        .disponibilite
        .as_ref()
        .is_some_and(|d| d.disponible);
    let resolutions: Vec<Choix> = RESOLUTIONS
        .iter()
        .map(|id| {
            let choix = options.and_then(|o| o.resolutions.iter().find(|c| c.id == *id));
            Choix {
                libelle: libelle_resolution(*id).into(),
                disponible: choix.is_none_or(|c| c.disponible),
                choisi: r.resolution == *id,
            }
        })
        .collect();
    let cadences: Vec<Choix> = CADENCES
        .iter()
        .map(|ips| {
            let choix = options.and_then(|o| o.cadences.iter().find(|c| c.ips == *ips));
            Choix {
                libelle: ips.to_string(),
                disponible: choix.is_none_or(|c| c.disponible),
                choisi: r.ips == *ips,
            }
        })
        .collect();
    let raisons: Vec<String> = options
        .map(|o| {
            let resolution = o
                .resolutions
                .iter()
                .find(|c| c.id == r.resolution && !c.disponible)
                .and_then(|c| c.raison.clone());
            let cadence = o
                .cadences
                .iter()
                .find(|c| c.ips == r.ips && !c.disponible)
                .and_then(|c| c.raison.clone());
            resolution.into_iter().chain(cadence).collect()
        })
        .unwrap_or_default();
    let hors_portee = if raisons.is_empty() {
        String::new()
    } else {
        format!(
            "Réglage hors de portée ({}) : le moteur applique le plus proche.",
            raisons.join(" ; ")
        )
    };
    let maximum = duree_max(options);
    let plafond = match options {
        Some(o) if maximum < DUREE_MAX_S => format!(
            "Durée plafonnée à {} au lieu de {}{}.",
            duree_tampon(maximum),
            duree_tampon(DUREE_MAX_S),
            o.raison_duree
                .as_ref()
                .map(|r| format!(" : {r}"))
                .unwrap_or_default()
        ),
        _ => String::new(),
    };
    let codec = match &materiel.options {
        Some(Ok(o)) => format!(
            "Codec utilisé : {}, environ {} Mbit/s ; mémoire du tampon : environ {}.{}",
            nom_codec(o.codec),
            mbits(o.debit_kbps),
            taille(o.memoire_octets),
            if o.codec == Codec::Hevc {
                " Ce réglage dépasse ce que l'encodeur H.264 sait faire : un clip HEVC est converti en H.264 à l'envoi dans un salon."
            } else {
                ""
            }
        ),
        Some(Err(e)) => format!("Plafonds du matériel illisibles : {e}"),
        None => String::new(),
    };
    let encodeur = materiel
        .disponibilite
        .as_ref()
        .and_then(|d| d.encodeur.as_ref())
        .map(|e| format!(" Encodeur : {e}."))
        .unwrap_or_default();
    let quota = r.quota_octets / GO;
    VueReplay {
        materiel_lu: materiel.disponibilite.is_some(),
        disponible,
        raison: materiel
            .disponibilite
            .as_ref()
            .and_then(|d| d.raison.clone())
            .unwrap_or_else(|| "aucun encodeur vidéo matériel n'a été trouvé".into()),
        actif: r.actif,
        etat: format!("{}{encodeur}", texte_etat(r, etat)),
        resume: texte_etat(r, etat),
        tourne: matches!(etat.etat, Etat::Arme | Etat::Pause(_)),
        touche: libelle_touche(&r.touche),
        resolutions,
        cadences,
        hors_portee,
        qualite: QUALITES.iter().position(|q| *q == r.qualite).unwrap_or(2) as i32,
        duree: r.duree_s.min(maximum) as i32,
        duree_max: maximum as i32,
        plafond,
        curseur: r.curseur,
        micro: r.micro,
        codec,
        jeux: texte_jeux(jeux),
        dossier: match dossier {
            Ok(d) => d.display().to_string(),
            Err(e) => e.clone(),
        },
        dossier_defaut: r.dossier.is_none(),
        quota: quota as i32,
        quota_max: quota.max(QUOTA_MAX_GO) as i32,
    }
}

fn modele(choix: &[Choix]) -> ModelRc<ChoixReplay> {
    ModelRc::new(VecModel::from(
        choix
            .iter()
            .map(|c| ChoixReplay {
                libelle: c.libelle.as_str().into(),
                disponible: c.disponible,
                choisi: c.choisi,
            })
            .collect::<Vec<_>>(),
    ))
}

pub fn afficher(f: &Principale, v: &VueReplay) {
    let g = f.global::<DonneesReplay>();
    g.set_materiel_lu(v.materiel_lu);
    g.set_disponible(v.disponible);
    g.set_raison(v.raison.as_str().into());
    g.set_actif(v.actif);
    g.set_etat(v.etat.as_str().into());
    g.set_resume(v.resume.as_str().into());
    g.set_tourne(v.tourne);
    g.set_touche(v.touche.as_str().into());
    g.set_resolutions(modele(&v.resolutions));
    g.set_cadences(modele(&v.cadences));
    g.set_hors_portee(v.hors_portee.as_str().into());
    g.set_qualite(v.qualite);
    g.set_duree_max(v.duree_max);
    g.set_duree(v.duree);
    g.set_plafond(v.plafond.as_str().into());
    g.set_curseur(v.curseur);
    g.set_micro(v.micro);
    g.set_codec(v.codec.as_str().into());
    g.set_jeux(v.jeux.as_str().into());
    g.set_dossier(v.dossier.as_str().into());
    g.set_dossier_defaut(v.dossier_defaut);
    g.set_quota_max(v.quota_max);
    g.set_quota(v.quota);
}

pub fn choisir_touche_replay(texte: &str, prises: &[String]) -> ChoixTouche {
    choisir_touche(texte, "", prises)
}

pub struct OutilsReglages {
    pub service: Arc<ServiceReplay>,
    pub memoire: Memoire,
    pub fichiers: SystemeFichiers,
    pub overlay: Option<Arc<ServiceOverlayReplay>>,
    pub touches_globales: bool,
}

pub struct ReglagesVue {
    outils: OutilsReglages,
    materiel: Mutex<Materiel>,
}

impl ReglagesVue {
    fn materiel(&self) -> MutexGuard<'_, Materiel> {
        self.materiel.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn vue(&self) -> VueReplay {
        let (r, jeux) = self
            .outils
            .memoire
            .lire(|p| (p.replay.clone(), p.jeux.clone()));
        vue(
            &r,
            &self.outils.service.actuel(),
            &self.materiel(),
            &self.outils.service.dossier(),
            &jeux,
        )
    }

    pub fn rafraichir(&self, f: &Principale) {
        afficher(f, &self.vue());
    }

    pub fn modifier(
        self: &Arc<Self>,
        f: &Principale,
        modification: impl FnOnce(&mut ReglagesReplay),
    ) {
        let avant = self.outils.memoire.lire(|p| p.replay.clone());
        let mut apres = avant.clone();
        modification(&mut apres);
        let apres = apres.borne();
        if apres == avant {
            self.rafraichir(f);
            return;
        }
        self.outils.memoire.modifier(|p| p.replay = apres.clone());
        self.outils
            .service
            .configurer(self.outils.memoire.lire(reglages_replay));
        if let Some(overlay) = &self.outils.overlay {
            overlay.definir_actif(apres.actif);
        }
        let options_changent = !apres.meme_capture(&avant) || apres.actif != avant.actif;
        self.rafraichir(f);
        if options_changent && self.materiel().disponibilite.is_some() {
            lire_le_materiel(self.clone(), f);
        }
    }

    pub fn touche(self: &Arc<Self>, f: &Principale, texte: &str) -> bool {
        let prises = self
            .outils
            .memoire
            .lire(|p| p.touches_des_autres(UsageTouche::Replay));
        let g = f.global::<DonneesReplay>();
        match choisir_touche_replay(texte, &prises) {
            ChoixTouche::Refusee(raison) => {
                g.set_erreur_touche(raison.into());
                false
            }
            ChoixTouche::Retenue(code) => {
                g.set_erreur_touche("".into());
                self.modifier(f, |r| r.touche = code);
                true
            }
        }
    }
}

pub fn lire_le_materiel(r: Arc<ReglagesVue>, f: &Principale) {
    let faible = f.as_weak();
    let _ = std::thread::Builder::new()
        .name("estompe-replay-materiel".into())
        .spawn(move || {
            let disponibilite = r.outils.service.disponibilite();
            let voulus = r.outils.memoire.lire(|p| p.replay.clone());
            let options = disponibilite
                .disponible
                .then(|| r.outils.service.options(&voulus));
            *r.materiel() = Materiel {
                disponibilite: Some(disponibilite),
                options,
            };
            let _ = faible.upgrade_in_event_loop(move |f| r.rafraichir(&f));
        });
}

pub fn brancher(
    f: &Principale,
    outils: OutilsReglages,
    runtime: &tokio::runtime::Handle,
) -> Arc<ReglagesVue> {
    let g = f.global::<DonneesReplay>();
    g.set_touches_globales(outils.touches_globales);
    g.set_choix_dossier(outils.fichiers.disponible());
    let mut etat = outils.service.etat();
    let r = Arc::new(ReglagesVue {
        outils,
        materiel: Mutex::new(Materiel::default()),
    });
    r.rafraichir(f);
    let faible = f.as_weak();
    let suivi = r.clone();
    runtime.spawn(async move {
        let mut jeu = etat.borrow_and_update().jeu.clone();
        while etat.changed().await.is_ok() {
            let nouveau = etat.borrow_and_update().jeu.clone();
            let relire = nouveau != jeu && nouveau.is_some();
            jeu = nouveau;
            let r = suivi.clone();
            if faible
                .upgrade_in_event_loop(move |f| {
                    r.rafraichir(&f);
                    if relire && f.global::<DonneesReplay>().get_ouvert() {
                        lire_le_materiel(r.clone(), &f);
                    }
                })
                .is_err()
            {
                break;
            }
        }
    });
    macro_rules! rappel {
        ($methode:ident, |$r:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let $r = r.clone();
            let faible = f.as_weak();
            g.$methode(move |$($arg),*| {
                if let Some($f) = faible.upgrade() {
                    $corps;
                }
            });
        }};
    }
    rappel!(on_ouvrir, |r, f| {
        let g = f.global::<DonneesReplay>();
        g.set_erreur_touche("".into());
        g.set_capture(false);
        r.rafraichir(&f);
        g.set_ouvert(true);
        lire_le_materiel(r.clone(), &f);
    });
    rappel!(on_fermer, |_r, f| f
        .global::<DonneesReplay>()
        .set_ouvert(false));
    rappel!(on_definir_actif, |r, f, actif| {
        let pret = r
            .materiel()
            .disponibilite
            .as_ref()
            .is_some_and(|d| d.disponible);
        if !actif || pret {
            r.modifier(&f, |o| o.actif = actif);
        }
    });
    rappel!(on_definir_resolution, |r, f, rang| {
        let v = r.vue();
        if let Some((id, c)) = usize::try_from(rang)
            .ok()
            .and_then(|i| RESOLUTIONS.get(i).zip(v.resolutions.get(i)))
        {
            if c.disponible {
                r.modifier(&f, |o| o.resolution = *id);
            }
        }
    });
    rappel!(on_definir_cadence, |r, f, rang| {
        let v = r.vue();
        if let Some((ips, c)) = usize::try_from(rang)
            .ok()
            .and_then(|i| CADENCES.get(i).zip(v.cadences.get(i)))
        {
            if c.disponible {
                r.modifier(&f, |o| o.ips = *ips);
            }
        }
    });
    rappel!(on_definir_qualite, |r, f, rang| {
        if let Some(q) = usize::try_from(rang).ok().and_then(|i| QUALITES.get(i)) {
            r.modifier(&f, |o| o.qualite = *q);
        }
    });
    rappel!(on_definir_duree, |r, f, valeur, fini| {
        if fini {
            let maximum = r.vue().duree_max.max(0) as u32;
            r.modifier(&f, |o| o.duree_s = duree_choisie(valeur, maximum));
        }
    });
    rappel!(on_definir_curseur, |r, f, curseur| r
        .modifier(&f, |o| o.curseur = curseur));
    rappel!(on_definir_micro, |r, f, micro| r
        .modifier(&f, |o| o.micro = micro));
    rappel!(on_definir_quota, |r, f, valeur, fini| {
        if fini {
            r.modifier(&f, |o| o.quota_octets = quota_choisi(valeur));
        }
    });
    rappel!(on_dossier_par_defaut, |r, f| r
        .modifier(&f, |o| o.dossier = None));
    rappel!(on_ouvrir_dossier, |r, _f| {
        if let Ok(dossier) = r.outils.service.dossier() {
            let fichiers = r.outils.fichiers.clone();
            let _ = std::thread::Builder::new()
                .name("estompe-replay-dossier".into())
                .spawn(move || {
                    if std::fs::create_dir_all(&dossier).is_ok() {
                        let _ = fichiers.ouvrir_dossier(&dossier);
                    }
                });
        }
    });
    rappel!(on_changer_dossier, |r, f| {
        let fichiers = r.outils.fichiers.clone();
        if !fichiers.disponible() {
            return;
        }
        let depart = r.outils.service.dossier().ok();
        let faible = f.as_weak();
        let r = r.clone();
        let _ = std::thread::Builder::new()
            .name("estompe-replay-dossier".into())
            .spawn(move || {
                let Ok(Some(chemin)) = fichiers.choisir_dossier(depart.as_deref()) else {
                    return;
                };
                let chemin = chemin.to_string_lossy().into_owned();
                let _ = faible.upgrade_in_event_loop(move |f| {
                    r.modifier(&f, |o| o.dossier = Some(chemin));
                });
            });
    });
    let r2 = r.clone();
    let faible = f.as_weak();
    g.on_touche_choisie(move |texte| faible.upgrade().is_some_and(|f| r2.touche(&f, &texte)));
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_client::preferences::Preferences;
    use estompe_client::replay::automate::Pause;
    use estompe_client::replay::reglages::{ChoixCadence, ChoixResolution};
    use estompe_client::replay::tampon::Format;
    use estompe_client::replay::types::DUREE_MIN_S;

    fn options() -> Options {
        Options {
            resolutions: RESOLUTIONS
                .iter()
                .map(|id| ChoixResolution {
                    id: *id,
                    disponible: *id != Resolution::R4k,
                    raison: (*id == Resolution::R4k)
                        .then(|| "plus grand que l'image du jeu (1920 × 1080)".into()),
                })
                .collect(),
            cadences: CADENCES
                .iter()
                .map(|ips| ChoixCadence {
                    ips: *ips,
                    disponible: *ips <= 60,
                    raison: (*ips > 60).then(|| "écran en 60 Hz".into()),
                })
                .collect(),
            duree_max_s: 90,
            raison_duree: Some("mémoire libre insuffisante (1.0 Go libres)".into()),
            codec: Codec::H264,
            debit_kbps: 12000,
            memoire_octets: 200 * 1024 * 1024,
        }
    }

    fn materiel() -> Materiel {
        Materiel {
            disponibilite: Some(Disponibilite {
                disponible: true,
                raison: None,
                encodeur: Some("AMD AMF".into()),
            }),
            options: Some(Ok(options())),
        }
    }

    #[test]
    fn la_vue_reprend_les_reglages_et_les_plafonds_du_materiel() {
        let r = ReglagesReplay {
            actif: true,
            resolution: Resolution::R4k,
            ips: 120,
            duree_s: 300,
            ..ReglagesReplay::default()
        };
        let etat = EtatReplay {
            etat: Etat::Arme,
            jeu: Some("Counter-Strike 2".into()),
            format: Some(Format {
                codec: Codec::H264,
                largeur: 1920,
                hauteur: 1080,
                ips: 60,
                micro: false,
            }),
            ..EtatReplay::default()
        };
        let jeux = ReglagesJeux {
            ajoutes: vec![r"C:\Jeux\a.exe".into()],
            exclus: vec!["b.exe".into(), "c.exe".into()],
            ..ReglagesJeux::default()
        };
        let v = vue(&r, &etat, &materiel(), &Ok("/v/Estompe".into()), &jeux);
        assert!(v.materiel_lu && v.disponible && v.actif && v.tourne);
        assert_eq!(
            v.etat,
            "Le tampon tourne sur Counter-Strike 2 : H.264, 1920 × 1080, 60 i/s. Encodeur : AMD AMF."
        );
        assert!(!v.resolutions[3].disponible && v.resolutions[3].choisi);
        assert!(!v.cadences[2].disponible && v.cadences[2].choisi);
        assert_eq!(
            v.hors_portee,
            "Réglage hors de portée (plus grand que l'image du jeu (1920 × 1080) ; écran en 60 Hz) : le moteur applique le plus proche."
        );
        assert_eq!((v.duree, v.duree_max), (90, 90));
        assert_eq!(
            v.plafond,
            "Durée plafonnée à 1 min 30 au lieu de 5 min : mémoire libre insuffisante (1.0 Go libres)."
        );
        assert_eq!(
            v.codec,
            "Codec utilisé : H.264, environ 12 Mbit/s ; mémoire du tampon : environ 200 Mo."
        );
        assert_eq!(
            v.jeux,
            "Jeux ajoutés à la main : 1 jeu, exclus : 2 jeux. Ils se règlent dans « Jeux et activité », pour la détection comme pour l'instant replay."
        );
        assert_eq!((v.dossier.as_str(), v.dossier_defaut), ("/v/Estompe", true));
        assert_eq!((v.quota, v.quota_max, v.qualite), (20, 200, 2));
        assert_eq!(v.touche, "F10");
    }

    #[test]
    fn etats_du_moteur_et_materiel_absent() {
        let mut r = ReglagesReplay::default();
        let etat = |e| EtatReplay {
            etat: e,
            ..EtatReplay::default()
        };
        assert_eq!(texte_etat(&r, &etat(Etat::Arme)), "Désactivé.");
        r.actif = true;
        assert_eq!(
            texte_etat(&r, &etat(Etat::Pause(Pause::Partage))),
            "En pause : le partage d'écran est prioritaire. Le tampon est gardé."
        );
        assert_eq!(
            texte_etat(&r, &etat(Etat::Indisponible("x".into()))),
            "Indisponible : x."
        );
        assert!(texte_etat(&r, &etat(Etat::Inactif)).starts_with("En attente"));
        let v = vue(
            &r,
            &etat(Etat::Inactif),
            &Materiel::default(),
            &Err("dossier introuvable".into()),
            &ReglagesJeux::default(),
        );
        assert!(!v.materiel_lu && !v.disponible);
        assert!(v.resolutions.iter().all(|c| c.disponible));
        assert_eq!((v.duree_max, v.plafond.as_str()), (300, ""));
        assert_eq!(v.dossier, "dossier introuvable");
        let indisponible = Materiel {
            disponibilite: Some(Disponibilite {
                disponible: false,
                raison: Some(super::super::HORS_WINDOWS.into()),
                encodeur: None,
            }),
            options: None,
        };
        let v = vue(
            &r,
            &etat(Etat::Inactif),
            &indisponible,
            &Ok("/v".into()),
            &ReglagesJeux::default(),
        );
        assert!(v.materiel_lu && !v.disponible);
        assert_eq!(v.raison, super::super::HORS_WINDOWS);
        let erreur = Materiel {
            options: Some(Err("matériel non lu".into())),
            ..materiel()
        };
        let v = vue(
            &r,
            &etat(Etat::Inactif),
            &erreur,
            &Ok("/v".into()),
            &ReglagesJeux::default(),
        );
        assert_eq!(v.codec, "Plafonds du matériel illisibles : matériel non lu");
    }

    #[test]
    fn hors_bornes_la_vue_ramene_la_duree_et_le_quota() {
        let r = ReglagesReplay {
            duree_s: 3,
            quota_octets: 10,
            ips: 75,
            ..ReglagesReplay::default()
        }
        .borne();
        let v = vue(
            &r,
            &EtatReplay::default(),
            &Materiel::default(),
            &Ok("/v".into()),
            &ReglagesJeux::default(),
        );
        assert_eq!((v.duree, v.quota), (DUREE_MIN_S as i32, 1));
        assert!(v.cadences[1].choisi);
        let r = ReglagesReplay {
            quota_octets: 500 * GO,
            ..ReglagesReplay::default()
        };
        let v = vue(
            &r,
            &EtatReplay::default(),
            &Materiel::default(),
            &Ok("/v".into()),
            &ReglagesJeux::default(),
        );
        assert_eq!((v.quota, v.quota_max), (500, 500));
    }

    fn texte(code: &str) -> String {
        crate::vocal::touches::texte_de_code(code).unwrap()
    }

    #[test]
    fn la_touche_du_replay_ne_reprend_aucune_autre_touche_globale() {
        let mut p = Preferences::default();
        p.ptt.actif = true;
        p.ptt.touche = "KeyV".into();
        let prises = p.touches_des_autres(UsageTouche::Replay);
        assert_eq!(
            choisir_touche_replay("v", &prises),
            ChoixTouche::Refusee(crate::overlay::reglages::TOUCHE_PRISE)
        );
        assert_eq!(
            choisir_touche_replay(&texte("F8"), &prises),
            ChoixTouche::Refusee(crate::overlay::reglages::TOUCHE_PRISE)
        );
        assert_eq!(
            choisir_touche_replay(&texte("F9"), &prises),
            ChoixTouche::Retenue("F9".into())
        );
        p.replay.actif = true;
        p.replay.touche = "F9".into();
        assert!(p
            .touches_des_autres(UsageTouche::Messages)
            .contains(&"F9".to_string()));
        assert_eq!(
            crate::overlay::reglages::choisir_touche(
                &texte("F9"),
                &p.overlay_messages.touche_ouvrir,
                &p.touches_des_autres(UsageTouche::Messages)
            ),
            ChoixTouche::Refusee(crate::overlay::reglages::TOUCHE_PRISE)
        );
    }
}
