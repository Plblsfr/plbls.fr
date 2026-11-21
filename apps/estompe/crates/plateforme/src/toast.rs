use crate::{ActionNotification, Notification};

pub const ARGUMENT_OUVRIR: &str = "ouvrir";
pub const ARGUMENT_REPONDRE: &str = "repondre";
pub const ARGUMENT_LIRE: &str = "lire";
pub const CHAMP_REPONSE: &str = "reponse";

pub fn echapper_xml(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    for caractere in texte.chars() {
        match caractere {
            '&' => sortie.push_str("&amp;"),
            '<' => sortie.push_str("&lt;"),
            '>' => sortie.push_str("&gt;"),
            '"' => sortie.push_str("&quot;"),
            '\'' => sortie.push_str("&apos;"),
            c if (c as u32) < 0x20 && !matches!(c, '\n' | '\t') => {}
            c => sortie.push(c),
        }
    }
    sortie
}

pub fn xml_du_toast(notification: &Notification) -> String {
    let mut xml = format!(
        "<toast launch=\"{ARGUMENT_OUVRIR}\" activationType=\"foreground\"><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual>",
        echapper_xml(&notification.titre),
        echapper_xml(&notification.corps)
    );
    if notification.actions {
        xml.push_str("<actions>");
        if notification.reponse {
            xml.push_str(&format!(
                "<input id=\"{CHAMP_REPONSE}\" type=\"text\" placeHolderContent=\"Répondre…\"/><action content=\"Répondre\" arguments=\"{ARGUMENT_REPONDRE}\" activationType=\"foreground\" hint-inputId=\"{CHAMP_REPONSE}\"/>"
            ));
        }
        xml.push_str(&format!(
            "<action content=\"Marquer comme lu\" arguments=\"{ARGUMENT_LIRE}\" activationType=\"foreground\"/></actions>"
        ));
    }
    if notification.silencieuse {
        xml.push_str("<audio silent=\"true\"/>");
    }
    xml.push_str("</toast>");
    xml
}

pub fn action_du_toast(arguments: &str, reponse: Option<String>) -> ActionNotification {
    match arguments {
        ARGUMENT_LIRE => ActionNotification::MarquerLu,
        ARGUMENT_REPONDRE => ActionNotification::Repondre(reponse.unwrap_or_default()),
        _ => ActionNotification::Ouvrir,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification() -> Notification {
        Notification {
            titre: "alice <a&b>".into(),
            corps: "« salut » \"toi\" l'ami".into(),
            etiquette: "msg-1".into(),
            actions: true,
            reponse: true,
            silencieuse: false,
        }
    }

    #[test]
    fn le_texte_est_echappe() {
        let xml = xml_du_toast(&notification());
        assert!(xml.contains("<text>alice &lt;a&amp;b&gt;</text>"));
        assert!(xml.contains("<text>« salut » &quot;toi&quot; l&apos;ami</text>"));
        assert_eq!(echapper_xml("a\u{0}b\u{7}c\nd"), "abc\nd");
    }

    #[test]
    fn le_champ_et_les_boutons_selon_la_notification() {
        let xml = xml_du_toast(&notification());
        assert!(xml.contains("<input id=\"reponse\" type=\"text\""));
        assert!(xml.contains("arguments=\"repondre\""));
        assert!(xml.contains("hint-inputId=\"reponse\""));
        assert!(xml.contains("arguments=\"lire\""));
        assert!(!xml.contains("<audio"));
        let sans_reponse = xml_du_toast(&Notification {
            reponse: false,
            ..notification()
        });
        assert!(!sans_reponse.contains("<input") && !sans_reponse.contains("repondre"));
        assert!(sans_reponse.contains("arguments=\"lire\""));
        let sans_actions = xml_du_toast(&Notification {
            actions: false,
            ..notification()
        });
        assert!(!sans_actions.contains("<actions>") && !sans_actions.contains("<input"));
    }

    #[test]
    fn une_notification_silencieuse_coupe_le_son() {
        let xml = xml_du_toast(&Notification {
            silencieuse: true,
            ..notification()
        });
        assert!(xml.ends_with("<audio silent=\"true\"/></toast>"));
    }

    #[test]
    fn les_arguments_du_toast_donnent_l_action() {
        assert_eq!(action_du_toast("lire", None), ActionNotification::MarquerLu);
        assert_eq!(
            action_du_toast("repondre", Some("coucou".into())),
            ActionNotification::Repondre("coucou".into())
        );
        assert_eq!(action_du_toast("ouvrir", None), ActionNotification::Ouvrir);
        assert_eq!(action_du_toast("", None), ActionNotification::Ouvrir);
    }
}
