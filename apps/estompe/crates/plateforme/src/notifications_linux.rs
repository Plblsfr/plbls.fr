use crate::toast::ARGUMENT_LIRE;
use crate::{
    ActionNotification, Erreur, Notification, Notifications, RecepteurActions, Resultat,
    NOM_AFFICHE,
};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

pub const ACTION_DEFAUT: &str = "default";
pub const SON: &str = "message-new-instant";
pub const ATTENTES_MAX: usize = 8;

type Recepteur = Arc<Mutex<Option<Arc<RecepteurActions>>>>;

#[derive(Default)]
struct Suivi {
    identifiants: HashMap<String, u32>,
    attentes: VecDeque<String>,
}

pub struct NotificationsLinux {
    actions: bool,
    disponible: bool,
    recepteur: Recepteur,
    suivi: Arc<Mutex<Suivi>>,
}

fn echec(e: notify_rust::error::Error) -> Erreur {
    Erreur::Echec(format!("notification : {e}"))
}

pub fn action_de_linux(action: &str) -> Option<ActionNotification> {
    match action {
        ACTION_DEFAUT => Some(ActionNotification::Ouvrir),
        ARGUMENT_LIRE => Some(ActionNotification::MarquerLu),
        _ => None,
    }
}

impl NotificationsLinux {
    pub fn nouvelles() -> Self {
        let capacites = notify_rust::get_capabilities();
        Self {
            disponible: capacites.is_ok(),
            actions: capacites
                .map(|c| c.iter().any(|c| c == "actions"))
                .unwrap_or(false),
            recepteur: Arc::new(Mutex::new(None)),
            suivi: Arc::new(Mutex::new(Suivi::default())),
        }
    }

    fn suivi(&self) -> std::sync::MutexGuard<'_, Suivi> {
        self.suivi.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn fermer(identifiant: u32) {
    if let Ok(connexion) = zbus::blocking::Connection::session() {
        let _ = connexion.call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "CloseNotification",
            &(identifiant,),
        );
    }
}

impl Notifications for NotificationsLinux {
    fn disponible(&self) -> bool {
        self.disponible
    }

    fn accepte_les_actions(&self) -> bool {
        self.actions
    }

    fn accepte_la_reponse(&self) -> bool {
        false
    }

    fn ecouter(&self, actions: RecepteurActions) {
        *self.recepteur.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(actions));
    }

    fn notifier(&self, notification: &Notification) -> Resultat<()> {
        let mut message = notify_rust::Notification::new();
        message
            .appname(NOM_AFFICHE)
            .summary(&notification.titre)
            .body(&notification.corps)
            .hint(notify_rust::Hint::Category("im.received".into()));
        if notification.silencieuse {
            message.hint(notify_rust::Hint::SuppressSound(true));
        } else {
            message.hint(notify_rust::Hint::SoundName(SON.into()));
        }
        let actions = self.actions && notification.actions;
        if self.actions {
            message.action(ACTION_DEFAUT, "Ouvrir");
        }
        if actions {
            message.action(ARGUMENT_LIRE, "Marquer comme lu");
        }
        let etiquette = notification.etiquette.clone();
        if let Some(identifiant) = self.suivi().identifiants.get(&etiquette) {
            message.id(*identifiant);
        }
        let poignee = message.show().map_err(echec)?;
        let trop = {
            let mut suivi = self.suivi();
            suivi.identifiants.insert(etiquette.clone(), poignee.id());
            if !self.actions {
                return Ok(());
            }
            suivi.attentes.retain(|e| e != &etiquette);
            suivi.attentes.push_back(etiquette.clone());
            let mut trop = Vec::new();
            while suivi.attentes.len() > ATTENTES_MAX {
                if let Some(ancienne) = suivi.attentes.pop_front() {
                    if let Some(id) = suivi.identifiants.remove(&ancienne) {
                        trop.push(id);
                    }
                }
            }
            trop
        };
        for identifiant in trop {
            fermer(identifiant);
        }
        let recepteur = self.recepteur.clone();
        let suivi = self.suivi.clone();
        let identifiant = poignee.id();
        let _ = std::thread::Builder::new()
            .name("estompe-notification".into())
            .spawn(move || {
                poignee.wait_for_action(|action| {
                    if let Some(action) = action_de_linux(action) {
                        let recepteur = recepteur.lock().unwrap_or_else(|e| e.into_inner()).clone();
                        if let Some(recepteur) = recepteur {
                            recepteur(etiquette.clone(), action);
                        }
                    }
                });
                let mut suivi = suivi.lock().unwrap_or_else(|e| e.into_inner());
                if suivi.identifiants.get(&etiquette) == Some(&identifiant) {
                    suivi.identifiants.remove(&etiquette);
                    suivi.attentes.retain(|e| e != &etiquette);
                }
            });
        Ok(())
    }

    fn retirer(&self, etiquette: &str) -> Resultat<()> {
        let identifiant = {
            let mut suivi = self.suivi();
            suivi.attentes.retain(|e| e != etiquette);
            suivi.identifiants.remove(etiquette)
        };
        if let Some(identifiant) = identifiant {
            fermer(identifiant);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    use zbus::zvariant::OwnedValue;

    const NOM: &str = "org.freedesktop.Notifications";
    const CHEMIN: &str = "/org/freedesktop/Notifications";

    #[derive(Debug, Clone, PartialEq)]
    struct Recue {
        remplace: u32,
        identifiant: u32,
        actions: Vec<String>,
        son: Option<String>,
    }

    struct ServeurFactice {
        capacites: Vec<String>,
        recues: Arc<Mutex<Vec<Recue>>>,
    }

    #[zbus::interface(name = "org.freedesktop.Notifications")]
    impl ServeurFactice {
        fn get_capabilities(&self) -> Vec<String> {
            self.capacites.clone()
        }

        #[allow(clippy::too_many_arguments)]
        fn notify(
            &self,
            _application: String,
            remplace: u32,
            _icone: String,
            _titre: String,
            _corps: String,
            actions: Vec<String>,
            indices: HashMap<String, OwnedValue>,
            _expiration: i32,
        ) -> u32 {
            let mut recues = self.recues.lock().unwrap();
            let identifiant = if remplace == 0 {
                recues.len() as u32 + 1
            } else {
                remplace
            };
            recues.push(Recue {
                remplace,
                identifiant,
                actions,
                son: indices
                    .get("sound-name")
                    .and_then(|v| String::try_from(v.clone()).ok()),
            });
            identifiant
        }

        fn close_notification(&self, _identifiant: u32) {}

        fn get_server_information(&self) -> (String, String, String, String) {
            ("factice".into(), "plbls".into(), "1".into(), "1.2".into())
        }
    }

    fn nom_tenu() -> bool {
        crate::zone::nom_tenu_sur_le_bus(NOM)
    }

    fn serveur(capacites: &[&str]) -> (zbus::blocking::Connection, Arc<Mutex<Vec<Recue>>>) {
        let recues = Arc::new(Mutex::new(Vec::new()));
        let connexion = zbus::blocking::connection::Builder::session()
            .unwrap()
            .name(NOM)
            .unwrap()
            .serve_at(
                CHEMIN,
                ServeurFactice {
                    capacites: capacites.iter().map(|c| c.to_string()).collect(),
                    recues: recues.clone(),
                },
            )
            .unwrap()
            .build()
            .unwrap();
        (connexion, recues)
    }

    fn notification(etiquette: &str) -> Notification {
        Notification {
            titre: "alice".into(),
            corps: "salut".into(),
            etiquette: etiquette.into(),
            actions: true,
            reponse: true,
            silencieuse: false,
        }
    }

    fn attendre(mut condition: impl FnMut() -> bool, quoi: &str) {
        let limite = Instant::now() + Duration::from_secs(10);
        while !condition() {
            assert!(Instant::now() < limite, "condition non atteinte : {quoi}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn les_notifications_passent_par_le_serveur_du_bureau() {
        if zbus::blocking::Connection::session().is_err() {
            assert!(
                std::env::var_os("ESTOMPE_DBUS_EXIGE").is_none(),
                "ESTOMPE_DBUS_EXIGE est défini mais le bus de session ne répond pas"
            );
            eprintln!("ignoré : aucun bus de session");
            return;
        }
        if nom_tenu() {
            eprintln!("ignoré : un vrai serveur de notifications tient déjà le nom");
            return;
        }

        let (connexion, recues) = serveur(&["body"]);
        let sans_actions = NotificationsLinux::nouvelles();
        assert!(sans_actions.disponible() && !sans_actions.accepte_les_actions());
        assert!(!sans_actions.accepte_la_reponse());
        sans_actions.notifier(&notification("t0")).unwrap();
        assert_eq!(
            recues.lock().unwrap().as_slice(),
            [Recue {
                remplace: 0,
                identifiant: 1,
                actions: vec![],
                son: Some(SON.into()),
            }]
        );
        drop(connexion);
        attendre(|| !nom_tenu(), "nom rendu");

        let (connexion, recues) = serveur(&["body", "actions"]);
        let avec_actions = NotificationsLinux::nouvelles();
        assert!(avec_actions.accepte_les_actions());
        let (envoi, actions) = mpsc::channel();
        avec_actions.ecouter(Box::new(move |etiquette, action| {
            let _ = envoi.send((etiquette, action));
        }));
        avec_actions.notifier(&notification("msg-1")).unwrap();
        avec_actions
            .notifier(&Notification {
                silencieuse: true,
                ..notification("msg-1")
            })
            .unwrap();
        {
            let recues = recues.lock().unwrap();
            assert_eq!(
                recues[0].actions,
                ["default", "Ouvrir", "lire", "Marquer comme lu"]
            );
            assert_eq!(recues[1].remplace, recues[0].identifiant, "même étiquette");
            assert_eq!(recues[1].son, None, "silencieuse");
        }
        let identifiant = recues.lock().unwrap()[1].identifiant;
        let limite = Instant::now() + Duration::from_secs(10);
        let recue = loop {
            connexion
                .emit_signal(
                    None::<&str>,
                    CHEMIN,
                    NOM,
                    "ActionInvoked",
                    &(identifiant, ARGUMENT_LIRE),
                )
                .unwrap();
            if let Ok(recue) = actions.recv_timeout(Duration::from_millis(50)) {
                break recue;
            }
            assert!(Instant::now() < limite, "action jamais reçue");
        };
        assert_eq!(recue, ("msg-1".to_string(), ActionNotification::MarquerLu));
        avec_actions
            .notifier(&Notification {
                actions: false,
                ..notification("msg-2")
            })
            .unwrap();
        assert_eq!(
            recues.lock().unwrap().last().unwrap().actions,
            ["default", "Ouvrir"]
        );
    }

    #[test]
    fn les_actions_du_bureau_donnent_l_action() {
        assert_eq!(action_de_linux("default"), Some(ActionNotification::Ouvrir));
        assert_eq!(action_de_linux("lire"), Some(ActionNotification::MarquerLu));
        assert_eq!(action_de_linux("__closed"), None);
    }
}
