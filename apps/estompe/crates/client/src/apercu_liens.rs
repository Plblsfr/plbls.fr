use crate::route::lien_musique;
use crate::types::LinkPreview;
use serde::Deserialize;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoYoutube {
    pub id: String,
    pub debut: u64,
}

fn chiffres(texte: &str) -> Option<(u64, &str)> {
    let fin = texte
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(texte.len());
    if fin == 0 {
        return None;
    }
    let valeur = texte[..fin].parse::<u64>().unwrap_or(u64::MAX);
    Some((valeur, &texte[fin..]))
}

fn temps_de_depart(texte: &str) -> u64 {
    let mut reste = texte;
    let lire = |lettre: char, reste: &mut &str| -> u64 {
        match chiffres(reste) {
            Some((v, apres)) if apres.starts_with(lettre) => {
                *reste = &apres[lettre.len_utf8()..];
                v
            }
            _ => 0,
        }
    };
    let heures = lire('h', &mut reste);
    let minutes = lire('m', &mut reste);
    let secondes = match chiffres(reste) {
        Some((v, apres)) => {
            reste = apres.strip_prefix('s').unwrap_or(apres);
            v
        }
        None => 0,
    };
    if !reste.is_empty() {
        return 0;
    }
    heures
        .saturating_mul(3600)
        .saturating_add(minutes.saturating_mul(60))
        .saturating_add(secondes)
}

fn identifiant_valide(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|o| o.is_ascii_alphanumeric() || o == b'_' || o == b'-')
}

pub fn video_youtube(lien: &str) -> Option<VideoYoutube> {
    let url = Url::parse(lien).ok()?;
    let hote = url.host_str().unwrap_or_default();
    let hote = hote
        .strip_prefix("www.")
        .or_else(|| hote.strip_prefix("m."))
        .unwrap_or(hote);
    let chemin = url.path();
    let id: Option<String> = if hote == "youtu.be" {
        chemin
            .strip_prefix('/')
            .and_then(|c| c.split('/').next())
            .map(str::to_string)
    } else if hote == "youtube.com" {
        let direct = ["/shorts/", "/embed/"]
            .iter()
            .find_map(|p| chemin.strip_prefix(p))
            .and_then(|c| c.split('/').next())
            .filter(|c| !c.is_empty())
            .map(str::to_string);
        direct.or_else(|| {
            if chemin == "/watch" {
                url.query_pairs()
                    .find(|(k, _)| k == "v")
                    .map(|(_, v)| v.into_owned())
            } else {
                None
            }
        })
    } else {
        None
    };
    let id = id.filter(|i| identifiant_valide(i))?;
    let t = url
        .query_pairs()
        .find(|(k, _)| k == "t")
        .or_else(|| url.query_pairs().find(|(k, _)| k == "start"))
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default();
    Some(VideoYoutube {
        id,
        debut: temps_de_depart(&t),
    })
}

pub fn url_integree(video: &VideoYoutube) -> String {
    format!(
        "https://www.youtube-nocookie.com/embed/{}?autoplay=1&start={}",
        video.id, video.debut
    )
}

pub fn url_miniature(video: &VideoYoutube) -> String {
    format!("https://img.youtube.com/vi/{}/hqdefault.jpg", video.id)
}

pub const TAILLE_DU_CACHE: usize = 200;

pub fn cle_de_cache(message: Option<&str>, lien: &str) -> String {
    format!("{} {}", message.unwrap_or_default(), lien)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum GenreSillon {
    #[serde(rename = "album")]
    Album,
    #[serde(rename = "track")]
    Titre,
    #[serde(rename = "artist")]
    Artiste,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CouvertureSillon {
    pub id: String,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CarteSillon {
    #[serde(rename = "type")]
    pub genre: GenreSillon,
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub cover: Option<CouvertureSillon>,
    #[serde(default)]
    pub listen: bool,
}

impl CarteSillon {
    pub fn libelle_du_genre(&self) -> &'static str {
        match self.genre {
            GenreSillon::Album => "Album",
            GenreSillon::Titre => "Titre",
            GenreSillon::Artiste => "Artiste",
        }
    }

    pub fn lien_d_ecoute(&self) -> String {
        if self.genre == GenreSillon::Artiste {
            lien_musique(Some("artist"), Some(&self.id))
        } else {
            lien_musique(
                Some("album"),
                Some(self.album.as_deref().unwrap_or(&self.id)),
            )
        }
    }

    pub fn afficher_ecouter(&self) -> bool {
        self.listen
    }

    pub fn afficher_artiste(&self) -> bool {
        self.genre != GenreSillon::Artiste
    }

    pub fn identifiant_de_pochette(&self) -> &str {
        self.cover.as_ref().map_or(&self.id, |c| &c.id)
    }

    pub fn pochette_en_image(&self) -> bool {
        self.cover.is_some()
    }

    pub fn type_de_pochette(&self) -> &'static str {
        match self.cover.as_ref().and_then(|c| c.kind.as_deref()) {
            Some("artist") => "artiste",
            _ => "album",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarteLien {
    pub url: String,
    pub titre: String,
    pub description: Option<String>,
    pub site: Option<String>,
    pub image: Option<String>,
}

impl CarteLien {
    pub fn avec_image(&self) -> bool {
        self.image.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Carte {
    Youtube {
        video: VideoYoutube,
        infos: Option<CarteLien>,
    },
    Sillon(CarteSillon),
    Lien(CarteLien),
    Aucune,
}

fn non_vide(valeur: &Option<String>) -> Option<String> {
    valeur.clone().filter(|v| !v.is_empty())
}

pub fn carte_de_lien(apercu: &LinkPreview) -> CarteLien {
    CarteLien {
        url: apercu.url.clone().unwrap_or_default(),
        titre: apercu.title.clone().unwrap_or_default(),
        description: non_vide(&apercu.description),
        site: non_vide(&apercu.site_name),
        image: non_vide(&apercu.image),
    }
}

pub fn choisir_carte(lien: &str, apercu: Option<&LinkPreview>) -> Carte {
    if let Some(video) = video_youtube(lien) {
        return Carte::Youtube {
            video,
            infos: apercu.map(carte_de_lien),
        };
    }
    let Some(apercu) = apercu else {
        return Carte::Aucune;
    };
    if let Some(sillon) = apercu
        .sillon
        .as_ref()
        .and_then(|v| serde_json::from_value::<CarteSillon>(v.clone()).ok())
    {
        return Carte::Sillon(sillon);
    }
    Carte::Lien(carte_de_lien(apercu))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EtatYoutube {
    lecture: bool,
    introuvable: bool,
    agrandi: bool,
}

impl EtatYoutube {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn visible(&self) -> bool {
        !self.introuvable
    }

    pub fn lecteur_charge(&self) -> bool {
        self.lecture
    }

    pub fn agrandi(&self) -> bool {
        self.agrandi
    }

    pub fn lire(&mut self) {
        self.lecture = true;
    }

    pub fn agrandir(&mut self) {
        self.lecture = false;
        self.agrandi = true;
    }

    pub fn fermer_agrandi(&mut self) {
        self.agrandi = false;
    }

    pub fn miniature_introuvable(&mut self) {
        self.introuvable = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ID: &str = "dQw4w9WgXcQ";

    fn aucune_video(u: &str) -> bool {
        video_youtube(u).is_none()
    }

    #[test]
    fn reconnait_les_formats_youtube() {
        for u in [
            format!("https://www.youtube.com/watch?v={ID}"),
            format!("https://youtu.be/{ID}"),
            format!("https://youtube.com/shorts/{ID}"),
            format!("https://m.youtube.com/embed/{ID}"),
        ] {
            assert_eq!(
                video_youtube(&u),
                Some(VideoYoutube {
                    id: ID.to_string(),
                    debut: 0
                }),
                "{u}"
            );
        }
    }

    #[test]
    fn lit_le_temps_de_depart() {
        assert_eq!(
            video_youtube("https://youtu.be/dQw4w9WgXcQ?t=90").map(|v| v.debut),
            Some(90)
        );
        assert_eq!(
            video_youtube("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1h2m3s").map(|v| v.debut),
            Some(3723)
        );
    }

    #[test]
    fn ignore_le_reste() {
        for u in [
            "https://example.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/court",
            "https://www.youtube.com/",
            "pas un lien",
        ] {
            assert!(aucune_video(u), "{u}");
        }
    }

    #[test]
    fn variantes_du_temps_de_depart() {
        let debut = |q: &str| video_youtube(&format!("https://youtu.be/{ID}{q}")).map(|v| v.debut);
        assert_eq!(debut("?start=42"), Some(42));
        assert_eq!(debut("?t=90s"), Some(90));
        assert_eq!(debut("?t=1h"), Some(3600));
        assert_eq!(debut("?t=2m"), Some(120));
        assert_eq!(debut("?t=1m30"), Some(90));
        assert_eq!(debut("?t=1h5"), Some(3605));
        assert_eq!(debut("?t="), Some(0));
        assert_eq!(debut("?t=abc"), Some(0));
        assert_eq!(debut("?t=1h2x"), Some(0));
        assert_eq!(debut("?t=m"), Some(0));
        assert_eq!(debut("?t=10&start=20"), Some(10));
        assert_eq!(debut("?t=&start=20"), Some(0));
        assert_eq!(debut("?start=20"), Some(20));
        assert_eq!(debut("?t=99999999999999999999999"), Some(u64::MAX));
        assert_eq!(debut("?t=99999999999999999999999h"), Some(u64::MAX));
    }

    #[test]
    fn variantes_de_formes_de_lien() {
        let id = |u: &str| video_youtube(u).map(|v| v.id);
        assert_eq!(
            id(&format!("https://youtu.be/{ID}/suite")),
            Some(ID.to_string())
        );
        assert_eq!(
            id(&format!("https://youtube.com/embed/{ID}/x")),
            Some(ID.to_string())
        );
        assert_eq!(id(&format!("http://YOUTU.BE/{ID}")), Some(ID.to_string()));
        assert_eq!(
            id(&format!("https://www.youtube.com/watch?v={ID}&list=x")),
            Some(ID.to_string())
        );
        assert_eq!(
            id(&format!("https://www.youtube.com/watch?x=1&v={ID}")),
            Some(ID.to_string())
        );
        assert_eq!(id(&format!("https://www.youtube.com/watch/{ID}")), None);
        assert_eq!(id(&format!("https://www.youtube.com/live/{ID}")), None);
        assert_eq!(id(&format!("https://music.youtube.com/watch?v={ID}")), None);
        assert_eq!(id(&format!("https://notyoutu.be/{ID}")), None);
        assert_eq!(id(&format!("https://youtu.be.evil.fr/{ID}")), None);
        assert_eq!(id("https://www.youtube.com/watch?v=trop_long_xxxx"), None);
        assert_eq!(id("https://www.youtube.com/watch?v=abc$defghij"), None);
        assert_eq!(id("https://youtu.be/"), None);
        assert_eq!(id("https://youtube.com/shorts/"), None);
        assert_eq!(id(&format!("mailto:{ID}")), None);
        assert_eq!(id(""), None);
    }

    #[test]
    fn adresses_de_lecteur_et_de_miniature() {
        let v = video_youtube("https://youtu.be/dQw4w9WgXcQ?t=5").unwrap();
        assert_eq!(
            url_integree(&v),
            "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?autoplay=1&start=5"
        );
        assert!(url_miniature(&v).contains("/vi/dQw4w9WgXcQ/"));
        assert_eq!(
            url_miniature(&v),
            "https://img.youtube.com/vi/dQw4w9WgXcQ/hqdefault.jpg"
        );
    }

    fn apercu(valeur: serde_json::Value) -> LinkPreview {
        serde_json::from_value(valeur).unwrap()
    }

    #[test]
    fn la_carte_youtube_affiche_le_titre_de_la_video() {
        let p = apercu(
            json!({"url": "https://youtu.be/aaaaaaaaaaa", "title": "Ma vidéo", "site_name": "YouTube"}),
        );
        match choisir_carte("https://youtu.be/aaaaaaaaaaa", Some(&p)) {
            Carte::Youtube { video, infos } => {
                assert_eq!(video.id, "aaaaaaaaaaa");
                let infos = infos.unwrap();
                assert_eq!(infos.titre, "Ma vidéo");
                assert_eq!(infos.site.as_deref(), Some("YouTube"));
                assert_eq!(infos.description, None);
            }
            autre => panic!("carte inattendue : {autre:?}"),
        }
        assert!(matches!(
            choisir_carte("https://youtu.be/aaaaaaaaaaa", None),
            Carte::Youtube { infos: None, .. }
        ));
    }

    #[test]
    fn la_miniature_ne_charge_le_lecteur_qu_au_clic() {
        let mut e = EtatYoutube::new();
        assert!(e.visible());
        assert!(!e.lecteur_charge());
        e.lire();
        assert!(e.lecteur_charge());
    }

    #[test]
    fn agrandir_ouvre_la_fenetre_et_la_fermeture_la_referme() {
        let mut e = EtatYoutube::new();
        e.lire();
        e.agrandir();
        assert!(e.agrandi());
        assert!(!e.lecteur_charge());
        e.fermer_agrandi();
        assert!(!e.agrandi());
        assert!(!e.lecteur_charge());
    }

    #[test]
    fn une_miniature_introuvable_masque_la_carte() {
        let mut e = EtatYoutube::new();
        e.miniature_introuvable();
        assert!(!e.visible());
    }

    fn carte_sillon(extra: serde_json::Value) -> LinkPreview {
        let mut sillon = json!({"type": "album", "id": "a1", "title": "Lemonade", "artist": "Beyoncé", "album": "a1", "cover": null, "listen": true});
        for (k, v) in extra.as_object().unwrap() {
            sillon[k] = v.clone();
        }
        apercu(
            json!({"url": "https://estompe.plbls.fr/#/musique/album/a1", "title": "Lemonade", "description": "Beyoncé", "site_name": "Sillon", "sillon": sillon}),
        )
    }

    fn sillon(p: &LinkPreview, lien: &str) -> CarteSillon {
        match choisir_carte(lien, Some(p)) {
            Carte::Sillon(s) => s,
            autre => panic!("carte inattendue : {autre:?}"),
        }
    }

    #[test]
    fn la_carte_sillon_affiche_titre_artiste_et_ecouter_ouvre_la_section() {
        let lien = "https://estompe.plbls.fr/#/musique/album/a1";
        let s = sillon(&carte_sillon(json!({})), lien);
        assert_eq!(s.title, "Lemonade");
        assert_eq!(s.artist.as_deref(), Some("Beyoncé"));
        assert!(s.afficher_artiste());
        assert!(s.afficher_ecouter());
        assert_eq!(s.libelle_du_genre(), "Album");
        assert_eq!(s.lien_d_ecoute(), "#/musique/album/a1");
        assert_eq!(cle_de_cache(Some("42"), lien), format!("42 {lien}"));
    }

    #[test]
    fn ouvre_l_album_d_un_titre_et_masque_ecouter_sans_acces() {
        let lien = "https://estompe.plbls.fr/#/musique/track/t1";
        let sans_acces = carte_sillon(
            json!({"type": "track", "id": "t1", "title": "Formation", "album": "a2", "listen": false}),
        );
        let s = sillon(&sans_acces, lien);
        assert_eq!(s.title, "Formation");
        assert!(!s.afficher_ecouter());
        assert_eq!(s.libelle_du_genre(), "Titre");
        let avec_acces = carte_sillon(
            json!({"type": "track", "id": "t1", "title": "Formation 2", "album": "a2"}),
        );
        let s = sillon(&avec_acces, lien);
        assert!(s.afficher_ecouter());
        assert_eq!(s.lien_d_ecoute(), "#/musique/album/a2");
        assert_ne!(
            cle_de_cache(Some("43"), lien),
            cle_de_cache(Some("44"), lien)
        );
    }

    #[test]
    fn carte_sillon_artiste_et_pochette() {
        let lien = "https://estompe.plbls.fr/#/musique/artist/x y";
        let p = carte_sillon(
            json!({"type": "artist", "id": "x y", "album": null, "cover": {"id": "c9", "kind": "artist"}}),
        );
        let s = sillon(&p, lien);
        assert_eq!(s.libelle_du_genre(), "Artiste");
        assert!(!s.afficher_artiste());
        assert_eq!(s.lien_d_ecoute(), "#/musique/artist/x%20y");
        assert_eq!(s.identifiant_de_pochette(), "c9");
        assert!(s.pochette_en_image());
        assert_eq!(s.type_de_pochette(), "artiste");
        let p = carte_sillon(json!({"album": null}));
        let s = sillon(&p, lien);
        assert_eq!(s.identifiant_de_pochette(), "a1");
        assert!(!s.pochette_en_image());
        assert_eq!(s.type_de_pochette(), "album");
        assert_eq!(s.lien_d_ecoute(), "#/musique/album/a1");
        let p = carte_sillon(json!({"cover": {"id": "c1", "kind": "album"}}));
        assert_eq!(sillon(&p, lien).type_de_pochette(), "album");
    }

    #[test]
    fn un_lien_ordinaire_donne_une_carte_avec_ou_sans_image() {
        let p = apercu(
            json!({"url": "https://exemple.fr/a", "title": "Titre", "description": "", "site_name": "Exemple", "image": "https://exemple.fr/i.png"}),
        );
        match choisir_carte("https://exemple.fr/a", Some(&p)) {
            Carte::Lien(c) => {
                assert_eq!(c.titre, "Titre");
                assert_eq!(c.description, None);
                assert_eq!(c.site.as_deref(), Some("Exemple"));
                assert!(c.avec_image());
            }
            autre => panic!("carte inattendue : {autre:?}"),
        }
        let sans = apercu(json!({"url": "https://exemple.fr/a", "title": "Titre"}));
        match choisir_carte("https://exemple.fr/a", Some(&sans)) {
            Carte::Lien(c) => assert!(!c.avec_image()),
            autre => panic!("carte inattendue : {autre:?}"),
        }
        assert_eq!(choisir_carte("https://exemple.fr/a", None), Carte::Aucune);
    }

    #[test]
    fn un_bloc_sillon_illisible_retombe_sur_la_carte_ordinaire() {
        let p = apercu(
            json!({"url": "https://exemple.fr/a", "title": "T", "sillon": {"type": "inconnu"}}),
        );
        assert!(matches!(
            choisir_carte("https://exemple.fr/a", Some(&p)),
            Carte::Lien(_)
        ));
    }

    #[test]
    fn le_cache_est_borne_comme_dans_le_front() {
        assert_eq!(TAILLE_DU_CACHE, 200);
        assert_eq!(cle_de_cache(None, "https://x.fr"), " https://x.fr");
    }
}
