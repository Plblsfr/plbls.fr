use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};

use estompe_client::overlay::{
    placer, retenus, taille_fenetre, visible, Placement, ReglagesOverlay, TailleOverlay,
};
use estompe_client::store::Etat;
use estompe_client::vocal::{EtatVocal, Phase};
use estompe_plateforme::ZoneEcran;
use slint::{Image as ImageSlint, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};
use tokio::sync::watch;

use super::{MoteurOverlay, SceneOverlay};
use crate::coquille::avatars::{CleAvatar, Image, Profils};
use crate::jeux::EtatJeux;
use crate::vocal::projection::{self, Participant};
use crate::{LigneOverlayVoix, OverlayVoix};

pub const DISQUES_EN_MEMOIRE: usize = 64;

#[derive(Clone)]
pub struct AvatarOverlay {
    pub cle: CleAvatar,
    pub image: Image,
}

impl PartialEq for AvatarOverlay {
    fn eq(&self, autre: &Self) -> bool {
        self.cle == autre.cle
    }
}

impl std::fmt::Debug for AvatarOverlay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AvatarOverlay").field(&self.cle).finish()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LigneVoix {
    pub id: String,
    pub pseudo: String,
    pub initiales: String,
    pub avatar: Option<AvatarOverlay>,
    pub parle: bool,
    pub muet: bool,
    pub sourd: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EtatVoix {
    pub taille: TailleOverlay,
    pub lignes: Vec<LigneVoix>,
}

pub fn participants_du_vocal(
    etat: &Etat,
    vocal: &EtatVocal,
    moi: &str,
    je_parle: bool,
) -> Option<Vec<Participant>> {
    if !matches!(vocal.phase, Phase::Connecte | Phase::Reconnexion) {
        return None;
    }
    let salon = vocal.salon.as_deref()?;
    Some(projection::participants(etat, vocal, salon, moi, je_parle))
}

pub fn lignes(participants: Vec<Participant>, profils: &Profils) -> Vec<LigneVoix> {
    participants
        .into_iter()
        .map(|p| LigneVoix {
            avatar: p.avatar.and_then(|cle| {
                profils
                    .avatar(&cle)
                    .map(|image| AvatarOverlay { cle, image })
            }),
            id: p.utilisateur,
            pseudo: p.nom,
            initiales: p.initiales,
            parle: p.parle,
            muet: p.muet,
            sourd: p.sourd,
        })
        .collect()
}

pub fn disque(source: &Image) -> Image {
    let (largeur, hauteur) = (source.width(), source.height());
    let cote = largeur.min(hauteur).max(1);
    let (x0, y0) = ((largeur - cote) / 2, (hauteur - cote) / 2);
    let mut sortie = SharedPixelBuffer::<Rgba8Pixel>::new(cote, cote);
    let rayon = cote as f32 / 2.0;
    let pixels = source.as_slice();
    for (i, p) in sortie.make_mut_slice().iter_mut().enumerate() {
        let (x, y) = (i as u32 % cote, i as u32 / cote);
        let mut px = pixels[((y + y0) * largeur + x + x0) as usize];
        let d = ((x as f32 + 0.5 - rayon).powi(2) + (y as f32 + 0.5 - rayon).powi(2)).sqrt();
        let couverture = (rayon - d + 0.5).clamp(0.0, 1.0);
        px.a = (f32::from(px.a) * couverture).round() as u8;
        *p = px;
    }
    sortie
}

thread_local! {
    static DISQUES: RefCell<HashMap<CleAvatar, ImageSlint>> = RefCell::new(HashMap::new());
}

fn portrait(avatar: &AvatarOverlay) -> ImageSlint {
    DISQUES.with(|disques| {
        let mut disques = disques.borrow_mut();
        if disques.len() >= DISQUES_EN_MEMOIRE && !disques.contains_key(&avatar.cle) {
            disques.clear();
        }
        disques
            .entry(avatar.cle.clone())
            .or_insert_with(|| ImageSlint::from_rgba8(disque(&avatar.image)))
            .clone()
    })
}

pub fn ligne_slint(l: &LigneVoix) -> LigneOverlayVoix {
    LigneOverlayVoix {
        pseudo: l.pseudo.as_str().into(),
        initiales: l.initiales.as_str().into(),
        a_portrait: l.avatar.is_some(),
        portrait: l.avatar.as_ref().map(portrait).unwrap_or_default(),
        parle: l.parle,
        muet: l.muet,
        sourd: l.sourd,
    }
}

pub fn appliquer(ui: &OverlayVoix, etat: EtatVoix) {
    ui.set_avatar(etat.taille.avatar() as f32);
    let lignes: Vec<LigneOverlayVoix> = etat.lignes.iter().map(ligne_slint).collect();
    let modele = ui.get_lignes();
    match modele.as_any().downcast_ref::<VecModel<LigneOverlayVoix>>() {
        Some(vec) => vec.set_vec(lignes),
        None => ui.set_lignes(ModelRc::from(Rc::new(VecModel::from(lignes)))),
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Entrees {
    pub salon: Option<Vec<LigneVoix>>,
    pub en_jeu: bool,
    pub ecran_jeu: Option<ZoneEcran>,
    pub ecran_recent: Option<ZoneEcran>,
    pub reglages: ReglagesOverlay,
}

impl Entrees {
    pub fn jeux(&mut self, etat: &EtatJeux) {
        self.en_jeu = etat.premier_plan.is_some();
        self.ecran_jeu = etat.premier_plan.as_ref().and_then(|j| j.application.ecran);
        if let Some(ecran) = etat.derniere_application.as_ref().and_then(|a| a.ecran) {
            self.ecran_recent = Some(ecran);
        }
    }
}

pub fn calculer(
    entrees: &Entrees,
    ecran_principal: Option<ZoneEcran>,
    echelle: impl Fn(&ZoneEcran) -> f32,
) -> Option<(EtatVoix, Placement, u8)> {
    let r = &entrees.reglages;
    if !visible(r.quand, entrees.en_jeu) {
        return None;
    }
    let lignes = retenus(entrees.salon.clone()?, r.mode, |l| l.parle);
    if lignes.is_empty() {
        return None;
    }
    let ecran = entrees
        .ecran_jeu
        .filter(|_| entrees.en_jeu)
        .or(entrees.ecran_recent)
        .or(ecran_principal)?;
    let facteur = f64::from(echelle(&ecran).max(0.5));
    let placement = placer(
        r.coin,
        &ecran,
        facteur,
        taille_fenetre(r.taille, lignes.len()),
    );
    Some((
        EtatVoix {
            taille: r.taille,
            lignes,
        },
        placement,
        r.alpha(),
    ))
}

pub struct ServiceOverlayVoix {
    moteur: MoteurOverlay,
    scene: SceneOverlay<EtatVoix>,
    entrees: Mutex<Entrees>,
}

impl ServiceOverlayVoix {
    pub fn nouveau(moteur: &MoteurOverlay, reglages: ReglagesOverlay) -> Arc<Self> {
        Arc::new(Self {
            moteur: moteur.clone(),
            scene: moteur.scene(OverlayVoix::new, appliquer),
            entrees: Mutex::new(Entrees {
                reglages,
                ..Entrees::default()
            }),
        })
    }

    fn entrees(&self) -> MutexGuard<'_, Entrees> {
        self.entrees.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn disponible(&self) -> bool {
        self.moteur.disponible()
    }

    pub fn reglages(&self) -> ReglagesOverlay {
        self.entrees().reglages
    }

    pub fn definir_salon(&self, salon: Option<Vec<LigneVoix>>) {
        self.modifier(|e| e.salon = salon);
    }

    pub fn definir_jeux(&self, etat: &EtatJeux) {
        self.modifier(|e| e.jeux(etat));
    }

    pub fn definir_reglages(&self, reglages: ReglagesOverlay) {
        self.modifier(|e| e.reglages = reglages);
    }

    pub fn affiche(&self) -> bool {
        self.scene.affichee()
    }

    fn modifier(&self, modification: impl FnOnce(&mut Entrees)) {
        if !self.disponible() {
            return;
        }
        let mut entrees = self.entrees();
        let avant = entrees.clone();
        modification(&mut entrees);
        if *entrees == avant && self.scene.affichee() {
            return;
        }
        let overlay = self.moteur.overlay();
        match calculer(&entrees, overlay.ecran_principal(), |e| overlay.echelle(e)) {
            Some((etat, placement, opacite)) => {
                self.scene.afficher(etat, placement, opacite);
            }
            None => {
                self.scene.masquer();
            }
        }
    }

    pub fn suivre_les_jeux(
        self: &Arc<Self>,
        runtime: &tokio::runtime::Handle,
        mut jeux: watch::Receiver<EtatJeux>,
    ) {
        if !self.disponible() {
            return;
        }
        self.definir_jeux(&jeux.borrow_and_update().clone());
        let faible = Arc::downgrade(self);
        runtime.spawn(async move {
            while jeux.changed().await.is_ok() {
                let etat = jeux.borrow_and_update().clone();
                let Some(service) = faible.upgrade() else {
                    break;
                };
                service.definir_jeux(&etat);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_client::overlay::{CoinOverlay, ModeOverlay, QuandOverlay};

    fn ligne(id: &str, parle: bool) -> LigneVoix {
        LigneVoix {
            id: id.into(),
            pseudo: id.into(),
            initiales: id.chars().take(2).collect::<String>().to_uppercase(),
            avatar: None,
            parle,
            muet: false,
            sourd: false,
        }
    }

    fn ecran(x: i32) -> ZoneEcran {
        ZoneEcran {
            x,
            y: 0,
            largeur: 1920,
            hauteur: 1080,
        }
    }

    #[test]
    fn l_avatar_est_recadre_en_disque_transparent_aux_coins() {
        let mut source = SharedPixelBuffer::<Rgba8Pixel>::new(96, 64);
        for p in source.make_mut_slice() {
            *p = Rgba8Pixel {
                r: 200,
                g: 10,
                b: 10,
                a: 255,
            };
        }
        let d = disque(&source);
        assert_eq!((d.width(), d.height()), (64, 64));
        let a = |x: u32, y: u32| d.as_slice()[(y * 64 + x) as usize].a;
        assert_eq!(a(0, 0), 0);
        assert_eq!(a(6, 6), 0);
        assert_eq!(a(32, 32), 255);
        assert!(a(32, 0) > 0);
    }

    #[test]
    fn l_overlay_suit_l_ecran_du_jeu_puis_le_dernier_ecran_vu() {
        let mut e = Entrees {
            salon: Some(vec![ligne("alice", true), ligne("bob", false)]),
            ..Entrees::default()
        };
        assert_eq!(calculer(&e, Some(ecran(0)), |_| 1.0), None);
        e.en_jeu = true;
        e.ecran_jeu = Some(ecran(1920));
        e.ecran_recent = Some(ecran(-1920));
        let (etat, placement, opacite) = calculer(&e, Some(ecran(0)), |_| 1.0).unwrap();
        assert_eq!(etat.lignes.len(), 2);
        assert_eq!((placement.x, opacite), (1936, 255));
        assert_eq!(placement.y, 540 - 48);
        e.en_jeu = false;
        e.reglages.quand = QuandOverlay::Toujours;
        assert_eq!(
            calculer(&e, Some(ecran(0)), |_| 1.0).unwrap().1.x,
            -1920 + 16
        );
        e.ecran_recent = None;
        assert_eq!(calculer(&e, Some(ecran(0)), |_| 2.0).unwrap().1.x, 32);
        assert_eq!(calculer(&e, None, |_| 1.0), None);
    }

    #[test]
    fn rien_a_montrer_hors_du_vocal_ou_quand_personne_ne_parle() {
        let mut e = Entrees {
            en_jeu: true,
            ecran_jeu: Some(ecran(0)),
            ..Entrees::default()
        };
        assert_eq!(calculer(&e, None, |_| 1.0), None);
        e.salon = Some(vec![ligne("alice", false)]);
        assert!(calculer(&e, None, |_| 1.0).is_some());
        e.reglages.mode = ModeOverlay::Parlent;
        assert_eq!(calculer(&e, None, |_| 1.0), None);
        e.salon = Some(vec![ligne("alice", false), ligne("bob", true)]);
        let (etat, ..) = calculer(&e, None, |_| 1.0).unwrap();
        assert_eq!(etat.lignes, vec![ligne("bob", true)]);
        e.reglages.quand = QuandOverlay::Jamais;
        assert_eq!(calculer(&e, None, |_| 1.0), None);
    }

    #[test]
    fn coin_taille_et_opacite_viennent_des_reglages() {
        let e = Entrees {
            salon: Some(vec![ligne("alice", false)]),
            en_jeu: true,
            ecran_jeu: Some(ecran(0)),
            reglages: ReglagesOverlay {
                coin: CoinOverlay::BasDroite,
                taille: TailleOverlay::Grande,
                opacite: 50,
                ..ReglagesOverlay::default()
            },
            ..Entrees::default()
        };
        let (etat, placement, opacite) = calculer(&e, None, |_| 1.0).unwrap();
        assert_eq!(etat.taille, TailleOverlay::Grande);
        assert_eq!(
            (
                placement.x,
                placement.y,
                placement.largeur,
                placement.hauteur
            ),
            (1920 - 276 - 16, 1080 - 64 - 16, 276, 64)
        );
        assert_eq!(opacite, 128);
    }

    #[test]
    fn sans_changement_d_etat_aucun_rendu_pendant_dix_secondes() {
        use crate::jeux::ServiceJeux;
        use estompe_client::jeux::ReglagesJeux;
        use estompe_plateforme::{
            application_de_chemin, ApplicationPremierPlan, OverlayFactice, PremierPlanFactice,
        };
        use std::time::{Duration, Instant};

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .unwrap();
        let detection = PremierPlanFactice::nouveau();
        detection.mettre_au_premier_plan(Some(ApplicationPremierPlan {
            processus: 4242,
            plein_ecran: true,
            ecran: Some(ecran(0)),
            ..application_de_chemin(r"C:\Games\cs2.exe")
        }));
        let jeux = ServiceJeux::demarrer(
            runtime.handle(),
            Arc::new(detection.clone()),
            Arc::new(ReglagesJeux::default),
            Duration::from_secs(5),
        );
        let factice = OverlayFactice::nouveau(None, 1.0);
        let moteur = MoteurOverlay::nouveau(Arc::new(factice.clone()));
        let service = ServiceOverlayVoix::nouveau(&moteur, ReglagesOverlay::default());
        let salon = vec![ligne("alice", true), ligne("bob", false)];
        runtime.block_on(async {
            let mut etat = jeux.abonner();
            etat.wait_for(|e| e.premier_plan.is_some()).await.unwrap();
        });
        service.suivre_les_jeux(runtime.handle(), jeux.abonner());
        service.definir_salon(Some(salon.clone()));
        let limite = Instant::now() + Duration::from_secs(20);
        while moteur.rendus() < 1 {
            assert!(Instant::now() < limite, "premier rendu");
            std::thread::sleep(Duration::from_millis(2));
        }
        let sondages = detection.sondages();
        runtime.block_on(async {
            for _ in 0..10 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                service.definir_salon(Some(salon.clone()));
                service.definir_reglages(ReglagesOverlay::default());
            }
        });
        assert!(
            detection.sondages() > sondages,
            "le jeu a été sondé pendant les 10 secondes"
        );
        assert_eq!(moteur.demandes(), 1);
        assert_eq!(moteur.rendus(), 1);
        assert_eq!(factice.affichages(), 1);
        let mut silence = salon.clone();
        silence[0].parle = false;
        service.definir_salon(Some(silence));
        assert_eq!(moteur.demandes(), 2);
        detection.mettre_au_premier_plan(None);
        jeux.relancer();
        runtime.block_on(async {
            let mut etat = jeux.abonner();
            etat.wait_for(|e| e.premier_plan.is_none()).await.unwrap();
            while service.affiche() {
                tokio::task::yield_now().await;
            }
        });
        assert!(!service.affiche(), "hors du jeu, l'overlay se masque");
    }
}
