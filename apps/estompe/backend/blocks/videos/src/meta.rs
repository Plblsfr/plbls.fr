use estompe_block_apercu_liens::{Erreur, Fetcher};
use estompe_block_musique::{Musique, est_id_video};

use crate::source::{Source, analyser, titre_de_repli};

const MINIATURE_MAX: usize = 200 << 10;
const LISTE_INDISPONIBLE: &str = "Playlist privée ou indisponible";

pub struct Bases {
    pub oembed_youtube: String,
    pub oembed_vimeo: String,
    pub oembed_dailymotion: String,
    pub miniatures: String,
}

impl Bases {
    pub fn production() -> Self {
        Self {
            oembed_youtube: "https://www.youtube.com/oembed".into(),
            oembed_vimeo: "https://vimeo.com/api/oembed.json".into(),
            oembed_dailymotion: "https://www.dailymotion.com/services/oembed".into(),
            miniatures: "https://i.ytimg.com".into(),
        }
    }

    fn oembed(&self, fournisseur: &str) -> Option<&str> {
        match fournisseur {
            "youtube" => Some(&self.oembed_youtube),
            "vimeo" => Some(&self.oembed_vimeo),
            "dailymotion" => Some(&self.oembed_dailymotion),
            _ => None,
        }
    }

    fn miniature(&self, id: &str) -> String {
        format!("{}/vi/{id}/hqdefault.jpg", self.miniatures)
    }
}

pub struct Metadonnees {
    pub provider: String,
    pub reference: String,
    pub titre: String,
    pub site: String,
    pub miniature: String,
    pub echec: bool,
    pub erreur: Option<String>,
}

pub struct Moteur {
    pub fetcher: Fetcher,
    pub musique: Musique,
    pub bases: Bases,
}

fn duree(secondes: u32) -> String {
    match secondes {
        0 => String::new(),
        s if s >= 3600 => format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60),
        s => format!("{}:{:02}", s / 60, s % 60),
    }
}

fn joindre(parties: &[&str]) -> String {
    parties
        .iter()
        .filter(|p| !p.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" · ")
}

impl Moteur {
    async fn image_carte(&self, url: &str) -> String {
        if url.is_empty() {
            return String::new();
        }
        let miniature = self.fetcher.image(url).await;
        if miniature.len() <= MINIATURE_MAX {
            miniature
        } else {
            String::new()
        }
    }

    pub async fn carte_musique(
        &self,
        cle: &str,
        reference: &str,
    ) -> (String, String, String, Option<String>) {
        if est_id_video(reference) {
            let pistes = self.musique.pistes(cle, &[reference.to_string()]).await;
            let meta = pistes
                .into_iter()
                .next()
                .map(|p| p.meta)
                .unwrap_or_default();
            let titre = if meta.title.is_empty() {
                "Titre YouTube Music".to_string()
            } else {
                meta.title.clone()
            };
            let site = joindre(&[&meta.artist, &meta.album, &duree(meta.duration)]);
            let miniature = self.image_carte(&self.bases.miniature(reference)).await;
            return (titre, site, miniature, None);
        }
        self.carte_liste(cle, reference).await
    }

    async fn carte_liste(&self, cle: &str, id: &str) -> (String, String, String, Option<String>) {
        let genre = if id.starts_with("OLAK5uy_") {
            "Album"
        } else {
            "Playlist"
        };
        let (titre, auteur, total, couverture, erreur) =
            match self.musique.carte_liste(cle, id).await {
                Ok(l) => {
                    let couverture = l
                        .ids
                        .first()
                        .map(|v| self.bases.miniature(v))
                        .unwrap_or_default();
                    (l.titre, l.auteur, l.total, couverture, None)
                }
                Err(e) => {
                    let cible = format!("https://www.youtube.com/playlist?list={id}");
                    match self
                        .fetcher
                        .oembed(&self.bases.oembed_youtube, &cible)
                        .await
                    {
                        Ok(o) if !o.title.is_empty() => {
                            (o.title, o.author_name, 0, o.thumbnail_url, None)
                        }
                        _ => {
                            return (
                                LISTE_INDISPONIBLE.to_string(),
                                String::new(),
                                String::new(),
                                Some(e.to_string()),
                            );
                        }
                    }
                }
            };
        let nombre = if total > 0 {
            format!("{total} titres")
        } else {
            String::new()
        };
        let site = joindre(&[genre, &auteur, &nombre]);
        let miniature = self.image_carte(&couverture).await;
        (titre, site, miniature, erreur)
    }

    pub async fn recuperer(&self, cle: &str, src: &Source) -> Metadonnees {
        let mut m = Metadonnees {
            provider: src.provider.clone(),
            reference: src.reference.clone(),
            titre: titre_de_repli(&src.provider).to_string(),
            site: String::new(),
            miniature: String::new(),
            echec: false,
            erreur: None,
        };
        if src.provider == "ytmusic" {
            let (titre, site, miniature, erreur) = self.carte_musique(cle, &src.reference).await;
            if !titre.is_empty() {
                m.titre = titre;
            }
            m.echec = erreur.is_some();
            (m.site, m.miniature, m.erreur) = (site, miniature, erreur);
            return m;
        }
        if let Some(endpoint) = self.bases.oembed(&src.provider) {
            match self.fetcher.oembed(endpoint, &src.url).await {
                Ok(o) => {
                    if !o.title.is_empty() {
                        m.titre = o.title;
                    }
                    m.site = o.author_name;
                    if !o.thumbnail_url.is_empty() {
                        m.miniature = self.fetcher.image(&o.thumbnail_url).await;
                    }
                }
                Err(e) => {
                    m.echec = true;
                    m.erreur = Some(e.to_string());
                }
            }
            if m.miniature.is_empty() && src.provider == "youtube" {
                m.miniature = self
                    .fetcher
                    .image(&self.bases.miniature(&src.reference))
                    .await;
            }
            if m.miniature.len() > MINIATURE_MAX {
                m.miniature.clear();
            }
            return m;
        }
        match self.fetcher.fetch(&src.url).await {
            Err(e) => {
                m.echec = true;
                m.erreur = (e != Erreur::SansApercu).then(|| e.to_string());
            }
            Ok(p) => {
                if !p.title.is_empty() {
                    m.titre = p.title;
                }
                m.site = p.site_name;
                m.miniature = if p.image.len() > MINIATURE_MAX {
                    String::new()
                } else {
                    p.image
                };
                if m.provider == "simple"
                    && let Some(v) = analyser(&p.video)
                    && v.provider == "file"
                {
                    m.provider = "page".into();
                    m.reference = v.reference;
                }
            }
        }
        m
    }
}
