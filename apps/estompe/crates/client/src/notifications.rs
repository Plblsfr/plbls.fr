use crate::route::{analyser_hash, Route};
use crate::store::CleConversation;
use crate::temps_reel::evenements::{GenreNotification, NotificationServeur};
use estompe_plateforme::{ActionNotification, Notification};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContexteNotification {
    pub fenetre_active: bool,
    pub conversation_affichee: Option<CleConversation>,
    pub salon_vocal: Option<String>,
    pub notifications_bureau: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationAffichee {
    pub titre: String,
    pub corps: String,
    pub etiquette: String,
    pub cible: String,
    pub genre: Option<GenreNotification>,
    pub conversation: Option<CleConversation>,
    pub message: Option<String>,
    pub actions: bool,
    pub silencieuse: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Decision {
    pub systeme: Option<NotificationAffichee>,
    pub son: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffetNotification {
    Naviguer {
        route: Route,
        message: Option<String>,
    },
    Lire(CleConversation),
    Envoyer {
        conversation: CleConversation,
        texte: String,
    },
}

fn conversation_de(notification: &NotificationServeur) -> Option<CleConversation> {
    notification
        .conv
        .as_deref()
        .filter(|c| !c.is_empty())
        .and_then(CleConversation::analyser)
}

fn vocal_de_mon_salon(
    notification: &NotificationServeur,
    conversation: Option<&CleConversation>,
    salon_vocal: Option<&str>,
) -> bool {
    notification.kind == Some(GenreNotification::Vocal)
        && matches!(
            (conversation, salon_vocal),
            (Some(CleConversation::Salon(salon)), Some(vocal)) if salon == vocal
        )
}

pub fn decider(notification: &NotificationServeur, contexte: &ContexteNotification) -> Decision {
    let conversation = conversation_de(notification);
    if vocal_de_mon_salon(
        notification,
        conversation.as_ref(),
        contexte.salon_vocal.as_deref(),
    ) {
        return Decision::default();
    }
    let affichee =
        conversation.is_some() && conversation.as_ref() == contexte.conversation_affichee.as_ref();
    if affichee && contexte.fenetre_active {
        return Decision::default();
    }
    let son = !notification.silent;
    if !contexte.notifications_bureau {
        return Decision { systeme: None, son };
    }
    let message = notification.msg.clone().filter(|m| !m.is_empty());
    Decision {
        systeme: Some(NotificationAffichee {
            titre: notification.title.clone(),
            corps: notification.body.clone(),
            etiquette: notification.tag.clone(),
            cible: notification.url.trim_start_matches('/').to_string(),
            genre: notification.kind,
            actions: message.is_some() && conversation.is_some(),
            conversation,
            message,
            silencieuse: notification.silent,
        }),
        son,
    }
}

impl NotificationAffichee {
    pub fn pour_le_systeme(&self, accepte_la_reponse: bool) -> Notification {
        Notification {
            titre: self.titre.clone(),
            corps: self.corps.clone(),
            etiquette: self.etiquette.clone(),
            actions: self.actions,
            reponse: self.actions && accepte_la_reponse,
            silencieuse: self.silencieuse,
        }
    }

    fn naviguer(&self) -> EffetNotification {
        let route = analyser_hash(&self.cible, &[]);
        let message = match &route {
            Route::Salon { message, .. } | Route::MessagePrive { message, .. } => message.clone(),
            _ => None,
        }
        .or_else(|| self.message.clone());
        EffetNotification::Naviguer { route, message }
    }
}

pub fn effet_de_l_action(
    action: &ActionNotification,
    notification: &NotificationAffichee,
) -> Option<EffetNotification> {
    match action {
        ActionNotification::Ouvrir => Some(notification.naviguer()),
        ActionNotification::MarquerLu => notification
            .conversation
            .clone()
            .map(EffetNotification::Lire),
        ActionNotification::Repondre(texte) => match (&notification.conversation, texte.trim()) {
            (Some(conversation), texte) if !texte.is_empty() => Some(EffetNotification::Envoyer {
                conversation: conversation.clone(),
                texte: texte.to_string(),
            }),
            _ => Some(notification.naviguer()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification() -> NotificationServeur {
        NotificationServeur {
            title: "alice".into(),
            body: "salut".into(),
            tag: "msg-1".into(),
            url: "/#/dm/d1".into(),
            conv: Some("d:d1".into()),
            msg: Some("1".into()),
            kind: Some(GenreNotification::MessagePrive),
            silent: true,
            ..NotificationServeur::default()
        }
    }

    fn contexte() -> ContexteNotification {
        ContexteNotification {
            fenetre_active: false,
            conversation_affichee: None,
            salon_vocal: None,
            notifications_bureau: true,
        }
    }

    fn affichee() -> NotificationAffichee {
        decider(&notification(), &contexte()).systeme.unwrap()
    }

    #[test]
    fn le_clic_ouvre_la_conversation_sans_barre_initiale() {
        let n = affichee();
        assert_eq!(n.cible, "#/dm/d1");
        assert_eq!(
            effet_de_l_action(&ActionNotification::Ouvrir, &n),
            Some(EffetNotification::Naviguer {
                route: Route::MessagePrive {
                    id: "d1".into(),
                    message: None,
                },
                message: Some("1".into()),
            })
        );
    }

    #[test]
    fn le_clic_va_au_message_de_l_adresse() {
        let mut serveur = notification();
        serveur.url = "/#/s/s1/c1/m9".into();
        serveur.conv = Some("c:c1".into());
        let n = decider(&serveur, &contexte()).systeme.unwrap();
        assert_eq!(
            effet_de_l_action(&ActionNotification::Ouvrir, &n),
            Some(EffetNotification::Naviguer {
                route: Route::Salon {
                    serveur: "s1".into(),
                    salon: Some("c1".into()),
                    message: Some("m9".into()),
                },
                message: Some("m9".into()),
            })
        );
    }

    #[test]
    fn rien_si_conversation_affichee_et_fenetre_active() {
        let c = ContexteNotification {
            fenetre_active: true,
            conversation_affichee: Some(CleConversation::Prive("d1".into())),
            ..contexte()
        };
        assert_eq!(
            decider(
                &NotificationServeur {
                    silent: false,
                    ..notification()
                },
                &c
            ),
            Decision {
                systeme: None,
                son: false
            }
        );
    }

    #[test]
    fn notifie_si_conversation_affichee_mais_fenetre_inactive() {
        let c = ContexteNotification {
            conversation_affichee: Some(CleConversation::Prive("d1".into())),
            ..contexte()
        };
        assert!(decider(&notification(), &c).systeme.is_some());
        let autre = ContexteNotification {
            fenetre_active: true,
            conversation_affichee: Some(CleConversation::Prive("d2".into())),
            ..contexte()
        };
        assert!(decider(&notification(), &autre).systeme.is_some());
    }

    #[test]
    fn marquer_comme_lu_lit_sans_naviguer() {
        assert_eq!(
            effet_de_l_action(&ActionNotification::MarquerLu, &affichee()),
            Some(EffetNotification::Lire(CleConversation::Prive("d1".into())))
        );
    }

    #[test]
    fn repondre_sans_texte_ouvre_la_conversation() {
        assert!(matches!(
            effet_de_l_action(&ActionNotification::Repondre("  ".into()), &affichee()),
            Some(EffetNotification::Naviguer { .. })
        ));
    }

    #[test]
    fn repondre_avec_texte_envoie_par_la_file() {
        assert_eq!(
            effet_de_l_action(
                &ActionNotification::Repondre(" coucou ".into()),
                &affichee()
            ),
            Some(EffetNotification::Envoyer {
                conversation: CleConversation::Prive("d1".into()),
                texte: "coucou".into(),
            })
        );
    }

    #[test]
    fn rien_pour_le_vocal_de_mon_salon() {
        let vocal = NotificationServeur {
            kind: Some(GenreNotification::Vocal),
            conv: Some("c:v1".into()),
            silent: false,
            ..notification()
        };
        let dans_le_salon = ContexteNotification {
            salon_vocal: Some("v1".into()),
            ..contexte()
        };
        assert_eq!(decider(&vocal, &dans_le_salon), Decision::default());
        let ailleurs = ContexteNotification {
            salon_vocal: Some("v2".into()),
            ..contexte()
        };
        assert!(decider(&vocal, &ailleurs).systeme.is_some());
        assert!(decider(&vocal, &contexte()).son);
    }

    #[test]
    fn silencieuse_sans_son() {
        let d = decider(&notification(), &contexte());
        assert!(!d.son);
        assert!(d.systeme.unwrap().pour_le_systeme(true).silencieuse);
        let sonore = NotificationServeur {
            silent: false,
            ..notification()
        };
        assert!(decider(&sonore, &contexte()).son);
    }

    #[test]
    fn preference_coupee_ni_notification_ni_son_du_systeme() {
        let c = ContexteNotification {
            notifications_bureau: false,
            ..contexte()
        };
        let d = decider(&notification(), &c);
        assert_eq!(d.systeme, None);
    }

    #[test]
    fn les_actions_demandent_un_message_et_une_conversation() {
        let n = affichee();
        assert!(n.actions);
        assert!(n.pour_le_systeme(true).reponse);
        assert!(!n.pour_le_systeme(false).reponse);
        let sans_message = NotificationServeur {
            msg: None,
            ..notification()
        };
        let n = decider(&sans_message, &contexte()).systeme.unwrap();
        assert!(!n.actions && !n.pour_le_systeme(true).reponse);
        assert_eq!(
            effet_de_l_action(&ActionNotification::MarquerLu, &n),
            Some(EffetNotification::Lire(CleConversation::Prive("d1".into())))
        );
        let sans_conversation = NotificationServeur {
            conv: None,
            ..notification()
        };
        let n = decider(&sans_conversation, &contexte()).systeme.unwrap();
        assert!(!n.actions);
        assert_eq!(effet_de_l_action(&ActionNotification::MarquerLu, &n), None);
    }
}
