use std::collections::HashMap;

use crate::regarder_ensemble::{
    adresse_navigateur, fournisseur, EtatLecture, Fournisseur, LecteurSeance,
};

use super::{artiste_de, est_morceau, lien_lecture, musique, titre_video, PostVideo};

pub const MAX_FILE: usize = 200;
pub const REPRISE_MIN: f64 = 5.0;
pub const FIN_MARGE: f64 = 10.0;
pub const VOLUME_DEFAUT: u8 = 70;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeSolo {
    Lecteur,
    Youtube,
}

pub fn mode_solo(provider: &str) -> Option<ModeSolo> {
    match fournisseur(provider) {
        Fournisseur::Fichier => Some(ModeSolo::Lecteur),
        Fournisseur::YouTube => Some(ModeSolo::Youtube),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ElementSolo {
    pub id: String,
    pub titre: String,
    pub sous_titre: String,
    pub provider: String,
    pub reference: String,
    pub salon: String,
}

impl ElementSolo {
    pub fn de_post(p: &PostVideo, salon: &str) -> Self {
        let reference = match mode_solo(&p.provider) {
            Some(ModeSolo::Lecteur) => lien_lecture(p).to_string(),
            _ => p.reference.clone(),
        };
        let sous_titre = if musique(&p.provider) {
            artiste_de(&p.site).to_string()
        } else {
            p.site.clone()
        };
        Self {
            id: p.message_id.clone(),
            titre: titre_video(p),
            sous_titre,
            provider: p.provider.clone(),
            reference,
            salon: salon.to_string(),
        }
    }

    pub fn morceau(id: &str, titre: &str, artiste: &str, salon: &str) -> Self {
        Self {
            id: id.to_string(),
            titre: titre.to_string(),
            sous_titre: artiste.to_string(),
            provider: "ytmusic".into(),
            reference: id.to_string(),
            salon: salon.to_string(),
        }
    }

    pub fn a_resoudre(&self) -> bool {
        musique(&self.provider) && !est_morceau(&self.reference)
    }

    pub fn adresse_externe(&self, depart: f64) -> Option<String> {
        match fournisseur(&self.provider) {
            Fournisseur::Fichier => Some(self.reference.clone()),
            _ => adresse_navigateur(&self.provider, &self.reference, depart),
        }
    }
}

pub type FabriqueSolo = Box<dyn FnMut(ModeSolo) -> Option<Box<dyn LecteurSeance>> + Send>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lancement {
    Lecture,
    Ajout,
    Navigateur(String),
    Rien,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceSolo {
    Aucune,
    Ancree,
    Mini,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VueSolo {
    pub actif: bool,
    pub id: String,
    pub titre: String,
    pub sous_titre: String,
    pub a_suivre: usize,
    pub lecture: bool,
    pub position: f64,
    pub duree: f64,
    pub volume: u8,
    pub mode: Option<ModeSolo>,
    pub salon: String,
    pub erreur: String,
    pub cede: bool,
}

impl VueSolo {
    pub fn place(&self, salon_affiche: Option<&str>) -> PlaceSolo {
        if !self.actif {
            PlaceSolo::Aucune
        } else if salon_affiche == Some(self.salon.as_str()) {
            PlaceSolo::Ancree
        } else {
            PlaceSolo::Mini
        }
    }
}

pub struct Solo {
    file: Vec<ElementSolo>,
    index: Option<usize>,
    lecture: bool,
    volume: u8,
    lecteur: Option<(ModeSolo, Box<dyn LecteurSeance>)>,
    fabrique: FabriqueSolo,
    reprises: HashMap<String, f64>,
    erreur: String,
    position: f64,
    duree: f64,
    cede: bool,
}

impl Solo {
    pub fn nouveau(fabrique: FabriqueSolo) -> Self {
        Self {
            file: Vec::new(),
            index: None,
            lecture: false,
            volume: VOLUME_DEFAUT,
            lecteur: None,
            fabrique,
            reprises: HashMap::new(),
            erreur: String::new(),
            position: 0.0,
            duree: 0.0,
            cede: false,
        }
    }

    pub fn courant(&self) -> Option<&ElementSolo> {
        self.index.and_then(|i| self.file.get(i))
    }

    pub fn a_venir(&self) -> &[ElementSolo] {
        match self.index {
            Some(i) if i < self.file.len() => &self.file[i + 1..],
            _ => &[],
        }
    }

    pub fn mode(&self) -> Option<ModeSolo> {
        self.lecteur.as_ref().map(|(m, _)| *m)
    }

    pub fn lecteur_mut(&mut self) -> Option<&mut Box<dyn LecteurSeance>> {
        self.lecteur.as_mut().map(|(_, l)| l)
    }

    pub fn reprise(&self, id: &str) -> Option<f64> {
        self.reprises.get(id).copied()
    }

    pub fn jouer(&mut self, elements: Vec<ElementSolo>, ajouter: bool) -> Lancement {
        if let [seul] = elements.as_slice() {
            if mode_solo(&seul.provider).is_none() {
                return seul
                    .adresse_externe(0.0)
                    .map_or(Lancement::Rien, Lancement::Navigateur);
            }
        }
        let mut lisibles: Vec<ElementSolo> = elements
            .into_iter()
            .filter(|e| mode_solo(&e.provider).is_some())
            .collect();
        lisibles.truncate(MAX_FILE);
        if lisibles.is_empty() {
            return Lancement::Rien;
        }
        if ajouter && self.courant().is_some() {
            self.file.extend(lisibles);
            self.file.truncate(MAX_FILE);
            return Lancement::Ajout;
        }
        let meme = match (lisibles.as_slice(), self.courant()) {
            ([seul], Some(c)) => seul.id == c.id && self.lecteur.is_some() && !self.cede,
            _ => false,
        };
        if meme {
            if !self.lecture {
                self.basculer();
            }
            return Lancement::Lecture;
        }
        self.memoriser();
        self.file = lisibles;
        self.index = Some(0);
        self.charger_courant()
    }

    fn charger_courant(&mut self) -> Lancement {
        let Some(e) = self.courant().cloned() else {
            return Lancement::Rien;
        };
        let Some(mode) = mode_solo(&e.provider) else {
            return self.suivant();
        };
        let depart = self.reprise(&e.id).unwrap_or(0.0);
        if self.mode() != Some(mode) {
            if let Some((_, mut l)) = self.lecteur.take() {
                if !self.cede {
                    l.detruire();
                }
            }
            match (self.fabrique)(mode) {
                Some(l) => self.lecteur = Some((mode, l)),
                None => {
                    self.vider();
                    return e
                        .adresse_externe(depart)
                        .map_or(Lancement::Rien, Lancement::Navigateur);
                }
            }
        }
        self.cede = false;
        self.erreur.clear();
        self.position = depart;
        self.duree = 0.0;
        self.lecture = true;
        let volume = self.volume;
        if let Some(l) = self.lecteur_mut() {
            l.charger(&e.reference, depart, true);
            l.regler_volume(volume);
        }
        Lancement::Lecture
    }

    fn memoriser(&mut self) {
        let Some(id) = self.courant().map(|e| e.id.clone()) else {
            return;
        };
        if self.lecteur.is_none() {
            return;
        }
        let t = self.position;
        if t >= REPRISE_MIN && (self.duree <= 0.0 || t < self.duree - FIN_MARGE) {
            self.reprises.insert(id, t);
        } else {
            self.reprises.remove(&id);
        }
    }

    fn vider(&mut self) {
        if let Some((_, mut l)) = self.lecteur.take() {
            if !self.cede {
                l.detruire();
            }
        }
        self.file.clear();
        self.index = None;
        self.lecture = false;
        self.cede = false;
        self.position = 0.0;
        self.duree = 0.0;
    }

    pub fn basculer(&mut self) -> Lancement {
        if self.courant().is_none() {
            return Lancement::Rien;
        }
        if self.cede || self.lecteur.is_none() {
            return self.charger_courant();
        }
        self.lecture = !self.lecture;
        let lecture = self.lecture;
        if let Some(l) = self.lecteur_mut() {
            if lecture {
                l.lire();
            } else {
                l.pause();
            }
        }
        Lancement::Lecture
    }

    pub fn suivant(&mut self) -> Lancement {
        self.memoriser();
        match self.index {
            Some(i) if i + 1 < self.file.len() => {
                self.index = Some(i + 1);
                self.charger_courant()
            }
            _ => {
                self.vider();
                Lancement::Rien
            }
        }
    }

    pub fn arreter(&mut self) {
        self.memoriser();
        self.vider();
    }

    pub fn ceder(&mut self) {
        if self.lecteur.is_none() || self.cede {
            return;
        }
        self.memoriser();
        self.lecteur = None;
        self.lecture = false;
        self.cede = true;
    }

    pub fn aller(&mut self, position: f64) {
        let position = position.max(0.0);
        self.position = position;
        if let Some(l) = self.lecteur_mut() {
            l.aller(position);
        }
    }

    pub fn regler_volume(&mut self, volume: u8) {
        self.volume = volume.min(100);
        let v = self.volume;
        if let Some(l) = self.lecteur_mut() {
            l.regler_volume(v);
        }
    }

    pub fn actualiser(&mut self, maintenant_ms: i64) -> Lancement {
        if self.cede {
            return Lancement::Rien;
        }
        let Some((_, l)) = self.lecteur.as_mut() else {
            return Lancement::Rien;
        };
        let mut fin = false;
        while let Some(etat) = l.suivre(maintenant_ms) {
            match etat {
                EtatLecture::Fin => fin = true,
                EtatLecture::Lecture => self.lecture = true,
                EtatLecture::Pause => self.lecture = false,
                _ => {}
            }
        }
        self.position = l.temps();
        let duree = l.duree();
        if duree > 0.0 {
            self.duree = duree;
        }
        if let Some(repli) = l.repli() {
            let depart = self.position;
            let adresse = self.courant().and_then(|e| e.adresse_externe(depart));
            self.erreur = repli;
            self.lecteur = None;
            self.vider();
            return adresse.map_or(Lancement::Rien, Lancement::Navigateur);
        }
        if let Some(message) = l.erreur() {
            let suite = self.suivant();
            self.erreur = message;
            return suite;
        }
        if fin {
            if let Some(id) = self.courant().map(|e| e.id.clone()) {
                self.reprises.remove(&id);
            }
            self.position = self.duree;
            return self.suivant();
        }
        self.memoriser();
        Lancement::Rien
    }

    pub fn oublier_erreur(&mut self) {
        self.erreur.clear();
    }

    pub fn vue(&self) -> VueSolo {
        let Some(e) = self.courant() else {
            return VueSolo {
                volume: self.volume,
                erreur: self.erreur.clone(),
                ..VueSolo::default()
            };
        };
        VueSolo {
            actif: true,
            id: e.id.clone(),
            titre: e.titre.clone(),
            sous_titre: e.sous_titre.clone(),
            a_suivre: self.a_venir().len(),
            lecture: self.lecture && !self.cede,
            position: self.position,
            duree: self.duree,
            volume: self.volume,
            mode: mode_solo(&e.provider),
            salon: e.salon.clone(),
            erreur: self.erreur.clone(),
            cede: self.cede,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::lecteur::{CommandeMoteur, Evenement, Lecteur, MediaFactice, MoteurFactice};
    use crate::regarder_ensemble::{LecteurFichier, LecteurYoutube, LienVue};

    fn element(id: &str, provider: &str, reference: &str, salon: &str) -> ElementSolo {
        ElementSolo {
            id: id.into(),
            titre: format!("Titre {id}"),
            provider: provider.into(),
            reference: reference.into(),
            salon: salon.into(),
            ..ElementSolo::default()
        }
    }

    struct Banc {
        lecteur: Arc<Lecteur>,
        moteur: MoteurFactice,
        liens: Arc<Mutex<Vec<LienVue>>>,
    }

    fn banc(youtube: bool) -> (Solo, Banc) {
        let (lecteur, moteur) = Lecteur::factice();
        let lecteur = Arc::new(lecteur);
        let liens = Arc::new(Mutex::new(Vec::new()));
        let (l, v) = (lecteur.clone(), liens.clone());
        let solo = Solo::nouveau(Box::new(move |mode| match mode {
            ModeSolo::Lecteur => {
                Some(Box::new(LecteurFichier::nouveau(l.clone())) as Box<dyn LecteurSeance>)
            }
            ModeSolo::Youtube if youtube => {
                let lien = LienVue::nouveau();
                v.lock().unwrap().push(lien.clone());
                Some(Box::new(LecteurYoutube::nouveau(lien)) as Box<dyn LecteurSeance>)
            }
            ModeSolo::Youtube => None,
        }));
        (
            solo,
            Banc {
                lecteur,
                moteur,
                liens,
            },
        )
    }

    fn charges(moteur: &MoteurFactice) -> Vec<(String, f64)> {
        moteur
            .commandes()
            .into_iter()
            .filter_map(|c| match c {
                CommandeMoteur::Charger(s) => Some((s.adresse, s.depart)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn un_post_donne_son_element_et_son_mode() {
        let p = PostVideo {
            message_id: "10".into(),
            provider: "page".into(),
            reference: "https://x.fr/a.mp4".into(),
            url: "https://x.fr/page".into(),
            site: "x.fr".into(),
            ..PostVideo::default()
        };
        let e = ElementSolo::de_post(&p, "w1");
        assert_eq!(
            (
                e.reference.as_str(),
                e.titre.as_str(),
                e.sous_titre.as_str()
            ),
            ("https://x.fr/a.mp4", "Vidéo Fichier", "x.fr")
        );
        let m = ElementSolo::de_post(
            &PostVideo {
                provider: "ytmusic".into(),
                reference: "dQw4w9WgXcQ".into(),
                site: "Art · Album · 3:25".into(),
                title: "Un".into(),
                ..PostVideo::default()
            },
            "m1",
        );
        assert_eq!(
            (m.reference.as_str(), m.sous_titre.as_str()),
            ("dQw4w9WgXcQ", "Art")
        );
        assert!(!m.a_resoudre());
        assert!(element("p", "ytmusic", "OLAK5uy_abcdefghij", "m1").a_resoudre());
        assert_eq!(mode_solo("youtube"), Some(ModeSolo::Youtube));
        assert_eq!(mode_solo("file"), Some(ModeSolo::Lecteur));
        assert_eq!(mode_solo("vimeo"), None);
        assert_eq!(mode_solo("simple"), None);
    }

    #[test]
    fn vimeo_twitch_et_dailymotion_s_ouvrent_dans_le_navigateur() {
        let (mut solo, b) = banc(true);
        assert_eq!(
            solo.jouer(vec![element("1", "vimeo", "123", "w1")], false),
            Lancement::Navigateur("https://vimeo.com/123#t=0s".into())
        );
        assert_eq!(
            solo.jouer(vec![element("2", "twitch_live", "chaine", "w1")], false),
            Lancement::Navigateur("https://www.twitch.tv/chaine".into())
        );
        assert_eq!(
            solo.jouer(vec![element("3", "simple", "x", "w1")], false),
            Lancement::Rien
        );
        assert!(!solo.vue().actif);
        assert!(charges(&b.moteur).is_empty());
    }

    #[test]
    fn la_file_avance_puis_s_arrete() {
        let (mut solo, b) = banc(false);
        b.moteur.ajouter_media("a.mp4", MediaFactice::video(30.0));
        b.moteur.ajouter_media("b.mp4", MediaFactice::video(30.0));
        b.moteur.ajouter_media("c.mp4", MediaFactice::video(30.0));
        let un = element("1", "file", "a.mp4", "w1");
        let deux = element("2", "file", "b.mp4", "w1");
        assert_eq!(solo.jouer(vec![un, deux], false), Lancement::Lecture);
        assert_eq!(solo.courant().unwrap().id, "1");
        assert!(solo.vue().lecture);
        assert_eq!(
            solo.jouer(vec![element("3", "file", "c.mp4", "w1")], true),
            Lancement::Ajout
        );
        assert_eq!(
            solo.a_venir()
                .iter()
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>(),
            ["2", "3"]
        );
        assert_eq!(solo.vue().a_suivre, 2);
        solo.basculer();
        assert!(!solo.vue().lecture);
        assert_eq!(
            b.moteur.commandes().last(),
            Some(&CommandeMoteur::Pause(true))
        );
        solo.suivant();
        solo.suivant();
        assert_eq!(solo.courant().unwrap().id, "3");
        solo.suivant();
        assert!(solo.courant().is_none());
        assert!(!solo.vue().actif && !solo.vue().lecture);
        assert_eq!(
            charges(&b.moteur)
                .into_iter()
                .map(|(a, _)| a)
                .collect::<Vec<_>>(),
            ["a.mp4", "b.mp4", "c.mp4"]
        );
        assert_eq!(b.moteur.commandes().last(), Some(&CommandeMoteur::Arreter));
    }

    #[test]
    fn la_fin_d_un_titre_passe_au_suivant() {
        let (mut solo, b) = banc(false);
        b.moteur.ajouter_media("a.mp4", MediaFactice::video(30.0));
        b.moteur.ajouter_media("b.mp4", MediaFactice::video(30.0));
        solo.jouer(
            vec![
                element("1", "file", "a.mp4", "w1"),
                element("2", "file", "b.mp4", "w1"),
            ],
            false,
        );
        solo.actualiser(0);
        b.moteur.avancer(31.0);
        assert_eq!(solo.actualiser(1000), Lancement::Lecture);
        assert_eq!(solo.courant().unwrap().id, "2");
        assert_eq!(solo.reprise("1"), None);
    }

    #[test]
    fn seek_deplace_le_lecteur_et_la_position_affichee() {
        let (mut solo, b) = banc(false);
        b.moteur.ajouter_media("a.mp4", MediaFactice::video(300.0));
        solo.jouer(vec![element("1", "file", "a.mp4", "w1")], false);
        solo.aller(42.0);
        assert_eq!(solo.vue().position, 42.0);
        assert!(b.moteur.commandes().contains(&CommandeMoteur::Aller(42.0)));
        solo.regler_volume(130);
        assert_eq!(solo.vue().volume, 100);
    }

    #[test]
    fn la_position_est_reprise_au_retour_sur_le_meme_titre() {
        let (mut solo, b) = banc(false);
        b.moteur.ajouter_media("a.mp4", MediaFactice::video(300.0));
        b.moteur.ajouter_media("b.mp4", MediaFactice::video(300.0));
        let un = element("1", "file", "a.mp4", "w1");
        solo.jouer(vec![un.clone()], false);
        solo.actualiser(0);
        b.moteur.emettre(Evenement::EnPause(true));
        b.moteur.emettre(Evenement::Position(120.0));
        solo.actualiser(500);
        assert_eq!(solo.reprise("1"), Some(120.0));
        solo.jouer(vec![element("2", "file", "b.mp4", "w1")], false);
        solo.jouer(vec![un], false);
        assert_eq!(
            charges(&b.moteur),
            [
                ("a.mp4".to_string(), 0.0),
                ("b.mp4".to_string(), 0.0),
                ("a.mp4".to_string(), 120.0)
            ]
        );
        b.moteur.emettre(Evenement::Position(296.0));
        solo.actualiser(1000);
        assert_eq!(
            solo.reprise("1"),
            None,
            "presque fini : on repartira du début"
        );
    }

    #[test]
    fn un_autre_media_prend_le_lecteur_sans_etre_arrete_et_la_lecture_reprend_a_sa_position() {
        let (mut solo, b) = banc(false);
        b.moteur.ajouter_media("a.mp4", MediaFactice::video(300.0));
        solo.jouer(vec![element("1", "file", "a.mp4", "w1")], false);
        b.moteur.emettre(Evenement::EnPause(true));
        b.moteur.emettre(Evenement::Position(60.0));
        solo.actualiser(0);
        b.moteur.vider_commandes();
        solo.ceder();
        assert!(
            b.moteur.commandes().is_empty(),
            "rien n'est envoyé au média qui a pris la main"
        );
        let v = solo.vue();
        assert!(v.actif && v.cede && !v.lecture);
        assert_eq!(solo.actualiser(100), Lancement::Rien);
        assert_eq!(solo.basculer(), Lancement::Lecture);
        assert_eq!(charges(&b.moteur), [("a.mp4".to_string(), 60.0)]);
        assert!(!solo.vue().cede && solo.vue().lecture);
        let _ = &b.lecteur;
    }

    #[test]
    fn changer_de_salon_garde_la_lecture_dans_le_mini_lecteur() {
        let (mut solo, b) = banc(false);
        b.moteur.ajouter_media("a.mp4", MediaFactice::video(300.0));
        solo.jouer(vec![element("1", "file", "a.mp4", "w1")], false);
        solo.actualiser(0);
        assert_eq!(solo.vue().place(Some("w1")), PlaceSolo::Ancree);
        b.moteur.vider_commandes();
        for salon in [Some("c1"), None, Some("t1")] {
            assert_eq!(solo.vue().place(salon), PlaceSolo::Mini);
            assert_eq!(solo.actualiser(100), Lancement::Rien);
        }
        assert!(solo.vue().lecture && solo.vue().actif);
        assert!(
            !b.moteur
                .commandes()
                .iter()
                .any(|c| matches!(c, CommandeMoteur::Arreter | CommandeMoteur::Pause(true))),
            "ni arrêt ni pause en changeant de salon"
        );
        assert_eq!(solo.vue().place(Some("w1")), PlaceSolo::Ancree);
        solo.arreter();
        assert_eq!(solo.vue().place(Some("c1")), PlaceSolo::Aucune);
        assert_eq!(b.moteur.commandes().last(), Some(&CommandeMoteur::Arreter));
    }

    #[test]
    fn youtube_passe_par_la_vue_et_sans_elle_par_le_navigateur() {
        let (mut solo, b) = banc(true);
        let yt = element("1", "youtube", "dQw4w9WgXcQ", "w1");
        assert_eq!(solo.jouer(vec![yt.clone()], false), Lancement::Lecture);
        assert_eq!(solo.vue().mode, Some(ModeSolo::Youtube));
        let lien = b.liens.lock().unwrap()[0].clone();
        let page = lien.page_a_ouvrir().unwrap();
        assert_eq!((page.video.as_str(), page.lecture), ("dQw4w9WgXcQ", true));
        lien.signaler_indisponible("vue absente");
        assert_eq!(
            solo.actualiser(0),
            Lancement::Navigateur("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=0s".into())
        );
        assert!(!solo.vue().actif);
        assert_eq!(solo.vue().erreur, "vue absente");
        let (mut sans_vue, _) = banc(false);
        assert_eq!(
            sans_vue.jouer(vec![element("2", "ytmusic", "aaaaaaaaaaa", "m1")], false),
            Lancement::Navigateur("https://music.youtube.com/watch?v=aaaaaaaaaaa".into())
        );
    }

    #[test]
    fn passer_d_un_fichier_a_youtube_libere_le_lecteur() {
        let (mut solo, b) = banc(true);
        b.moteur.ajouter_media("a.mp4", MediaFactice::video(30.0));
        solo.jouer(
            vec![
                element("1", "file", "a.mp4", "w1"),
                element("2", "youtube", "dQw4w9WgXcQ", "w1"),
            ],
            false,
        );
        b.moteur.vider_commandes();
        solo.suivant();
        assert_eq!(solo.vue().mode, Some(ModeSolo::Youtube));
        assert!(b.moteur.commandes().contains(&CommandeMoteur::Arreter));
        solo.arreter();
        assert!(b.liens.lock().unwrap()[0].ferme());
    }
}
