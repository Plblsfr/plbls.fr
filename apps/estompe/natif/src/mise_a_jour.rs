use std::sync::Arc;

use estompe_client::journal::Journal;
use estompe_client::mise_a_jour::{
    self as maj, Bloqueur, Contexte, EmetteurMaj, EtatMaj, EvenementMaj, MiseAJour,
    OptionsMiseAJour, Phase, PolitiqueAdresses, ReceptionMaj, Redemarreur, SourceContexte,
    SourceMaj, SourceReseau, Telechargeur,
};
use estompe_plateforme::Installation;
use slint::{ComponentHandle, Model};
use tokio::sync::oneshot;

use crate::entree::{Emetteur, Evenement, Installateur};
use crate::{DonneesMiseAJour, DonneesSaisie, DonneesVocal, EtapeVocal, Principale};

pub fn contexte_de_la_fenetre(fenetre: slint::Weak<Principale>) -> SourceContexte {
    Arc::new(move || {
        let fenetre = fenetre.clone();
        Box::pin(async move {
            let (envoi, reception) = oneshot::channel();
            let _ = fenetre.upgrade_in_event_loop(move |f| {
                let saisie = f.global::<DonneesSaisie>();
                let _ = envoi.send(Contexte {
                    saisie_en_cours: !saisie.get_texte().trim().is_empty()
                        || saisie.get_pieces().row_count() > 0
                        || f.global::<DonneesVocal>().get_etape() != EtapeVocal::Aucune,
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

pub fn installateur(emetteur: EmetteurMaj) -> Installateur {
    Arc::new(move || emetteur.envoyer(EvenementMaj::Installer))
}

pub fn afficher(f: &Principale, etat: &EtatMaj) {
    let d = f.global::<DonneesMiseAJour>();
    let (version, notes) = etat
        .disponible
        .as_ref()
        .map(|m| (m.version.as_str(), m.notes.trim()))
        .unwrap_or_default();
    d.set_automatique(etat.automatique);
    d.set_disponible(etat.disponible.is_some());
    d.set_proposee(etat.proposee());
    d.set_version(version.into());
    d.set_notes(notes.into());
    d.set_en_cours(etat.en_cours());
    d.set_installation(etat.phase == Phase::Prete);
    d.set_progression(etat.progression());
    d.set_verification(etat.phase == Phase::Verification);
    d.set_a_jour(etat.phase == Phase::AJour);
    d.set_erreur(etat.erreur.as_str().into());
    d.set_attente_appel(etat.phase == Phase::AttenteFinAppel);
    d.set_compte_a_rebours(i32::try_from(etat.compte_a_rebours).unwrap_or(i32::MAX));
}

pub fn brancher(f: &Principale, service: &MiseAJour, runtime: &tokio::runtime::Handle) {
    afficher(f, &service.etat());
    let d = f.global::<DonneesMiseAJour>();
    let rappel = |evenement: EvenementMaj| {
        let emetteur = service.emetteur();
        move || emetteur.envoyer(evenement.clone())
    };
    d.on_verifier(rappel(EvenementMaj::Verifier { manuelle: true }));
    d.on_installer(rappel(EvenementMaj::Installer));
    d.on_plus_tard(rappel(EvenementMaj::PlusTard));
    d.on_confirmer(rappel(EvenementMaj::Confirmer));
    let mut etat = service.observer();
    let fenetre = f.as_weak();
    runtime.spawn(async move {
        while etat.changed().await.is_ok() {
            let courant = etat.borrow_and_update().clone();
            if fenetre
                .upgrade_in_event_loop(move |f| afficher(&f, &courant))
                .is_err()
            {
                break;
            }
        }
    });
}
