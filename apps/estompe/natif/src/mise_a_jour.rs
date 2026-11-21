use std::sync::Arc;

use estompe_client::journal::Journal;
use estompe_client::mise_a_jour::{
    self as maj, Bloqueur, Contexte, EmetteurMaj, EvenementMaj, MiseAJour, OptionsMiseAJour,
    PolitiqueAdresses, ReceptionMaj, Redemarreur, SourceContexte, SourceMaj, SourceReseau,
    Telechargeur,
};
use estompe_plateforme::Installation;
use slint::ComponentHandle;
use tokio::sync::oneshot;

use crate::entree::{Emetteur, Evenement};
use crate::{DonneesSaisie, Principale};

pub fn contexte_de_la_fenetre(fenetre: slint::Weak<Principale>) -> SourceContexte {
    Arc::new(move || {
        let fenetre = fenetre.clone();
        Box::pin(async move {
            let (envoi, reception) = oneshot::channel();
            let _ = fenetre.upgrade_in_event_loop(move |f| {
                let _ = envoi.send(Contexte {
                    saisie_en_cours: !f.global::<DonneesSaisie>().get_texte().trim().is_empty(),
                    en_appel: false,
                });
            });
            reception.await.unwrap_or_default()
        })
    })
}

pub fn redemarrage() -> Redemarreur {
    Arc::new(|| crate::bureau::sur_le_bureau(|bureau| bureau.fermeture().quitter()))
}

pub fn blocage_par_l_entree(entree: Emetteur) -> Bloqueur {
    Arc::new(move |message: &str, version_minimale: &str| {
        entree.envoyer(Evenement::MiseAJourSignalee {
            message: message.to_string(),
            version_minimale: version_minimale.to_string(),
        })
    })
}

pub fn source_reseau(
    installation: &dyn Installation,
    dossier: std::path::PathBuf,
    surcharge: Option<&str>,
) -> Result<SourceReseau, String> {
    let politique = PolitiqueAdresses::production();
    let adresse = maj::adresse_manifeste(surcharge, &politique);
    let telechargeur = Telechargeur::nouveau(politique, true).map_err(|e| e.to_string())?;
    Ok(SourceReseau::nouvelle(
        telechargeur,
        &adresse,
        maj::CLE_PUBLIQUE,
        installation.cible().unwrap_or_default(),
        crate::VERSION,
        dossier,
    ))
}

pub struct OptionsService {
    pub canal: (EmetteurMaj, ReceptionMaj),
    pub source: Arc<dyn SourceMaj>,
    pub installation: Arc<dyn Installation>,
    pub fenetre: slint::Weak<Principale>,
    pub entree: Emetteur,
    pub redemarrer: Redemarreur,
    pub journal: Option<Arc<Journal>>,
}

pub fn demarrer_service(runtime: &tokio::runtime::Handle, options: OptionsService) -> MiseAJour {
    let service = MiseAJour::demarrer(
        runtime,
        options.canal,
        OptionsMiseAJour {
            source: options.source,
            installation: options.installation,
            contexte: contexte_de_la_fenetre(options.fenetre),
            redemarrer: options.redemarrer,
            bloquer: blocage_par_l_entree(options.entree),
            journal: options.journal,
        },
    );
    service.envoyer(EvenementMaj::Demarrer);
    service
}
