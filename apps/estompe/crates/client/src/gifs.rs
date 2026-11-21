use std::collections::HashSet;
use std::ops::Range;
use std::time::Duration;

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::http::{encoder_composant, Client, Corps, ErreurApi, OptionsAppel};
use crate::types::{Attachment, PostGifsImportRequete};

pub const DECALAGE_MAX: usize = 480;
pub const LONGUEUR_MAX: usize = 100;
pub const TEMPORISATION: Duration = Duration::from_millis(350);
pub const MARGE_DE_SUITE: f32 = 160.0;
pub const IDEES: [&str; 8] = [
    "lol", "ok", "merci", "bravo", "triste", "wtf", "bonjour", "fête",
];
pub const TENDANCES: &str = "Tendances";
pub const ATTRIBUTION: &str = "Propulsé par GIPHY · les aperçus sont chargés depuis giphy.com";
pub const AUCUN_GIF: &str = "Aucun GIF trouvé.";
pub const NON_ACTIVE: &str = "La recherche de GIF n'est pas activée sur ce serveur.";
pub const IMPOSSIBLE: &str = "Recherche de GIF impossible.";
pub const TROP_DE_RECHERCHES: &str =
    "Trop de recherches de GIF en peu de temps : réessaie dans une minute.";
pub const ECART: f32 = 6.0;
pub const PAR_RANGEE: usize = 2;
pub const HAUTEUR_MAX: f32 = 200.0;
pub const RAPPORT_MIN: f32 = 0.4;
pub const RAPPORT_MAX: f32 = 2.5;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gif {
    pub id: String,
    pub title: String,
    pub preview: String,
    pub url: String,
    pub width: u32,
    pub height: u32,
}

impl Gif {
    pub fn rapport(&self) -> f32 {
        if self.width == 0 || self.height == 0 {
            return 1.0;
        }
        (self.width as f32 / self.height as f32).clamp(RAPPORT_MIN, RAPPORT_MAX)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemandeGifs {
    pub sequence: u64,
    pub terme: String,
    pub decalage: usize,
}

impl DemandeGifs {
    pub fn chemin(&self) -> String {
        if self.terme.is_empty() {
            format!("/gifs/trending?offset={}", self.decalage)
        } else {
            format!(
                "/gifs/search?q={}&offset={}",
                encoder_composant(&self.terme),
                self.decalage
            )
        }
    }
}

pub fn message_d_erreur(erreur: &ErreurApi) -> &'static str {
    match erreur.statut() {
        501 => NON_ACTIVE,
        429 => TROP_DE_RECHERCHES,
        _ => IMPOSSIBLE,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RechercheGifs {
    pub terme: String,
    pub resultats: Vec<Gif>,
    pub en_cours: bool,
    pub encore: bool,
    pub message: String,
    sequence: u64,
}

impl RechercheGifs {
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn lancer(&mut self, terme: &str) -> DemandeGifs {
        self.terme = terme.trim().chars().take(LONGUEUR_MAX).collect();
        self.resultats.clear();
        self.encore = true;
        self.message.clear();
        self.demande(0)
    }

    fn demande(&mut self, decalage: usize) -> DemandeGifs {
        self.sequence += 1;
        self.en_cours = true;
        DemandeGifs {
            sequence: self.sequence,
            terme: self.terme.clone(),
            decalage,
        }
    }

    pub fn suite(&mut self) -> Option<DemandeGifs> {
        (self.encore && !self.en_cours && self.resultats.len() <= DECALAGE_MAX)
            .then(|| self.demande(self.resultats.len()))
    }

    pub fn recevoir(
        &mut self,
        sequence: u64,
        resultat: Result<Vec<Gif>, ErreurApi>,
    ) -> Option<usize> {
        if sequence != self.sequence {
            return None;
        }
        self.en_cours = false;
        let debut = self.resultats.len();
        match resultat {
            Ok(page) => {
                let mut connus: HashSet<String> =
                    self.resultats.iter().map(|g| g.id.clone()).collect();
                let vide = page.is_empty();
                self.resultats
                    .extend(page.into_iter().filter(|g| connus.insert(g.id.clone())));
                self.encore = !vide && self.resultats.len() <= DECALAGE_MAX;
                self.message = if self.resultats.is_empty() {
                    AUCUN_GIF.to_string()
                } else {
                    String::new()
                };
            }
            Err(erreur) => {
                self.encore = false;
                self.message = message_d_erreur(&erreur).to_string();
            }
        }
        Some(debut)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RangeeGifs {
    pub debut: usize,
    pub largeurs: Vec<f32>,
    pub hauteur: f32,
}

impl RangeeGifs {
    pub fn indices(&self) -> Range<usize> {
        self.debut..self.debut + self.largeurs.len()
    }
}

pub fn ranger(gifs: &[Gif], largeur: f32) -> Vec<RangeeGifs> {
    let utile = (largeur - ECART * (PAR_RANGEE - 1) as f32).max(1.0);
    gifs.chunks(PAR_RANGEE)
        .enumerate()
        .map(|(n, morceau)| {
            let rapports: Vec<f32> = morceau.iter().map(Gif::rapport).collect();
            let somme = rapports.iter().sum::<f32>() + (PAR_RANGEE - morceau.len()) as f32;
            let hauteur = (utile / somme).min(HAUTEUR_MAX).floor();
            RangeeGifs {
                debut: n * PAR_RANGEE,
                largeurs: rapports.iter().map(|r| (r * hauteur).floor()).collect(),
                hauteur,
            }
        })
        .collect()
}

pub fn hauteur_totale(rangees: &[RangeeGifs]) -> f32 {
    rangees.iter().map(|r| r.hauteur + ECART).sum()
}

pub fn rangees_visibles(rangees: &[RangeeGifs], haut: f32, hauteur: f32) -> Range<usize> {
    let mut y = 0.0;
    let mut debut = None;
    let mut fin = 0;
    for (n, r) in rangees.iter().enumerate() {
        let bas = y + r.hauteur;
        if bas > haut && y < haut + hauteur {
            debut.get_or_insert(n);
            fin = n + 1;
        }
        y = bas + ECART;
    }
    debut.map_or(0..0, |d| d..fin)
}

pub fn rangee_de(rangees: &[RangeeGifs], index: usize) -> Option<usize> {
    rangees.iter().position(|r| r.indices().contains(&index))
}

pub fn haut_de_rangee(rangees: &[RangeeGifs], rangee: usize) -> f32 {
    hauteur_totale(&rangees[..rangee.min(rangees.len())])
}

pub fn voisin(rangees: &[RangeeGifs], actif: Option<usize>, dx: i32, dy: i32) -> Option<usize> {
    let total = rangees.last().map_or(0, |r| r.indices().end);
    if total == 0 {
        return None;
    }
    let Some(i) = actif else {
        return (dx > 0 || dy > 0).then_some(0);
    };
    if dx != 0 {
        return Some((i as i64 + i64::from(dx)).clamp(0, total as i64 - 1) as usize);
    }
    let r = rangee_de(rangees, i)?;
    let colonne = i - rangees[r].debut;
    let cible = if dy < 0 {
        r.checked_sub(1)?
    } else if r + 1 < rangees.len() {
        r + 1
    } else {
        return Some(i);
    };
    let rangee = &rangees[cible];
    Some(rangee.debut + colonne.min(rangee.largeurs.len() - 1))
}

impl Client {
    pub async fn chercher_gifs(&self, demande: &DemandeGifs) -> Result<Vec<Gif>, ErreurApi> {
        let options = OptionsAppel {
            sans_relance: true,
            ..OptionsAppel::default()
        };
        self.requete(Method::GET, &demande.chemin(), &Corps::Aucun, options)
            .await
            .map(Option::unwrap_or_default)
    }

    pub async fn importer_gif(&self, url: &str) -> Result<Attachment, ErreurApi> {
        self.post(
            "/gifs/import",
            &PostGifsImportRequete {
                url: Some(url.to_string()),
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::serveur_factice::{Reponse, ServeurFactice};

    fn gif(id: &str, largeur: u32, hauteur: u32) -> Gif {
        Gif {
            id: id.to_string(),
            title: format!("gif {id}"),
            preview: format!("https://media1.giphy.com/{id}/p.gif"),
            url: format!("https://media1.giphy.com/{id}/g.gif"),
            width: largeur,
            height: hauteur,
        }
    }

    fn page(debut: usize, nombre: usize) -> Vec<Gif> {
        (debut..debut + nombre)
            .map(|n| gif(&n.to_string(), 200, 150))
            .collect()
    }

    fn refus(statut: u16) -> ErreurApi {
        ErreurApi::Autre(crate::http::Refus {
            statut,
            code: "x".into(),
            message: String::new(),
            donnees: Default::default(),
        })
    }

    #[test]
    fn tendances_sans_terme_et_recherche_encodee() {
        let mut r = RechercheGifs::default();
        assert_eq!(r.lancer("  ").chemin(), "/gifs/trending?offset=0");
        let d = r.lancer(" chat & chien ");
        assert_eq!(d.chemin(), "/gifs/search?q=chat%20%26%20chien&offset=0");
        let long = "a".repeat(150);
        assert_eq!(r.lancer(&long).terme.len(), LONGUEUR_MAX);
    }

    #[test]
    fn la_suite_ajoute_sans_doublon_et_ignore_une_recherche_remplacee() {
        let mut r = RechercheGifs::default();
        let ancienne = r.lancer("chat");
        let d = r.lancer("chien");
        assert_eq!(r.recevoir(ancienne.sequence, Ok(page(0, 24))), None);
        assert!(r.resultats.is_empty() && r.en_cours);
        assert_eq!(r.suite(), None, "pas de suite pendant un chargement");
        assert_eq!(r.recevoir(d.sequence, Ok(page(0, 24))), Some(0));
        let suite = r.suite().unwrap();
        assert_eq!(suite.chemin(), "/gifs/search?q=chien&offset=24");
        assert_eq!(r.recevoir(suite.sequence, Ok(page(20, 24))), Some(24));
        assert_eq!(r.resultats.len(), 44);
        let fin = r.suite().unwrap();
        r.recevoir(fin.sequence, Ok(Vec::new()));
        assert!(!r.encore);
        assert_eq!(r.suite(), None);
        assert_eq!(r.message, "");
    }

    #[test]
    fn aucun_resultat_et_borne_du_decalage() {
        let mut r = RechercheGifs::default();
        let d = r.lancer("zzz");
        r.recevoir(d.sequence, Ok(Vec::new()));
        assert_eq!(r.message, AUCUN_GIF);
        let d = r.lancer("chat");
        r.recevoir(d.sequence, Ok(page(0, DECALAGE_MAX + 1)));
        assert_eq!(
            r.suite(),
            None,
            "au-delà de 480 le serveur repartirait de 0"
        );
    }

    #[test]
    fn une_erreur_donne_un_message_clair_et_arrete_la_suite() {
        let mut r = RechercheGifs::default();
        for (statut, message) in [
            (501, NON_ACTIVE),
            (429, TROP_DE_RECHERCHES),
            (502, IMPOSSIBLE),
        ] {
            let d = r.lancer("chat");
            r.recevoir(d.sequence, Err(refus(statut)));
            assert_eq!(r.message, message);
            assert!(!r.en_cours && !r.encore);
            assert_eq!(r.suite(), None, "pas de nouvelle tentative après {statut}");
        }
        let d = r.lancer("chat");
        r.recevoir(d.sequence, Ok(page(0, 24)));
        let suite = r.suite().unwrap();
        r.recevoir(suite.sequence, Err(refus(429)));
        assert_eq!(r.resultats.len(), 24, "les résultats déjà là restent");
        assert_eq!(r.message, TROP_DE_RECHERCHES);
    }

    #[test]
    fn rangees_justifiees_visibles_et_parcourues_au_clavier() {
        let gifs = vec![
            gif("a", 200, 100),
            gif("b", 100, 100),
            gif("c", 0, 0),
            gif("d", 1000, 10),
            gif("e", 100, 200),
        ];
        let rangees = ranger(&gifs, 386.0);
        assert_eq!(rangees.len(), 3);
        assert_eq!(rangees[0].hauteur, 126.0);
        assert_eq!(rangees[0].largeurs, [252.0, 126.0]);
        assert!(rangees[1].largeurs.iter().sum::<f32>() <= 380.0);
        assert_eq!(
            rangees[1].largeurs[1],
            (RAPPORT_MAX * rangees[1].hauteur).floor()
        );
        assert_eq!(rangees[2].indices(), 4..5);
        assert!(rangees[2].hauteur <= HAUTEUR_MAX);
        let total = hauteur_totale(&rangees);
        assert_eq!(rangees_visibles(&rangees, 0.0, 100.0), 0..1);
        assert_eq!(rangees_visibles(&rangees, 130.0, 50.0), 1..2);
        assert_eq!(rangees_visibles(&rangees, 0.0, total), 0..3);
        assert_eq!(rangees_visibles(&[], 0.0, 100.0), 0..0);
        assert_eq!(haut_de_rangee(&rangees, 1), 132.0);
        assert_eq!(voisin(&rangees, None, 0, -1), None);
        assert_eq!(voisin(&rangees, None, 0, 1), Some(0));
        assert_eq!(voisin(&rangees, Some(1), 0, 1), Some(3));
        assert_eq!(voisin(&rangees, Some(3), 0, 1), Some(4));
        assert_eq!(voisin(&rangees, Some(4), 0, 1), Some(4));
        assert_eq!(voisin(&rangees, Some(4), 1, 0), Some(4));
        assert_eq!(voisin(&rangees, Some(1), 0, -1), None);
        assert_eq!(voisin(&rangees, Some(2), -1, 0), Some(1));
    }

    #[tokio::test]
    async fn la_recherche_n_est_pas_relancee_apres_un_429_et_l_import_envoie_l_url() {
        let serveur = ServeurFactice::demarrer([
            Reponse::json(429, json!({"error": "too_many_requests"}))
                .avec_entete("Retry-After", "1"),
            Reponse::json(
                200,
                json!([{"id": "1", "title": "un chat", "preview": "p", "url": "u", "width": 200, "height": 100}]),
            ),
            Reponse::json(201, json!({"id": "f1", "name": "giphy.gif", "mime": "image/gif", "size": 10})),
        ])
        .await;
        let client = serveur.client();
        let mut r = RechercheGifs::default();
        let d = r.lancer("chat");
        let erreur = client.chercher_gifs(&d).await.unwrap_err();
        assert_eq!(message_d_erreur(&erreur), TROP_DE_RECHERCHES);
        assert_eq!(serveur.requetes().len(), 1);
        let trouves = client.chercher_gifs(&d).await.unwrap();
        assert_eq!(trouves, [gif_brut()]);
        let piece = client
            .importer_gif("https://media1.giphy.com/g.gif")
            .await
            .unwrap();
        assert_eq!(piece.id.as_deref(), Some("f1"));
        let requetes = serveur.requetes();
        assert_eq!(requetes[1].ligne(), ("/gifs/search?q=chat&offset=0", "GET"));
        assert_eq!(requetes[2].ligne(), ("/gifs/import", "POST"));
        assert_eq!(
            requetes[2].json(),
            json!({"url": "https://media1.giphy.com/g.gif"})
        );
    }

    fn gif_brut() -> Gif {
        Gif {
            id: "1".into(),
            title: "un chat".into(),
            preview: "p".into(),
            url: "u".into(),
            width: 200,
            height: 100,
        }
    }
}
