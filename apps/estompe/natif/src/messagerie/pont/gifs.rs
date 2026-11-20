use estompe_client::gifs::{DemandeGifs, Gif};
use estompe_client::http::ErreurApi;
use estompe_client::store::messages::Brouillon;
use slint::ComponentHandle;

use super::super::gifs::Gifs;
use super::super::medias::{CleImage, Format, Source};
use super::super::saisie::Mode;
use super::PontMessagerie;
use crate::{DonneesSaisie, Principale};

pub const ECHEC_D_ENVOI: &str = "Le GIF n'a pas pu être envoyé";

impl PontMessagerie {
    pub fn gifs(&self) -> &Gifs {
        &self.gifs
    }

    pub fn gifs_possibles(&self) -> bool {
        self.conversation().is_some()
            && self.raison_lecture_seule().is_none()
            && self
                .store
                .lire(|etat| etat.bloc_actif("gifs") && etat.capacite("gifs.use"))
    }

    pub fn ouvrir_gifs(&self, f: &Principale) {
        if self.gifs.ouvert() {
            self.fermer_gifs(f);
            return;
        }
        if !self.gifs_possibles() {
            return;
        }
        if self.selecteur.ouvert() {
            self.selecteur.fermer(f);
            f.global::<DonneesSaisie>().set_selecteur_ouvert(false);
        }
        self.fermer_propositions(f);
        let demande = self.gifs.ouvrir(f);
        f.global::<DonneesSaisie>().set_gifs_ouvert(true);
        self.demander_gifs(demande);
    }

    pub fn fermer_gifs(&self, f: &Principale) {
        self.gifs.fermer(f);
        f.global::<DonneesSaisie>().set_gifs_ouvert(false);
        self.saisie.demander_focus(f);
    }

    pub fn chercher_gifs(&self, texte: &str) {
        let pont = self.clone();
        let texte = texte.to_string();
        self.gifs.temporiser(move || {
            if let Some(f) = pont.fenetre.upgrade() {
                let demande = pont.gifs.lancer(&f, &texte);
                pont.demander_gifs(demande);
            }
        });
    }

    pub fn idee_de_gif(&self, f: &Principale, libelle: &str) {
        let demande = self.gifs.idee(f, libelle);
        self.demander_gifs(demande);
    }

    pub fn defile_gifs(&self, f: &Principale, haut: f32, hauteur: f32, largeur: f32) {
        if let Some(demande) = self.gifs.defile(f, haut, hauteur, largeur) {
            self.demander_gifs(demande);
        }
    }

    fn demander_gifs(&self, demande: DemandeGifs) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            let resultat = pont.store.client().chercher_gifs(&demande).await;
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.recevoir_gifs(&f, demande.sequence, resultat));
        });
    }

    pub fn recevoir_gifs(
        &self,
        f: &Principale,
        sequence: u64,
        resultat: Result<Vec<Gif>, ErreurApi>,
    ) {
        let Some(nouveaux) = self.gifs.recevoir(f, sequence, resultat) else {
            return;
        };
        let medias = self.medias();
        for gif in nouveaux {
            let cle = CleImage {
                source: Source::Externe(gif.preview.clone()),
                format: Format::Gif,
            };
            if let Some(image) = medias.image(&cle) {
                self.gifs.installer_image(f, &gif.id, image);
                continue;
            }
            let medias = medias.clone();
            let gifs = self.gifs.clone();
            let faible = self.fenetre.clone();
            self.runtime.spawn(async move {
                if let Some(image) = medias.charger_image(cle).await {
                    let _ = faible
                        .upgrade_in_event_loop(move |f| gifs.installer_image(&f, &gif.id, image));
                }
            });
        }
        if let Some(demande) = self.gifs.suite_si_besoin() {
            self.demander_gifs(demande);
        }
    }

    pub fn choisir_gif(&self, f: &Principale, index: Option<usize>) {
        let Some(cle) = self.conversation() else {
            return;
        };
        let Some(gif) = self.gifs.choisir(index) else {
            return;
        };
        self.fermer_gifs(f);
        let reponse_a = match self.saisie.mode() {
            Mode::Reponse { id, .. } => Some(id),
            _ => None,
        };
        {
            let mut i = self.verrou();
            i.nouveau_depuis = None;
            i.defilement.aller_en_bas();
        }
        let pont = self.clone();
        self.runtime.spawn(async move {
            let client = pont.store.client().clone();
            match client.importer_gif(&gif.url).await {
                Ok(piece) => {
                    let brouillon = Brouillon {
                        pieces: vec![piece],
                        reponse_a: reponse_a.clone(),
                        ..Brouillon::default()
                    };
                    let p = pont.clone();
                    let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                        let cite = matches!(p.saisie.mode(), Mode::Reponse { id, .. } if Some(&id) == reponse_a.as_ref());
                        if cite {
                            p.saisie.annuler(&f);
                        }
                    });
                    pont.store.envoyer(&cle, brouillon).await;
                }
                Err(erreur) => {
                    let saisie = pont.saisie.clone();
                    let texte = format!("{ECHEC_D_ENVOI} : {}", erreur.message());
                    let _ = pont
                        .fenetre
                        .upgrade_in_event_loop(move |f| saisie.signaler_erreur(&f, &texte));
                }
            }
        });
    }
}
