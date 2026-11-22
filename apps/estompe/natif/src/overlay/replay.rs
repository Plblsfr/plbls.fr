use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::overlay::{placer, CoinOverlay, Placement};
use estompe_client::replay::affichage::{
    a_gauche, coin, etat_indicateur, message_clip, message_erreur, message_etat, taille_fenetre,
    EtatIndicateur, FileReplay, GenreMessage, MessageReplay,
};
use estompe_client::replay::automate::Etat;
use estompe_client::replay::types::{EtatReplay, EvenementReplay};
use estompe_plateforme::ZoneEcran;
use slint::{Model, ModelRc, VecModel};
use tokio::sync::{broadcast, watch, Notify};

use super::messages::Horloge;
use super::{MoteurOverlay, SceneOverlay};
use crate::jeux::EtatJeux;
use crate::replay::ServiceReplay;
use crate::{LigneOverlayReplay, OverlayReplay};

pub const OPACITE: u8 = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoisinsOverlay {
    pub duree: Duration,
    pub coin_participants: Option<CoinOverlay>,
    pub coin_messages: Option<CoinOverlay>,
}

pub type LectureVoisins = Arc<dyn Fn() -> VoisinsOverlay + Send + Sync>;

pub fn voisins(p: &estompe_client::preferences::Preferences) -> VoisinsOverlay {
    VoisinsOverlay {
        duree: p.overlay_messages.duree(),
        coin_participants: (p.overlay.quand != estompe_client::overlay::QuandOverlay::Jamais)
            .then_some(p.overlay.coin),
        coin_messages: p
            .overlay_messages
            .actif()
            .then_some(p.overlay_messages.coin),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EtatOverlayReplay {
    pub contenu: EtatIndicateur,
    pub a_gauche: bool,
}

pub fn genre_slint(genre: GenreMessage) -> i32 {
    match genre {
        GenreMessage::Ok => 0,
        GenreMessage::Info => 1,
        GenreMessage::Erreur => 2,
    }
}

pub fn appliquer(ui: &OverlayReplay, etat: EtatOverlayReplay) {
    ui.set_indicateur(etat.contenu.indicateur);
    ui.set_a_gauche(etat.a_gauche);
    let lignes: Vec<LigneOverlayReplay> = etat
        .contenu
        .messages
        .iter()
        .map(|m| LigneOverlayReplay {
            texte: m.texte.as_str().into(),
            genre: genre_slint(m.genre),
        })
        .collect();
    let modele = ui.get_messages();
    match modele
        .as_any()
        .downcast_ref::<VecModel<LigneOverlayReplay>>()
    {
        Some(vec) => vec.set_vec(lignes),
        None => ui.set_messages(ModelRc::from(Rc::new(VecModel::from(lignes)))),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entrees {
    pub etat: Etat,
    pub actif: bool,
    pub file: FileReplay,
    pub duree: Duration,
    pub en_jeu: bool,
    pub ecran_jeu: Option<ZoneEcran>,
    pub ecran_recent: Option<ZoneEcran>,
    pub coin_participants: Option<CoinOverlay>,
    pub coin_messages: Option<CoinOverlay>,
}

impl Default for Entrees {
    fn default() -> Self {
        Self {
            etat: Etat::Inactif,
            actif: false,
            file: FileReplay::default(),
            duree: Duration::from_secs(u64::from(estompe_client::overlay_messages::DUREE_DEFAUT)),
            en_jeu: false,
            ecran_jeu: None,
            ecran_recent: None,
            coin_participants: None,
            coin_messages: None,
        }
    }
}

impl Entrees {
    pub fn jeux(&mut self, etat: &EtatJeux) {
        self.en_jeu = etat.premier_plan.is_some();
        self.ecran_jeu = etat.premier_plan.as_ref().and_then(|j| j.application.ecran);
        if let Some(ecran) = etat.derniere_application.as_ref().and_then(|a| a.ecran) {
            self.ecran_recent = Some(ecran);
        }
    }

    pub fn coin(&self) -> CoinOverlay {
        coin(self.coin_participants, self.coin_messages)
    }
}

pub fn calculer(
    entrees: &Entrees,
    ecran_principal: Option<ZoneEcran>,
    echelle: impl Fn(&ZoneEcran) -> f32,
) -> Option<(EtatOverlayReplay, Placement)> {
    let contenu = etat_indicateur(&entrees.etat, entrees.actif, &entrees.file)?;
    let ecran = entrees
        .ecran_jeu
        .filter(|_| entrees.en_jeu)
        .or(entrees.ecran_recent)
        .or(ecran_principal)?;
    let facteur = f64::from(echelle(&ecran).max(0.5));
    let coin = entrees.coin();
    let placement = placer(coin, &ecran, facteur, taille_fenetre(&contenu));
    Some((
        EtatOverlayReplay {
            contenu,
            a_gauche: a_gauche(coin),
        },
        placement,
    ))
}

pub struct ServiceOverlayReplay {
    moteur: MoteurOverlay,
    scene: SceneOverlay<EtatOverlayReplay>,
    horloge: Horloge,
    voisins: Option<LectureVoisins>,
    entrees: Mutex<Entrees>,
    reveil: Arc<Notify>,
}

fn horloge_monotone() -> Horloge {
    let origine = std::time::Instant::now();
    Arc::new(move || origine.elapsed())
}

impl ServiceOverlayReplay {
    pub fn nouveau(
        moteur: &MoteurOverlay,
        horloge: Option<Horloge>,
        voisins: Option<LectureVoisins>,
    ) -> Arc<Self> {
        Arc::new(Self {
            moteur: moteur.clone(),
            scene: moteur.scene(OverlayReplay::new, appliquer),
            horloge: horloge.unwrap_or_else(horloge_monotone),
            voisins,
            entrees: Mutex::new(Entrees::default()),
            reveil: Arc::new(Notify::new()),
        })
    }

    fn entrees(&self) -> MutexGuard<'_, Entrees> {
        self.entrees.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn disponible(&self) -> bool {
        self.moteur.disponible()
    }

    pub fn affiche(&self) -> bool {
        self.scene.affichee()
    }

    pub fn messages(&self) -> Vec<MessageReplay> {
        self.entrees().file.messages().cloned().collect()
    }

    pub fn coin(&self) -> CoinOverlay {
        self.entrees().coin()
    }

    fn annoncer(e: &mut Entrees, message: MessageReplay, maintenant: Duration) {
        let duree = e.duree;
        e.file.pousser(message, maintenant, duree);
    }

    pub fn definir_etat(&self, etat: &EtatReplay) {
        let maintenant = (self.horloge)();
        self.modifier(|e| {
            if let Some(message) = message_etat(&e.etat, &etat.etat) {
                Self::annoncer(e, message, maintenant);
            }
            e.etat = etat.etat.clone();
        });
        self.reveil.notify_one();
    }

    pub fn recevoir(&self, evenement: &EvenementReplay) {
        let message = match evenement {
            EvenementReplay::Clip(clip) => message_clip(clip),
            EvenementReplay::Erreur(erreur) => message_erreur(erreur),
        };
        let maintenant = (self.horloge)();
        self.modifier(|e| Self::annoncer(e, message, maintenant));
        self.reveil.notify_one();
    }

    pub fn definir_actif(&self, actif: bool) {
        self.modifier(|e| e.actif = actif);
    }

    pub fn definir_duree(&self, duree: Duration) {
        self.modifier(|e| e.duree = duree);
    }

    pub fn definir_coins(&self, participants: Option<CoinOverlay>, messages: Option<CoinOverlay>) {
        self.modifier(|e| {
            e.coin_participants = participants;
            e.coin_messages = messages;
        });
    }

    pub fn definir_jeux(&self, etat: &EtatJeux) {
        self.modifier(|e| e.jeux(etat));
    }

    pub fn attente(&self) -> Option<Duration> {
        let echeance = self.entrees().file.prochaine_echeance()?;
        Some(echeance.saturating_sub((self.horloge)()))
    }

    pub fn expirer(&self) {
        let maintenant = (self.horloge)();
        self.modifier(|e| {
            e.file.expirer(maintenant);
        });
    }

    fn modifier(&self, modification: impl FnOnce(&mut Entrees)) {
        if !self.disponible() {
            return;
        }
        let mut entrees = self.entrees();
        let avant = entrees.clone();
        if let Some(lire) = &self.voisins {
            let v = lire();
            entrees.duree = v.duree;
            entrees.coin_participants = v.coin_participants;
            entrees.coin_messages = v.coin_messages;
        }
        modification(&mut entrees);
        if *entrees == avant {
            return;
        }
        let overlay = self.moteur.overlay();
        match calculer(&entrees, overlay.ecran_principal(), |e| overlay.echelle(e)) {
            Some((etat, placement)) => {
                self.scene.afficher(etat, placement, OPACITE);
            }
            None => {
                self.scene.masquer();
            }
        }
    }

    pub fn demarrer_minuterie(self: &Arc<Self>, runtime: &tokio::runtime::Handle) {
        if !self.disponible() {
            return;
        }
        let faible = Arc::downgrade(self);
        let reveil = self.reveil.clone();
        runtime.spawn(async move {
            while let Some(attente) = faible.upgrade().map(|s| s.attente()) {
                match attente {
                    Some(duree) => {
                        let _ = tokio::time::timeout(duree, reveil.notified()).await;
                    }
                    None => reveil.notified().await,
                }
                let Some(service) = faible.upgrade() else {
                    break;
                };
                service.expirer();
            }
        });
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

    pub fn suivre_le_replay(
        self: &Arc<Self>,
        runtime: &tokio::runtime::Handle,
        replay: &ServiceReplay,
    ) {
        if !self.disponible() {
            return;
        }
        let mut etat = replay.etat();
        let mut evenements = replay.evenements();
        self.definir_actif(replay.reglages().replay.actif);
        self.definir_etat(&etat.borrow_and_update().clone());
        let faible = Arc::downgrade(self);
        runtime.spawn(async move {
            while etat.changed().await.is_ok() {
                let e = etat.borrow_and_update().clone();
                let Some(service) = faible.upgrade() else {
                    break;
                };
                service.definir_etat(&e);
            }
        });
        let faible = Arc::downgrade(self);
        runtime.spawn(async move {
            loop {
                match evenements.recv().await {
                    Ok(evenement) => {
                        let Some(service) = faible.upgrade() else {
                            break;
                        };
                        service.recevoir(&evenement);
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_client::replay::automate::Pause;
    use estompe_client::replay::types::{ClipEnregistre, ErreurReplay};
    use estompe_plateforme::{Indisponible, OverlayFactice};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    const S: Duration = Duration::from_secs(1);

    fn ecran(x: i32) -> ZoneEcran {
        ZoneEcran {
            x,
            y: 0,
            largeur: 1920,
            hauteur: 1080,
        }
    }

    fn etat(e: Etat) -> EtatReplay {
        EtatReplay {
            etat: e,
            ..EtatReplay::default()
        }
    }

    fn clip() -> EvenementReplay {
        EvenementReplay::Clip(ClipEnregistre {
            chemin: "/v/clip.mp4".into(),
            nom: "clip".into(),
            jeu: "Jeu".into(),
            duree_s: 30.0,
            taille_octets: 10,
        })
    }

    fn attendre(condition: impl Fn() -> bool, quoi: &str) {
        let limite = Instant::now() + Duration::from_secs(20);
        while !condition() {
            assert!(Instant::now() < limite, "condition non atteinte : {quoi}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn placement_sur_l_ecran_du_jeu_au_coin_libre() {
        let mut e = Entrees {
            actif: true,
            etat: Etat::Arme,
            en_jeu: true,
            ecran_jeu: Some(ecran(1920)),
            coin_participants: Some(CoinOverlay::CentreGauche),
            coin_messages: Some(CoinOverlay::HautDroite),
            ..Entrees::default()
        };
        let (etat, placement) = calculer(&e, Some(ecran(0)), |_| 1.0).unwrap();
        assert!(etat.contenu.indicateur && etat.a_gauche);
        assert_eq!(
            (
                placement.x,
                placement.y,
                placement.largeur,
                placement.hauteur
            ),
            (1920 + 16, 16, 320, 26)
        );
        e.coin_messages = Some(CoinOverlay::BasDroite);
        let (etat, placement) = calculer(&e, Some(ecran(0)), |_| 2.0).unwrap();
        assert!(!etat.a_gauche);
        assert_eq!(placement.x, 1920 + 1920 - 640 - 32);
        e.etat = Etat::Pause(Pause::PremierPlan);
        assert_eq!(calculer(&e, Some(ecran(0)), |_| 1.0), None);
        e.ecran_jeu = None;
        e.ecran_recent = None;
        e.etat = Etat::Arme;
        assert_eq!(calculer(&e, None, |_| 1.0), None);
        assert!(calculer(&e, Some(ecran(0)), |_| 1.0).is_some());
    }

    #[test]
    fn indicateur_clip_sauve_puis_erreur_puis_expiration() {
        let factice = OverlayFactice::nouveau(Some(ecran(0)), 1.0);
        let moteur = MoteurOverlay::nouveau(Arc::new(factice.clone()));
        let temps = Arc::new(AtomicU64::new(0));
        let lecture = temps.clone();
        let service = ServiceOverlayReplay::nouveau(
            &moteur,
            Some(Arc::new(move || {
                Duration::from_millis(lecture.load(Ordering::SeqCst))
            })),
            None,
        );
        service.definir_duree(S * 4);
        service.definir_etat(&etat(Etat::Arme));
        assert!(!service.affiche(), "inactif : rien");
        service.definir_actif(true);
        assert!(service.affiche());
        attendre(|| factice.affichages() == 1, "indicateur");
        let image = factice.calques()[0].clone().unwrap().image;
        assert_eq!((image.largeur, image.hauteur), (320, 26));
        service.recevoir(&clip());
        assert_eq!(service.messages()[0].texte, "Clip enregistré (0:30)");
        attendre(|| factice.affichages() == 2, "clip sauvé");
        assert_eq!(factice.calques()[0].clone().unwrap().image.hauteur, 76);
        temps.store(2000, Ordering::SeqCst);
        service.recevoir(&EvenementReplay::Erreur(ErreurReplay::nouvelle(
            "tampon-vide",
            "Rien à enregistrer : le tampon de l'instant replay est vide.",
        )));
        assert_eq!(service.messages().len(), 2);
        assert_eq!(service.attente(), Some(S * 2));
        temps.store(4000, Ordering::SeqCst);
        service.expirer();
        assert_eq!(service.messages().len(), 1);
        service.definir_etat(&etat(Etat::Pause(Pause::PremierPlan)));
        assert_eq!(
            service.messages()[1].texte,
            "Instant replay en pause : le jeu n'est plus au premier plan"
        );
        temps.store(60_000, Ordering::SeqCst);
        service.expirer();
        assert!(!service.affiche(), "tout a expiré, tampon en pause");
        attendre(|| factice.calques() == vec![None], "masqué");
    }

    #[test]
    fn la_minuterie_retire_le_message_a_son_echeance() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let factice = OverlayFactice::nouveau(Some(ecran(0)), 1.0);
        let moteur = MoteurOverlay::nouveau(Arc::new(factice));
        let service = ServiceOverlayReplay::nouveau(&moteur, None, None);
        service.definir_duree(Duration::from_millis(50));
        service.demarrer_minuterie(runtime.handle());
        service.recevoir(&clip());
        assert!(service.affiche());
        runtime.block_on(async {
            let limite = tokio::time::Instant::now() + Duration::from_secs(10);
            while service.affiche() {
                assert!(tokio::time::Instant::now() < limite, "message expiré");
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        });
    }

    #[test]
    fn les_voisins_viennent_des_preferences() {
        let mut p = estompe_client::preferences::Preferences::default();
        let v = voisins(&p);
        assert_eq!(v.duree, S * 8);
        assert_eq!(
            (v.coin_participants, v.coin_messages),
            (
                Some(CoinOverlay::CentreGauche),
                Some(CoinOverlay::HautDroite)
            )
        );
        p.overlay.quand = estompe_client::overlay::QuandOverlay::Jamais;
        p.overlay_messages.genres = estompe_client::overlay_messages::GenresMessages::Aucun;
        let v = voisins(&p);
        assert_eq!((v.coin_participants, v.coin_messages), (None, None));
        let factice = OverlayFactice::nouveau(Some(ecran(0)), 1.0);
        let moteur = MoteurOverlay::nouveau(Arc::new(factice));
        let service = ServiceOverlayReplay::nouveau(
            &moteur,
            None,
            Some(Arc::new(|| VoisinsOverlay {
                duree: Duration::from_secs(3),
                coin_participants: Some(CoinOverlay::HautGauche),
                coin_messages: Some(CoinOverlay::HautDroite),
            })),
        );
        service.recevoir(&clip());
        assert_eq!(service.coin(), CoinOverlay::BasDroite);
    }

    #[test]
    fn sans_overlay_rien_ne_demarre() {
        let moteur = MoteurOverlay::nouveau(Arc::new(Indisponible));
        let service = ServiceOverlayReplay::nouveau(&moteur, None, None);
        service.definir_actif(true);
        service.definir_etat(&etat(Etat::Arme));
        service.recevoir(&clip());
        assert!(!service.affiche());
        assert!(!moteur.demarre());
    }
}
