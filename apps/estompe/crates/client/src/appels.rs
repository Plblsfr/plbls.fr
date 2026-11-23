use std::collections::BTreeMap;

use bytes::Bytes;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::fichiers::est_une_video;
use crate::http::{encoder_composant, Client, Corps, ErreurApi, OptionsAppel, Partie};
use crate::store::CleConversation;
use crate::televersement::{Progression, SourceEnvoi, Televersement};
use crate::temps_reel::evenements::{Lecture, StatutPresence};
use crate::types::{
    Attachment, DeleteMessagesIdVoteRequete, GetChannelsCidReadsReponseElement, GetMeReponse,
    GetServersIdReponse, GetServersIdVoiceReponse, GetUnreadReponse, LinkPreview, Mention, Message,
    MessagePage, Name50, NotifySettings, PatchMessagesIdRequete, Person, PostActivateRequete,
    PostAdminBadgesIdUsersRequete, PostAdminBadgesRequete, PostDmsRequete,
    PostNotificationsReadRequete, PostVoiceCidTokenReponse, PutMeActivityRequete,
    PutMeBannerRequete, PutMeStatusRequete, PutMessagesIdVoteRequete, Server, DM,
};

pub const LIMITE_MESSAGES: u32 = 50;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EmojiPersonnalise {
    pub id: String,
    pub server_id: String,
    pub name: String,
    pub created_by: String,
}

pub fn resoudre_emoji<'a>(
    emojis: &'a [EmojiPersonnalise],
    nom: &str,
    serveur: Option<&str>,
) -> Option<&'a EmojiPersonnalise> {
    let nom = nom.to_lowercase();
    emojis
        .iter()
        .find(|e| e.name == nom && Some(e.server_id.as_str()) == serveur)
        .or_else(|| emojis.iter().find(|e| e.name == nom))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Badge {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub color: String,
    pub description: String,
    pub icon_version: i64,
    pub sort_order: i64,
}

pub fn code_d_invitation(saisie: &str) -> String {
    let saisie = saisie.trim();
    let fin: Vec<char> = saisie.chars().rev().take(11).collect();
    let forme = fin.len() == 11
        && fin.iter().enumerate().all(|(i, c)| {
            if i == 5 {
                *c == '-'
            } else {
                c.is_ascii_alphanumeric()
            }
        });
    let code = if forme {
        fin.into_iter().rev().collect::<String>()
    } else {
        saisie.to_string()
    };
    code.to_uppercase()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub limite: u32,
    pub avant: Option<String>,
    pub apres: Option<String>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            limite: LIMITE_MESSAGES,
            avant: None,
            apres: None,
        }
    }
}

impl Page {
    fn parametres(&self) -> String {
        let borne = |nom: &str, valeur: &Option<String>| {
            valeur
                .as_deref()
                .map(|v| format!("&{nom}={}", encoder_composant(v)))
                .unwrap_or_default()
        };
        format!(
            "?limit={}{}{}",
            self.limite,
            borne("before", &self.avant),
            borne("after", &self.apres)
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NouveauSondage {
    pub options: Vec<String>,
    pub multiple: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closes_at: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NouveauMessage {
    pub content: String,
    pub file_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll: Option<NouveauSondage>,
}

fn chemin_de(conversation: &CleConversation, suite: &str) -> String {
    match conversation {
        CleConversation::Salon(id) => format!("/channels/{id}/{suite}"),
        CleConversation::Prive(id) => format!("/dms/{id}/{suite}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vocal {
    pub duree_ms: u64,
    pub forme_d_onde: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envoi {
    pub nom: String,
    pub type_mime: String,
    pub contenu: Bytes,
    pub vocal: Option<Vocal>,
}

impl Client {
    pub async fn moi(&self) -> Result<GetMeReponse, ErreurApi> {
        self.get("/me").await
    }

    pub async fn activer(&self, pseudo: &str) -> Result<Person, ErreurApi> {
        self.post(
            "/activate",
            &PostActivateRequete {
                pseudo: pseudo.to_string(),
            },
        )
        .await
    }

    pub async fn rejoindre_serveur(&self, code: &str) -> Result<Server, ErreurApi> {
        self.requete(
            Method::POST,
            &format!("/server-invites/{}/join", encoder_composant(code)),
            &Corps::Aucun,
            OptionsAppel::default(),
        )
        .await?
        .ok_or_else(|| ErreurApi::ReponseInvalide("réponse sans corps JSON".to_string()))
    }

    pub async fn serveurs(&self) -> Result<Vec<Server>, ErreurApi> {
        self.get("/servers").await
    }

    pub async fn serveur(&self, serveur: &str) -> Result<GetServersIdReponse, ErreurApi> {
        self.get(&format!("/servers/{serveur}")).await
    }

    pub async fn conversations_privees(&self) -> Result<Vec<DM>, ErreurApi> {
        self.get("/dms").await
    }

    pub async fn ouvrir_conversation_privee(&self, pseudo: &str) -> Result<DM, ErreurApi> {
        self.post(
            "/dms",
            &PostDmsRequete {
                pseudo: pseudo.to_string(),
            },
        )
        .await
    }

    pub async fn notifications(&self) -> Result<Vec<Mention>, ErreurApi> {
        self.get("/notifications").await
    }

    pub async fn lire_notifications(&self, messages: &[String]) -> Result<(), ErreurApi> {
        let corps = Corps::json(&PostNotificationsReadRequete {
            message_ids: Some(messages.to_vec()),
        })?;
        self.envoyer(Method::POST, "/notifications/read", &corps)
            .await
    }

    pub async fn non_lus(&self) -> Result<GetUnreadReponse, ErreurApi> {
        self.get("/unread").await
    }

    pub async fn marquer_lu(&self, conversation: &CleConversation) -> Result<(), ErreurApi> {
        let chemin = match conversation {
            CleConversation::Salon(id) => format!("/channels/{id}/read"),
            CleConversation::Prive(id) => format!("/dms/{id}/read"),
        };
        self.envoyer(Method::POST, &chemin, &Corps::Aucun).await
    }

    pub async fn statuts(&self) -> Result<BTreeMap<String, StatutPresence>, ErreurApi> {
        self.get("/statuses").await
    }

    pub async fn definir_statut(&self, statut: StatutPresence) -> Result<(), ErreurApi> {
        let corps = Corps::json(&PutMeStatusRequete {
            status: Some(statut),
        })?;
        self.envoyer(Method::PUT, "/me/status", &corps).await
    }

    pub async fn definir_activite(&self, nom: &str) -> Result<(), ErreurApi> {
        let corps = Corps::json(&PutMeActivityRequete {
            name: Some(nom.to_string()),
        })?;
        self.envoyer(Method::PUT, "/me/activity", &corps).await
    }

    pub async fn reglages_notification(&self) -> Result<NotifySettings, ErreurApi> {
        self.get("/me/notify-settings").await
    }

    pub async fn enregistrer_reglages_notification(
        &self,
        reglages: &NotifySettings,
    ) -> Result<(), ErreurApi> {
        self.envoyer(Method::PUT, "/me/notify-settings", &Corps::json(reglages)?)
            .await
    }

    pub async fn versions_avatars(&self) -> Result<BTreeMap<String, i64>, ErreurApi> {
        self.get("/avatars").await
    }

    pub async fn versions_avatars_du_serveur(
        &self,
        serveur: &str,
    ) -> Result<BTreeMap<String, i64>, ErreurApi> {
        self.get(&format!("/servers/{serveur}/avatars")).await
    }

    pub async fn vocal_du_serveur(
        &self,
        serveur: &str,
    ) -> Result<GetServersIdVoiceReponse, ErreurApi> {
        self.get(&format!("/servers/{serveur}/voice")).await
    }

    pub async fn jeton_vocal(&self, salon: &str) -> Result<PostVoiceCidTokenReponse, ErreurApi> {
        self.requete(
            Method::POST,
            &format!("/voice/{}/token", encoder_composant(salon)),
            &Corps::Aucun,
            OptionsAppel::default(),
        )
        .await?
        .ok_or_else(|| ErreurApi::ReponseInvalide("réponse sans corps JSON".to_string()))
    }

    pub async fn moderer_vocal(
        &self,
        salon: &str,
        utilisateur: &str,
        micro_coupe: bool,
        son_coupe: bool,
    ) -> Result<(), ErreurApi> {
        self.envoyer(
            Method::PUT,
            &format!(
                "/voice/{}/participants/{}/mod",
                encoder_composant(salon),
                encoder_composant(utilisateur)
            ),
            &Corps::json(&serde_json::json!({"mic_off": micro_coupe, "sound_off": son_coupe}))?,
        )
        .await
    }

    pub async fn deplacer_participant_vocal(
        &self,
        salon: &str,
        utilisateur: &str,
        vers: &str,
    ) -> Result<(), ErreurApi> {
        self.envoyer(
            Method::PUT,
            &format!(
                "/voice/{}/participants/{}/move",
                encoder_composant(salon),
                encoder_composant(utilisateur)
            ),
            &Corps::json(&serde_json::json!({"channel_id": vers}))?,
        )
        .await
    }

    pub async fn deconnecter_participant_vocal(
        &self,
        salon: &str,
        utilisateur: &str,
    ) -> Result<(), ErreurApi> {
        self.envoyer(
            Method::POST,
            &format!(
                "/voice/{}/participants/{}/disconnect",
                encoder_composant(salon),
                encoder_composant(utilisateur)
            ),
            &Corps::Aucun,
        )
        .await
    }

    pub async fn creer_serveur(&self, nom: &str) -> Result<Server, ErreurApi> {
        self.post(
            "/servers",
            &Name50 {
                name: nom.to_string(),
            },
        )
        .await
    }

    pub async fn revoquer_invitation_serveur(
        &self,
        serveur: &str,
        code: &str,
    ) -> Result<(), ErreurApi> {
        self.delete(&format!(
            "/servers/{serveur}/invites/{}",
            encoder_composant(code)
        ))
        .await
    }

    pub async fn messages_du_salon(
        &self,
        salon: &str,
        page: &Page,
    ) -> Result<MessagePage, ErreurApi> {
        self.get(&format!("/channels/{salon}/messages{}", page.parametres()))
            .await
    }

    pub async fn messages_prives(
        &self,
        conversation: &str,
        page: &Page,
    ) -> Result<MessagePage, ErreurApi> {
        self.get(&format!(
            "/dms/{conversation}/messages{}",
            page.parametres()
        ))
        .await
    }

    pub async fn messages_de(
        &self,
        conversation: &CleConversation,
        page: &Page,
    ) -> Result<MessagePage, ErreurApi> {
        match conversation {
            CleConversation::Salon(id) => self.messages_du_salon(id, page).await,
            CleConversation::Prive(id) => self.messages_prives(id, page).await,
        }
    }

    pub async fn epingles(
        &self,
        conversation: &CleConversation,
    ) -> Result<Vec<Message>, ErreurApi> {
        let chemin = match conversation {
            CleConversation::Salon(id) => format!("/channels/{id}/pins"),
            CleConversation::Prive(id) => format!("/dms/{id}/pins"),
        };
        let page: MessagePage = self.get(&chemin).await?;
        Ok(page.messages.unwrap_or_default())
    }

    pub async fn lectures(
        &self,
        conversation: &CleConversation,
    ) -> Result<Vec<Lecture>, ErreurApi> {
        let chemin = match conversation {
            CleConversation::Salon(id) => format!("/channels/{id}/reads"),
            CleConversation::Prive(id) => format!("/dms/{id}/reads"),
        };
        let liste: Vec<GetChannelsCidReadsReponseElement> = self.get(&chemin).await?;
        Ok(liste
            .into_iter()
            .map(|l| Lecture {
                channel_id: conversation.salon().map(str::to_string),
                dm_id: match conversation {
                    CleConversation::Prive(id) => Some(id.clone()),
                    CleConversation::Salon(_) => None,
                },
                user_id: l.user_id.unwrap_or_default(),
                pseudo: l.pseudo.unwrap_or_default(),
                last_id: l.last_id.unwrap_or_default(),
                read_at: l.read_at.unwrap_or_default(),
            })
            .collect())
    }

    pub async fn reagir(&self, message: &str, emoji: &str) -> Result<(), ErreurApi> {
        let chemin = format!("/messages/{message}/reactions/{}", encoder_composant(emoji));
        self.envoyer(Method::PUT, &chemin, &Corps::Aucun).await
    }

    pub async fn retirer_reaction(&self, message: &str, emoji: &str) -> Result<(), ErreurApi> {
        let chemin = format!("/messages/{message}/reactions/{}", encoder_composant(emoji));
        self.delete(&chemin).await
    }

    pub async fn envoyer_message(
        &self,
        conversation: &CleConversation,
        message: &NouveauMessage,
    ) -> Result<Message, ErreurApi> {
        self.post(&chemin_de(conversation, "messages"), message)
            .await
    }

    pub async fn modifier_message(
        &self,
        message: &str,
        contenu: &str,
        fichiers: Option<Vec<String>>,
    ) -> Result<Message, ErreurApi> {
        self.patch(
            &format!("/messages/{message}"),
            &PatchMessagesIdRequete {
                content: contenu.to_string(),
                file_ids: fichiers,
            },
        )
        .await
    }

    pub async fn supprimer_message(&self, message: &str) -> Result<(), ErreurApi> {
        self.delete(&format!("/messages/{message}")).await
    }

    pub async fn signaler_saisie(&self, conversation: &CleConversation) -> Result<(), ErreurApi> {
        self.envoyer(
            Method::POST,
            &chemin_de(conversation, "typing"),
            &Corps::Aucun,
        )
        .await
    }

    pub async fn epingler(&self, message: &str, epingle: bool) -> Result<(), ErreurApi> {
        let methode = if epingle { Method::PUT } else { Method::DELETE };
        self.envoyer(methode, &format!("/messages/{message}/pin"), &Corps::Aucun)
            .await
    }

    pub async fn voter(&self, message: &str, choix: &str) -> Result<(), ErreurApi> {
        let corps = Corps::json(&PutMessagesIdVoteRequete {
            option_id: choix.to_string(),
        })?;
        self.envoyer(Method::PUT, &format!("/messages/{message}/vote"), &corps)
            .await
    }

    pub async fn retirer_vote(&self, message: &str, choix: Option<&str>) -> Result<(), ErreurApi> {
        let corps = Corps::json(&DeleteMessagesIdVoteRequete {
            option_id: choix.map(str::to_string),
        })?;
        self.envoyer(Method::DELETE, &format!("/messages/{message}/vote"), &corps)
            .await
    }

    pub async fn clore_sondage(&self, message: &str) -> Result<(), ErreurApi> {
        self.envoyer(
            Method::POST,
            &format!("/messages/{message}/poll/close"),
            &Corps::Aucun,
        )
        .await
    }

    pub async fn votants(&self, message: &str) -> Result<BTreeMap<String, Vec<Person>>, ErreurApi> {
        self.get(&format!("/messages/{message}/poll/votes")).await
    }

    pub async fn envoyer_fichier(&self, envoi: &Envoi) -> Result<Attachment, ErreurApi> {
        let mut parties = Vec::new();
        if let Some(vocal) = &envoi.vocal {
            let forme: Vec<String> = vocal.forme_d_onde.iter().map(u8::to_string).collect();
            parties.push(Partie::texte("kind", "voice"));
            parties.push(Partie::texte("duration_ms", &vocal.duree_ms.to_string()));
            parties.push(Partie::texte("waveform", &forme.join(",")));
        }
        parties.push(Partie::fichier(
            "file",
            &envoi.nom,
            &envoi.type_mime,
            envoi.contenu.clone(),
        ));
        self.televerser("/files", &parties).await
    }

    pub async fn envoyer_piece(
        &self,
        nom: &str,
        type_mime: &str,
        source: SourceEnvoi,
        progres: Progression,
    ) -> Result<Attachment, ErreurApi> {
        let parties = if est_une_video(type_mime) {
            vec![Partie::texte("kind", "video")]
        } else {
            Vec::new()
        };
        let envoi = Televersement {
            parties,
            nom: nom.to_string(),
            type_mime: type_mime.to_string(),
            source,
        };
        self.televerser_en_flux("/files", &envoi, progres).await
    }

    pub async fn envoyer_vocal(
        &self,
        fichier: Vec<u8>,
        duree_ms: u64,
        forme: &[u8],
    ) -> Result<Attachment, ErreurApi> {
        let forme: Vec<String> = forme.iter().map(u8::to_string).collect();
        let envoi = Televersement {
            parties: vec![
                Partie::texte("kind", "voice"),
                Partie::texte(
                    "duration_ms",
                    &duree_ms
                        .clamp(1, crate::message_vocal::DUREE_MAX_MS)
                        .to_string(),
                ),
                Partie::texte("waveform", &forme.join(",")),
            ],
            nom: crate::message_vocal::NOM_DU_FICHIER.to_string(),
            type_mime: crate::message_vocal::TYPE_MIME.to_string(),
            source: SourceEnvoi::Octets(fichier.into()),
        };
        self.televerser_en_flux("/files", &envoi, std::sync::Arc::new(|_, _| {}))
            .await
    }

    pub async fn definir_avatar(
        &self,
        type_mime: &str,
        image: impl Into<Bytes>,
    ) -> Result<(), ErreurApi> {
        self.envoyer_image("/me/avatar", type_mime, image).await
    }

    pub async fn definir_avatar_de_serveur(
        &self,
        serveur: &str,
        type_mime: &str,
        image: impl Into<Bytes>,
    ) -> Result<(), ErreurApi> {
        self.envoyer_image(&format!("/servers/{serveur}/me/avatar"), type_mime, image)
            .await
    }

    pub async fn supprimer_avatar(&self) -> Result<(), ErreurApi> {
        self.delete("/me/avatar").await
    }

    pub async fn supprimer_avatar_de_serveur(&self, serveur: &str) -> Result<(), ErreurApi> {
        self.delete(&format!("/servers/{serveur}/me/avatar")).await
    }

    pub async fn definir_banniere(&self, couleur: &str) -> Result<(), ErreurApi> {
        let corps = Corps::json(&PutMeBannerRequete {
            color: Some(couleur.to_string()),
        })?;
        self.envoyer(Method::PUT, "/me/banner", &corps).await
    }

    pub async fn icone_de_badge(&self, badge: &str) -> Result<Bytes, ErreurApi> {
        self.octets(&format!("/badges/{}/icon", encoder_composant(badge)))
            .await
    }

    pub async fn definir_fond_de_camera(
        &self,
        type_mime: &str,
        image: impl Into<Bytes>,
    ) -> Result<(), ErreurApi> {
        self.envoyer_image("/me/camera-background", type_mime, image)
            .await
    }

    pub async fn avatar(
        &self,
        utilisateur: &str,
        serveur: Option<&str>,
    ) -> Result<Bytes, ErreurApi> {
        let prefixe = serveur.map(|s| format!("/servers/{s}")).unwrap_or_default();
        self.octets(&format!("{prefixe}/users/{utilisateur}/avatar"))
            .await
    }

    pub async fn emojis_personnalises(&self) -> Result<Vec<EmojiPersonnalise>, ErreurApi> {
        self.get("/emojis").await
    }

    pub async fn image_d_emoji(&self, emoji: &str) -> Result<Bytes, ErreurApi> {
        self.octets(&format!("/emojis/{}/image", encoder_composant(emoji)))
            .await
    }

    pub async fn contenu_de_fichier(&self, fichier: &str) -> Result<Bytes, ErreurApi> {
        self.octets(&format!("/files/{}", encoder_composant(fichier)))
            .await
    }

    pub async fn apercu_de_lien(
        &self,
        url: &str,
        message: Option<&str>,
    ) -> Result<Option<LinkPreview>, ErreurApi> {
        let mut chemin = format!("/link-preview?url={}", encoder_composant(url));
        if let Some(message) = message.filter(|m| !m.is_empty()) {
            chemin.push_str(&format!("&message={}", encoder_composant(message)));
        }
        match self.get::<LinkPreview>(&chemin).await {
            Ok(apercu) => Ok(Some(apercu)),
            Err(ErreurApi::Introuvable(_)) => Ok(None),
            Err(erreur) => Err(erreur),
        }
    }

    pub async fn badges_de(&self, utilisateur: &str) -> Result<Vec<Badge>, ErreurApi> {
        self.get(&format!("/users/{}/badges", encoder_composant(utilisateur)))
            .await
    }

    pub async fn icone_de_serveur(&self, serveur: &str) -> Result<Bytes, ErreurApi> {
        self.octets(&format!("/servers/{serveur}/icon")).await
    }

    pub async fn accorder_badge(&self, badge: &str, pseudo: &str) -> Result<(), ErreurApi> {
        let chemin = format!("/admin/badges/{badge}/users/{}", encoder_composant(pseudo));
        self.envoyer(Method::PUT, &chemin, &Corps::Aucun).await
    }

    pub async fn retirer_badge(&self, badge: &str, pseudo: &str) -> Result<(), ErreurApi> {
        self.delete(&format!(
            "/admin/badges/{badge}/users/{}",
            encoder_composant(pseudo)
        ))
        .await
    }

    pub async fn modifier_badge(
        &self,
        badge: &str,
        valeurs: &PostAdminBadgesRequete,
    ) -> Result<Option<Value>, ErreurApi> {
        let corps = Corps::json(valeurs)?;
        self.requete(
            Method::PUT,
            &format!("/admin/badges/{badge}"),
            &corps,
            Default::default(),
        )
        .await
    }

    pub async fn accorder_badges(
        &self,
        badge: &str,
        pseudos: &[String],
    ) -> Result<Option<Value>, ErreurApi> {
        let corps = Corps::json(&PostAdminBadgesIdUsersRequete {
            pseudos: pseudos.to_vec(),
        })?;
        self.requete(
            Method::POST,
            &format!("/admin/badges/{badge}/users"),
            &corps,
            Default::default(),
        )
        .await
    }

    pub async fn supprimer_icone_de_badge(&self, badge: &str) -> Result<(), ErreurApi> {
        self.delete(&format!("/admin/badges/{badge}/icon")).await
    }

    pub async fn comptes(&self, recherche: &str) -> Result<Option<Value>, ErreurApi> {
        let chemin = format!("/admin/users?q={}", encoder_composant(recherche));
        self.requete(Method::GET, &chemin, &Corps::Aucun, Default::default())
            .await
    }

    pub async fn suspendre_compte(&self, compte: &str, suspendu: bool) -> Result<(), ErreurApi> {
        let methode = if suspendu {
            Method::PUT
        } else {
            Method::DELETE
        };
        self.envoyer(
            methode,
            &format!("/admin/users/{compte}/suspension"),
            &Corps::Aucun,
        )
        .await
    }

    pub async fn supprimer_serveur_en_administrateur(
        &self,
        serveur: &str,
    ) -> Result<(), ErreurApi> {
        self.delete(&format!("/admin/servers/{serveur}")).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::serveur_factice::{Reponse, ServeurFactice};

    fn lignes(serveur: &ServeurFactice) -> Vec<(String, String)> {
        serveur.lignes()
    }

    fn ligne(chemin: &str, methode: &str) -> (String, String) {
        (chemin.to_string(), methode.to_string())
    }

    #[tokio::test]
    async fn cree_un_serveur_en_json() {
        let serveur = ServeurFactice::demarrer([Reponse::json(201, json!({"id": "s1"}))]).await;
        let cree = serveur.client().creer_serveur("Amis").await.unwrap();
        assert_eq!(
            cree,
            Server {
                id: Some("s1".to_string()),
                ..Server::default()
            }
        );
        let requete = &serveur.requetes()[0];
        assert_eq!(requete.ligne(), ("/servers", "POST"));
        assert_eq!(requete.entete("content-type"), Some("application/json"));
        assert_eq!(requete.json(), json!({"name": "Amis"}));
        assert_eq!(requete.entete("cookie"), None);
    }

    #[tokio::test]
    async fn lit_le_compte_avec_le_jeton_porte() {
        let serveur = ServeurFactice::demarrer([Reponse::json(
            200,
            json!({"activated": true, "allowed": true, "transcription": "alpha", "rights": ["access", "futur"], "nouveau_champ": 1}),
        )])
        .await;
        let moi = serveur
            .client()
            .avec_jeton(|| Some("jeton".to_string()))
            .moi()
            .await
            .unwrap();
        assert_eq!((moi.activated, moi.allowed), (Some(true), Some(true)));
        assert_eq!(
            moi.transcription,
            Some(crate::types::GetMeReponseTranscription::Alpha)
        );
        let droits = moi.rights.unwrap();
        assert_eq!(droits[0], crate::types::GetMeReponseRightsElement::Access);
        assert_eq!(droits[1], crate::types::GetMeReponseRightsElement::Inconnu);
        assert_eq!(
            serveur.requetes()[0].entete("authorization"),
            Some("Bearer jeton")
        );
        assert_eq!(serveur.requetes()[0].ligne(), ("/me", "GET"));
    }

    #[tokio::test]
    async fn active_le_compte_ou_rend_le_conflit_du_serveur() {
        let serveur =
            ServeurFactice::demarrer([Reponse::json(201, json!({"id": "u1", "pseudo": "bob"}))])
                .await;
        let client = serveur.client();
        let compte = client.activer("bob").await.unwrap();
        assert_eq!(compte.pseudo.as_deref(), Some("bob"));
        assert_eq!(serveur.requetes()[0].json(), json!({"pseudo": "bob"}));
        serveur.programmer([Reponse::json(
            409,
            json!({"error": "pseudo_taken", "message": "pseudo déjà pris"}),
        )]);
        let erreur = client.activer("bob").await.unwrap_err();
        assert!(matches!(erreur, ErreurApi::Conflit(_)));
        assert_eq!(
            (erreur.statut(), erreur.code(), erreur.message()),
            (409, "pseudo_taken", "pseudo déjà pris")
        );
    }

    #[test]
    fn extrait_le_code_d_un_lien_ou_d_une_saisie() {
        for (saisie, attendu) in [
            ("abcde-23456", "ABCDE-23456"),
            ("  https://chat.plbls.fr/#/join/abcde-23456 ", "ABCDE-23456"),
            ("estompe://join/Q2W3E-R4T5Y", "Q2W3E-R4T5Y"),
            ("xabcde-23456", "ABCDE-23456"),
            ("court", "COURT"),
            ("abcde_23456", "ABCDE_23456"),
            ("", ""),
        ] {
            assert_eq!(code_d_invitation(saisie), attendu, "{saisie:?}");
        }
    }

    #[tokio::test]
    async fn demande_le_jeton_vocal_et_modere_un_participant() {
        let serveur = ServeurFactice::demarrer([
            Reponse::json(
                200,
                json!({"url": "wss://voice.plbls.fr", "token": "t", "room": "r", "max_participants": 10}),
            ),
            Reponse::json(409, json!({"error": "voice_full", "message": "complet"})),
            Reponse::vide(204),
            Reponse::vide(204),
            Reponse::json(409, json!({"error": "not_in_voice", "message": "absent"})),
        ])
        .await;
        let client = serveur.client();
        let jeton = client.jeton_vocal("v1").await.unwrap();
        assert_eq!(jeton.url.as_deref(), Some("wss://voice.plbls.fr"));
        assert_eq!(jeton.max_participants, Some(10));
        assert_eq!(
            client.jeton_vocal("v1").await.unwrap_err().code(),
            "voice_full"
        );
        client.moderer_vocal("v1", "u2", true, false).await.unwrap();
        let requetes = serveur.requetes();
        assert_eq!(requetes[0].ligne(), ("/voice/v1/token", "POST"));
        assert_eq!(
            requetes[2].ligne(),
            ("/voice/v1/participants/u2/mod", "PUT")
        );
        assert_eq!(
            requetes[2].json(),
            json!({"mic_off": true, "sound_off": false})
        );
        client
            .deplacer_participant_vocal("v1", "u2", "v2")
            .await
            .unwrap();
        assert_eq!(
            client
                .deconnecter_participant_vocal("v1", "u2")
                .await
                .unwrap_err()
                .code(),
            "not_in_voice"
        );
        let requetes = serveur.requetes();
        assert_eq!(
            requetes[3].ligne(),
            ("/voice/v1/participants/u2/move", "PUT")
        );
        assert_eq!(requetes[3].json(), json!({"channel_id": "v2"}));
        assert_eq!(
            requetes[4].ligne(),
            ("/voice/v1/participants/u2/disconnect", "POST")
        );
    }

    #[tokio::test]
    async fn rejoint_un_serveur_par_son_code_sans_corps() {
        let serveur =
            ServeurFactice::demarrer([Reponse::json(200, json!({"id": "s1", "name": "Amis"}))])
                .await;
        let rejoint = serveur.client().rejoindre_serveur("AB/12").await.unwrap();
        assert_eq!(rejoint.name.as_deref(), Some("Amis"));
        let requete = &serveur.requetes()[0];
        assert_eq!(requete.ligne(), ("/server-invites/AB%2F12/join", "POST"));
        assert!(requete.corps.is_empty());
        assert_eq!(requete.entete("content-type"), None);
    }

    #[tokio::test]
    async fn gere_les_204_sans_corps_et_les_chemins_encodes() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(204)]).await;
        let client = serveur.client();
        client.reagir("12", "👍").await.unwrap();
        client
            .revoquer_invitation_serveur("s1", "AB/CD")
            .await
            .unwrap();
        assert_eq!(
            lignes(&serveur),
            [
                ligne("/messages/12/reactions/%F0%9F%91%8D", "PUT"),
                ligne("/servers/s1/invites/AB%2FCD", "DELETE")
            ]
        );
    }

    #[tokio::test]
    async fn envoie_un_fichier_en_multipart() {
        let serveur =
            ServeurFactice::demarrer([Reponse::json(201, json!({"id": "f1", "kind": "voice"}))])
                .await;
        let client = serveur.client();
        let mut envoi = Envoi {
            nom: "a.txt".to_string(),
            type_mime: "text/plain".to_string(),
            contenu: Bytes::from_static(b"x"),
            vocal: None,
        };
        let piece = client.envoyer_fichier(&envoi).await.unwrap();
        assert_eq!(piece.id.as_deref(), Some("f1"));
        assert_eq!(piece.kind, Some(crate::types::AttachmentKind::Voice));
        envoi.vocal = Some(Vocal {
            duree_ms: 1500,
            forme_d_onde: vec![0, 50, 100],
        });
        client.envoyer_fichier(&envoi).await.unwrap();
        let requetes = serveur.requetes();
        assert_eq!(requetes[0].ligne(), ("/files", "POST"));
        let type_mime = requetes[0].entete("content-type").unwrap();
        let frontiere = type_mime
            .strip_prefix("multipart/form-data; boundary=")
            .unwrap();
        let simple = requetes[0].texte();
        assert!(simple.starts_with(&format!("--{frontiere}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.txt\"\r\n")));
        assert!(simple.ends_with(&format!("\r\n\r\nx\r\n--{frontiere}--\r\n")));
        let vocal = requetes[1].texte();
        let positions: Vec<usize> = [
            "name=\"kind\"\r\n\r\nvoice",
            "name=\"duration_ms\"\r\n\r\n1500",
            "name=\"waveform\"\r\n\r\n0,50,100",
            "name=\"file\"",
        ]
        .iter()
        .map(|champ| vocal.find(champ).unwrap())
        .collect();
        assert!(positions.is_sorted());
    }

    #[tokio::test]
    async fn construit_la_pagination() {
        let serveur = ServeurFactice::demarrer([Reponse::json(
            200,
            json!({"messages": [], "has_more": false}),
        )])
        .await;
        let client = serveur.client();
        let page = client
            .messages_du_salon(
                "c1",
                &Page {
                    limite: 30,
                    avant: Some("42".to_string()),
                    apres: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            (page.messages, page.has_more),
            (Some(Vec::new()), Some(false))
        );
        client
            .messages_prives("d1", &Page::default())
            .await
            .unwrap();
        client
            .messages_prives(
                "d1",
                &Page {
                    apres: Some("7".to_string()),
                    ..Page::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(
            lignes(&serveur),
            [
                ligne("/channels/c1/messages?limit=30&before=42", "GET"),
                ligne("/dms/d1/messages?limit=50", "GET"),
                ligne("/dms/d1/messages?limit=50&after=7", "GET"),
            ]
        );
    }

    #[tokio::test]
    async fn lit_les_pages_et_les_accuses_de_lecture_d_une_conversation() {
        let serveur = ServeurFactice::demarrer([Reponse::json(
            200,
            json!({"messages": [], "has_more": false}),
        )])
        .await;
        serveur.router(
            "GET",
            "/dms/d1/reads",
            Reponse::json(
                200,
                json!([{"user_id": "bob", "pseudo": "bob", "last_id": "7", "read_at": "x"}]),
            ),
        );
        let client = serveur.client();
        let page = Page {
            apres: Some("4".to_string()),
            ..Page::default()
        };
        client
            .messages_de(&CleConversation::Salon("c1".into()), &page)
            .await
            .unwrap();
        client
            .messages_de(&CleConversation::Prive("d1".into()), &Page::default())
            .await
            .unwrap();
        let lectures = client
            .lectures(&CleConversation::Prive("d1".into()))
            .await
            .unwrap();
        assert_eq!(
            (
                lectures[0].dm_id.as_deref(),
                lectures[0].user_id.as_str(),
                lectures[0].last_id.as_str()
            ),
            (Some("d1"), "bob", "7")
        );
        assert_eq!(
            lignes(&serveur),
            [
                ligne("/channels/c1/messages?limit=50&after=4", "GET"),
                ligne("/dms/d1/messages?limit=50", "GET"),
                ligne("/dms/d1/reads", "GET"),
            ]
        );
    }

    #[tokio::test]
    async fn apercu_de_lien_emojis_et_fichiers() {
        let serveur = ServeurFactice::demarrer([
            Reponse::json(
                200,
                json!({"url": "https://plbls.fr/", "title": "plbls", "site_name": "plbls.fr"}),
            ),
            Reponse::json(404, json!({"error": "no_preview"})),
            Reponse::json(
                200,
                json!([{"id": "e1", "server_id": "s1", "name": "chat", "created_by": "u"}]),
            ),
            Reponse::brute(200, "image/png", b"png"),
            Reponse::brute(200, "image/png", b"img"),
        ])
        .await;
        let client = serveur.client();
        let apercu = client
            .apercu_de_lien("https://plbls.fr/?a=1&b", Some("m 1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(apercu.title.as_deref(), Some("plbls"));
        assert_eq!(
            client
                .apercu_de_lien("https://plbls.fr/x", None)
                .await
                .unwrap(),
            None
        );
        let emojis = client.emojis_personnalises().await.unwrap();
        assert_eq!(emojis[0].name, "chat");
        assert_eq!(&client.image_d_emoji("e1").await.unwrap()[..], b"png");
        assert_eq!(&client.contenu_de_fichier("f/1").await.unwrap()[..], b"img");
        assert_eq!(
            lignes(&serveur),
            [
                ligne(
                    "/link-preview?url=https%3A%2F%2Fplbls.fr%2F%3Fa%3D1%26b&message=m%201",
                    "GET"
                ),
                ligne("/link-preview?url=https%3A%2F%2Fplbls.fr%2Fx", "GET"),
                ligne("/emojis", "GET"),
                ligne("/emojis/e1/image", "GET"),
                ligne("/files/f%2F1", "GET"),
            ]
        );
    }

    #[test]
    fn un_emoji_personnalise_du_serveur_courant_passe_avant_les_homonymes() {
        let e = |id: &str, serveur: &str, nom: &str| EmojiPersonnalise {
            id: id.into(),
            server_id: serveur.into(),
            name: nom.into(),
            created_by: String::new(),
        };
        let liste = [
            e("1", "s1", "chat"),
            e("2", "s2", "chat"),
            e("3", "s2", "chien"),
        ];
        assert_eq!(resoudre_emoji(&liste, "chat", Some("s2")).unwrap().id, "2");
        assert_eq!(resoudre_emoji(&liste, "CHAT", Some("s3")).unwrap().id, "1");
        assert_eq!(resoudre_emoji(&liste, "chat", None).unwrap().id, "1");
        assert!(resoudre_emoji(&liste, "loup", Some("s1")).is_none());
    }

    #[tokio::test]
    async fn construit_les_routes_des_badges_avec_le_pseudo_encode() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(204)]).await;
        let client = serveur.client();
        client.accorder_badge("b1", "a/b c").await.unwrap();
        client.retirer_badge("b1", "x").await.unwrap();
        assert_eq!(
            lignes(&serveur),
            [
                ligne("/admin/badges/b1/users/a%2Fb%20c", "PUT"),
                ligne("/admin/badges/b1/users/x", "DELETE")
            ]
        );
    }

    #[tokio::test]
    async fn lit_les_badges_d_un_utilisateur_avec_les_champs_absents_par_defaut() {
        let serveur = ServeurFactice::demarrer([Reponse::json(
            200,
            json!([{"id": "b1", "name": "Fondateur", "icon": "🏅", "color": "#d0688a", "icon_version": 0}, {"id": "b2"}]),
        )])
        .await;
        let badges = serveur.client().badges_de("u/1").await.unwrap();
        assert_eq!(lignes(&serveur), [ligne("/users/u%2F1/badges", "GET")]);
        assert_eq!(
            badges[0],
            Badge {
                id: "b1".into(),
                name: "Fondateur".into(),
                icon: "🏅".into(),
                color: "#d0688a".into(),
                ..Badge::default()
            }
        );
        assert_eq!(badges[1].name, "");
    }

    #[tokio::test]
    async fn construit_les_routes_de_modification_des_badges() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(204)]).await;
        let client = serveur.client();
        let valeurs = PostAdminBadgesRequete {
            name: "N".to_string(),
            color: "#000000".to_string(),
            description: Some(String::new()),
            sort_order: Some(1),
            ..PostAdminBadgesRequete::default()
        };
        assert_eq!(client.modifier_badge("b1", &valeurs).await.unwrap(), None);
        client
            .accorder_badges("b1", &["a".to_string(), "b".to_string()])
            .await
            .unwrap();
        client.supprimer_icone_de_badge("b1").await.unwrap();
        assert_eq!(
            lignes(&serveur),
            [
                ligne("/admin/badges/b1", "PUT"),
                ligne("/admin/badges/b1/users", "POST"),
                ligne("/admin/badges/b1/icon", "DELETE")
            ]
        );
        let requetes = serveur.requetes();
        assert_eq!(
            requetes[0].json(),
            json!({"name": "N", "color": "#000000", "description": "", "sort_order": 1})
        );
        assert_eq!(requetes[1].json(), json!({"pseudos": ["a", "b"]}));
    }

    #[tokio::test]
    async fn construit_les_routes_de_l_espace_d_administration() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(204)]).await;
        let client = serveur.client();
        client.comptes("a b").await.unwrap();
        client.suspendre_compte("u1", true).await.unwrap();
        client.suspendre_compte("u1", false).await.unwrap();
        client
            .supprimer_serveur_en_administrateur("s1")
            .await
            .unwrap();
        assert_eq!(
            lignes(&serveur),
            [
                ligne("/admin/users?q=a%20b", "GET"),
                ligne("/admin/users/u1/suspension", "PUT"),
                ligne("/admin/users/u1/suspension", "DELETE"),
                ligne("/admin/servers/s1", "DELETE"),
            ]
        );
    }

    #[tokio::test]
    async fn envoie_les_images_par_le_meme_chemin_que_les_autres_requetes() {
        let non_autorise = || Reponse::json(401, json!({"error": "unauthorized", "message": "x"}));
        let serveur = ServeurFactice::demarrer([non_autorise(), Reponse::vide(204)]).await;
        let pertes = Arc::new(AtomicUsize::new(0));
        let refus = Arc::new(AtomicUsize::new(0));
        let (p, r) = (pertes.clone(), refus.clone());
        let client = serveur
            .client()
            .a_la_perte_de_session(move || {
                p.fetch_add(1, Ordering::SeqCst);
            })
            .a_la_mise_a_jour_requise(move |message| {
                assert_eq!(message, "trop vieux");
                r.fetch_add(1, Ordering::SeqCst);
            });
        let image = Bytes::from_static(b"\x89PNG x");
        client
            .definir_avatar("image/png", image.clone())
            .await
            .unwrap();
        let requetes = serveur.requetes();
        assert_eq!(requetes.len(), 2);
        assert_eq!(requetes[1].ligne(), ("/me/avatar", "PUT"));
        assert_eq!(
            (
                requetes[1].corps.as_slice(),
                requetes[1].entete("content-type")
            ),
            (&image[..], Some("image/png"))
        );
        assert_eq!(pertes.load(Ordering::SeqCst), 0);
        serveur.programmer([non_autorise()]);
        assert_eq!(
            client
                .definir_avatar_de_serveur("s1", "image/png", image.clone())
                .await
                .unwrap_err()
                .statut(),
            401
        );
        assert_eq!(
            client.avatar("u1", Some("s1")).await.unwrap_err().statut(),
            401
        );
        assert_eq!(pertes.load(Ordering::SeqCst), 2);
        serveur.programmer([Reponse::json(
            426,
            json!({"error": "upgrade_required", "message": "trop vieux"}),
        )]);
        assert_eq!(
            client
                .definir_fond_de_camera("image/png", image)
                .await
                .unwrap_err()
                .statut(),
            426
        );
        assert_eq!(
            client.icone_de_serveur("s1").await.unwrap_err().statut(),
            426
        );
        assert_eq!(refus.load(Ordering::SeqCst), 2);
        let chemins: Vec<String> = lignes(&serveur)
            .into_iter()
            .skip(2)
            .map(|(chemin, _)| chemin)
            .collect();
        assert_eq!(
            chemins,
            [
                "/servers/s1/me/avatar",
                "/servers/s1/me/avatar",
                "/servers/s1/users/u1/avatar",
                "/servers/s1/users/u1/avatar",
                "/me/camera-background",
                "/servers/s1/icon",
            ]
        );
    }

    #[tokio::test]
    async fn construit_les_routes_du_store() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(204)]).await;
        let client = serveur.client();
        client
            .marquer_lu(&CleConversation::Salon("c1".into()))
            .await
            .unwrap();
        client
            .marquer_lu(&CleConversation::Prive("d1".into()))
            .await
            .unwrap();
        client.lire_notifications(&["7".to_string()]).await.unwrap();
        client.definir_statut(StatutPresence::Dnd).await.unwrap();
        client.definir_activite("Hades").await.unwrap();
        client
            .enregistrer_reglages_notification(&NotifySettings {
                sound: Some(false),
                ..NotifySettings::default()
            })
            .await
            .unwrap();
        client.definir_banniere("#aa00ff").await.unwrap();
        client.supprimer_avatar().await.unwrap();
        client.supprimer_avatar_de_serveur("s1").await.unwrap();
        assert_eq!(
            lignes(&serveur),
            [
                ligne("/channels/c1/read", "POST"),
                ligne("/dms/d1/read", "POST"),
                ligne("/notifications/read", "POST"),
                ligne("/me/status", "PUT"),
                ligne("/me/activity", "PUT"),
                ligne("/me/notify-settings", "PUT"),
                ligne("/me/banner", "PUT"),
                ligne("/me/avatar", "DELETE"),
                ligne("/servers/s1/me/avatar", "DELETE"),
            ]
        );
        let requetes = serveur.requetes();
        assert_eq!(requetes[2].json(), json!({"message_ids": ["7"]}));
        assert_eq!(requetes[3].json(), json!({"status": "dnd"}));
        assert_eq!(requetes[4].json(), json!({"name": "Hades"}));
        assert_eq!(requetes[5].json(), json!({"sound": false}));
        assert_eq!(requetes[6].json(), json!({"color": "#aa00ff"}));
        serveur.programmer([Reponse::json(
            200,
            json!({"u1": "idle", "u2": "dnd", "u3": "invisible"}),
        )]);
        let statuts = client.statuts().await.unwrap();
        assert_eq!(
            statuts.values().copied().collect::<Vec<_>>(),
            [
                StatutPresence::Idle,
                StatutPresence::Dnd,
                StatutPresence::Inconnu
            ]
        );
    }

    #[tokio::test]
    async fn telecharge_un_avatar() {
        let serveur =
            ServeurFactice::demarrer([Reponse::brute(200, "image/png", b"\x89PNG")]).await;
        let client = serveur.client();
        assert_eq!(&client.avatar("u1", None).await.unwrap()[..], b"\x89PNG");
        assert_eq!(serveur.requetes()[0].ligne(), ("/users/u1/avatar", "GET"));
        serveur.programmer([Reponse::brute(200, "image/svg+xml", b"<svg/>")]);
        assert_eq!(&client.icone_de_badge("b 1").await.unwrap()[..], b"<svg/>");
        assert_eq!(serveur.requetes()[1].ligne(), ("/badges/b%201/icon", "GET"));
    }
}
