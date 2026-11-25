use reqwest::Method;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::http::{encoder_composant, Client, Corps, ErreurApi};
use crate::temps_reel::evenements::{EtatSeance, Seance};

use super::film::{DemandeFilm, Intro, PistesFilm, PrefTampon, Qualite, ReponseFilm};
use super::fournisseurs::{
    adresse_navigateur, fournisseur, EtatLecture, Fournisseur, LecteurSeance, Plage,
    MESSAGE_NON_PRIS_EN_CHARGE,
};
use super::seance::{message_erreur_youtube, position_attendue, GenreSeance, SeanceEnCours};
use super::synchro::{Commande, MoteurSynchro, SortieSynchro};
use super::youtube::RUNTIME_ABSENT;

pub const SEUL_L_HOTE: &str = "Seul l’hôte pilote la lecture";
pub const SANS_REPONSE: &str = "Le serveur n’a pas répondu";
pub const PASSER_ILLISIBLE_MS: i64 = 1000;
pub const TICK_MS: i64 = 500;

pub fn touche_passer_intro(texte: &str, ctrl: bool, meta: bool, alt: bool) -> bool {
    !(ctrl || meta || alt) && texte.eq_ignore_ascii_case("s")
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppelSeance {
    Action {
        numero: Option<u64>,
        action: String,
        position: Option<f64>,
        vitesse: Option<f64>,
        rev: Option<i64>,
        video: Option<String>,
    },
    Pret {
        pret: bool,
        attente: bool,
    },
    Etat,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActionPilote {
    Appel(AppelSeance),
    Film(DemandeFilm),
    Refus(String),
    Info(String),
    Journal(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VuePilote {
    pub erreur: String,
    pub bloque: bool,
    pub suspendu: bool,
    pub lent: bool,
    pub chargement: bool,
    pub position: f64,
    pub duree: f64,
    pub plages: Vec<Plage>,
    pub navigateur: Option<String>,
    pub peut_piloter: bool,
}

pub type FabriqueLecteur = Box<dyn FnMut(Fournisseur) -> Option<Box<dyn LecteurSeance>> + Send>;

pub struct PiloteSeance {
    genre: GenreSeance,
    moi: String,
    fabrique: FabriqueLecteur,
    lecteur: Option<Box<dyn LecteurSeance>>,
    moteur: Option<MoteurSynchro>,
    seance: Option<SeanceEnCours>,
    fournisseur: Option<String>,
    video: String,
    erreur: String,
    navigateur: Option<String>,
    passer: Option<(i64, i64)>,
    actions: Vec<ActionPilote>,
    position: f64,
    youtube_indisponible: bool,
}

impl PiloteSeance {
    pub fn nouveau(genre: GenreSeance, moi: impl Into<String>, fabrique: FabriqueLecteur) -> Self {
        Self {
            genre,
            moi: moi.into(),
            fabrique,
            lecteur: None,
            moteur: None,
            seance: None,
            fournisseur: None,
            video: String::new(),
            erreur: String::new(),
            navigateur: None,
            passer: None,
            actions: Vec::new(),
            position: 0.0,
            youtube_indisponible: false,
        }
    }

    pub fn actions(&mut self) -> Vec<ActionPilote> {
        std::mem::take(&mut self.actions)
    }

    pub fn lecteur(&self) -> Option<&dyn LecteurSeance> {
        self.lecteur.as_deref()
    }

    pub fn moteur(&self) -> Option<&MoteurSynchro> {
        self.moteur.as_ref()
    }

    pub fn peut_piloter(&self) -> bool {
        self.seance
            .as_ref()
            .is_some_and(|s| !s.etat.host_only || s.etat.host == self.moi)
    }

    pub fn prochaine_echeance(&self) -> Option<i64> {
        let moteur = self
            .moteur
            .as_ref()
            .and_then(MoteurSynchro::prochaine_echeance);
        [moteur, self.passer.map(|(_, t)| t)]
            .into_iter()
            .flatten()
            .min()
    }

    pub fn vue(&self, maintenant: i64) -> VuePilote {
        let (bloque, suspendu, lent, chargement, position) = match (&self.moteur, &self.lecteur) {
            (Some(m), Some(l)) => (
                m.bloque(),
                m.suspendu(),
                m.lent(maintenant),
                m.en_chargement(l.as_ref()),
                m.position(),
            ),
            _ => (false, false, false, false, self.position),
        };
        VuePilote {
            erreur: self.erreur.clone(),
            bloque,
            suspendu,
            lent,
            chargement,
            position,
            duree: self.lecteur.as_ref().map_or(0.0, |l| l.duree()),
            plages: self.lecteur.as_ref().map_or_else(Vec::new, |l| l.plages()),
            navigateur: self.navigateur.clone(),
            peut_piloter: self.peut_piloter(),
        }
    }

    fn etat(&self) -> Option<&EtatSeance> {
        self.seance.as_ref().map(|s| &s.etat)
    }

    pub fn seance(&self) -> Option<&SeanceEnCours> {
        self.seance.as_ref()
    }

    pub fn pistes(&self) -> Option<PistesFilm> {
        self.lecteur.as_ref().and_then(|l| l.pistes_film())
    }

    pub fn intro(&self) -> Option<Intro> {
        if self.fournisseur.as_deref() != Some("film") {
            return None;
        }
        self.pistes().and_then(|p| p.intro)
    }

    pub fn intro_passable(&self, maintenant: i64) -> bool {
        let v = self.vue(maintenant);
        v.peut_piloter
            && v.erreur.is_empty()
            && !v.chargement
            && self.lecteur.is_some()
            && self.intro().is_some_and(|i| i.contient(v.position))
    }

    pub fn passer_intro(&mut self, maintenant: i64) -> bool {
        if !self.intro_passable(maintenant) {
            return false;
        }
        let (Some(intro), Some(titre)) = (self.intro(), self.etat().map(|e| e.video.clone()))
        else {
            return false;
        };
        if self
            .commande(Commande::aller(intro.fin), maintenant)
            .is_err()
        {
            return false;
        }
        self.actions
            .push(ActionPilote::Film(DemandeFilm::IntroPassee(titre)));
        true
    }

    fn utiliser(&mut self, provider: &str, maintenant: i64) {
        self.fournisseur = Some(provider.to_string());
        if let Some(l) = &mut self.lecteur {
            l.detruire();
        }
        self.drainer_film();
        self.lecteur = None;
        self.moteur = None;
        self.navigateur = None;
        self.erreur.clear();
        let f = fournisseur(provider);
        let lecteur = match f {
            Fournisseur::NonPrisEnCharge | Fournisseur::Navigateur => None,
            Fournisseur::YouTube if self.youtube_indisponible => None,
            autre => (self.fabrique)(autre),
        };
        match lecteur {
            Some(l) => {
                self.lecteur = Some(l);
                self.moteur = Some(MoteurSynchro::nouveau(self.genre == GenreSeance::Cinema));
            }
            None if matches!(f, Fournisseur::Navigateur | Fournisseur::YouTube) => {
                self.majour_navigateur(maintenant);
                if f == Fournisseur::YouTube && self.youtube_indisponible {
                    self.actions.push(ActionPilote::Info(RUNTIME_ABSENT.into()));
                }
            }
            None if f == Fournisseur::NonPrisEnCharge => {
                self.erreur = MESSAGE_NON_PRIS_EN_CHARGE.into();
            }
            None => self.erreur = "Lecteur indisponible".into(),
        }
    }

    fn majour_navigateur(&mut self, maintenant: i64) {
        let Some(s) = &self.seance else {
            return;
        };
        let depart = position_attendue(&s.etat, s.decalage, maintenant);
        self.position = depart;
        let provider = self.fournisseur.clone().unwrap_or_default();
        self.navigateur = adresse_navigateur(&provider, &s.etat.video, depart);
    }

    pub fn recevoir(&mut self, seance: Option<&SeanceEnCours>, maintenant: i64) {
        let Some(s) = seance else {
            return;
        };
        self.seance = Some(s.clone());
        let provider = if s.etat.provider.is_empty() {
            "youtube".to_string()
        } else {
            s.etat.provider.clone()
        };
        if s.etat.video != self.video {
            self.video = s.etat.video.clone();
            if self.fournisseur.as_deref() == Some(provider.as_str()) {
                self.erreur.clear();
            }
        }
        if self.fournisseur.as_deref() != Some(provider.as_str()) {
            self.utiliser(&provider, maintenant);
        }
        if self.lecteur.is_none() {
            if self.navigateur.is_some() {
                self.majour_navigateur(maintenant);
            }
            return;
        }
        if let (Some(m), Some(l)) = (&mut self.moteur, &mut self.lecteur) {
            m.recevoir(l.as_mut(), s.etat.clone(), s.decalage, maintenant);
        }
        self.suivre_lecteur(maintenant);
    }

    pub fn tick(&mut self, maintenant: i64) {
        if let Some((rev, t)) = self.passer {
            if maintenant >= t {
                self.passer = None;
                self.actions.push(ActionPilote::Appel(AppelSeance::Action {
                    numero: None,
                    action: "ended".into(),
                    position: None,
                    vitesse: None,
                    rev: Some(rev),
                    video: None,
                }));
            }
        }
        if self.moteur.is_none() || !self.erreur.is_empty() {
            self.drainer_film();
            return;
        }
        self.suivre_lecteur(maintenant);
        if let (Some(m), Some(l)) = (&mut self.moteur, &mut self.lecteur) {
            m.tick(l.as_mut(), maintenant);
        }
        self.drainer();
    }

    pub fn lecteur_change(&mut self, maintenant: i64) {
        self.suivre_lecteur(maintenant);
    }

    fn suivre_lecteur(&mut self, maintenant: i64) {
        let (Some(m), Some(l)) = (&mut self.moteur, &mut self.lecteur) else {
            self.drainer_film();
            return;
        };
        while let Some(st) = l.suivre(maintenant) {
            m.observer(l.as_mut(), st, maintenant);
            if st == EtatLecture::Fin {
                break;
            }
        }
        if let Some(raison) = l.repli() {
            self.repli_navigateur(raison, maintenant);
            return;
        }
        if let Some(message) = l.erreur() {
            self.signaler_erreur(message, maintenant);
        }
        self.drainer();
    }

    fn repli_navigateur(&mut self, raison: String, maintenant: i64) {
        self.actions.push(ActionPilote::Journal(format!(
            "lecteur YouTube indisponible : {raison}"
        )));
        if let Some(l) = &mut self.lecteur {
            l.detruire();
        }
        self.drainer_film();
        self.lecteur = None;
        self.moteur = None;
        self.youtube_indisponible = true;
        self.majour_navigateur(maintenant);
        self.actions.push(ActionPilote::Info(RUNTIME_ABSENT.into()));
    }

    fn signaler_erreur(&mut self, message: String, maintenant: i64) {
        self.actions.push(ActionPilote::Journal(format!(
            "erreur du lecteur : {message}"
        )));
        self.erreur = message;
        if !self.passer_illisible(maintenant) {
            self.actions.push(ActionPilote::Appel(AppelSeance::Pret {
                pret: false,
                attente: false,
            }));
        }
    }

    fn passer_illisible(&mut self, maintenant: i64) -> bool {
        let Some(s) = self.etat() else {
            return false;
        };
        let Some(meta) = &s.meta else {
            return false;
        };
        if self.genre != GenreSeance::Musique || self.erreur == message_erreur_youtube(153) {
            return false;
        }
        let info = format!(
            "« {} » ne peut pas être lu ici : passage au suivant.",
            meta.title
        );
        let rev = s.rev;
        self.actions.push(ActionPilote::Info(info));
        self.passer = Some((rev, maintenant + PASSER_ILLISIBLE_MS));
        true
    }

    fn drainer_film(&mut self) {
        if let Some(l) = &mut self.lecteur {
            self.actions
                .extend(l.demandes().into_iter().map(ActionPilote::Film));
        }
    }

    fn drainer(&mut self) {
        self.drainer_film();
        let Some(m) = &mut self.moteur else {
            return;
        };
        for sortie in m.sorties() {
            let action = match sortie {
                SortieSynchro::Envoyer {
                    numero,
                    action,
                    position,
                    vitesse,
                    video,
                } => ActionPilote::Appel(AppelSeance::Action {
                    numero: Some(numero),
                    action: action.nom().into(),
                    position,
                    vitesse,
                    rev: None,
                    video: Some(video),
                }),
                SortieSynchro::Fin { rev } => ActionPilote::Appel(AppelSeance::Action {
                    numero: None,
                    action: "ended".into(),
                    position: None,
                    vitesse: None,
                    rev: Some(rev),
                    video: None,
                }),
                SortieSynchro::Pret(pret) => ActionPilote::Appel(AppelSeance::Pret {
                    pret,
                    attente: false,
                }),
                SortieSynchro::Attente => ActionPilote::Appel(AppelSeance::Pret {
                    pret: false,
                    attente: true,
                }),
                SortieSynchro::Refus(e) => {
                    ActionPilote::Refus(e.unwrap_or_else(|| SANS_REPONSE.into()))
                }
                SortieSynchro::Journal(m) => ActionPilote::Journal(m),
            };
            self.actions.push(action);
        }
    }

    pub fn commande(&mut self, cmd: Commande, maintenant: i64) -> Result<(), &'static str> {
        if !self.peut_piloter() {
            return Err(SEUL_L_HOTE);
        }
        if let (Some(m), Some(l)) = (&mut self.moteur, &mut self.lecteur) {
            m.commande(l.as_mut(), cmd, maintenant);
        }
        self.drainer();
        Ok(())
    }

    pub fn reponse_appel(
        &mut self,
        appel: &AppelSeance,
        resultat: Result<(), String>,
        maintenant: i64,
    ) {
        if let AppelSeance::Action {
            numero: Some(n), ..
        } = appel
        {
            if let (Some(m), Some(l)) = (&mut self.moteur, &mut self.lecteur) {
                m.reponse(l.as_mut(), *n, resultat, maintenant);
            }
            self.drainer();
        }
    }

    pub fn reponse_film(&mut self, reponse: ReponseFilm, maintenant: i64) {
        if let Some(l) = &mut self.lecteur {
            l.repondre(reponse, maintenant);
        }
        self.suivre_lecteur(maintenant);
    }

    pub fn reconnecte(&mut self) {
        self.actions.push(ActionPilote::Appel(AppelSeance::Etat));
    }

    pub fn recharger(&mut self, maintenant: i64) {
        self.actions
            .push(ActionPilote::Journal("lecteur rechargé".into()));
        self.erreur.clear();
        self.fournisseur = None;
        self.video.clear();
        self.youtube_indisponible = false;
        let seance = self.seance.clone();
        self.recevoir(seance.as_ref(), maintenant);
    }

    pub fn reprendre(&mut self) {
        if let Some(l) = &mut self.lecteur {
            l.lire();
        }
    }

    pub fn regler_volume(&mut self, volume: u8) {
        if let Some(l) = &mut self.lecteur {
            l.regler_volume(volume);
        }
    }

    pub fn choisir_audio(&mut self, piste: i64) {
        if let Some(l) = &mut self.lecteur {
            l.choisir_audio(piste);
        }
        self.drainer_film();
    }

    pub fn choisir_sous_titres(&mut self, piste: Option<i64>) {
        if let Some(l) = &mut self.lecteur {
            l.choisir_sous_titres(piste);
        }
    }

    pub fn choisir_qualite(&mut self, qualite: Qualite) {
        if let Some(l) = &mut self.lecteur {
            l.choisir_qualite(qualite);
        }
        self.drainer_film();
    }

    pub fn regler_tampon(&mut self, tampon: PrefTampon) {
        if let Some(l) = &mut self.lecteur {
            l.regler_tampon(tampon);
        }
        self.drainer_film();
    }

    pub fn regler_wifi_seulement(&mut self, actif: bool) {
        if let Some(l) = &mut self.lecteur {
            l.regler_wifi_seulement(actif);
        }
        self.drainer_film();
    }

    pub fn detruire(&mut self) {
        if let Some(l) = &mut self.lecteur {
            l.detruire();
        }
        self.drainer_film();
        self.lecteur = None;
        self.moteur = None;
    }

    pub fn ceder(&mut self) {
        self.drainer_film();
        self.lecteur = None;
        self.moteur = None;
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct VueSeance {
    state: Option<EtatSeance>,
    now: Option<String>,
}

#[derive(Serialize)]
struct CorpsAction<'a> {
    action: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rev: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    video: Option<&'a str>,
}

fn chemin(salon: &str, genre: GenreSeance) -> String {
    format!("/voice/{}/{}", encoder_composant(salon), genre.chemin())
}

impl Client {
    pub async fn etat_seance(&self, salon: &str, genre: GenreSeance) -> Result<Seance, ErreurApi> {
        let v: VueSeance = self.get(&chemin(salon, genre)).await?;
        Ok(Seance {
            channel_id: salon.to_string(),
            state: v.state,
            now: v.now,
            ..Seance::default()
        })
    }

    pub async fn seances_du_serveur(
        &self,
        serveur: &str,
        genre: GenreSeance,
    ) -> Result<Vec<Seance>, ErreurApi> {
        let v: BTreeMap<String, VueSeance> = self
            .get(&format!(
                "/servers/{}/{}",
                encoder_composant(serveur),
                genre.chemin()
            ))
            .await?;
        Ok(v.into_iter()
            .map(|(salon, v)| Seance {
                server_id: serveur.to_string(),
                channel_id: salon,
                state: v.state,
                now: v.now,
            })
            .collect())
    }

    pub async fn lancer_seance(
        &self,
        salon: &str,
        genre: GenreSeance,
        video: &str,
        file: &[String],
    ) -> Result<(), ErreurApi> {
        let corps = if file.is_empty() {
            serde_json::json!({ "video": video })
        } else {
            serde_json::json!({ "video": video, "queue": file })
        };
        self.envoyer(Method::PUT, &chemin(salon, genre), &Corps::json(&corps)?)
            .await
    }

    pub async fn arreter_seance(&self, salon: &str, genre: GenreSeance) -> Result<(), ErreurApi> {
        self.delete(&chemin(salon, genre)).await
    }

    pub async fn options_seance(
        &self,
        salon: &str,
        genre: GenreSeance,
        hote_seul: bool,
    ) -> Result<(), ErreurApi> {
        let corps = serde_json::json!({ "host_only": hote_seul });
        self.envoyer(
            Method::PUT,
            &format!("{}/options", chemin(salon, genre)),
            &Corps::json(&corps)?,
        )
        .await
    }

    pub async fn ajouter_a_la_file(
        &self,
        salon: &str,
        genre: GenreSeance,
        video: &str,
    ) -> Result<(), ErreurApi> {
        let corps = serde_json::json!({ "video": video });
        self.envoyer(
            Method::POST,
            &format!("{}/queue", chemin(salon, genre)),
            &Corps::json(&corps)?,
        )
        .await
    }

    pub async fn reordonner_file(
        &self,
        salon: &str,
        genre: GenreSeance,
        ids: &[String],
    ) -> Result<(), ErreurApi> {
        let corps = serde_json::json!({ "ids": ids });
        self.envoyer(
            Method::PUT,
            &format!("{}/queue", chemin(salon, genre)),
            &Corps::json(&corps)?,
        )
        .await
    }

    pub async fn retirer_de_la_file(
        &self,
        salon: &str,
        genre: GenreSeance,
        id: &str,
    ) -> Result<(), ErreurApi> {
        self.delete(&format!(
            "{}/queue/{}",
            chemin(salon, genre),
            encoder_composant(id)
        ))
        .await
    }

    pub async fn executer_appel_seance(
        &self,
        salon: &str,
        genre: GenreSeance,
        appel: &AppelSeance,
    ) -> Result<Option<Seance>, ErreurApi> {
        let base = chemin(salon, genre);
        match appel {
            AppelSeance::Action {
                action,
                position,
                vitesse,
                rev,
                video,
                ..
            } => {
                let corps = CorpsAction {
                    action,
                    position: *position,
                    rate: *vitesse,
                    rev: *rev,
                    video: video.as_deref(),
                };
                self.envoyer(
                    Method::POST,
                    &format!("{base}/action"),
                    &Corps::json(&corps)?,
                )
                .await
                .map(|_| None)
            }
            AppelSeance::Pret { pret, attente } => {
                let corps = if *attente {
                    serde_json::json!({ "ready": pret, "wait": true })
                } else {
                    serde_json::json!({ "ready": pret })
                };
                self.envoyer(
                    Method::POST,
                    &format!("{base}/ready"),
                    &Corps::json(&corps)?,
                )
                .await
                .map(|_| None)
            }
            AppelSeance::Etat => self.etat_seance(salon, genre).await.map(Some),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::format::iso_de_ms;
    use crate::lecteur::{CommandeMoteur, Lecteur, MediaFactice, MoteurFactice};
    use crate::regarder_ensemble::fournisseurs::LecteurFichier;
    use crate::regarder_ensemble::seance::RegistreSeances;
    use crate::regarder_ensemble::synchro::tests::base;
    use crate::serveur_factice::{Reponse, ServeurFactice};
    use crate::temps_reel::evenements::{ElementFile, MetaMorceau};

    const VIDEO: &str = "https://estompe.test/f/film.mp4";
    const PAS: i64 = 20;
    const LATENCE: i64 = 80;

    struct LecteurWeb {
        t: f64,
        r: f64,
        voulu: bool,
        st: EtatLecture,
        tampon_jusqua: Option<i64>,
        horloge: i64,
        dernier: EtatLecture,
        reference: String,
        durees: BTreeMap<String, f64>,
    }

    impl LecteurWeb {
        fn nouveau() -> Self {
            Self {
                t: 0.0,
                r: 1.0,
                voulu: false,
                st: EtatLecture::NonDemarre,
                tampon_jusqua: None,
                horloge: 0,
                dernier: EtatLecture::NonDemarre,
                reference: String::new(),
                durees: BTreeMap::new(),
            }
        }

        fn avancer(&mut self, ms: i64) {
            self.horloge += ms;
            if self.tampon_jusqua.is_some_and(|t| self.horloge >= t) {
                self.tampon_jusqua = None;
                self.st = if self.voulu {
                    EtatLecture::Lecture
                } else {
                    EtatLecture::Pause
                };
            }
            if self.st == EtatLecture::Lecture {
                self.t += ms as f64 / 1000.0 * self.r;
                if let Some(&d) = self.durees.get(&self.reference) {
                    if self.t >= d {
                        self.t = d;
                        self.voulu = false;
                        self.st = EtatLecture::Fin;
                    }
                }
            }
        }

        fn charger_pendant(&mut self, ms: i64) {
            self.tampon_jusqua = Some(self.horloge + ms);
            if self.voulu {
                self.st = EtatLecture::Tampon;
            }
        }
    }

    impl LecteurSeance for LecteurWeb {
        fn charger(&mut self, reference: &str, depart: f64, lecture: bool) {
            self.reference = reference.to_string();
            self.t = depart;
            self.voulu = lecture;
            self.st = EtatLecture::Tampon;
            self.tampon_jusqua = Some(self.horloge + 400);
        }
        fn lire(&mut self) {
            self.voulu = true;
            self.st = if self.tampon_jusqua.is_some() {
                EtatLecture::Tampon
            } else {
                EtatLecture::Lecture
            };
        }
        fn pause(&mut self) {
            self.voulu = false;
            self.st = EtatLecture::Pause;
        }
        fn aller(&mut self, position: f64) {
            self.t = position;
            self.charger_pendant(200);
        }
        fn temps(&self) -> f64 {
            self.t
        }
        fn vitesse(&self) -> f64 {
            self.r
        }
        fn regler_vitesse(&mut self, vitesse: f64) {
            self.r = vitesse;
        }
        fn etat(&self) -> EtatLecture {
            self.st
        }
        fn suivre(&mut self, _maintenant_ms: i64) -> Option<EtatLecture> {
            (std::mem::replace(&mut self.dernier, self.st) != self.st).then_some(self.st)
        }
    }

    struct Partage(Arc<std::sync::Mutex<LecteurWeb>>);

    impl LecteurSeance for Partage {
        fn charger(&mut self, r: &str, d: f64, l: bool) {
            self.0.lock().unwrap().charger(r, d, l)
        }
        fn lire(&mut self) {
            self.0.lock().unwrap().lire()
        }
        fn pause(&mut self) {
            self.0.lock().unwrap().pause()
        }
        fn aller(&mut self, p: f64) {
            self.0.lock().unwrap().aller(p)
        }
        fn temps(&self) -> f64 {
            self.0.lock().unwrap().temps()
        }
        fn vitesse(&self) -> f64 {
            self.0.lock().unwrap().vitesse()
        }
        fn regler_vitesse(&mut self, v: f64) {
            self.0.lock().unwrap().regler_vitesse(v)
        }
        fn etat(&self) -> EtatLecture {
            self.0.lock().unwrap().etat()
        }
        fn suivre(&mut self, m: i64) -> Option<EtatLecture> {
            self.0.lock().unwrap().suivre(m)
        }
    }

    struct Serveur {
        etat: EtatSeance,
    }

    fn changer_de_titre(e: &mut EtatSeance, suivant: ElementFile, maintenant: i64) {
        e.rev += 1;
        e.act = "video".into();
        e.at = iso_de_ms(maintenant);
        e.provider = suivant.provider;
        e.video = suivant.video;
        e.meta = suivant.meta;
        e.pos = 0.0;
        e.playing = true;
        e.ready.clear();
    }

    fn courant(e: &EtatSeance) -> ElementFile {
        ElementFile {
            id: format!("h{}", e.rev),
            provider: e.provider.clone(),
            video: e.video.clone(),
            meta: e.meta.clone(),
            ..ElementFile::default()
        }
    }

    impl Serveur {
        fn appliquer(
            &mut self,
            action: &str,
            position: Option<f64>,
            vitesse: Option<f64>,
            rev: Option<i64>,
            maintenant: i64,
        ) -> bool {
            let actuelle = position_attendue(&self.etat, 0, maintenant);
            let e = &mut self.etat;
            match action {
                "ended" if rev != Some(e.rev) || e.queue.is_empty() => return false,
                "ended" => {
                    let suivant = e.queue.remove(0);
                    let present = courant(e);
                    e.history.push(present);
                    changer_de_titre(e, suivant, maintenant);
                    return true;
                }
                "previous" => {
                    let Some(precedent) = e.history.pop() else {
                        return false;
                    };
                    let present = courant(e);
                    e.queue.insert(0, present);
                    changer_de_titre(e, precedent, maintenant);
                    return true;
                }
                _ => {}
            }
            e.rev += 1;
            e.act = action.into();
            e.at = iso_de_ms(maintenant);
            match action {
                "play" => {
                    e.playing = true;
                    e.pos = position.unwrap_or(actuelle);
                }
                "pause" => {
                    e.playing = false;
                    e.pos = position.unwrap_or(actuelle);
                }
                "seek" => e.pos = position.unwrap_or(actuelle),
                "rate" => {
                    e.pos = actuelle;
                    e.rate = vitesse.unwrap_or(1.0);
                }
                _ => {
                    e.rev -= 1;
                    e.pos = actuelle;
                }
            }
            true
        }

        fn vue(&self, maintenant: i64) -> Seance {
            Seance {
                channel_id: "v1".into(),
                state: Some(self.etat.clone()),
                now: Some(iso_de_ms(maintenant)),
                ..Seance::default()
            }
        }
    }

    enum Message {
        VersServeur {
            client: usize,
            appel: AppelSeance,
        },
        VersClient {
            client: usize,
            vue: Box<Seance>,
            frais: bool,
            reponse: Option<AppelSeance>,
        },
    }

    struct Client2 {
        pilote: PiloteSeance,
        registre: RegistreSeances,
        decalage_horloge: i64,
        coupe: bool,
    }

    struct Monde {
        maintenant: i64,
        serveur: Serveur,
        clients: Vec<Client2>,
        reseau: VecDeque<(i64, Message)>,
        moteur: MoteurFactice,
        webs: Vec<Arc<std::sync::Mutex<LecteurWeb>>>,
        genre: GenreSeance,
    }

    impl Monde {
        fn nouveau() -> Self {
            let (lecteur, moteur) = Lecteur::factice();
            moteur.ajouter_media(VIDEO, MediaFactice::video(7200.0));
            let lecteur = Arc::new(lecteur);
            let natif: FabriqueLecteur = Box::new(move |f| {
                (f == Fournisseur::Fichier).then(|| {
                    Box::new(LecteurFichier::nouveau(lecteur.clone())) as Box<dyn LecteurSeance>
                })
            });
            let web = Arc::new(std::sync::Mutex::new(LecteurWeb::nouveau()));
            let w = web.clone();
            let navigateur: FabriqueLecteur =
                Box::new(move |_| Some(Box::new(Partage(w.clone())) as Box<dyn LecteurSeance>));
            let maintenant = base();
            let etat = EtatSeance {
                rev: 1,
                provider: "file".into(),
                video: VIDEO.into(),
                playing: true,
                pos: 0.0,
                rate: 1.0,
                at: iso_de_ms(maintenant),
                act: "video".into(),
                ..EtatSeance::default()
            };
            let mut m = Self {
                maintenant,
                serveur: Serveur { etat },
                clients: vec![
                    Client2 {
                        pilote: PiloteSeance::nouveau(GenreSeance::Regarder, "web", navigateur),
                        registre: RegistreSeances::default(),
                        decalage_horloge: 1500,
                        coupe: false,
                    },
                    Client2 {
                        pilote: PiloteSeance::nouveau(GenreSeance::Regarder, "natif", natif),
                        registre: RegistreSeances::default(),
                        decalage_horloge: -700,
                        coupe: false,
                    },
                ],
                reseau: VecDeque::new(),
                moteur,
                webs: vec![web],
                genre: GenreSeance::Regarder,
            };
            for c in 0..2 {
                let vue = m.serveur.vue(m.maintenant);
                m.livrer(c, vue, true);
            }
            m
        }

        fn musique(titres: &[(&str, f64)]) -> Self {
            let (_, moteur) = Lecteur::factice();
            let maintenant = base();
            let webs: Vec<_> = (0..2)
                .map(|_| {
                    let mut w = LecteurWeb::nouveau();
                    w.durees = titres.iter().map(|(t, d)| (t.to_string(), *d)).collect();
                    Arc::new(std::sync::Mutex::new(w))
                })
                .collect();
            let fabrique = |w: Arc<std::sync::Mutex<LecteurWeb>>| -> FabriqueLecteur {
                Box::new(move |f| {
                    (f == Fournisseur::YouTube)
                        .then(|| Box::new(Partage(w.clone())) as Box<dyn LecteurSeance>)
                })
            };
            let element = |(t, d): &(&str, f64)| ElementFile {
                id: format!("q-{t}"),
                provider: "youtube".into(),
                video: t.to_string(),
                by: "web".into(),
                by_id: "web".into(),
                meta: Some(MetaMorceau {
                    title: format!("Titre {t}"),
                    duration: *d as i64,
                    ..MetaMorceau::default()
                }),
            };
            let mut etat = EtatSeance {
                rev: 1,
                rate: 1.0,
                at: iso_de_ms(maintenant),
                queue: titres.iter().map(element).collect(),
                ..EtatSeance::default()
            };
            let premier = etat.queue.remove(0);
            changer_de_titre(&mut etat, premier, maintenant);
            let clients = [("web", 1500), ("natif", -700)]
                .into_iter()
                .zip(&webs)
                .map(|((moi, decalage), w)| Client2 {
                    pilote: PiloteSeance::nouveau(GenreSeance::Musique, moi, fabrique(w.clone())),
                    registre: RegistreSeances::default(),
                    decalage_horloge: decalage,
                    coupe: false,
                })
                .collect();
            let mut m = Self {
                maintenant,
                serveur: Serveur { etat },
                clients,
                reseau: VecDeque::new(),
                moteur,
                webs,
                genre: GenreSeance::Musique,
            };
            for c in 0..2 {
                let vue = m.serveur.vue(m.maintenant);
                m.livrer(c, vue, true);
            }
            m
        }

        fn titres(&self) -> Vec<String> {
            self.webs
                .iter()
                .map(|w| w.lock().unwrap().reference.clone())
                .collect()
        }

        fn heure(&self, client: usize) -> i64 {
            self.maintenant + self.clients[client].decalage_horloge
        }

        fn livrer(&mut self, client: usize, vue: Seance, frais: bool) {
            let heure = self.heure(client);
            let c = &mut self.clients[client];
            c.registre.appliquer(self.genre, &vue, frais, heure);
            let seance = c.registre.seance(self.genre, "v1").cloned();
            c.pilote.recevoir(seance.as_ref(), heure);
        }

        fn ramasser(&mut self) {
            for client in 0..self.clients.len() {
                for action in self.clients[client].pilote.actions() {
                    if let ActionPilote::Appel(appel) = action {
                        if self.clients[client].coupe {
                            continue;
                        }
                        self.reseau.push_back((
                            self.maintenant + LATENCE,
                            Message::VersServeur { client, appel },
                        ));
                    }
                }
            }
        }

        fn traiter(&mut self) {
            let mut restants = VecDeque::new();
            while let Some((a, message)) = self.reseau.pop_front() {
                if a > self.maintenant {
                    restants.push_back((a, message));
                    continue;
                }
                match message {
                    Message::VersServeur { client, appel } => {
                        let retour = self.maintenant + LATENCE;
                        match &appel {
                            AppelSeance::Action {
                                action,
                                position,
                                vitesse,
                                rev,
                                ..
                            } => {
                                let change = self.serveur.appliquer(
                                    action,
                                    *position,
                                    *vitesse,
                                    *rev,
                                    self.maintenant,
                                );
                                for c in 0..self.clients.len() {
                                    if !change && c != client {
                                        continue;
                                    }
                                    let vue = self.serveur.vue(self.maintenant);
                                    restants.push_back((
                                        retour,
                                        Message::VersClient {
                                            client: c,
                                            vue: Box::new(vue),
                                            frais: false,
                                            reponse: (c == client).then(|| appel.clone()),
                                        },
                                    ));
                                }
                            }
                            AppelSeance::Etat => {
                                let vue = self.serveur.vue(self.maintenant);
                                restants.push_back((
                                    retour,
                                    Message::VersClient {
                                        client,
                                        vue: Box::new(vue),
                                        frais: true,
                                        reponse: None,
                                    },
                                ));
                            }
                            _ => {}
                        }
                    }
                    Message::VersClient {
                        client,
                        vue,
                        frais,
                        reponse,
                    } => {
                        if self.clients[client].coupe {
                            continue;
                        }
                        self.livrer(client, *vue, frais);
                        if let Some(appel) = reponse {
                            let heure = self.heure(client);
                            self.clients[client]
                                .pilote
                                .reponse_appel(&appel, Ok(()), heure);
                        }
                    }
                }
            }
            self.reseau = restants;
        }

        fn passer(&mut self, ms: i64) {
            let fin = self.maintenant + ms;
            while self.maintenant < fin {
                self.maintenant += PAS;
                self.moteur.avancer(PAS as f64 / 1000.0);
                for w in &self.webs {
                    w.lock().unwrap().avancer(PAS);
                }
                self.traiter();
                for c in 0..self.clients.len() {
                    let heure = self.heure(c);
                    let p = &mut self.clients[c].pilote;
                    p.lecteur_change(heure);
                    if (self.maintenant - base()) % TICK_MS == (c as i64 * 240) % TICK_MS {
                        p.tick(heure);
                    }
                }
                self.ramasser();
            }
        }

        fn commande(&mut self, client: usize, f: impl FnOnce(f64) -> Commande) {
            let heure = self.heure(client);
            let p = &mut self.clients[client].pilote;
            let position = p.lecteur().map_or(0.0, |l| l.temps());
            p.commande(f(position), heure).unwrap();
            self.ramasser();
        }

        fn positions(&self) -> (f64, f64, f64) {
            let web = self.clients[0].pilote.lecteur().unwrap().temps();
            let natif = self.clients[1].pilote.lecteur().unwrap().temps();
            (
                web,
                natif,
                position_attendue(&self.serveur.etat, 0, self.maintenant),
            )
        }

        fn ecart(&self) -> f64 {
            let (a, b, _) = self.positions();
            (a - b).abs()
        }
    }

    #[test]
    fn un_client_web_simule_et_un_client_natif_restent_a_moins_de_500_ms_apres_dix_commandes() {
        let mut m = Monde::nouveau();
        m.passer(3000);
        assert!(m.ecart() < 0.5, "au démarrage : {:?}", m.positions());
        type Etape = (usize, fn(f64) -> Commande);
        let commandes: [Etape; 10] = [
            (0, Commande::pause),
            (1, Commande::lire),
            (0, |p| Commande::aller(p + 30.0)),
            (1, |_| Commande::vitesse(1.5)),
            (0, |p| Commande::aller((p - 10.0).max(0.0))),
            (1, |_| Commande::vitesse(1.0)),
            (1, Commande::pause),
            (0, |p| Commande::aller(p + 120.0)),
            (0, Commande::lire),
            (1, |p| Commande::aller(p + 5.0)),
        ];
        for (i, (client, f)) in commandes.into_iter().enumerate() {
            m.commande(client, f);
            m.passer(2500);
            assert!(
                m.ecart() < 0.5,
                "après la commande {} : {:?}",
                i + 1,
                m.positions()
            );
        }
        m.passer(5000);
        let (web, natif, serveur) = m.positions();
        assert!(m.ecart() < 0.5, "web {web} natif {natif} serveur {serveur}");
        assert!(
            (natif - serveur).abs() < 0.5,
            "natif {natif} serveur {serveur}"
        );
        assert_eq!(m.serveur.etat.rev, 11);
        assert!(m.serveur.etat.playing);
        for c in &m.clients {
            assert!(!c.pilote.vue(m.maintenant).suspendu);
        }
    }

    #[test]
    fn deux_clients_factices_restent_synchronises_sur_un_changement_de_titre() {
        let mut m = Monde::musique(&[("un", 12.0), ("deux", 240.0), ("trois", 180.0)]);
        m.passer(3000);
        assert_eq!(m.titres(), ["un", "un"]);
        assert!(m.ecart() < 0.5, "au démarrage : {:?}", m.positions());
        m.passer(11_000);
        assert_eq!(m.titres(), ["deux", "deux"]);
        assert_eq!(m.serveur.etat.video, "deux");
        assert_eq!(m.serveur.etat.history.len(), 1, "une seule fin comptée");
        assert!(
            m.ecart() < 0.5,
            "après la fin du titre : {:?}",
            m.positions()
        );
        m.passer(5000);
        let (web, natif, serveur) = m.positions();
        assert!(
            m.ecart() < 0.5 && (natif - serveur).abs() < 0.5,
            "{web} {natif} {serveur}"
        );
        assert!(serveur > 5.0);
        let rev = m.serveur.etat.rev;
        for client in &mut m.clients[..2] {
            client
                .pilote
                .actions
                .push(ActionPilote::Appel(AppelSeance::Action {
                    numero: None,
                    action: "ended".into(),
                    position: None,
                    vitesse: None,
                    rev: Some(rev),
                    video: None,
                }));
        }
        m.ramasser();
        m.passer(3000);
        assert_eq!(m.titres(), ["trois", "trois"]);
        assert_eq!(
            m.serveur.etat.rev,
            rev + 1,
            "le second « suivant » est ignoré"
        );
        assert!(m.ecart() < 0.5, "après « suivant » : {:?}", m.positions());
        m.commande(1, |p| Commande::aller(p + 60.0));
        m.passer(2500);
        assert!(
            m.ecart() < 0.5,
            "après un déplacement : {:?}",
            m.positions()
        );
        m.clients[0]
            .pilote
            .actions
            .push(ActionPilote::Appel(AppelSeance::Action {
                numero: None,
                action: "previous".into(),
                position: None,
                vitesse: None,
                rev: None,
                video: None,
            }));
        m.ramasser();
        m.passer(3000);
        assert_eq!(m.titres(), ["deux", "deux"]);
        let (web, natif, serveur) = m.positions();
        assert!(
            m.ecart() < 0.5 && (natif - serveur).abs() < 0.5,
            "{web} {natif} {serveur}"
        );
        assert!(
            serveur < 5.0,
            "le titre précédent repart du début : {serveur}"
        );
        for c in &m.clients {
            assert!(!c.pilote.vue(m.maintenant).suspendu);
            assert!(c.pilote.vue(m.maintenant).erreur.is_empty());
        }
    }

    #[test]
    fn une_coupure_du_reseau_de_30_secondes_est_rattrapee_sans_intervention() {
        let mut m = Monde::nouveau();
        m.passer(3000);
        m.clients[1].coupe = true;
        m.passer(2000);
        m.commande(1, Commande::pause);
        m.passer(3000);
        m.commande(0, Commande::pause);
        m.passer(5000);
        m.commande(0, |_| Commande::aller(300.0));
        m.passer(5000);
        m.commande(0, Commande::lire);
        m.passer(5000);
        m.commande(0, |_| Commande::vitesse(1.5));
        m.passer(10_000);
        let (_, natif, serveur) = m.positions();
        assert!((natif - serveur).abs() > 5.0);
        m.clients[1].coupe = false;
        m.clients[1].pilote.reconnecte();
        m.ramasser();
        m.passer(3000);
        let (web, natif, serveur) = m.positions();
        assert!(m.ecart() < 0.5, "web {web} natif {natif} serveur {serveur}");
        assert!((natif - serveur).abs() < 0.5);
        assert!(m.moteur.position() > 300.0);
    }

    fn seance(etat: EtatSeance) -> SeanceEnCours {
        SeanceEnCours { etat, decalage: 0 }
    }

    fn etat(provider: &str, video: &str) -> EtatSeance {
        EtatSeance {
            rev: 1,
            provider: provider.into(),
            video: video.into(),
            playing: true,
            pos: 60.0,
            rate: 1.0,
            at: iso_de_ms(base()),
            host: "hote".into(),
            host_only: true,
            ..EtatSeance::default()
        }
    }

    fn sans_lecteur() -> FabriqueLecteur {
        Box::new(|_| None)
    }

    #[test]
    fn vimeo_twitch_et_dailymotion_s_ouvrent_dans_le_navigateur_au_bon_moment() {
        let mut p = PiloteSeance::nouveau(GenreSeance::Regarder, "moi", sans_lecteur());
        p.recevoir(Some(&seance(etat("vimeo", "76979871"))), base() + 12_000);
        let v = p.vue(base());
        assert_eq!(
            v.navigateur.as_deref(),
            Some("https://vimeo.com/76979871#t=72s")
        );
        assert_eq!(v.erreur, "");
        p.recevoir(Some(&seance(etat("twitch_vod", "123"))), base());
        assert_eq!(
            p.vue(base()).navigateur.as_deref(),
            Some("https://www.twitch.tv/videos/123?t=0h1m0s")
        );
        p.recevoir(Some(&seance(etat("youtube", "dQw4w9WgXcQ"))), base());
        assert!(p.vue(base()).navigateur.unwrap().contains("dQw4w9WgXcQ"));
        p.recevoir(Some(&seance(etat("inconnu", "x"))), base());
        assert_eq!(p.vue(base()).erreur, MESSAGE_NON_PRIS_EN_CHARGE);
        assert!(p.vue(base()).navigateur.is_none());
        assert!(p.actions().is_empty());
    }

    #[test]
    fn seul_l_hote_pilote_quand_la_seance_le_demande() {
        let (lecteur, moteur) = Lecteur::factice();
        moteur.ajouter_media(VIDEO, MediaFactice::video(600.0));
        let lecteur = Arc::new(lecteur);
        let fabrique: FabriqueLecteur = Box::new(move |_| {
            Some(Box::new(LecteurFichier::nouveau(lecteur.clone())) as Box<dyn LecteurSeance>)
        });
        let mut p = PiloteSeance::nouveau(GenreSeance::Cinema, "moi", fabrique);
        p.recevoir(Some(&seance(etat("file", VIDEO))), base());
        assert!(!p.peut_piloter());
        assert_eq!(p.commande(Commande::pause(1.0), base()), Err(SEUL_L_HOTE));
        let mut libre = etat("file", VIDEO);
        libre.host_only = false;
        p.recevoir(Some(&seance(libre)), base());
        assert!(p.commande(Commande::pause(1.0), base()).is_ok());
        assert!(p.actions().iter().any(|a| matches!(
            a,
            ActionPilote::Appel(AppelSeance::Action { action, position: Some(_), .. }) if action == "pause"
        )));
    }

    #[test]
    fn ceder_le_lecteur_ne_l_arrete_pas_detruire_l_arrete() {
        let (lecteur, moteur) = Lecteur::factice();
        moteur.ajouter_media(VIDEO, MediaFactice::video(600.0));
        let lecteur = Arc::new(lecteur);
        let pilote = || {
            let l = lecteur.clone();
            let fabrique: FabriqueLecteur = Box::new(move |_| {
                Some(Box::new(LecteurFichier::nouveau(l.clone())) as Box<dyn LecteurSeance>)
            });
            let mut p = PiloteSeance::nouveau(GenreSeance::Regarder, "moi", fabrique);
            p.recevoir(Some(&seance(etat("file", VIDEO))), base());
            assert!(p.lecteur().is_some());
            p
        };
        let mut p = pilote();
        moteur.vider_commandes();
        p.ceder();
        assert!(p.lecteur().is_none());
        assert!(!moteur.commandes().contains(&CommandeMoteur::Arreter));
        let mut p = pilote();
        moteur.vider_commandes();
        p.detruire();
        assert!(moteur.commandes().contains(&CommandeMoteur::Arreter));
    }

    #[test]
    fn un_morceau_illisible_est_passe_par_le_premier_client_qui_le_signale() {
        let (lecteur, moteur) = Lecteur::factice();
        moteur.ajouter_echec("https://x.test/m.mp3", "HTTP 451");
        let lecteur = Arc::new(lecteur);
        let fabrique: FabriqueLecteur = Box::new(move |_| {
            Some(Box::new(LecteurFichier::nouveau(lecteur.clone())) as Box<dyn LecteurSeance>)
        });
        let mut p = PiloteSeance::nouveau(GenreSeance::Musique, "moi", fabrique);
        let mut e = etat("file", "https://x.test/m.mp3");
        e.rev = 7;
        e.meta = Some(MetaMorceau {
            title: "Titre".into(),
            ..MetaMorceau::default()
        });
        p.recevoir(Some(&seance(e)), base());
        let actions = p.actions();
        assert!(actions.contains(&ActionPilote::Info(
            "« Titre » ne peut pas être lu ici : passage au suivant.".into()
        )));
        assert!(!p.vue(base()).erreur.is_empty());
        assert_eq!(p.prochaine_echeance(), Some(base() + PASSER_ILLISIBLE_MS));
        p.tick(base() + 500);
        assert!(p.actions().is_empty());
        p.tick(base() + 1000);
        assert_eq!(
            p.actions(),
            [ActionPilote::Appel(AppelSeance::Action {
                numero: None,
                action: "ended".into(),
                position: None,
                vitesse: None,
                rev: Some(7),
                video: None
            })]
        );
    }

    type Journal = Arc<std::sync::Mutex<Vec<String>>>;

    struct Enregistreur {
        journal: Journal,
        n: usize,
        t: f64,
        st: EtatLecture,
    }

    impl LecteurSeance for Enregistreur {
        fn charger(&mut self, reference: &str, depart: f64, lecture: bool) {
            self.t = depart;
            self.st = if lecture {
                EtatLecture::Lecture
            } else {
                EtatLecture::Pause
            };
            self.journal
                .lock()
                .unwrap()
                .push(format!("{} load {reference} {depart} {lecture}", self.n));
        }
        fn lire(&mut self) {
            self.st = EtatLecture::Lecture;
        }
        fn pause(&mut self) {
            self.st = EtatLecture::Pause;
        }
        fn aller(&mut self, position: f64) {
            self.t = position;
        }
        fn temps(&self) -> f64 {
            self.t
        }
        fn vitesse(&self) -> f64 {
            1.0
        }
        fn regler_vitesse(&mut self, _vitesse: f64) {}
        fn etat(&self) -> EtatLecture {
            self.st
        }
        fn detruire(&mut self) {
            self.journal
                .lock()
                .unwrap()
                .push(format!("{} destroy", self.n));
        }
    }

    #[test]
    fn recharger_recree_le_lecteur_et_recharge_le_film_a_la_position_de_la_seance() {
        let journal: Journal = Arc::default();
        let j = journal.clone();
        let mut n = 0;
        let fabrique: FabriqueLecteur = Box::new(move |f| {
            assert_eq!(f, Fournisseur::Film);
            n += 1;
            Some(Box::new(Enregistreur {
                journal: j.clone(),
                n,
                t: 0.0,
                st: EtatLecture::NonDemarre,
            }) as Box<dyn LecteurSeance>)
        });
        let mut p = PiloteSeance::nouveau(GenreSeance::Cinema, "u", fabrique);
        let mut e = etat("film", "f1");
        e.playing = false;
        e.pos = 600.0;
        e.host = "u".into();
        e.host_only = false;
        p.recevoir(Some(&seance(e)), base());
        assert_eq!(*journal.lock().unwrap(), ["1 load f1 600 false"]);
        p.recharger(base() + 1000);
        assert_eq!(
            *journal.lock().unwrap(),
            ["1 load f1 600 false", "1 destroy", "2 load f1 600 false"]
        );
        p.detruire();
        assert_eq!(journal.lock().unwrap().last().unwrap(), "2 destroy");
    }

    const ENTRACTE: &str = "https://entracte.test";

    fn seance_film(host: &str, host_only: bool) -> (PiloteSeance, MoteurFactice) {
        let (lecteur, moteur) = Lecteur::factice();
        let lecteur = Arc::new(lecteur);
        let fabrique: FabriqueLecteur = Box::new(move |_| {
            Some(
                Box::new(crate::regarder_ensemble::film::LecteurFilm::nouveau(
                    lecteur.clone(),
                    ENTRACTE,
                    crate::regarder_ensemble::film::PreferencesFilm::default(),
                )) as Box<dyn LecteurSeance>,
            )
        });
        let mut p = PiloteSeance::nouveau(GenreSeance::Cinema, "u", fabrique);
        let mut e = etat("film", "f1");
        e.pos = 45.0;
        e.host = host.into();
        e.host_only = host_only;
        p.recevoir(Some(&seance(e)), base());
        let tour = p
            .actions()
            .into_iter()
            .find_map(|a| match a {
                ActionPilote::Film(DemandeFilm::Ouvrir { tour, .. }) => Some(tour),
                _ => None,
            })
            .expect("flux demandé");
        assert!(p.intro().is_none());
        assert!(!p.passer_intro(base()));
        let flux = crate::regarder_ensemble::film::FluxFilm {
            stream_id: "s1".into(),
            url: "/stream/j1/direct".into(),
            duration: 3000.0,
            intro: Some(Intro {
                debut: 30.0,
                fin: 90.0,
            }),
            ..Default::default()
        };
        moteur.ajouter_media(
            &format!("{ENTRACTE}/stream/j1/direct"),
            MediaFactice::video(3000.0),
        );
        p.reponse_film(
            ReponseFilm::Flux {
                tour,
                resultat: Ok(Box::new(flux)),
            },
            base(),
        );
        p.tick(base() + TICK_MS);
        p.actions();
        (p, moteur)
    }

    #[test]
    fn passer_l_intro_deplace_la_seance_de_tous_au_bouton_comme_a_la_touche_s() {
        let (mut p, moteur) = seance_film("u", false);
        let maintenant = base() + TICK_MS;
        assert_eq!(
            p.intro(),
            Some(Intro {
                debut: 30.0,
                fin: 90.0
            })
        );
        assert!(p.intro_passable(maintenant));
        assert!(touche_passer_intro("S", false, false, false));
        assert!(!touche_passer_intro("s", true, false, false));
        assert!(p.passer_intro(maintenant));
        let actions = p.actions();
        assert!(actions.iter().any(|a| matches!(
            a,
            ActionPilote::Appel(AppelSeance::Action { action, position: Some(pos), video: Some(v), .. })
                if action == "seek" && (*pos - 90.0).abs() < 1e-6 && v == "f1"
        )));
        assert!(actions.contains(&ActionPilote::Film(DemandeFilm::IntroPassee("f1".into()))));
        assert!(moteur.commandes().iter().any(
            |c| matches!(c, crate::lecteur::CommandeMoteur::Aller(t) if (*t - 90.0).abs() < 1.0)
        ));
        assert!(!p.intro_passable(maintenant));
        assert!(!p.passer_intro(maintenant));
    }

    #[test]
    fn sans_le_droit_de_piloter_une_intro_connue_ne_fait_ni_bouton_ni_saut() {
        let (mut p, moteur) = seance_film("autre", true);
        let maintenant = base() + TICK_MS;
        assert!(p.intro().is_some());
        assert!(!p.intro_passable(maintenant));
        assert!(!p.passer_intro(maintenant));
        assert!(!p.actions().iter().any(|a| matches!(
            a,
            ActionPilote::Appel(AppelSeance::Action { action, .. }) if action == "seek"
        ) || matches!(
            a,
            ActionPilote::Film(DemandeFilm::IntroPassee(_))
        )));
        assert!(!moteur.commandes().iter().any(
            |c| matches!(c, crate::lecteur::CommandeMoteur::Aller(t) if (*t - 90.0).abs() < 1.0)
        ));
    }

    #[tokio::test]
    async fn les_appels_de_seance_partent_aux_bons_chemins() {
        let serveur = ServeurFactice::demarrer([
            Reponse::vide(204),
            Reponse::vide(204),
            Reponse::json(200, json!({"state": {"rev": 3, "provider": "film", "video": "t1"}, "now": "2026-01-01T00:00:00Z"})),
            Reponse::json(200, json!({"v3": {"state": null, "now": "2026-01-01T00:00:00Z"}})),
            Reponse::vide(204),
            Reponse::vide(204),
            Reponse::vide(204),
            Reponse::vide(204),
            Reponse::vide(204),
            Reponse::vide(204),
            Reponse::vide(204),
        ])
        .await;
        let client = serveur.client();
        let action = AppelSeance::Action {
            numero: Some(1),
            action: "seek".into(),
            position: Some(12.5),
            vitesse: None,
            rev: None,
            video: Some("t1".into()),
        };
        client
            .executer_appel_seance("v3", GenreSeance::Cinema, &action)
            .await
            .unwrap();
        client
            .executer_appel_seance(
                "v3",
                GenreSeance::Cinema,
                &AppelSeance::Pret {
                    pret: false,
                    attente: true,
                },
            )
            .await
            .unwrap();
        let etat = client
            .executer_appel_seance("v3", GenreSeance::Cinema, &AppelSeance::Etat)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            (etat.channel_id.as_str(), etat.state.unwrap().rev),
            ("v3", 3)
        );
        let toutes = client
            .seances_du_serveur("s1", GenreSeance::Cinema)
            .await
            .unwrap();
        assert_eq!(
            (toutes[0].channel_id.as_str(), toutes[0].state.is_none()),
            ("v3", true)
        );
        client
            .lancer_seance("v1", GenreSeance::Regarder, "abc", &["def".into()])
            .await
            .unwrap();
        client
            .arreter_seance("v1", GenreSeance::Musique)
            .await
            .unwrap();
        let r = serveur.requetes();
        assert_eq!(r[0].ligne(), ("/voice/v3/cinema/action", "POST"));
        assert_eq!(
            r[0].json(),
            json!({"action": "seek", "position": 12.5, "video": "t1"})
        );
        assert_eq!(r[1].ligne(), ("/voice/v3/cinema/ready", "POST"));
        assert_eq!(r[1].json(), json!({"ready": false, "wait": true}));
        assert_eq!(r[2].ligne(), ("/voice/v3/cinema", "GET"));
        assert_eq!(r[3].ligne(), ("/servers/s1/cinema", "GET"));
        assert_eq!(r[4].ligne(), ("/voice/v1/watch", "PUT"));
        assert_eq!(r[4].json(), json!({"video": "abc", "queue": ["def"]}));
        assert_eq!(r[5].ligne(), ("/voice/v1/music", "DELETE"));
        client
            .options_seance("v1", GenreSeance::Regarder, true)
            .await
            .unwrap();
        client
            .ajouter_a_la_file("v1", GenreSeance::Regarder, "https://x.test/v.mp4")
            .await
            .unwrap();
        client
            .reordonner_file("v1", GenreSeance::Regarder, &["b".into(), "a".into()])
            .await
            .unwrap();
        client
            .retirer_de_la_file("v1", GenreSeance::Regarder, "a b")
            .await
            .unwrap();
        client.intro_passee("t 1").await.unwrap();
        let r = serveur.requetes();
        assert_eq!(r[6].ligne(), ("/voice/v1/watch/options", "PUT"));
        assert_eq!(r[6].json(), json!({"host_only": true}));
        assert_eq!(r[7].ligne(), ("/voice/v1/watch/queue", "POST"));
        assert_eq!(r[7].json(), json!({"video": "https://x.test/v.mp4"}));
        assert_eq!(r[8].ligne(), ("/voice/v1/watch/queue", "PUT"));
        assert_eq!(r[8].json(), json!({"ids": ["b", "a"]}));
        assert_eq!(r[9].ligne(), ("/voice/v1/watch/queue/a%20b", "DELETE"));
        assert_eq!(r[10].ligne(), ("/titles/t%201/intro-skipped", "POST"));
    }

    fn fabrique_youtube(
        liens: Arc<std::sync::Mutex<Vec<crate::regarder_ensemble::youtube::LienVue>>>,
    ) -> FabriqueLecteur {
        Box::new(move |f| {
            if f != Fournisseur::YouTube {
                return None;
            }
            let lien = crate::regarder_ensemble::youtube::LienVue::nouveau();
            liens.lock().unwrap().push(lien.clone());
            Some(
                Box::new(crate::regarder_ensemble::youtube::LecteurYoutube::nouveau(
                    lien,
                )) as Box<dyn LecteurSeance>,
            )
        })
    }

    const PAGE_VUE: &str = "https://estompe.localhost";
    const SOURCE_VUE: &str = "https://estompe.localhost/lecteur";

    fn commandes_youtube(lien: &crate::regarder_ensemble::youtube::LienVue) -> Vec<String> {
        lien.sortants()
            .iter()
            .filter_map(|m| serde_json::from_str::<serde_json::Value>(m).ok())
            .filter_map(|m| m["func"].as_str().map(str::to_string))
            .collect()
    }

    #[test]
    fn une_seance_youtube_ouvre_la_vue_a_la_bonne_position_et_la_pilote_par_messages() {
        let liens = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut p = PiloteSeance::nouveau(
            GenreSeance::Regarder,
            "hote",
            fabrique_youtube(liens.clone()),
        );
        let t = base() + 10_000;
        p.recevoir(Some(&seance(etat("youtube", "dQw4w9WgXcQ"))), t);
        assert!(p.vue(t).navigateur.is_none());
        let lien = liens.lock().unwrap()[0].clone();
        let page = lien.page_a_ouvrir().expect("vue demandée");
        assert_eq!(
            (page.video.as_str(), page.depart.round(), page.lecture),
            ("dQw4w9WgXcQ", 70.0, true)
        );
        lien.recevoir(SOURCE_VUE, PAGE_VUE, r#"{"estompe":"charge"}"#);
        lien.recevoir(SOURCE_VUE, PAGE_VUE, r#"{"event":"onReady"}"#);
        lien.recevoir(
            SOURCE_VUE,
            PAGE_VUE,
            r#"{"event":"infoDelivery","info":{"currentTime":70,"duration":212,"playerState":1}}"#,
        );
        p.lecteur_change(t);
        assert_eq!(p.vue(t).duree, 212.0);
        assert!(!p.vue(t).chargement);
        p.actions();
        lien.sortants();
        p.commande(Commande::pause(70.5), t + 500).unwrap();
        assert_eq!(commandes_youtube(&lien), vec!["pauseVideo"]);
        assert!(p.actions().iter().any(|a| matches!(
            a,
            ActionPilote::Appel(AppelSeance::Action { action, .. }) if action == "pause"
        )));
        let mut suivant = etat("youtube", "aaaaaaaaaaa");
        suivant.rev = 2;
        suivant.playing = false;
        p.recevoir(Some(&seance(suivant)), t + 1000);
        assert_eq!(liens.lock().unwrap().len(), 1);
        assert_eq!(lien.page_a_ouvrir(), None);
        assert!(commandes_youtube(&lien).contains(&"cueVideoById".to_string()));
        p.recevoir(Some(&seance(etat("file", VIDEO))), t + 2000);
        assert!(lien.ferme());
    }

    #[test]
    fn sans_runtime_la_seance_youtube_passe_au_navigateur_avec_un_message() {
        let liens = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut p = PiloteSeance::nouveau(
            GenreSeance::Regarder,
            "moi",
            fabrique_youtube(liens.clone()),
        );
        p.recevoir(Some(&seance(etat("youtube", "dQw4w9WgXcQ"))), base());
        let lien = liens.lock().unwrap()[0].clone();
        assert!(lien.page_a_ouvrir().is_some());
        p.actions();
        lien.signaler_indisponible("WebView2 absent");
        p.lecteur_change(base() + 300);
        let v = p.vue(base() + 300);
        assert!(p.lecteur().is_none());
        assert!(lien.ferme());
        assert_eq!(
            v.navigateur.as_deref(),
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=60s")
        );
        let actions = p.actions();
        assert!(actions.contains(&ActionPilote::Info(RUNTIME_ABSENT.into())));
        assert!(actions
            .iter()
            .any(|a| matches!(a, ActionPilote::Journal(j) if j.contains("WebView2 absent"))));
        let mut suivant = etat("youtube", "aaaaaaaaaaa");
        suivant.rev = 2;
        p.recevoir(Some(&seance(suivant)), base() + 1000);
        assert_eq!(liens.lock().unwrap().len(), 1);
        assert!(p.vue(base()).navigateur.unwrap().contains("aaaaaaaaaaa"));
        p.recharger(base() + 2000);
        assert_eq!(liens.lock().unwrap().len(), 2);
        assert!(p.lecteur().is_some());
    }
}
