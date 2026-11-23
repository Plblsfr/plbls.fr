use estompe_client::parametres::{avec_type_de_notification, types_de_notification};
use slint::ComponentHandle;

use super::Pont;
use crate::{DonneesParametres, Principale};

pub const ERREUR_NOTIFICATIONS: &str = "Impossible d'enregistrer les réglages.";

impl Pont {
    pub fn projeter_parametres(&self, f: &Principale) {
        let [prive, mention, salon, vocal] = self
            .store
            .lire(|e| types_de_notification(&e.reglages_notification));
        let g = f.global::<DonneesParametres>();
        g.set_type_prive(prive);
        g.set_type_mention(mention);
        g.set_type_salon(salon);
        g.set_type_vocal(vocal);
    }

    pub fn definir_type_notification(&self, f: &Principale, genre: &str, actif: bool) {
        let Some(reglages) = self
            .store
            .lire(|e| avec_type_de_notification(&e.reglages_notification, genre, actif))
        else {
            return;
        };
        f.global::<DonneesParametres>()
            .set_erreur_notifications("".into());
        let store = self.store.clone();
        let pont = self.clone();
        let fenetre = self.fenetre.clone();
        self.runtime.spawn(async move {
            let echec = store
                .enregistrer_reglages_notification(reglages)
                .await
                .is_err();
            let _ = fenetre.upgrade_in_event_loop(move |f| {
                if echec {
                    f.global::<DonneesParametres>()
                        .set_erreur_notifications(ERREUR_NOTIFICATIONS.into());
                }
                pont.projeter_parametres(&f);
            });
        });
    }

    pub fn definir_apercus(&self, f: &Principale, apercus: bool) {
        if let Some(memoire) = &self.memoire {
            memoire.modifier(|p| p.apercus = apercus);
        }
        f.global::<DonneesParametres>().set_apercus(apercus);
        self.messagerie.definir_apercus(apercus);
        self.messagerie.rafraichir_maintenant(f);
    }
}
