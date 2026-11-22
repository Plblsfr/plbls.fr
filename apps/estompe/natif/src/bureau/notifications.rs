use super::sur_le_bureau;
use crate::coquille::{ObservateurSession, SourcePont};
use crate::overlay::messages::{cle_avatar, toast_de, EffetMessages, ServiceOverlayMessages};
use crate::overlay::voix::AvatarOverlay;
use crate::session::Memoire;
use estompe_client::journal::{Journal, Niveau};
use estompe_client::notifications::{
    decider, effet_de_l_action, ContexteNotification, EffetNotification, NotificationAffichee,
};
use estompe_client::store::messages::Brouillon;
use estompe_client::store::{Changement, CleConversation, Presence, Store};
use estompe_client::temps_reel::evenements::NotificationServeur;
use estompe_plateforme::{ActionNotification, Notifications};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

pub const CAPACITE_AFFICHEES: usize = 64;
const SOURCE_JOURNAL: &str = "notifications";

pub struct OptionsNotifieur {
    pub systeme: Arc<dyn Notifications>,
    pub memoire: Memoire,
    pub fenetre_active: Arc<AtomicBool>,
    pub source: SourcePont,
    pub runtime: tokio::runtime::Handle,
    pub journal: Option<Arc<Journal>>,
    pub overlay: Option<Arc<ServiceOverlayMessages>>,
}

pub struct Notifieur {
    options: OptionsNotifieur,
    affichees: Mutex<VecDeque<NotificationAffichee>>,
    mentions: Mutex<Option<usize>>,
}

fn cle_texte(cle: &CleConversation) -> String {
    match cle {
        CleConversation::Salon(id) => format!("c:{id}"),
        CleConversation::Prive(id) => format!("d:{id}"),
    }
}

impl Notifieur {
    pub fn nouveau(options: OptionsNotifieur) -> Arc<Self> {
        let notifieur = Arc::new(Self {
            options,
            affichees: Mutex::new(VecDeque::new()),
            mentions: Mutex::new(None),
        });
        let faible: Weak<Self> = Arc::downgrade(&notifieur);
        notifieur
            .options
            .systeme
            .ecouter(Box::new(move |etiquette, action| {
                if let Some(notifieur) = faible.upgrade() {
                    notifieur.agir(&etiquette, action);
                }
            }));
        if let Some(overlay) = &notifieur.options.overlay {
            let faible: Weak<Self> = Arc::downgrade(&notifieur);
            overlay.definir_effets(Arc::new(move |effet| {
                if let Some(notifieur) = faible.upgrade() {
                    notifieur.effet_de_l_overlay(effet);
                }
            }));
        }
        notifieur
    }

    fn vers_l_overlay(&self, store: &Store, notification: &NotificationServeur) {
        let Some(overlay) = &self.options.overlay else {
            return;
        };
        let ne_pas_deranger =
            store.lire(|etat| etat.presence_de(etat.moi()) == Presence::NePasDeranger);
        if !overlay.voulu(notification, ne_pas_deranger) {
            return;
        }
        let Some(toast) = toast_de(notification) else {
            return;
        };
        let cle = toast
            .auteur
            .as_deref()
            .and_then(|auteur| store.lire(|etat| cle_avatar(etat, auteur)));
        let profils = (self.options.source)().map(|pont| pont.profils().clone());
        let avatar = match (&cle, &profils) {
            (Some(cle), Some(profils)) => profils.avatar(cle).map(|image| AvatarOverlay {
                cle: cle.clone(),
                image,
            }),
            _ => None,
        };
        let id = toast.id.clone();
        let manque = avatar.is_none();
        overlay.recevoir(toast, avatar);
        if let (true, Some(cle), Some(profils)) = (manque, cle, profils) {
            let overlay = overlay.clone();
            self.options.runtime.spawn(async move {
                if let Some(image) = profils.charger_avatar(cle.clone()).await {
                    overlay.definir_avatar(&id, AvatarOverlay { cle, image });
                }
            });
        }
    }

    fn effet_de_l_overlay(&self, effet: EffetMessages) {
        match effet {
            EffetMessages::Envoyer(reponse) => {
                self.executer(
                    EffetNotification::Envoyer {
                        conversation: reponse.conversation.clone(),
                        texte: reponse.texte,
                    },
                    None,
                    None,
                );
                self.executer(EffetNotification::Lire(reponse.conversation), None, None);
            }
            EffetMessages::Ouvrir(toast) => {
                let notification = NotificationAffichee {
                    titre: toast.titre,
                    corps: toast.corps,
                    etiquette: String::new(),
                    cible: toast.cible,
                    genre: None,
                    conversation: Some(toast.conversation),
                    message: Some(toast.id),
                    actions: true,
                    silencieuse: true,
                };
                if let Some(effet) = effet_de_l_action(&ActionNotification::Ouvrir, &notification) {
                    self.executer(effet, notification.conversation.clone(), None);
                }
            }
        }
    }

    fn noter(&self, niveau: Niveau, message: &str) {
        if let Some(journal) = &self.options.journal {
            journal.ecrire(niveau, SOURCE_JOURNAL, message);
        }
    }

    fn affichees(&self) -> std::sync::MutexGuard<'_, VecDeque<NotificationAffichee>> {
        self.affichees.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn retenir(&self, notification: NotificationAffichee) {
        let mut affichees = self.affichees();
        affichees.retain(|n| n.etiquette != notification.etiquette);
        affichees.push_back(notification);
        while affichees.len() > CAPACITE_AFFICHEES {
            affichees.pop_front();
        }
    }

    fn retrouver(&self, etiquette: &str) -> Option<NotificationAffichee> {
        self.affichees()
            .iter()
            .rev()
            .find(|n| n.etiquette == etiquette)
            .cloned()
    }

    fn contexte(&self, store: &Store) -> ContexteNotification {
        ContexteNotification {
            fenetre_active: self.options.fenetre_active.load(Ordering::SeqCst),
            conversation_affichee: store.lire(|etat| etat.courante.clone()),
            salon_vocal: None,
            notifications_bureau: self.options.memoire.lire(|p| p.notifications),
        }
    }

    fn recevoir(
        &self,
        store: &Store,
        notification: &estompe_client::temps_reel::evenements::NotificationServeur,
    ) {
        let decision = decider(notification, &self.contexte(store));
        let Some(affichee) = decision.systeme else {
            return;
        };
        let systeme = &self.options.systeme;
        if !systeme.disponible() {
            return;
        }
        let demande = affichee.pour_le_systeme(systeme.accepte_la_reponse());
        match systeme.notifier(&demande) {
            Ok(()) => self.noter(
                Niveau::Info,
                &format!(
                    "notification affichée ({:?}, {})",
                    affichee.genre,
                    affichee
                        .conversation
                        .as_ref()
                        .map(cle_texte)
                        .unwrap_or_default()
                ),
            ),
            Err(e) => self.noter(Niveau::Avertissement, &format!("notification : {e}")),
        }
        self.retenir(affichee);
    }

    fn pastille(&self, total: usize) {
        let mut mentions = self.mentions.lock().unwrap_or_else(|e| e.into_inner());
        if *mentions == Some(total) {
            return;
        }
        *mentions = Some(total);
        sur_le_bureau(move |bureau| bureau.definir_pastille(total));
    }

    pub fn agir(&self, etiquette: &str, action: ActionNotification) {
        let Some(notification) = self.retrouver(etiquette) else {
            return;
        };
        let Some(effet) = effet_de_l_action(&action, &notification) else {
            return;
        };
        self.executer(effet, notification.conversation.clone(), Some(etiquette));
    }

    fn executer(
        &self,
        effet: EffetNotification,
        conversation: Option<CleConversation>,
        etiquette: Option<&str>,
    ) {
        let retirer = |etiquette: Option<&str>| {
            if let Some(etiquette) = etiquette {
                let _ = self.options.systeme.retirer(etiquette);
            }
        };
        let source = self.options.source.clone();
        let runtime = self.options.runtime.clone();
        match effet {
            EffetNotification::Naviguer { route, message } => {
                sur_le_bureau(move |bureau| {
                    bureau.ramener_fenetre();
                    let (Some(pont), Some(f)) = (source(), bureau.fenetre()) else {
                        return;
                    };
                    pont.naviguer(&f, route);
                    if let (Some(message), Some(cle)) = (message, conversation) {
                        let store = pont.store().clone();
                        runtime.spawn(async move {
                            let _ = store.sauter_a(&cle, &message).await;
                        });
                    }
                });
            }
            EffetNotification::Lire(cle) => {
                retirer(etiquette);
                if let Some(pont) = source() {
                    let store = pont.store().clone();
                    runtime.spawn(async move { store.lire_conversation(&cle).await });
                }
            }
            EffetNotification::Envoyer {
                conversation,
                texte,
            } => {
                retirer(etiquette);
                if let Some(pont) = source() {
                    let store = pont.store().clone();
                    runtime.spawn(async move {
                        store.envoyer(&conversation, Brouillon::texte(&texte)).await;
                    });
                }
            }
        }
    }
}

impl ObservateurSession for Notifieur {
    fn ouverte(&self, store: &Store) {
        self.pastille(store.lire(|etat| etat.total_mentions()));
    }

    fn changements(&self, store: &Store, lot: &[Changement]) {
        for changement in lot {
            if let Changement::Notification(notification) = changement {
                self.recevoir(store, notification);
                self.vers_l_overlay(store, notification);
            }
        }
        self.pastille(store.lire(|etat| etat.total_mentions()));
    }

    fn fermee(&self) {
        self.affichees().clear();
        self.pastille(0);
    }
}
