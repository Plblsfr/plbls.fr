use std::path::{Path, PathBuf};

use estompe_client::apercu_liens::{choisir_carte, url_miniature, Carte};
use estompe_client::fichiers::nom_de_fichier_sur;
use estompe_client::markdown::lien_ouvrable;
use estompe_client::types::LinkPreview;

use super::medias::{ajuster, CleImage, Format, Source};

pub const VIGNETTE_PAR_DEFAUT: (u32, u32) = (240, 180);
pub const SITE_YOUTUBE: &str = "YouTube";
pub const ESSAIS_DE_NOM_MAX: u32 = 999;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarteAffichee {
    Aucune,
    Lien {
        url: String,
        titre: String,
        description: String,
        site: String,
        image: Option<String>,
    },
    Youtube {
        url: String,
        titre: String,
        site: String,
        miniature: String,
    },
    Sillon {
        titre: String,
        site: String,
        artiste: String,
    },
}

impl CarteAffichee {
    pub fn image(&self) -> Option<CleImage> {
        match self {
            Self::Lien {
                image: Some(url), ..
            } => Some(CleImage {
                source: Source::Externe(url.clone()),
                format: Format::Carte,
            }),
            Self::Youtube { miniature, .. } => Some(CleImage {
                source: Source::Externe(miniature.clone()),
                format: Format::Miniature,
            }),
            _ => None,
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            Self::Lien { url, .. } | Self::Youtube { url, .. } => Some(url),
            Self::Aucune | Self::Sillon { .. } => None,
        }
    }
}

pub fn carte_affichee(lien: &str, apercu: Option<&LinkPreview>) -> CarteAffichee {
    match choisir_carte(lien, apercu) {
        Carte::Youtube { video, infos } => CarteAffichee::Youtube {
            url: lien.to_string(),
            titre: infos.as_ref().map(|i| i.titre.clone()).unwrap_or_default(),
            site: infos
                .and_then(|i| i.site)
                .unwrap_or_else(|| SITE_YOUTUBE.to_string()),
            miniature: url_miniature(&video),
        },
        Carte::Sillon(s) => CarteAffichee::Sillon {
            site: format!("Sillon · {}", s.libelle_du_genre()),
            artiste: if s.afficher_artiste() {
                s.artist.clone().unwrap_or_default()
            } else {
                String::new()
            },
            titre: s.title,
        },
        Carte::Lien(c) => {
            let url = lien_ouvrable(&c.url).unwrap_or_else(|| lien.to_string());
            CarteAffichee::Lien {
                url,
                titre: c.titre,
                description: c.description.unwrap_or_default(),
                site: c.site.unwrap_or_default(),
                image: c.image.and_then(|i| lien_ouvrable(&i)),
            }
        }
        Carte::Aucune => CarteAffichee::Aucune,
    }
}

pub fn taille_de_vignette(origine: Option<(u32, u32)>) -> (u32, u32) {
    match origine {
        Some((l, h)) if l > 0 && h > 0 => ajuster(l, h, Format::Vignette.boite()),
        _ => VIGNETTE_PAR_DEFAUT,
    }
}

pub fn chemin_libre(dossier: &Path, nom: &str) -> PathBuf {
    let sur = nom_de_fichier_sur(nom);
    let candidat = dossier.join(&sur);
    if !candidat.exists() {
        return candidat;
    }
    let (radical, extension) = match sur.rfind('.') {
        Some(i) if i > 0 => (&sur[..i], &sur[i..]),
        _ => (sur.as_str(), ""),
    };
    (1..=ESSAIS_DE_NOM_MAX)
        .map(|n| dossier.join(nom_de_fichier_sur(&format!("{radical} ({n}){extension}"))))
        .find(|c| !c.exists())
        .unwrap_or(candidat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn apercu(valeur: serde_json::Value) -> LinkPreview {
        serde_json::from_value(valeur).unwrap()
    }

    #[test]
    fn linkcard_youtube_titre_de_l_apercu_et_miniature_sans_lecteur() {
        let p = apercu(
            json!({"url": "https://youtu.be/aaaaaaaaaaa", "title": "Ma vidéo", "site_name": "YouTube"}),
        );
        let carte = carte_affichee("https://youtu.be/aaaaaaaaaaa", Some(&p));
        let CarteAffichee::Youtube {
            titre,
            miniature,
            url,
            ..
        } = &carte
        else {
            panic!("carte YouTube attendue : {carte:?}");
        };
        assert_eq!(titre, "Ma vidéo");
        assert!(miniature.contains("/vi/aaaaaaaaaaa/"));
        assert_eq!(url, "https://youtu.be/aaaaaaaaaaa");
        let sans = carte_affichee("https://youtu.be/dQw4w9WgXcQ", None);
        let CarteAffichee::Youtube {
            titre, miniature, ..
        } = &sans
        else {
            panic!("carte YouTube attendue sans aperçu");
        };
        assert!(titre.is_empty() && miniature.contains("/vi/dQw4w9WgXcQ/"));
        assert_eq!(sans.url(), Some("https://youtu.be/dQw4w9WgXcQ"));
        assert_eq!(sans.image().unwrap().format, Format::Miniature);
    }

    #[test]
    fn linkcard_sillon_titre_et_artiste_sauf_pour_un_artiste() {
        let carte = |o: serde_json::Value| {
            let mut sillon = json!({"type": "album", "id": "a1", "title": "Lemonade", "artist": "Beyoncé", "album": "a1", "cover": null, "listen": true});
            for (k, v) in o.as_object().unwrap() {
                sillon[k] = v.clone();
            }
            carte_affichee(
                "https://estompe.plbls.fr/#/musique/album/a1",
                Some(&apercu(
                    json!({"url": "https://estompe.plbls.fr/#/musique/album/a1", "title": "Lemonade", "sillon": sillon}),
                )),
            )
        };
        assert_eq!(
            carte(json!({})),
            CarteAffichee::Sillon {
                titre: "Lemonade".into(),
                site: "Sillon · Album".into(),
                artiste: "Beyoncé".into()
            }
        );
        assert_eq!(
            carte(json!({"type": "track", "title": "Formation"})),
            CarteAffichee::Sillon {
                titre: "Formation".into(),
                site: "Sillon · Titre".into(),
                artiste: "Beyoncé".into()
            }
        );
        let CarteAffichee::Sillon { artiste, .. } = carte(json!({"type": "artist"})) else {
            panic!("carte Sillon attendue");
        };
        assert!(artiste.is_empty());
    }

    #[test]
    fn carte_de_lien_avec_et_sans_image_et_lien_sans_apercu() {
        let avec = carte_affichee(
            "https://plbls.fr/a",
            Some(&apercu(json!({
                "url": "https://plbls.fr/a", "title": "Titre", "description": "Desc",
                "site_name": "plbls", "image": "https://plbls.fr/i.png"
            }))),
        );
        assert_eq!(
            avec.image().map(|c| c.source),
            Some(Source::Externe("https://plbls.fr/i.png".into()))
        );
        let sans = carte_affichee(
            "https://plbls.fr/a",
            Some(&apercu(
                json!({"url": "https://plbls.fr/a", "title": "Titre", "image": "javascript:x"}),
            )),
        );
        assert_eq!(sans.image(), None);
        assert_eq!(sans.url(), Some("https://plbls.fr/a"));
        assert_eq!(
            carte_affichee("https://plbls.fr/a", None),
            CarteAffichee::Aucune
        );
    }

    #[test]
    fn la_vignette_a_sa_taille_avant_le_chargement_quand_elle_est_connue() {
        assert_eq!(taille_de_vignette(None), VIGNETTE_PAR_DEFAUT);
        assert_eq!(taille_de_vignette(Some((8000, 8000))), (300, 300));
        assert_eq!(taille_de_vignette(Some((1920, 1080))), (360, 203));
        assert_eq!(taille_de_vignette(Some((0, 10))), VIGNETTE_PAR_DEFAUT);
    }

    #[test]
    fn c5_un_nom_hostile_devient_un_chemin_sur_dans_le_dossier_sans_ecraser() {
        let dossier = tempfile::tempdir().unwrap();
        for hostile in [
            "..\\..\\Windows\\system32\\evil.dll",
            "../../etc/passwd",
            "CON",
            "a<b>c:d\"e|f?g*h.txt",
            "   .",
        ] {
            let chemin = chemin_libre(dossier.path(), hostile);
            assert_eq!(chemin.parent(), Some(dossier.path()), "{hostile}");
            let nom = chemin.file_name().unwrap().to_str().unwrap();
            assert!(
                !nom.contains(['/', '\\', '<', '>', ':', '"', '|', '?', '*']),
                "{nom}"
            );
        }
        assert_eq!(
            chemin_libre(dossier.path(), "..\\..\\evil.dll")
                .file_name()
                .unwrap(),
            "evil.dll"
        );
        assert_eq!(
            chemin_libre(dossier.path(), "CON").file_name().unwrap(),
            "_CON"
        );
        std::fs::write(dossier.path().join("rapport.pdf"), b"x").unwrap();
        std::fs::write(dossier.path().join("rapport (1).pdf"), b"x").unwrap();
        assert_eq!(
            chemin_libre(dossier.path(), "rapport.pdf")
                .file_name()
                .unwrap(),
            "rapport (2).pdf"
        );
    }
}
