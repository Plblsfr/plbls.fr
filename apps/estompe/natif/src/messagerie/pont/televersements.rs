use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use estompe_client::fichiers::{refus_de_piece, Limites};
use estompe_client::televersement::Progression;
use estompe_plateforme::ContenuPressePapiers;
use slint::ComponentHandle;

use super::super::medias::{CleImage, Format, Source};
use super::super::pieces::{preparer, Ajout};
use super::super::saisie::SourcePiece;
use super::{verrou_de, PontMessagerie};
use crate::{DonneesSaisie, Principale};

impl PontMessagerie {
    fn limites_des_pieces(&self) -> Limites {
        self.store
            .lire(|etat| Limites::du_compte(etat.compte.max_video_size))
    }

    pub fn pieces_possibles(&self) -> bool {
        let Some(cle) = self.conversation() else {
            return false;
        };
        self.raison_lecture_seule().is_none()
            && self
                .store
                .lire(|etat| etat.peut_dans(Some(&cle), "attach_files"))
    }

    fn afficher_pieces_plus_tard(&self) {
        let saisie = self.saisie.clone();
        let _ = self
            .fenetre
            .upgrade_in_event_loop(move |f| saisie.afficher_pieces(&f));
    }

    pub fn joindre(&self, f: &Principale, ajouts: Vec<Ajout>) {
        if ajouts.is_empty() || !self.pieces_possibles() {
            return;
        }
        let limites = self.limites_des_pieces();
        for ajout in ajouts {
            let nom = ajout.nom();
            let ajoutee = self.saisie.avec_etat(|e| {
                let cle = e.nouvelle_piece();
                e.liste_courante()
                    .map(|l| l.ajouter(cle, &nom, &limites).map(|()| cle))
            });
            match ajoutee {
                Some(Ok(cle)) => self.preparer_piece(cle, ajout, limites),
                Some(Err(refus)) => {
                    self.saisie.signaler_erreur(f, &refus);
                    break;
                }
                None => break,
            }
        }
        self.saisie.afficher_pieces(f);
    }

    fn preparer_piece(&self, cle: u64, ajout: Ajout, limites: Limites) {
        let pont = self.clone();
        let tache = self.runtime.spawn(async move {
            let preparee = tokio::task::spawn_blocking(move || preparer(ajout))
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r);
            let acceptee = preparee.and_then(|p| {
                match refus_de_piece(&p.source.nom, p.taille, &p.source.type_mime, &limites) {
                    Some(refus) => Err(refus),
                    None => Ok(p),
                }
            });
            match acceptee {
                Ok(p) => {
                    if let Some(vignette) = p.vignette {
                        pont.saisie.definir_vignette(cle, vignette);
                    }
                    pont.saisie.definir_source(cle, p.source.clone());
                    pont.televerser(cle, p.source).await;
                }
                Err(refus) => {
                    pont.saisie
                        .avec_etat(|e| e.liste_de(cle).map(|l| l.retirer(cle)));
                    let saisie = pont.saisie.clone();
                    let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                        saisie.signaler_erreur(&f, &refus);
                        saisie.afficher_pieces(&f);
                    });
                }
            }
        });
        self.saisie.noter_televersement(cle, tache);
    }

    async fn televerser(&self, cle: u64, source: SourcePiece) {
        let dernier = Arc::new(AtomicU64::new(u64::MAX));
        let suivi = self.clone();
        let progres: Progression = Arc::new(move |envoyes, total| {
            let pourcentage = (envoyes * 100).checked_div(total).unwrap_or(100);
            if dernier.swap(pourcentage, Ordering::SeqCst) == pourcentage {
                return;
            }
            suivi
                .saisie
                .avec_etat(|e| e.liste_de(cle).map(|l| l.progression(cle, envoyes, total)));
            suivi.afficher_pieces_plus_tard();
        });
        let client = self.store.client().clone();
        let resultat = client
            .envoyer_piece(&source.nom, &source.type_mime, source.source, progres)
            .await
            .map_err(|e| e.message().to_string());
        self.saisie
            .avec_etat(|e| e.liste_de(cle).map(|l| l.terminer(cle, resultat)));
        self.afficher_pieces_plus_tard();
    }

    pub fn retirer_piece(&self, f: &Principale, cle: i32) {
        let cle = cle.max(0) as u64;
        self.saisie
            .avec_etat(|e| e.liste_de(cle).map(|l| l.retirer(cle)));
        self.saisie.afficher_pieces(f);
    }

    pub fn reessayer_piece(&self, f: &Principale, cle: i32) {
        let cle = cle.max(0) as u64;
        let relancee = self
            .saisie
            .avec_etat(|e| e.liste_de(cle).is_some_and(|l| l.reessayer(cle)));
        if let (true, Some(source)) = (relancee, self.saisie.source(cle)) {
            let pont = self.clone();
            let tache = self
                .runtime
                .spawn(async move { pont.televerser(cle, source).await });
            self.saisie.noter_televersement(cle, tache);
        }
        self.saisie.afficher_pieces(f);
    }

    pub fn coller(&self, f: &Principale) -> bool {
        let presse = verrou_de(&self.presse_papiers);
        if !presse.disponible() || !self.pieces_possibles() {
            return false;
        }
        let ajouts = match presse.lire_pieces() {
            Ok(Some(ContenuPressePapiers::Image(image))) => vec![Ajout::Image(image)],
            Ok(Some(ContenuPressePapiers::Fichiers(chemins))) => {
                chemins.into_iter().map(Ajout::Chemin).collect()
            }
            _ => return false,
        };
        self.joindre(f, ajouts);
        true
    }

    pub fn choisir_pieces(&self) {
        let fichiers = verrou_de(&self.fichiers);
        if !fichiers.disponible() || !self.pieces_possibles() {
            return;
        }
        let pont = self.clone();
        let _ = std::thread::Builder::new()
            .name("estompe-joindre".into())
            .spawn(move || {
                let chemins = fichiers.choisir_fichiers(None).unwrap_or_default();
                if chemins.is_empty() {
                    return;
                }
                let p = pont.clone();
                let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                    p.joindre(&f, chemins.into_iter().map(Ajout::Chemin).collect());
                    p.saisie.demander_focus(&f);
                });
            });
    }

    pub fn survol_depot(&self, f: &Principale, survol: bool) {
        f.global::<DonneesSaisie>()
            .set_depot_survole(survol && self.pieces_possibles());
    }

    pub fn deposer(&self, f: &Principale, chemins: Vec<PathBuf>) {
        f.global::<DonneesSaisie>().set_depot_survole(false);
        self.joindre(f, chemins.into_iter().map(Ajout::Chemin).collect());
    }

    pub(super) fn vignettes_d_edition(&self, f: &Principale) {
        let pieces = self.saisie.avec_etat(|e| {
            e.edition
                .pieces
                .iter()
                .filter_map(|p| Some((p.cle, p.piece_jointe()?.id.clone()?)))
                .collect::<Vec<_>>()
        });
        let medias = self.medias();
        for (cle, id) in pieces {
            let image = medias.image(&CleImage {
                source: Source::Fichier(id),
                format: Format::Vignette,
            });
            if let Some(image) = image {
                self.saisie.definir_vignette(cle, image.premiere().clone());
            }
        }
        self.saisie.afficher_pieces(f);
    }
}
