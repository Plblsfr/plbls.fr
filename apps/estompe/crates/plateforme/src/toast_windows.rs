use crate::toast::{action_du_toast, xml_du_toast, CHAMP_REPONSE};
use crate::{
    Erreur, Notification, Notifications, RecepteurActions, Resultat, ICONE_INSTALLEE, IDENTIFIANT,
    NOM_AFFICHE,
};
use std::sync::{Arc, Mutex};
use windows::core::{IInspectable, Interface, HSTRING};
use windows::Data::Xml::Dom::XmlDocument;
use windows::Foundation::{IPropertyValue, TypedEventHandler};
use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
use windows::UI::Notifications::{
    ToastActivatedEventArgs, ToastNotification, ToastNotificationManager, ToastNotifier,
};

pub const CLE_AUMID: &str = r"Software\Classes\AppUserModelId\fr.plbls.estompe.natif";

type Recepteur = Arc<Mutex<Option<Arc<RecepteurActions>>>>;

pub struct ToastsWindows {
    notifieur: Option<ToastNotifier>,
    recepteur: Recepteur,
}

fn echec(e: windows::core::Error) -> Erreur {
    Erreur::Echec(format!("toast : {e}"))
}

fn enregistrer_l_identite() {
    let _ = crate::registre_windows::ecrire(CLE_AUMID, "DisplayName", NOM_AFFICHE);
    if let Some(icone) = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join(ICONE_INSTALLEE)))
        .filter(|icone| icone.is_file())
    {
        let _ = crate::registre_windows::ecrire(CLE_AUMID, "IconUri", &icone.to_string_lossy());
    }
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(IDENTIFIANT));
    }
}

impl ToastsWindows {
    pub fn nouveaux() -> Self {
        enregistrer_l_identite();
        Self {
            notifieur: ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(
                IDENTIFIANT,
            ))
            .ok(),
            recepteur: Arc::new(Mutex::new(None)),
        }
    }

    fn toast(&self, notification: &Notification) -> windows::core::Result<ToastNotification> {
        let document = XmlDocument::new()?;
        document.LoadXml(&HSTRING::from(xml_du_toast(notification)))?;
        let toast = ToastNotification::CreateToastNotification(&document)?;
        toast.SetTag(&HSTRING::from(notification.etiquette.as_str()))?;
        toast.SetGroup(&HSTRING::from(IDENTIFIANT))?;
        let recepteur = self.recepteur.clone();
        let etiquette = notification.etiquette.clone();
        toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_, arguments| {
                let Ok(arguments) = arguments.ok() else {
                    return Ok(());
                };
                let activation: ToastActivatedEventArgs = arguments.cast()?;
                let reponse = activation
                    .UserInput()
                    .ok()
                    .and_then(|valeurs| valeurs.Lookup(&HSTRING::from(CHAMP_REPONSE)).ok())
                    .and_then(|valeur| valeur.cast::<IPropertyValue>().ok())
                    .and_then(|valeur| valeur.GetString().ok())
                    .map(|texte| texte.to_string());
                let action = action_du_toast(&activation.Arguments()?.to_string(), reponse);
                let recepteur = recepteur.lock().unwrap_or_else(|e| e.into_inner()).clone();
                if let Some(recepteur) = recepteur {
                    recepteur(etiquette.clone(), action);
                }
                Ok(())
            },
        ))?;
        Ok(toast)
    }
}

impl Notifications for ToastsWindows {
    fn disponible(&self) -> bool {
        self.notifieur.is_some()
    }

    fn accepte_les_actions(&self) -> bool {
        true
    }

    fn accepte_la_reponse(&self) -> bool {
        true
    }

    fn ecouter(&self, actions: RecepteurActions) {
        *self.recepteur.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(actions));
    }

    fn notifier(&self, notification: &Notification) -> Resultat<()> {
        let notifieur = self
            .notifieur
            .as_ref()
            .ok_or(Erreur::Indisponible("notifications"))?;
        let toast = self.toast(notification).map_err(echec)?;
        notifieur.Show(&toast).map_err(echec)
    }

    fn retirer(&self, etiquette: &str) -> Resultat<()> {
        ToastNotificationManager::History()
            .and_then(|historique| {
                historique.RemoveGroupedTagWithId(
                    &HSTRING::from(etiquette),
                    &HSTRING::from(IDENTIFIANT),
                    &HSTRING::from(IDENTIFIANT),
                )
            })
            .map_err(echec)
    }
}
