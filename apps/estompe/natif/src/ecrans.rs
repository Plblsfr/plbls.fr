use std::sync::Arc;

use slint::ComponentHandle;

use crate::entree::{Emetteur, Etat, Evenement, Publieur};
use crate::{Page, Principale};

pub fn afficher(f: &Principale, etat: &Etat) {
    let page = match etat {
        Etat::Demarrage => Page::Demarrage,
        Etat::Deconnecte | Etat::AttenteNavigateur | Etat::Erreur(_) => Page::Connexion,
        Etat::Verification => Page::Verification,
        Etat::Injoignable(_) => Page::Injoignable,
        Etat::Refuse => Page::Refuse,
        Etat::AActiver { .. } => Page::Activation,
        Etat::Invitation { .. } => Page::Invitation,
        Etat::Connecte { .. } => Page::Coquille,
        Etat::MiseAJourRequise { .. } => Page::MiseAJour,
    };
    let quittee = f.get_page() != page;
    f.set_page(page);
    f.set_connexion_en_attente(*etat == Etat::AttenteNavigateur);
    f.set_connexion_erreur(match etat {
        Etat::Erreur(message) => message.into(),
        _ => "".into(),
    });
    let (erreur, en_faute, en_cours) = match etat {
        Etat::AActiver {
            erreur,
            champ_en_faute,
            en_cours,
        } => (erreur.as_str(), *champ_en_faute, *en_cours),
        _ => ("", false, false),
    };
    f.set_activation_erreur(erreur.into());
    f.set_activation_champ_en_faute(en_faute);
    f.set_activation_en_cours(en_cours);
    let (erreur, en_cours) = match etat {
        Etat::Invitation { erreur, en_cours } => (erreur.as_str(), *en_cours),
        _ => ("", false),
    };
    f.set_invitation_erreur(erreur.into());
    f.set_invitation_en_cours(en_cours);
    f.set_injoignable_message(match etat {
        Etat::Injoignable(message) => message.into(),
        _ => "".into(),
    });
    let (message, minimale) = match etat {
        Etat::MiseAJourRequise {
            message,
            version_minimale,
        } => (message.as_str(), version_minimale.as_str()),
        _ => ("", ""),
    };
    f.set_mise_a_jour_message(message.into());
    f.set_mise_a_jour_version_minimale(minimale.into());
    f.set_pseudo(match etat {
        Etat::Connecte { compte, .. } => compte.pseudo.as_str().into(),
        _ => "".into(),
    });
    if quittee {
        if page != Page::Activation {
            f.set_activation_pseudo("".into());
        }
        if page != Page::Invitation {
            f.set_invitation_saisie("".into());
        }
    }
}

pub fn publieur(f: &Principale) -> Publieur {
    let faible = f.as_weak();
    Arc::new(move |etat: &Etat| {
        let etat = etat.clone();
        let _ = faible.upgrade_in_event_loop(move |f| afficher(&f, &etat));
    })
}

pub fn brancher(f: &Principale, emetteur: &Emetteur) {
    let simple = |evenement: Evenement| {
        let emetteur = emetteur.clone();
        move || emetteur.envoyer(evenement.clone())
    };
    f.on_se_connecter(simple(Evenement::SeConnecter));
    f.on_creer_compte(simple(Evenement::CreerCompte));
    f.on_annuler_connexion(simple(Evenement::Annuler));
    f.on_reessayer(simple(Evenement::Reessayer));
    f.on_se_deconnecter(simple(Evenement::SeDeconnecter));
    f.on_ouvrir_invitation(simple(Evenement::OuvrirInvitation));
    f.on_retour_accueil(simple(Evenement::RetourAccueil));
    f.on_telecharger_mise_a_jour(simple(Evenement::Telecharger));
    f.on_ouvrir_page_mise_a_jour(simple(Evenement::OuvrirPageTelechargement));
    let e = emetteur.clone();
    f.on_activer(move |pseudo| e.envoyer(Evenement::Activer(pseudo.into())));
    let e = emetteur.clone();
    f.on_rejoindre(move |saisie| e.envoyer(Evenement::Rejoindre(saisie.into())));
}
