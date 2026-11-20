use bytes::Bytes;
use reqwest::Method;
use serde_json::Value;

use crate::http::{encoder_composant, Client, Corps, ErreurApi, Partie};
use crate::types::{
    Attachment, GetMeReponse, MessagePage, Name50, Person, PostActivateRequete,
    PostAdminBadgesIdUsersRequete, PostAdminBadgesRequete, Server,
};

pub const LIMITE_MESSAGES: u32 = 50;

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

    pub async fn serveurs(&self) -> Result<Vec<Server>, ErreurApi> {
        self.get("/servers").await
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

    pub async fn reagir(&self, message: &str, emoji: &str) -> Result<(), ErreurApi> {
        let chemin = format!("/messages/{message}/reactions/{}", encoder_composant(emoji));
        self.envoyer(Method::PUT, &chemin, &Corps::Aucun).await
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
    async fn telecharge_un_avatar() {
        let serveur =
            ServeurFactice::demarrer([Reponse::brute(200, "image/png", b"\x89PNG")]).await;
        let client = serveur.client();
        assert_eq!(&client.avatar("u1", None).await.unwrap()[..], b"\x89PNG");
        assert_eq!(serveur.requetes()[0].ligne(), ("/users/u1/avatar", "GET"));
    }
}
