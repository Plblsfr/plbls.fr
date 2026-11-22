use std::rc::Rc;
use std::sync::Arc;

use estompe_client::jeux::{nom_executable, ReglagesJeux};
use estompe_plateforme::FiltreFichiers;
use slint::{ComponentHandle, ModelRc, VecModel};

use super::{EtatJeux, ServiceJeux};
use crate::messagerie::pont::SystemeFichiers;
use crate::session::Memoire;
use crate::{DonneesJeux, JeuAjoute, Principale};

pub const TITRE_DIALOGUE: &str = "Choisir un jeu";
pub const FILTRE_EXECUTABLES: FiltreFichiers = FiltreFichiers {
    nom: "Programmes",
    extensions: &["exe"],
};

pub fn disponible(f: &Principale) -> bool {
    f.global::<DonneesJeux>().get_disponible()
}

pub fn ouvrir(f: &Principale) {
    let g = f.global::<DonneesJeux>();
    if g.get_disponible() {
        g.invoke_ouvrir();
    }
}

fn afficher_reglages(f: &Principale, reglages: &ReglagesJeux) {
    let g = f.global::<DonneesJeux>();
    g.set_partager(reglages.partager);
    g.set_ajoutes(ModelRc::new(VecModel::from(
        reglages
            .ajoutes
            .iter()
            .map(|chemin| JeuAjoute {
                nom: nom_executable(chemin).into(),
                chemin: chemin.into(),
            })
            .collect::<Vec<_>>(),
    )));
    g.set_exclus(ModelRc::new(VecModel::from(
        reglages
            .exclus
            .iter()
            .map(|n| n.into())
            .collect::<Vec<slint::SharedString>>(),
    )));
}

pub fn afficher_etat(f: &Principale, etat: &EtatJeux) {
    let g = f.global::<DonneesJeux>();
    g.set_disponible(etat.disponible);
    g.set_jeu_en_cours(etat.activite.as_str().into());
    g.set_derniere_application(
        etat.derniere_application
            .as_ref()
            .map(|a| a.executable.as_str())
            .unwrap_or_default()
            .into(),
    );
}

struct Reglages {
    service: Arc<ServiceJeux>,
    memoire: Memoire,
}

impl Reglages {
    fn modifier(&self, f: &Principale, modification: impl FnOnce(&mut ReglagesJeux)) {
        self.memoire.modifier(|p| modification(&mut p.jeux));
        afficher_reglages(f, &self.memoire.lire(|p| p.jeux.clone()));
        self.service.relancer();
    }
}

pub fn brancher(
    f: &Principale,
    service: Arc<ServiceJeux>,
    memoire: Memoire,
    fichiers: SystemeFichiers,
    runtime: &tokio::runtime::Handle,
) {
    afficher_etat(f, &service.actuel());
    afficher_reglages(f, &memoire.lire(|p| p.jeux.clone()));
    let mut etat = service.abonner();
    let faible = f.as_weak();
    runtime.spawn(async move {
        while etat.changed().await.is_ok() {
            let e = etat.borrow_and_update().clone();
            if faible
                .upgrade_in_event_loop(move |f| afficher_etat(&f, &e))
                .is_err()
            {
                break;
            }
        }
    });
    let r = Rc::new(Reglages { service, memoire });
    let g = f.global::<DonneesJeux>();
    macro_rules! rappel {
        ($methode:ident, |$r:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let $r = r.clone();
            let faible = f.as_weak();
            g.$methode(move |$($arg),*| {
                if let Some($f) = faible.upgrade() {
                    $corps;
                }
            });
        }};
    }
    rappel!(on_ouvrir, |r, f| {
        afficher_reglages(&f, &r.memoire.lire(|p| p.jeux.clone()));
        afficher_etat(&f, &r.service.actuel());
        r.service.relancer();
        f.global::<DonneesJeux>().set_ouvert(true);
    });
    rappel!(on_fermer, |_r, f| f
        .global::<DonneesJeux>()
        .set_ouvert(false));
    rappel!(on_definir_partage, |r, f, partager| r
        .modifier(&f, |j| j.partager = partager));
    rappel!(on_ajouter_derniere, |r, f| {
        if let Some(a) = r.service.actuel().derniere_application {
            r.modifier(&f, |j| {
                j.retirer_exclu(&a.executable);
                j.ajouter(&a.chemin);
            });
        }
    });
    rappel!(on_exclure_derniere, |r, f| {
        if let Some(a) = r.service.actuel().derniere_application {
            r.modifier(&f, |j| {
                j.exclure(&a.executable);
            });
        }
    });
    rappel!(on_exclure, |r, f, nom| r.modifier(&f, |j| {
        j.exclure(&nom);
    }));
    rappel!(on_retirer_ajoute, |r, f, chemin| r.modifier(&f, |j| {
        j.retirer_ajoute(&chemin);
    }));
    rappel!(on_retirer_exclu, |r, f, nom| r.modifier(&f, |j| {
        j.retirer_exclu(&nom);
    }));
    for ajout in [true, false] {
        let fichiers = fichiers.clone();
        let faible = f.as_weak();
        let r = r.clone();
        let parcourir = move || {
            if !fichiers.disponible() {
                return;
            }
            let fichiers = fichiers.clone();
            let faible = faible.clone();
            let (service, memoire) = (r.service.clone(), r.memoire.clone());
            let _ = std::thread::Builder::new()
                .name("estompe-jeux".into())
                .spawn(move || {
                    let Ok(Some(chemin)) =
                        fichiers.choisir_fichier(TITRE_DIALOGUE, &FILTRE_EXECUTABLES, None)
                    else {
                        return;
                    };
                    let chemin = chemin.to_string_lossy().into_owned();
                    let _ = faible.upgrade_in_event_loop(move |f| {
                        Reglages { service, memoire }.modifier(&f, |j| {
                            if ajout {
                                j.ajouter(&chemin);
                            } else {
                                j.exclure(&chemin);
                            }
                        });
                    });
                });
        };
        if ajout {
            g.on_parcourir_ajout(parcourir);
        } else {
            g.on_parcourir_exclu(parcourir);
        }
    }
}
