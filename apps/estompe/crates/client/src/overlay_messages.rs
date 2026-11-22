use std::time::Duration;

use estompe_plateforme::touche;
use serde::{Deserialize, Serialize};

use crate::overlay::{CoinOverlay, TailleLogique};
use crate::store::CleConversation;
use crate::temps_reel::evenements::GenreNotification;

pub const TOASTS_MAX: usize = 4;
pub const LARGEUR: u32 = 340;
pub const LIGNE: u32 = 64;
pub const ECART: u32 = 8;
pub const REPONSE: u32 = 44;
pub const DUREE_MIN: u8 = 2;
pub const DUREE_MAX: u8 = 60;
pub const DUREE_DEFAUT: u8 = 8;
pub const LONGUEUR_MAX: usize = 2000;
pub const TOUCHE_REPONSE: &str = "F8";
pub const TOUCHE_OUVRIR: &str = "F7";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenresMessages {
    #[default]
    #[serde(rename = "both")]
    Tous,
    #[serde(rename = "dm")]
    Prives,
    #[serde(rename = "mention")]
    Mentions,
    #[serde(rename = "off")]
    Aucun,
}

impl GenresMessages {
    pub const TOUS: [GenresMessages; 4] = [Self::Tous, Self::Prives, Self::Mentions, Self::Aucun];

    pub fn libelle(self) -> &'static str {
        match self {
            Self::Tous => "Messages privés et mentions",
            Self::Prives => "Messages privés",
            Self::Mentions => "Mentions",
            Self::Aucun => "Rien",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReglagesMessages {
    pub genres: GenresMessages,
    pub en_jeu: bool,
    pub coin: CoinOverlay,
    pub duree: u8,
    pub masquer_contenu: bool,
    pub sourdine_ne_pas_deranger: bool,
    pub touche_reponse: String,
    pub touche_ouvrir: String,
}

impl Default for ReglagesMessages {
    fn default() -> Self {
        Self {
            genres: GenresMessages::default(),
            en_jeu: true,
            coin: CoinOverlay::HautDroite,
            duree: DUREE_DEFAUT,
            masquer_contenu: false,
            sourdine_ne_pas_deranger: true,
            touche_reponse: TOUCHE_REPONSE.to_string(),
            touche_ouvrir: TOUCHE_OUVRIR.to_string(),
        }
    }
}

impl ReglagesMessages {
    pub fn borne(mut self) -> Self {
        self.duree = self.duree.clamp(DUREE_MIN, DUREE_MAX);
        if touche(&self.touche_reponse).is_none() {
            self.touche_reponse = TOUCHE_REPONSE.to_string();
        }
        if touche(&self.touche_ouvrir).is_none() || self.touche_ouvrir == self.touche_reponse {
            self.touche_ouvrir = if self.touche_reponse == TOUCHE_OUVRIR {
                TOUCHE_REPONSE.to_string()
            } else {
                TOUCHE_OUVRIR.to_string()
            };
        }
        self
    }

    pub fn actif(&self) -> bool {
        self.genres != GenresMessages::Aucun
    }

    pub fn duree(&self) -> Duration {
        Duration::from_secs(u64::from(self.duree.clamp(DUREE_MIN, DUREE_MAX)))
    }
}

pub fn voulu(
    genre: Option<GenreNotification>,
    a_un_message: bool,
    reglages: &ReglagesMessages,
    ne_pas_deranger: bool,
) -> bool {
    if !a_un_message || (ne_pas_deranger && reglages.sourdine_ne_pas_deranger) {
        return false;
    }
    match (genre, reglages.genres) {
        (_, GenresMessages::Aucun) => false,
        (Some(GenreNotification::MessagePrive), g) => g != GenresMessages::Mentions,
        (Some(GenreNotification::Mention), g) => g != GenresMessages::Prives,
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastMessage {
    pub id: String,
    pub conversation: CleConversation,
    pub cible: String,
    pub titre: String,
    pub corps: String,
    pub auteur: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reponse {
    pub conversation: CleConversation,
    pub texte: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileMessages {
    toasts: Vec<(ToastMessage, Duration)>,
    saisie: Option<String>,
}

impl FileMessages {
    pub fn toasts(&self) -> impl Iterator<Item = &ToastMessage> {
        self.toasts.iter().map(|(t, _)| t)
    }

    pub fn len(&self) -> usize {
        self.toasts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.toasts.is_empty()
    }

    pub fn saisie(&self) -> Option<&str> {
        self.saisie.as_deref()
    }

    pub fn en_saisie(&self) -> bool {
        self.saisie.is_some()
    }

    pub fn pousser(&mut self, toast: ToastMessage, maintenant: Duration, duree: Duration) {
        self.toasts.retain(|(t, _)| t.id != toast.id);
        self.toasts.push((toast, maintenant + duree));
        let surplus = self.toasts.len().saturating_sub(TOASTS_MAX);
        self.toasts.drain(..surplus);
    }

    pub fn prochaine_echeance(&self) -> Option<Duration> {
        self.toasts.iter().map(|(_, e)| *e).min()
    }

    pub fn expirer(&mut self, maintenant: Duration, duree: Duration) -> bool {
        if self.saisie.is_some() {
            for (_, echeance) in self.toasts.iter_mut().filter(|(_, e)| *e <= maintenant) {
                *echeance = maintenant + duree;
            }
            return false;
        }
        let avant = self.toasts.len();
        self.toasts.retain(|(_, e)| *e > maintenant);
        self.toasts.len() != avant
    }

    pub fn commencer_reponse(&mut self) -> bool {
        if self.toasts.is_empty() || self.saisie.is_some() {
            return false;
        }
        self.saisie = Some(String::new());
        true
    }

    pub fn annuler(&mut self) -> bool {
        self.saisie.take().is_some()
    }

    pub fn taper(&mut self, texte: &str) -> bool {
        let Some(saisie) = self.saisie.as_mut() else {
            return false;
        };
        let place = LONGUEUR_MAX.saturating_sub(saisie.chars().count());
        let ajout: String = texte
            .chars()
            .filter(|c| !c.is_control())
            .take(place)
            .collect();
        saisie.push_str(&ajout);
        !ajout.is_empty()
    }

    pub fn effacer(&mut self) -> bool {
        self.saisie.as_mut().and_then(String::pop).is_some()
    }

    pub fn valider(&mut self) -> Option<Reponse> {
        let saisie = self.saisie.take()?;
        let texte = saisie.trim();
        let (dernier, _) = self.toasts.last()?;
        if texte.is_empty() {
            return None;
        }
        let conversation = dernier.conversation.clone();
        self.toasts.retain(|(t, _)| t.conversation != conversation);
        Some(Reponse {
            conversation,
            texte: texte.to_string(),
        })
    }

    pub fn ouvrir_dernier(&mut self) -> Option<ToastMessage> {
        let (dernier, _) = self.toasts.last()?.clone();
        self.toasts
            .retain(|(t, _)| t.conversation != dernier.conversation);
        self.saisie = None;
        Some(dernier)
    }

    pub fn vider(&mut self) {
        self.toasts.clear();
        self.saisie = None;
    }
}

pub fn vue(toast: &ToastMessage, masquer_contenu: bool) -> ToastMessage {
    ToastMessage {
        corps: if masquer_contenu {
            String::new()
        } else {
            toast.corps.clone()
        },
        ..toast.clone()
    }
}

pub fn taille_fenetre(toasts: usize, en_saisie: bool) -> TailleLogique {
    TailleLogique {
        largeur: f64::from(LARGEUR),
        hauteur: (toasts as f64 * f64::from(LIGNE + ECART)
            + if en_saisie { f64::from(REPONSE) } else { 0.0 })
        .max(1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toast(id: &str, conversation: &str) -> ToastMessage {
        ToastMessage {
            id: id.into(),
            conversation: CleConversation::Prive(conversation.into()),
            cible: "dm/1".into(),
            titre: "Ana".into(),
            corps: "salut".into(),
            auteur: Some("u1".into()),
        }
    }

    const S: Duration = Duration::from_secs(1);

    fn ids(file: &FileMessages) -> Vec<String> {
        file.toasts().map(|t| t.id.clone()).collect()
    }

    #[test]
    fn retient_les_prives_les_mentions_ou_les_deux_comme_l_app_windows() {
        let r = |genres| ReglagesMessages {
            genres,
            ..ReglagesMessages::default()
        };
        let dm = Some(GenreNotification::MessagePrive);
        let mention = Some(GenreNotification::Mention);
        assert!(voulu(dm, true, &r(GenresMessages::Tous), false));
        assert!(!voulu(mention, true, &r(GenresMessages::Prives), false));
        assert!(!voulu(dm, true, &r(GenresMessages::Mentions), false));
        assert!(voulu(mention, true, &r(GenresMessages::Mentions), false));
        assert!(!voulu(
            Some(GenreNotification::Salon),
            true,
            &r(GenresMessages::Tous),
            false
        ));
        assert!(!voulu(
            Some(GenreNotification::Vocal),
            false,
            &r(GenresMessages::Tous),
            false
        ));
        assert!(!voulu(dm, true, &r(GenresMessages::Aucun), false));
        assert!(!voulu(None, true, &r(GenresMessages::Tous), false));
    }

    #[test]
    fn en_ne_pas_deranger_rien_ne_s_affiche_sauf_si_on_le_veut() {
        let mut r = ReglagesMessages::default();
        let dm = Some(GenreNotification::MessagePrive);
        assert!(!voulu(dm, true, &r, true));
        r.sourdine_ne_pas_deranger = false;
        assert!(voulu(dm, true, &r, true));
    }

    #[test]
    fn la_file_garde_les_quatre_plus_recents_et_remplace_le_meme_message() {
        let mut f = FileMessages::default();
        for id in ["1", "2", "3", "4", "5"] {
            f.pousser(toast(id, "d1"), Duration::ZERO, 8 * S);
        }
        assert_eq!(ids(&f), ["2", "3", "4", "5"]);
        f.pousser(toast("3", "d1"), Duration::ZERO, 8 * S);
        assert_eq!(ids(&f), ["2", "4", "5", "3"]);
    }

    #[test]
    fn chaque_bandeau_expire_apres_sa_duree() {
        let mut f = FileMessages::default();
        f.pousser(toast("1", "d1"), Duration::ZERO, 8 * S);
        f.pousser(toast("2", "d1"), 3 * S, 8 * S);
        assert_eq!(f.prochaine_echeance(), Some(8 * S));
        assert!(!f.expirer(7 * S, 8 * S));
        assert!(f.expirer(8 * S, 8 * S));
        assert_eq!(ids(&f), ["2"]);
        assert_eq!(f.prochaine_echeance(), Some(11 * S));
        assert!(f.expirer(11 * S, 8 * S));
        assert!(f.is_empty());
        assert_eq!(f.prochaine_echeance(), None);
    }

    #[test]
    fn la_saisie_en_cours_garde_les_bandeaux_et_relance_leur_duree() {
        let mut f = FileMessages::default();
        f.pousser(toast("1", "d1"), Duration::ZERO, 8 * S);
        assert!(f.commencer_reponse());
        assert!(!f.commencer_reponse());
        assert!(!f.expirer(9 * S, 8 * S));
        assert_eq!(ids(&f), ["1"]);
        assert_eq!(f.prochaine_echeance(), Some(17 * S));
        assert!(f.annuler());
        assert!(f.expirer(17 * S, 8 * S));
        assert!(f.is_empty());
        assert!(!f.commencer_reponse(), "rien à quoi répondre");
    }

    #[test]
    fn la_reponse_part_vers_le_dernier_message_et_retire_sa_conversation() {
        let mut f = FileMessages::default();
        for (id, conv) in [("1", "d1"), ("2", "d2"), ("3", "d1")] {
            f.pousser(toast(id, conv), Duration::ZERO, 8 * S);
        }
        f.commencer_reponse();
        assert!(f.taper("  o"));
        assert!(f.taper("k\u{7}  "));
        assert_eq!(f.saisie(), Some("  ok  "));
        assert!(f.effacer());
        assert_eq!(
            f.valider(),
            Some(Reponse {
                conversation: CleConversation::Prive("d1".into()),
                texte: "ok".into()
            })
        );
        assert_eq!(ids(&f), ["2"]);
        assert!(!f.en_saisie());
    }

    #[test]
    fn une_reponse_vide_ne_part_pas_et_ferme_la_saisie() {
        let mut f = FileMessages::default();
        f.pousser(toast("1", "d1"), Duration::ZERO, 8 * S);
        f.commencer_reponse();
        f.taper("   ");
        assert_eq!(f.valider(), None);
        assert_eq!(f.len(), 1);
        assert!(!f.en_saisie());
        assert!(!f.taper("x"), "hors saisie, la frappe est ignorée");
        assert_eq!(FileMessages::default().valider(), None);
    }

    #[test]
    fn la_saisie_est_limitee_a_deux_mille_caracteres() {
        let mut f = FileMessages::default();
        f.pousser(toast("1", "d1"), Duration::ZERO, 8 * S);
        f.commencer_reponse();
        f.taper(&"é".repeat(LONGUEUR_MAX - 1));
        assert!(f.taper("ab"));
        assert_eq!(f.saisie().unwrap().chars().count(), LONGUEUR_MAX);
        assert!(!f.taper("c"));
    }

    #[test]
    fn ouvrir_prend_le_dernier_et_retire_sa_conversation() {
        let mut f = FileMessages::default();
        for (id, conv) in [("1", "d1"), ("2", "d2"), ("3", "d2")] {
            f.pousser(toast(id, conv), Duration::ZERO, 8 * S);
        }
        assert_eq!(f.ouvrir_dernier().map(|t| t.id), Some("3".into()));
        assert_eq!(ids(&f), ["1"]);
        f.vider();
        assert_eq!(f.ouvrir_dernier(), None);
    }

    #[test]
    fn masquer_le_contenu_ne_garde_que_l_expediteur() {
        assert_eq!(vue(&toast("1", "d1"), true).corps, "");
        assert_eq!(vue(&toast("1", "d1"), false).corps, "salut");
        assert_eq!(vue(&toast("1", "d1"), true).titre, "Ana");
    }

    #[test]
    fn la_fenetre_grandit_avec_les_bandeaux_et_le_champ_de_reponse() {
        assert_eq!(
            taille_fenetre(2, false),
            TailleLogique {
                largeur: 340.0,
                hauteur: 144.0
            }
        );
        assert_eq!(
            taille_fenetre(2, true).hauteur,
            taille_fenetre(2, false).hauteur + f64::from(REPONSE)
        );
        assert_eq!(taille_fenetre(0, false).hauteur, 1.0);
    }

    #[test]
    fn les_reglages_sont_bornes_et_se_relisent() {
        let r = ReglagesMessages {
            duree: 0,
            touche_reponse: "Inconnue".into(),
            touche_ouvrir: TOUCHE_REPONSE.into(),
            ..ReglagesMessages::default()
        }
        .borne();
        assert_eq!(r.duree, DUREE_MIN);
        assert_eq!(r.touche_reponse, TOUCHE_REPONSE);
        assert_eq!(r.touche_ouvrir, TOUCHE_OUVRIR);
        let r = ReglagesMessages {
            duree: 200,
            touche_reponse: TOUCHE_OUVRIR.into(),
            touche_ouvrir: TOUCHE_OUVRIR.into(),
            ..ReglagesMessages::default()
        }
        .borne();
        assert_eq!(r.duree, DUREE_MAX);
        assert_eq!(
            (r.touche_reponse.as_str(), r.touche_ouvrir.as_str()),
            (TOUCHE_OUVRIR, TOUCHE_REPONSE)
        );
        assert_eq!(r.duree(), Duration::from_secs(60));
        let lu: ReglagesMessages = serde_json::from_str(
            r#"{"genres":"mention","en_jeu":false,"coin":"bl","duree":12,"masquer_contenu":true}"#,
        )
        .unwrap();
        assert_eq!(lu.genres, GenresMessages::Mentions);
        assert_eq!(lu.coin, CoinOverlay::BasGauche);
        assert!(lu.masquer_contenu && !lu.en_jeu && lu.sourdine_ne_pas_deranger);
        assert_eq!(
            serde_json::from_str::<ReglagesMessages>("{}").unwrap(),
            ReglagesMessages::default()
        );
        assert!(!ReglagesMessages {
            genres: GenresMessages::Aucun,
            ..ReglagesMessages::default()
        }
        .actif());
    }
}
