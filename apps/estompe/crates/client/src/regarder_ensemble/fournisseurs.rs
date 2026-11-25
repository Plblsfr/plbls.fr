use std::sync::Arc;
use std::time::Instant;

use crate::http::encoder_composant;
use crate::lecteur::{EtatLecteur, Lecteur, Phase, Source};

use super::film::{DemandeFilm, PrefTampon, Qualite, ReponseFilm};

pub type Plage = (f64, f64);

pub const AVANCE_PRETE: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EtatLecture {
    NonDemarre,
    Lecture,
    Pause,
    Tampon,
    Fin,
    Amorce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeSynchro {
    Complete,
    Lancement,
}

pub trait LecteurSeance: Send {
    fn mode(&self) -> ModeSynchro {
        ModeSynchro::Complete
    }
    fn vitesse_reglable(&self) -> bool {
        true
    }
    fn charger(&mut self, reference: &str, depart: f64, lecture: bool);
    fn lire(&mut self);
    fn pause(&mut self);
    fn aller(&mut self, position: f64);
    fn temps(&self) -> f64;
    fn vitesse(&self) -> f64;
    fn regler_vitesse(&mut self, vitesse: f64);
    fn etat(&self) -> EtatLecture;
    fn pret(&self) -> Option<bool> {
        None
    }
    fn duree(&self) -> f64 {
        0.0
    }
    fn plages(&self) -> Vec<Plage> {
        Vec::new()
    }
    fn regler_volume(&mut self, _volume: u8) {}
    fn choisir_audio(&mut self, _piste: i64) {}
    fn choisir_sous_titres(&mut self, _piste: Option<i64>) {}
    fn choisir_qualite(&mut self, _qualite: Qualite) {}
    fn regler_tampon(&mut self, _tampon: PrefTampon) {}
    fn regler_wifi_seulement(&mut self, _actif: bool) {}
    fn suivre(&mut self, _maintenant_ms: i64) -> Option<EtatLecture> {
        None
    }
    fn erreur(&mut self) -> Option<String> {
        None
    }
    fn demandes(&mut self) -> Vec<DemandeFilm> {
        Vec::new()
    }
    fn repondre(&mut self, _reponse: ReponseFilm, _maintenant_ms: i64) {}
    fn detruire(&mut self) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fournisseur {
    Fichier,
    Film,
    YouTube,
    Navigateur,
    NonPrisEnCharge,
}

pub const MESSAGE_NON_PRIS_EN_CHARGE: &str =
    "Cette source de vidéo n’est pas prise en charge par cette version.";

pub fn fournisseur(provider: &str) -> Fournisseur {
    match provider {
        "" | "youtube" | "ytmusic" => Fournisseur::YouTube,
        "file" | "page" => Fournisseur::Fichier,
        "film" => Fournisseur::Film,
        "vimeo" | "dailymotion" | "twitch_vod" | "twitch_live" | "twitch_clip" => {
            Fournisseur::Navigateur
        }
        _ => Fournisseur::NonPrisEnCharge,
    }
}

pub fn libelle_fournisseur(provider: &str) -> &str {
    match provider {
        "youtube" => "YouTube",
        "ytmusic" => "YouTube Music",
        "twitch_vod" => "Twitch",
        "twitch_clip" => "Twitch (clip)",
        "twitch_live" => "Twitch (direct)",
        "vimeo" => "Vimeo",
        "dailymotion" => "Dailymotion",
        "file" | "page" => "Fichier",
        "simple" => "Lien",
        "film" => "Films et séries",
        autre => autre,
    }
}

fn temps_twitch(secondes: f64) -> String {
    let s = secondes.max(0.0).floor() as u64;
    format!("{}h{}m{}s", s / 3600, s / 60 % 60, s % 60)
}

pub fn adresse_navigateur(provider: &str, reference: &str, depart: f64) -> Option<String> {
    let r = encoder_composant(reference);
    let s = depart.max(0.0).floor() as u64;
    Some(match provider {
        "" | "youtube" => format!("https://www.youtube.com/watch?v={r}&t={s}s"),
        "ytmusic" => format!("https://music.youtube.com/watch?v={r}"),
        "vimeo" => format!("https://vimeo.com/{r}#t={s}s"),
        "dailymotion" => format!("https://www.dailymotion.com/video/{r}?start={s}"),
        "twitch_vod" => format!(
            "https://www.twitch.tv/videos/{r}?t={}",
            temps_twitch(depart)
        ),
        "twitch_live" => format!("https://www.twitch.tv/{r}"),
        "twitch_clip" => format!("https://clips.twitch.tv/{r}"),
        _ => return None,
    })
}

pub fn fusionner_plages(plages: &[Plage], ecart: f64) -> Vec<Plage> {
    let mut triees: Vec<Plage> = plages
        .iter()
        .copied()
        .filter(|(a, b)| a.is_finite() && b.is_finite() && b > a)
        .collect();
    triees.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut sortie: Vec<Plage> = Vec::new();
    for (a, b) in triees {
        match sortie.last_mut() {
            Some(derniere) if a <= derniere.1 + ecart => derniere.1 = derniere.1.max(b),
            _ => sortie.push((a, b)),
        }
    }
    sortie
}

pub fn avance_chargee(position: f64, plages: &[Plage]) -> f64 {
    fusionner_plages(plages, 0.5)
        .into_iter()
        .find(|(a, b)| position >= a - 0.5 && position < *b)
        .map_or(0.0, |(_, b)| (b - position).max(0.0))
}

pub fn etat_de(e: &EtatLecteur) -> EtatLecture {
    match e.phase {
        Phase::Pret if e.fin => EtatLecture::Fin,
        Phase::Pret if e.en_pause => EtatLecture::Pause,
        Phase::Pret if e.tampon || e.recherche => EtatLecture::Tampon,
        Phase::Pret => EtatLecture::Lecture,
        Phase::Chargement if !e.en_pause => EtatLecture::Tampon,
        _ => EtatLecture::NonDemarre,
    }
}

pub fn plages_de(e: &EtatLecteur, maintenant: Instant) -> Vec<Plage> {
    match e.cache_jusqua {
        Some(fin) if e.phase == Phase::Pret => {
            let position = e.position_a(maintenant);
            fusionner_plages(&[(position, fin)], 0.5)
        }
        _ => Vec::new(),
    }
}

pub fn pret_de(e: &EtatLecteur, maintenant: Instant) -> bool {
    if e.phase != Phase::Pret || e.tampon {
        return false;
    }
    let position = e.position_a(maintenant);
    match e.cache_jusqua {
        None => true,
        Some(fin) => fin - position >= AVANCE_PRETE || e.duree.is_some_and(|d| fin >= d - 0.5),
    }
}

pub struct LecteurFichier {
    lecteur: Arc<Lecteur>,
    jeton: Option<(String, String)>,
    dernier: EtatLecture,
    erreur_vue: bool,
}

impl LecteurFichier {
    pub fn nouveau(lecteur: Arc<Lecteur>) -> Self {
        Self {
            lecteur,
            jeton: None,
            dernier: EtatLecture::NonDemarre,
            erreur_vue: false,
        }
    }

    pub fn avec_jeton_pour(mut self, prefixe: impl Into<String>, jeton: impl Into<String>) -> Self {
        self.jeton = Some((prefixe.into(), jeton.into()));
        self
    }

    pub fn lecteur(&self) -> &Arc<Lecteur> {
        &self.lecteur
    }

    pub fn source(&self, reference: &str, depart: f64, lecture: bool) -> Source {
        let mut source = Source::adresse(reference).a_partir_de(depart);
        if let Some((prefixe, jeton)) = &self.jeton {
            if !prefixe.is_empty() && reference.starts_with(prefixe.as_str()) {
                source = source.avec_jeton(jeton.clone());
            }
        }
        if lecture {
            source
        } else {
            source.en_pause()
        }
    }
}

impl LecteurSeance for LecteurFichier {
    fn charger(&mut self, reference: &str, depart: f64, lecture: bool) {
        self.erreur_vue = false;
        let source = self.source(reference, depart, lecture);
        self.lecteur.charger(source);
    }

    fn lire(&mut self) {
        self.lecteur.lire();
    }

    fn pause(&mut self) {
        self.lecteur.pause();
    }

    fn aller(&mut self, position: f64) {
        self.lecteur.aller(position);
    }

    fn temps(&self) -> f64 {
        self.lecteur.position()
    }

    fn vitesse(&self) -> f64 {
        self.lecteur.etat().vitesse
    }

    fn regler_vitesse(&mut self, vitesse: f64) {
        self.lecteur.definir_vitesse(vitesse);
    }

    fn etat(&self) -> EtatLecture {
        etat_de(&self.lecteur.etat())
    }

    fn pret(&self) -> Option<bool> {
        Some(pret_de(&self.lecteur.etat(), Instant::now()))
    }

    fn duree(&self) -> f64 {
        self.lecteur.etat().duree.unwrap_or(0.0)
    }

    fn plages(&self) -> Vec<Plage> {
        plages_de(&self.lecteur.etat(), Instant::now())
    }

    fn regler_volume(&mut self, volume: u8) {
        self.lecteur.definir_son(volume, volume == 0);
    }

    fn suivre(&mut self, _maintenant_ms: i64) -> Option<EtatLecture> {
        let etat = self.etat();
        (std::mem::replace(&mut self.dernier, etat) != etat).then_some(etat)
    }

    fn erreur(&mut self) -> Option<String> {
        let e = self.lecteur.etat();
        if e.phase != Phase::Erreur {
            self.erreur_vue = false;
            return None;
        }
        if std::mem::replace(&mut self.erreur_vue, true) {
            return None;
        }
        Some(
            "Cette vidéo ne peut pas être lue (format ou accès refusé par le site qui l'héberge)."
                .to_string(),
        )
    }

    fn detruire(&mut self) {
        self.lecteur.arreter();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lecteur::{CommandeMoteur, MediaFactice};

    #[test]
    fn chaque_source_a_son_lecteur_et_les_iframes_s_ouvrent_dans_le_navigateur() {
        assert_eq!(fournisseur("film"), Fournisseur::Film);
        assert_eq!(fournisseur("file"), Fournisseur::Fichier);
        assert_eq!(fournisseur("page"), Fournisseur::Fichier);
        assert_eq!(fournisseur(""), Fournisseur::YouTube);
        assert_eq!(fournisseur("vimeo"), Fournisseur::Navigateur);
        assert_eq!(fournisseur("twitch_clip"), Fournisseur::Navigateur);
        assert_eq!(fournisseur("simple"), Fournisseur::NonPrisEnCharge);
        assert_eq!(libelle_fournisseur("twitch_live"), "Twitch (direct)");
        assert_eq!(libelle_fournisseur("film"), "Films et séries");
    }

    #[test]
    fn adresses_ouvertes_dans_le_navigateur_au_bon_moment() {
        assert_eq!(
            adresse_navigateur("twitch_vod", "123", 3723.0).as_deref(),
            Some("https://www.twitch.tv/videos/123?t=1h2m3s")
        );
        assert_eq!(
            adresse_navigateur("twitch_live", "chaine", 0.0).as_deref(),
            Some("https://www.twitch.tv/chaine")
        );
        assert_eq!(
            adresse_navigateur("twitch_clip", "Drole Clip", 0.0).as_deref(),
            Some("https://clips.twitch.tv/Drole%20Clip")
        );
        assert_eq!(
            adresse_navigateur("vimeo", "76979871", 12.7).as_deref(),
            Some("https://vimeo.com/76979871#t=12s")
        );
        assert_eq!(
            adresse_navigateur("dailymotion", "x8abc", 60.0).as_deref(),
            Some("https://www.dailymotion.com/video/x8abc?start=60")
        );
        assert_eq!(adresse_navigateur("file", "https://x/v.mp4", 0.0), None);
    }

    #[test]
    fn plages_fusionnees_et_avance_chargee() {
        assert_eq!(
            fusionner_plages(&[(10.0, 20.0), (0.0, 5.0), (20.3, 30.0), (40.0, 40.0)], 0.5),
            [(0.0, 5.0), (10.0, 30.0)]
        );
        assert_eq!(avance_chargee(12.0, &[(10.0, 30.0)]), 18.0);
        assert_eq!(avance_chargee(31.0, &[(10.0, 30.0)]), 0.0);
    }

    fn lecteur() -> (LecteurFichier, crate::lecteur::MoteurFactice) {
        let (lecteur, moteur) = Lecteur::factice();
        moteur.ajouter_media("https://api.test/f/v.mp4", MediaFactice::video(120.0));
        moteur.ajouter_media("https://ailleurs.test/v.mp4", MediaFactice::video(60.0));
        (
            LecteurFichier::nouveau(Arc::new(lecteur)).avec_jeton_pour("https://api.test/", "j"),
            moteur,
        )
    }

    #[test]
    fn le_lecteur_de_fichier_suit_le_moteur_et_ne_donne_le_jeton_qu_a_l_api() {
        let (mut l, moteur) = lecteur();
        assert_eq!(l.etat(), EtatLecture::NonDemarre);
        l.charger("https://api.test/f/v.mp4", 30.0, false);
        let CommandeMoteur::Charger(source) = &moteur.commandes()[0] else {
            panic!()
        };
        assert_eq!(source.jeton.as_deref(), Some("j"));
        assert!(source.en_pause);
        assert_eq!(l.suivre(0), Some(EtatLecture::Pause));
        assert_eq!(l.suivre(0), None);
        assert_eq!(l.pret(), Some(true));
        assert!((l.temps() - 30.0).abs() < 1e-3);
        l.lire();
        assert_eq!(l.suivre(0), Some(EtatLecture::Lecture));
        moteur.emettre(crate::lecteur::Evenement::Tampon(true));
        assert_eq!(l.etat(), EtatLecture::Tampon);
        assert_eq!(l.pret(), Some(false));
        moteur.emettre(crate::lecteur::Evenement::Tampon(false));
        moteur.emettre(crate::lecteur::Evenement::CacheJusqua(Some(32.0)));
        assert_eq!(l.pret(), Some(false));
        assert!(l.plages()[0].1 == 32.0);
        moteur.emettre(crate::lecteur::Evenement::CacheJusqua(Some(40.0)));
        assert_eq!(l.pret(), Some(true));
        moteur.avancer(500.0);
        assert_eq!(l.etat(), EtatLecture::Fin);
        l.charger("https://ailleurs.test/v.mp4", 0.0, true);
        let CommandeMoteur::Charger(source) = moteur.commandes().last().unwrap().clone() else {
            panic!()
        };
        assert_eq!(source.jeton, None);
        l.charger("https://inconnu.test/v.mp4", 0.0, true);
        assert!(l.erreur().is_some());
        assert_eq!(l.erreur(), None);
    }

    #[test]
    fn le_volume_a_zero_coupe_vraiment_le_son() {
        let (mut l, moteur) = lecteur();
        l.regler_volume(0);
        l.regler_volume(30);
        assert_eq!(
            moteur.commandes(),
            [
                CommandeMoteur::Volume(0, true),
                CommandeMoteur::Volume(30, false)
            ]
        );
    }
}
