use base64::Engine;
use reqwest::Method;
use serde::Deserialize;
use serde_json::json;

use crate::http::{encoder_composant, Client, Corps, ErreurApi};

use super::{ListeVideos, NouvelleSeance, Rsvp, SeanceVideos, TableauVideos};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct MorceauResolu {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub cover: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct MusiqueResolue {
    pub title: String,
    pub tracks: Vec<MorceauResolu>,
}

#[derive(Deserialize)]
struct Miniature {
    #[serde(default)]
    thumb: String,
}

fn chemin(salon: &str, suite: &str) -> String {
    format!("/channels/{}/videos{suite}", encoder_composant(salon))
}

pub fn octets_d_une_adresse_de_donnees(adresse: &str) -> Option<Vec<u8>> {
    let (entete, donnees) = adresse.strip_prefix("data:")?.split_once(',')?;
    if !entete.ends_with(";base64") {
        return None;
    }
    base64::engine::general_purpose::STANDARD
        .decode(donnees.trim())
        .ok()
}

impl Client {
    pub async fn videos(&self, salon: &str) -> Result<TableauVideos, ErreurApi> {
        self.get(&chemin(salon, "")).await
    }

    pub async fn miniature_video(&self, salon: &str, message: &str) -> Result<Vec<u8>, ErreurApi> {
        let m: Miniature = self
            .get(&chemin(
                salon,
                &format!("/posts/{}/thumb", encoder_composant(message)),
            ))
            .await?;
        octets_d_une_adresse_de_donnees(&m.thumb)
            .ok_or_else(|| ErreurApi::ReponseInvalide("miniature illisible".into()))
    }

    pub async fn marquer_video_vue(
        &self,
        salon: &str,
        message: &str,
        vue: bool,
    ) -> Result<(), ErreurApi> {
        let corps = Corps::json(&json!({ "watched": vue }))?;
        self.envoyer(
            Method::PUT,
            &chemin(
                salon,
                &format!("/posts/{}/watched", encoder_composant(message)),
            ),
            &corps,
        )
        .await
    }

    pub async fn creer_liste_videos(
        &self,
        salon: &str,
        nom: &str,
        collaborative: bool,
    ) -> Result<ListeVideos, ErreurApi> {
        self.post(
            &chemin(salon, "/lists"),
            &json!({ "name": nom, "collaborative": collaborative }),
        )
        .await
    }

    pub async fn modifier_liste_videos(
        &self,
        salon: &str,
        liste: &str,
        nom: &str,
        collaborative: Option<bool>,
    ) -> Result<(), ErreurApi> {
        let corps = match collaborative {
            Some(c) => json!({ "name": nom, "collaborative": c }),
            None => json!({ "name": nom }),
        };
        self.envoyer(
            Method::PUT,
            &chemin(salon, &format!("/lists/{}", encoder_composant(liste))),
            &Corps::json(&corps)?,
        )
        .await
    }

    pub async fn supprimer_liste_videos(&self, salon: &str, liste: &str) -> Result<(), ErreurApi> {
        self.delete(&chemin(
            salon,
            &format!("/lists/{}", encoder_composant(liste)),
        ))
        .await
    }

    pub async fn elements_liste_videos(
        &self,
        salon: &str,
        liste: &str,
        ids: &[String],
    ) -> Result<(), ErreurApi> {
        self.envoyer(
            Method::PUT,
            &chemin(salon, &format!("/lists/{}/items", encoder_composant(liste))),
            &Corps::json(&json!({ "ids": ids }))?,
        )
        .await
    }

    pub async fn planifier_seance_videos(
        &self,
        salon: &str,
        seance: &NouvelleSeance,
    ) -> Result<SeanceVideos, ErreurApi> {
        self.post(&chemin(salon, "/sessions"), seance).await
    }

    pub async fn annuler_seance_videos(&self, salon: &str, seance: &str) -> Result<(), ErreurApi> {
        self.delete(&chemin(
            salon,
            &format!("/sessions/{}", encoder_composant(seance)),
        ))
        .await
    }

    pub async fn repondre_seance_videos(
        &self,
        salon: &str,
        seance: &str,
        reponse: Rsvp,
    ) -> Result<(), ErreurApi> {
        self.envoyer(
            Method::PUT,
            &chemin(
                salon,
                &format!("/sessions/{}/rsvp", encoder_composant(seance)),
            ),
            &Corps::json(&json!({ "answer": reponse.cle() }))?,
        )
        .await
    }

    pub async fn resoudre_musique(&self, lien: &str) -> Result<MusiqueResolue, ErreurApi> {
        self.get(&format!("/music/resolve?url={}", encoder_composant(lien)))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serveur_factice::{Reponse, ServeurFactice};
    use crate::videos::{nouvelle_seance, FileSeance};

    #[test]
    fn une_adresse_de_donnees_donne_ses_octets() {
        assert_eq!(
            octets_d_une_adresse_de_donnees("data:image/png;base64,AQID"),
            Some(vec![1, 2, 3])
        );
        assert_eq!(octets_d_une_adresse_de_donnees("data:text/plain,abc"), None);
        assert_eq!(octets_d_une_adresse_de_donnees("https://x.fr/a.png"), None);
    }

    #[tokio::test]
    async fn les_appels_du_salon_videos_partent_aux_bons_chemins() {
        let serveur = ServeurFactice::demarrer([
            Reponse::json(
                200,
                json!({"posts": [{"message_id": "10", "provider": "page", "ref": "https://x.fr/a.mp4"}], "lists": [], "sessions": []}),
            ),
            Reponse::json(200, json!({"thumb": "data:image/png;base64,AQID"})),
            Reponse::vide(204),
            Reponse::json(
                201,
                json!({"id": "l1", "name": "À voir", "created_by": "u1", "collaborative": true, "items": []}),
            ),
            Reponse::vide(204),
            Reponse::vide(204),
            Reponse::vide(204),
            Reponse::json(
                201,
                json!({"id": "s1", "title": "Soirée", "starts_at": "2026-10-28T19:30:00Z", "voice_channel_id": "v1", "remind_minutes": 10, "videos": ["10"], "created_by": "u1", "started_at": null, "rsvps": {}}),
            ),
            Reponse::vide(204),
            Reponse::vide(204),
            Reponse::json(
                200,
                json!({"title": "Album", "tracks": [{"id": "aaaaaaaaaaa", "title": "Un", "artist": "Art", "cover": "/music/cover/aaaaaaaaaaa"}]}),
            ),
            Reponse::vide(204),
        ])
        .await;
        let client = serveur.client();
        let t = client.videos("w1").await.unwrap();
        assert_eq!(t.posts[0].reference, "https://x.fr/a.mp4");
        assert_eq!(client.miniature_video("w1", "10").await.unwrap(), [1, 2, 3]);
        client.marquer_video_vue("w1", "10", true).await.unwrap();
        let l = client
            .creer_liste_videos("w1", "À voir", true)
            .await
            .unwrap();
        assert_eq!(l.id, "l1");
        client
            .modifier_liste_videos("w1", "l1", "Plus tard", Some(false))
            .await
            .unwrap();
        client
            .elements_liste_videos("w1", "l1", &["10".into(), "8".into()])
            .await
            .unwrap();
        client.supprimer_liste_videos("w1", "l1").await.unwrap();
        let s = nouvelle_seance(
            "Soirée",
            crate::roles::date_en_millisecondes("2026-10-28T19:30:00Z"),
            "v1",
            10,
            &FileSeance::Liste("l1".into()),
        )
        .unwrap();
        assert_eq!(
            client.planifier_seance_videos("w1", &s).await.unwrap().id,
            "s1"
        );
        client
            .repondre_seance_videos("w1", "s1", Rsvp::PeutEtre)
            .await
            .unwrap();
        client.annuler_seance_videos("w1", "s1").await.unwrap();
        let m = client
            .resoudre_musique("https://music.youtube.com/playlist?list=OLAK5uy_abcdefghij")
            .await
            .unwrap();
        assert_eq!(m.tracks[0].artist, "Art");
        client
            .modifier_liste_videos("w1", "l2", "Nom", None)
            .await
            .unwrap();
        let r = serveur.requetes();
        assert_eq!(r[0].ligne(), ("/channels/w1/videos", "GET"));
        assert_eq!(r[1].ligne(), ("/channels/w1/videos/posts/10/thumb", "GET"));
        assert_eq!(
            r[2].ligne(),
            ("/channels/w1/videos/posts/10/watched", "PUT")
        );
        assert_eq!(r[2].json(), json!({"watched": true}));
        assert_eq!(r[3].ligne(), ("/channels/w1/videos/lists", "POST"));
        assert_eq!(
            r[3].json(),
            json!({"name": "À voir", "collaborative": true})
        );
        assert_eq!(r[4].ligne(), ("/channels/w1/videos/lists/l1", "PUT"));
        assert_eq!(
            r[4].json(),
            json!({"name": "Plus tard", "collaborative": false})
        );
        assert_eq!(r[5].ligne(), ("/channels/w1/videos/lists/l1/items", "PUT"));
        assert_eq!(r[5].json(), json!({"ids": ["10", "8"]}));
        assert_eq!(r[6].ligne(), ("/channels/w1/videos/lists/l1", "DELETE"));
        assert_eq!(r[7].ligne(), ("/channels/w1/videos/sessions", "POST"));
        assert_eq!(r[7].json()["list_id"], "l1");
        assert_eq!(
            r[8].ligne(),
            ("/channels/w1/videos/sessions/s1/rsvp", "PUT")
        );
        assert_eq!(r[8].json(), json!({"answer": "maybe"}));
        assert_eq!(r[9].ligne(), ("/channels/w1/videos/sessions/s1", "DELETE"));
        assert_eq!(
            r[10].ligne(),
            (
                "/music/resolve?url=https%3A%2F%2Fmusic.youtube.com%2Fplaylist%3Flist%3DOLAK5uy_abcdefghij",
                "GET"
            )
        );
        assert_eq!(r[11].json(), json!({"name": "Nom"}));
    }
}
