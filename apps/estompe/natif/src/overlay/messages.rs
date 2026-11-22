use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use estompe_client::format::initiales;
use estompe_client::overlay::{placer, Placement};
use estompe_client::overlay_messages::{
    taille_fenetre, voulu, vue, FileMessages, ReglagesMessages, Reponse, ToastMessage,
};
use estompe_client::store::{CleConversation, Etat};
use estompe_client::temps_reel::evenements::NotificationServeur;
use estompe_plateforme::{EtatTouche, EvenementSaisie, RaccourcisGlobaux, ZoneEcran};
use slint::{Model, ModelRc, VecModel};
use tokio::sync::{watch, Notify};

use super::voix::{portrait, AvatarOverlay};
use super::{MoteurOverlay, SceneOverlay};
use crate::coquille::avatars::CleAvatar;
use crate::jeux::EtatJeux;
use crate::{LigneOverlayMessage, OverlayMessages};

pub const OPACITE: u8 = 255;
pub const CARACTERES_VISIBLES: usize = 40;

#[derive(Debug, Clone, PartialEq)]
pub struct LigneMessage {
    pub titre: String,
    pub corps: String,
    pub initiales: String,
    pub avatar: Option<AvatarOverlay>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EtatMessages {
    pub lignes: Vec<LigneMessage>,
    pub saisie: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffetMessages {
    Envoyer(Reponse),
    Ouvrir(ToastMessage),
}

pub type Effets = Arc<dyn Fn(EffetMessages) + Send + Sync>;
pub type Horloge = Arc<dyn Fn() -> Duration + Send + Sync>;
pub type TouchesPrises = Arc<dyn Fn() -> Vec<String> + Send + Sync>;

pub fn texte_visible(texte: &str) -> String {
    let total = texte.chars().count();
    if total <= CARACTERES_VISIBLES {
        return texte.to_string();
    }
    let fin: String = texte.chars().skip(total - CARACTERES_VISIBLES).collect();
    format!("…{fin}")
}

pub fn ligne_slint(l: &LigneMessage) -> LigneOverlayMessage {
    LigneOverlayMessage {
        titre: l.titre.as_str().into(),
        corps: l.corps.as_str().into(),
        initiales: l.initiales.as_str().into(),
        a_portrait: l.avatar.is_some(),
        portrait: l.avatar.as_ref().map(portrait).unwrap_or_default(),
    }
}

pub fn appliquer(ui: &OverlayMessages, etat: EtatMessages) {
    let lignes: Vec<LigneOverlayMessage> = etat.lignes.iter().map(ligne_slint).collect();
    let modele = ui.get_lignes();
    match modele
        .as_any()
        .downcast_ref::<VecModel<LigneOverlayMessage>>()
    {
        Some(vec) => vec.set_vec(lignes),
        None => ui.set_lignes(ModelRc::from(Rc::new(VecModel::from(lignes)))),
    }
    ui.set_en_saisie(etat.saisie.is_some());
    ui.set_texte(texte_visible(etat.saisie.as_deref().unwrap_or_default()).into());
}

pub fn toast_de(notification: &NotificationServeur) -> Option<ToastMessage> {
    let id = notification.msg.clone().filter(|m| !m.is_empty())?;
    let conversation = notification
        .conv
        .as_deref()
        .and_then(CleConversation::analyser)?;
    Some(ToastMessage {
        id,
        conversation,
        cible: notification.url.trim_start_matches('/').to_string(),
        titre: notification.title.clone(),
        corps: notification.body.clone(),
        auteur: notification.sender.clone().filter(|s| !s.is_empty()),
    })
}

pub fn cle_avatar(etat: &Etat, auteur: &str) -> Option<CleAvatar> {
    let version = etat.version_avatar(auteur);
    (version > 0).then(|| CleAvatar {
        utilisateur: auteur.to_string(),
        version,
        serveur: None,
    })
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Entrees {
    pub file: FileMessages,
    pub reglages: ReglagesMessages,
    pub en_jeu: bool,
    pub ecran_jeu: Option<ZoneEcran>,
    pub ecran_recent: Option<ZoneEcran>,
    pub avatars: HashMap<String, AvatarOverlay>,
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
) -> Option<(EtatMessages, Placement)> {
    let r = &entrees.reglages;
    if !r.actif() || entrees.file.is_empty() || (r.en_jeu && !entrees.en_jeu) {
        return None;
    }
    let ecran = entrees
        .ecran_jeu
        .filter(|_| entrees.en_jeu)
        .or(entrees.ecran_recent)
        .or(ecran_principal)?;
    let facteur = f64::from(echelle(&ecran).max(0.5));
    let saisie = entrees.file.saisie().map(str::to_string);
    let placement = placer(
        r.coin,
        &ecran,
        facteur,
        taille_fenetre(entrees.file.len(), saisie.is_some()),
    );
    let lignes = entrees
        .file
        .toasts()
        .map(|t| {
            let v = vue(t, r.masquer_contenu);
            LigneMessage {
                initiales: initiales(&v.titre),
                avatar: entrees.avatars.get(&v.id).cloned(),
                titre: v.titre,
                corps: v.corps,
            }
        })
        .collect();
    Some((EtatMessages { lignes, saisie }, placement))
}

pub fn touches_voulues(
    entrees: &Entrees,
    affichee: bool,
    saisie_disponible: bool,
    prises: &[String],
) -> Vec<String> {
    if !affichee {
        return Vec::new();
    }
    let r = &entrees.reglages;
    let mut touches = Vec::new();
    if saisie_disponible {
        touches.push(r.touche_reponse.clone());
    }
    touches.push(r.touche_ouvrir.clone());
    touches.retain(|t| !prises.contains(t));
    touches
}

struct Interne {
    entrees: Entrees,
    touches: Vec<String>,
}

pub struct Options {
    pub raccourcis: Arc<dyn RaccourcisGlobaux>,
    pub touches_prises: TouchesPrises,
    pub horloge: Option<Horloge>,
}

pub struct ServiceOverlayMessages {
    moteur: MoteurOverlay,
    scene: SceneOverlay<EtatMessages>,
    raccourcis: Arc<dyn RaccourcisGlobaux>,
    touches_prises: TouchesPrises,
    horloge: Horloge,
    interne: Mutex<Interne>,
    effets: Mutex<Option<Effets>>,
    reveil: Arc<Notify>,
    moi: Weak<Self>,
}

fn horloge_monotone() -> Horloge {
    let origine = std::time::Instant::now();
    Arc::new(move || origine.elapsed())
}

impl ServiceOverlayMessages {
    pub fn nouveau(
        moteur: &MoteurOverlay,
        reglages: ReglagesMessages,
        options: Options,
    ) -> Arc<Self> {
        Arc::new_cyclic(|moi| Self {
            moteur: moteur.clone(),
            scene: moteur.scene(OverlayMessages::new, appliquer),
            raccourcis: options.raccourcis,
            touches_prises: options.touches_prises,
            horloge: options.horloge.unwrap_or_else(horloge_monotone),
            interne: Mutex::new(Interne {
                entrees: Entrees {
                    reglages,
                    ..Entrees::default()
                },
                touches: Vec::new(),
            }),
            effets: Mutex::new(None),
            reveil: Arc::new(Notify::new()),
            moi: moi.clone(),
        })
    }

    fn interne(&self) -> MutexGuard<'_, Interne> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn disponible(&self) -> bool {
        self.moteur.disponible()
    }

    pub fn saisie_disponible(&self) -> bool {
        self.moteur.overlay().saisie_disponible()
    }

    pub fn reglages(&self) -> ReglagesMessages {
        self.interne().entrees.reglages.clone()
    }

    pub fn affiche(&self) -> bool {
        self.scene.affichee()
    }

    pub fn en_saisie(&self) -> bool {
        self.interne().entrees.file.en_saisie()
    }

    pub fn toasts(&self) -> Vec<ToastMessage> {
        self.interne().entrees.file.toasts().cloned().collect()
    }

    pub fn touches(&self) -> Vec<String> {
        self.interne().touches.clone()
    }

    pub fn definir_effets(&self, effets: Effets) {
        *self.effets.lock().unwrap_or_else(|e| e.into_inner()) = Some(effets);
    }

    fn produire(&self, effet: EffetMessages) {
        let effets = self
            .effets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(effets) = effets {
            effets(effet);
        }
    }

    pub fn voulu(&self, notification: &NotificationServeur, ne_pas_deranger: bool) -> bool {
        self.disponible()
            && voulu(
                notification.kind,
                notification.msg.as_deref().is_some_and(|m| !m.is_empty()),
                &self.interne().entrees.reglages,
                ne_pas_deranger,
            )
    }

    pub fn recevoir(&self, toast: ToastMessage, avatar: Option<AvatarOverlay>) {
        if !self.disponible() {
            return;
        }
        let maintenant = (self.horloge)();
        self.modifier(|e| {
            if !e.reglages.actif() {
                return;
            }
            let duree = e.reglages.duree();
            match avatar {
                Some(a) => {
                    e.avatars.insert(toast.id.clone(), a);
                }
                None => {
                    e.avatars.remove(&toast.id);
                }
            }
            e.file.pousser(toast, maintenant, duree);
        });
        self.reveil.notify_one();
    }

    pub fn definir_avatar(&self, id: &str, avatar: AvatarOverlay) {
        self.modifier(|e| {
            if e.file.toasts().any(|t| t.id == id) {
                e.avatars.insert(id.to_string(), avatar);
            }
        });
    }

    pub fn definir_reglages(&self, reglages: ReglagesMessages) {
        self.modifier(|e| {
            if !reglages.actif() {
                e.file.vider();
            }
            e.reglages = reglages;
        });
    }

    pub fn definir_jeux(&self, etat: &EtatJeux) {
        self.modifier(|e| e.jeux(etat));
    }

    pub fn attente(&self) -> Option<Duration> {
        let echeance = self.interne().entrees.file.prochaine_echeance()?;
        Some(echeance.saturating_sub((self.horloge)()))
    }

    pub fn expirer(&self) {
        let maintenant = (self.horloge)();
        self.modifier(|e| {
            let duree = e.reglages.duree();
            e.file.expirer(maintenant, duree);
        });
    }

    pub fn repondre(&self) -> bool {
        if !self.saisie_disponible() || !self.scene.affichee() {
            return false;
        }
        let mut commence = false;
        self.modifier(|e| commence = e.file.commencer_reponse());
        if !commence {
            return false;
        }
        let faible = self.moi.clone();
        let saisi = self.scene.saisir(Arc::new(move |evenement| {
            if let Some(service) = faible.upgrade() {
                service.saisie(evenement);
            }
        }));
        if !saisi {
            self.modifier(|e| {
                e.file.annuler();
            });
        }
        saisi
    }

    pub fn saisie(&self, evenement: EvenementSaisie) {
        let mut reponse = None;
        let mut fin = false;
        self.modifier(|e| match evenement {
            EvenementSaisie::Texte(texte) => {
                e.file.taper(&texte);
            }
            EvenementSaisie::Effacer => {
                e.file.effacer();
            }
            EvenementSaisie::Valider => {
                fin = e.file.en_saisie();
                reponse = e.file.valider();
            }
            EvenementSaisie::Annuler | EvenementSaisie::FocusPerdu => {
                fin = e.file.annuler();
            }
        });
        if fin {
            self.scene.rendre_le_focus();
        }
        if let Some(reponse) = reponse {
            self.produire(EffetMessages::Envoyer(reponse));
        }
    }

    pub fn ouvrir(&self) -> bool {
        if !self.scene.affichee() {
            return false;
        }
        let mut etait_en_saisie = false;
        let mut dernier = None;
        self.modifier(|e| {
            etait_en_saisie = e.file.en_saisie();
            dernier = e.file.ouvrir_dernier();
        });
        if etait_en_saisie {
            self.scene.rendre_le_focus();
        }
        match dernier {
            Some(toast) => {
                self.produire(EffetMessages::Ouvrir(toast));
                true
            }
            None => false,
        }
    }

    fn modifier(&self, modification: impl FnOnce(&mut Entrees)) {
        if !self.disponible() {
            return;
        }
        let mut interne = self.interne();
        let avant = interne.entrees.clone();
        modification(&mut interne.entrees);
        if interne.entrees == avant {
            return;
        }
        let overlay = self.moteur.overlay();
        let calcul = calculer(&interne.entrees, overlay.ecran_principal(), |e| {
            overlay.echelle(e)
        });
        let mut rendre = false;
        match calcul {
            Some((etat, placement)) => {
                self.scene.afficher(etat, placement, OPACITE);
            }
            None => {
                rendre = interne.entrees.file.annuler();
                self.scene.masquer();
            }
        }
        if rendre {
            self.scene.rendre_le_focus();
        }
        self.accorder_les_touches(&mut interne);
    }

    fn accorder_les_touches(&self, interne: &mut Interne) {
        if !self.raccourcis.disponible() {
            return;
        }
        let prises = (self.touches_prises)();
        let voulues = touches_voulues(
            &interne.entrees,
            self.scene.affichee(),
            self.saisie_disponible(),
            &prises,
        );
        for touche in interne.touches.clone() {
            if !voulues.contains(&touche) {
                let _ = self.raccourcis.retirer(&touche);
            }
        }
        let mut enregistrees = Vec::new();
        for touche in voulues {
            if interne.touches.contains(&touche) {
                enregistrees.push(touche);
                continue;
            }
            let faible = self.moi.clone();
            let reponse = touche == interne.entrees.reglages.touche_reponse;
            let action = Box::new(move |etat| {
                if etat != EtatTouche::Appui {
                    return;
                }
                if let Some(service) = faible.upgrade() {
                    if reponse {
                        service.repondre();
                    } else {
                        service.ouvrir();
                    }
                }
            });
            if self.raccourcis.enregistrer(&touche, action).is_ok() {
                enregistrees.push(touche);
            }
        }
        interne.touches = enregistrees;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_client::overlay::CoinOverlay;
    use estompe_client::overlay_messages::GenresMessages;
    use estompe_client::temps_reel::evenements::GenreNotification;
    use estompe_plateforme::{Indisponible, OverlayFactice, RaccourcisFactices};
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

    fn toast(id: &str, conv: &str) -> ToastMessage {
        ToastMessage {
            id: id.into(),
            conversation: CleConversation::Prive(conv.into()),
            cible: format!("dm/{conv}"),
            titre: "Ana".into(),
            corps: format!("message {id}"),
            auteur: Some("u1".into()),
        }
    }

    fn attendre(condition: impl Fn() -> bool, quoi: &str) {
        let limite = Instant::now() + Duration::from_secs(20);
        while !condition() {
            assert!(Instant::now() < limite, "condition non atteinte : {quoi}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    struct Banc {
        factice: OverlayFactice,
        moteur: MoteurOverlay,
        raccourcis: Arc<RaccourcisFactices>,
        temps: Arc<AtomicU64>,
        effets: Arc<Mutex<Vec<EffetMessages>>>,
        service: Arc<ServiceOverlayMessages>,
    }

    fn banc_avec(factice: OverlayFactice, prises: Vec<String>) -> Banc {
        let moteur = MoteurOverlay::nouveau(Arc::new(factice.clone()));
        let raccourcis = Arc::new(RaccourcisFactices::nouveaux(true));
        let temps = Arc::new(AtomicU64::new(0));
        let horloge = temps.clone();
        let service = ServiceOverlayMessages::nouveau(
            &moteur,
            ReglagesMessages {
                en_jeu: false,
                ..ReglagesMessages::default()
            },
            Options {
                raccourcis: raccourcis.clone(),
                touches_prises: Arc::new(move || prises.clone()),
                horloge: Some(Arc::new(move || {
                    Duration::from_millis(horloge.load(Ordering::SeqCst))
                })),
            },
        );
        let effets = Arc::new(Mutex::new(Vec::new()));
        let copie = effets.clone();
        service.definir_effets(Arc::new(move |e| copie.lock().unwrap().push(e)));
        Banc {
            factice,
            moteur,
            raccourcis,
            temps,
            effets,
            service,
        }
    }

    fn banc() -> Banc {
        banc_avec(OverlayFactice::nouveau(Some(ecran(0)), 1.0), Vec::new())
    }

    impl Banc {
        fn avancer(&self, duree: Duration) {
            self.temps
                .fetch_add(duree.as_millis() as u64, Ordering::SeqCst);
            self.service.expirer();
        }

        fn hauteur(&self) -> Option<u32> {
            self.factice
                .calques()
                .first()
                .cloned()
                .flatten()
                .map(|a| a.image.hauteur)
        }
    }

    #[test]
    fn la_vue_suit_l_ecran_du_jeu_le_coin_et_masque_le_contenu() {
        let mut e = Entrees {
            reglages: ReglagesMessages {
                coin: CoinOverlay::BasGauche,
                masquer_contenu: true,
                ..ReglagesMessages::default()
            },
            ..Entrees::default()
        };
        assert_eq!(calculer(&e, Some(ecran(0)), |_| 1.0), None);
        e.file.pousser(toast("1", "d1"), Duration::ZERO, 8 * S);
        assert_eq!(
            calculer(&e, Some(ecran(0)), |_| 1.0),
            None,
            "seulement en jeu par défaut"
        );
        e.en_jeu = true;
        e.ecran_jeu = Some(ecran(1920));
        let (etat, placement) = calculer(&e, Some(ecran(0)), |_| 1.0).unwrap();
        assert_eq!(
            (
                placement.x,
                placement.y,
                placement.largeur,
                placement.hauteur
            ),
            (1936, 1080 - 72 - 16, 340, 72)
        );
        assert_eq!(etat.lignes[0].corps, "");
        assert_eq!(etat.lignes[0].initiales, "AN");
        assert_eq!(etat.saisie, None);
        e.file.commencer_reponse();
        let (etat, placement) = calculer(&e, Some(ecran(0)), |_| 2.0).unwrap();
        assert_eq!(etat.saisie.as_deref(), Some(""));
        assert_eq!(placement.hauteur, (72 + 44) * 2);
        e.reglages.genres = GenresMessages::Aucun;
        assert_eq!(calculer(&e, Some(ecran(0)), |_| 1.0), None);
    }

    #[test]
    fn le_texte_saisi_garde_sa_fin_visible() {
        assert_eq!(texte_visible("court"), "court");
        let long = "a".repeat(30) + &"b".repeat(CARACTERES_VISIBLES);
        assert_eq!(texte_visible(&long), format!("…{}", "b".repeat(40)));
    }

    #[test]
    fn la_notification_devient_un_bandeau_si_elle_a_un_message_et_une_conversation() {
        let mut n = NotificationServeur {
            title: "Ana".into(),
            body: "salut".into(),
            tag: "t".into(),
            url: "/dm/d1".into(),
            conv: Some("d:d1".into()),
            msg: Some("m1".into()),
            kind: Some(GenreNotification::MessagePrive),
            silent: false,
            read: None,
            sender: Some("u1".into()),
            room: None,
            text: None,
            attach: None,
        };
        assert_eq!(
            toast_de(&n),
            Some(ToastMessage {
                id: "m1".into(),
                conversation: CleConversation::Prive("d1".into()),
                cible: "dm/d1".into(),
                titre: "Ana".into(),
                corps: "salut".into(),
                auteur: Some("u1".into()),
            })
        );
        n.conv = None;
        assert_eq!(toast_de(&n), None);
    }

    #[test]
    fn les_bandeaux_expirent_et_les_touches_ne_servent_que_pendant_l_affichage() {
        let b = banc();
        b.service.recevoir(toast("1", "d1"), None);
        assert!(b.service.affiche() && b.moteur.demarre());
        assert_eq!(b.raccourcis.enregistres(), vec!["F7", "F8"]);
        attendre(|| b.hauteur() == Some(72), "un bandeau");
        b.avancer(3 * S);
        b.service.recevoir(toast("2", "d2"), None);
        attendre(|| b.hauteur() == Some(144), "deux bandeaux");
        assert_eq!(b.service.attente(), Some(5 * S));
        b.avancer(5 * S);
        assert_eq!(b.service.toasts().len(), 1);
        attendre(|| b.hauteur() == Some(72), "le premier a expiré");
        b.avancer(3 * S);
        assert!(!b.service.affiche());
        assert!(b.raccourcis.enregistres().is_empty());
        attendre(|| b.factice.calques() == vec![None], "calque masqué");
    }

    #[test]
    fn la_reponse_rapide_prend_le_focus_envoie_puis_le_rend() {
        let b = banc();
        assert!(!b.service.repondre(), "rien à quoi répondre");
        b.service.recevoir(toast("1", "d1"), None);
        b.service.recevoir(toast("2", "d2"), None);
        b.raccourcis.appuyer("F8");
        assert!(b.service.en_saisie());
        attendre(|| b.factice.en_saisie().is_some(), "focus pris");
        attendre(|| b.hauteur() == Some(144 + 44), "champ de réponse");
        b.avancer(20 * S);
        assert_eq!(b.service.toasts().len(), 2, "la saisie garde les bandeaux");
        for e in [
            EvenementSaisie::Texte("o".into()),
            EvenementSaisie::Texte("kk".into()),
            EvenementSaisie::Effacer,
            EvenementSaisie::Valider,
        ] {
            assert!(b.factice.taper(e));
        }
        attendre(|| b.factice.focus() == (1, 1), "focus rendu");
        assert_eq!(
            *b.effets.lock().unwrap(),
            vec![EffetMessages::Envoyer(Reponse {
                conversation: CleConversation::Prive("d2".into()),
                texte: "ok".into()
            })]
        );
        assert_eq!(b.service.toasts(), vec![toast("1", "d1")]);
        assert!(!b.service.en_saisie());
        attendre(|| b.hauteur() == Some(72), "champ refermé");
    }

    #[test]
    fn echap_ou_la_perte_du_focus_annulent_sans_rien_envoyer() {
        let b = banc();
        b.service.recevoir(toast("1", "d1"), None);
        for evenement in [EvenementSaisie::Annuler, EvenementSaisie::FocusPerdu] {
            assert!(b.service.repondre());
            attendre(|| b.factice.en_saisie().is_some(), "focus pris");
            b.factice.taper(EvenementSaisie::Texte("x".into()));
            b.factice.taper(evenement);
            attendre(|| b.factice.en_saisie().is_none(), "focus rendu");
            assert!(!b.service.en_saisie());
        }
        assert!(b.effets.lock().unwrap().is_empty());
        assert_eq!(b.service.toasts().len(), 1);
    }

    #[test]
    fn la_touche_ouvrir_ramene_la_conversation_du_dernier_message() {
        let b = banc();
        b.service.recevoir(toast("1", "d1"), None);
        b.service.recevoir(toast("2", "d2"), None);
        b.service.repondre();
        b.raccourcis.appuyer("F7");
        assert_eq!(
            *b.effets.lock().unwrap(),
            vec![EffetMessages::Ouvrir(toast("2", "d2"))]
        );
        assert!(!b.service.en_saisie());
        assert_eq!(b.service.toasts(), vec![toast("1", "d1")]);
    }

    #[test]
    fn sans_saisie_possible_seule_la_touche_ouvrir_est_prise() {
        let b = banc_avec(
            OverlayFactice::nouveau(Some(ecran(0)), 1.0).sans_saisie(),
            Vec::new(),
        );
        b.service.recevoir(toast("1", "d1"), None);
        assert_eq!(b.raccourcis.enregistres(), vec!["F7"]);
        assert!(!b.service.repondre());
        assert!(!b.service.en_saisie());
    }

    #[test]
    fn une_touche_deja_prise_par_l_appui_pour_parler_n_est_pas_volee() {
        let b = banc_avec(
            OverlayFactice::nouveau(Some(ecran(0)), 1.0),
            vec!["F8".into()],
        );
        b.service.recevoir(toast("1", "d1"), None);
        assert_eq!(b.raccourcis.enregistres(), vec!["F7"]);
    }

    #[test]
    fn hors_du_jeu_le_bandeau_se_cache_et_la_saisie_rend_le_focus() {
        use estompe_client::jeux::JeuDetecte;
        use estompe_plateforme::ApplicationPremierPlan;
        let b = banc();
        b.service.definir_reglages(ReglagesMessages::default());
        b.service.recevoir(toast("1", "d1"), None);
        assert!(!b.service.affiche(), "seulement en jeu");
        let mut jeux = EtatJeux {
            premier_plan: Some(JeuDetecte {
                nom: "Jeu".into(),
                application: ApplicationPremierPlan {
                    ecran: Some(ecran(1920)),
                    ..ApplicationPremierPlan::default()
                },
            }),
            ..EtatJeux::default()
        };
        b.service.definir_jeux(&jeux);
        assert!(b.service.affiche());
        assert!(b.service.repondre());
        attendre(|| b.factice.en_saisie().is_some(), "focus pris");
        jeux.premier_plan = None;
        b.service.definir_jeux(&jeux);
        assert!(!b.service.affiche());
        assert!(!b.service.en_saisie());
        attendre(|| b.factice.en_saisie().is_none(), "focus rendu");
        b.service.definir_reglages(ReglagesMessages {
            genres: GenresMessages::Aucun,
            ..ReglagesMessages::default()
        });
        assert!(b.service.toasts().is_empty());
        assert!(b.raccourcis.enregistres().is_empty());
    }

    #[test]
    fn sans_overlay_rien_ne_demarre() {
        let moteur = MoteurOverlay::nouveau(Arc::new(Indisponible));
        let raccourcis = Arc::new(RaccourcisFactices::nouveaux(true));
        let service = ServiceOverlayMessages::nouveau(
            &moteur,
            ReglagesMessages::default(),
            Options {
                raccourcis: raccourcis.clone(),
                touches_prises: Arc::new(Vec::new),
                horloge: None,
            },
        );
        service.recevoir(toast("1", "d1"), None);
        assert!(!service.affiche() && !moteur.demarre());
        assert!(raccourcis.enregistres().is_empty());
    }

    #[test]
    fn la_minuterie_retire_le_bandeau_a_son_echeance() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .unwrap();
        let _garde = runtime.enter();
        let factice = OverlayFactice::nouveau(Some(ecran(0)), 1.0);
        let moteur = MoteurOverlay::nouveau(Arc::new(factice));
        let service = ServiceOverlayMessages::nouveau(
            &moteur,
            ReglagesMessages {
                en_jeu: false,
                duree: 2,
                ..ReglagesMessages::default()
            },
            Options {
                raccourcis: Arc::new(RaccourcisFactices::nouveaux(true)),
                touches_prises: Arc::new(Vec::new),
                horloge: Some({
                    let origine = tokio::time::Instant::now();
                    Arc::new(move || origine.elapsed())
                }),
            },
        );
        service.demarrer_minuterie(runtime.handle());
        runtime.block_on(async {
            service.recevoir(toast("1", "d1"), None);
            assert!(service.affiche());
            tokio::time::sleep(Duration::from_millis(1900)).await;
            assert!(service.affiche());
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert!(!service.affiche());
        });
    }
}
