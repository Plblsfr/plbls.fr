use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GenrePiste {
    Video,
    Audio,
    SousTitres,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piste {
    pub id: i64,
    pub genre: GenrePiste,
    pub titre: Option<String>,
    pub langue: Option<String>,
    pub codec: Option<String>,
    pub defaut: bool,
    pub forcee: bool,
    pub externe: bool,
    pub image_fixe: bool,
    pub selectionnee: bool,
}

pub fn pistes_de_mpv(json: &str) -> Vec<Piste> {
    let Ok(Value::Array(pistes)) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    pistes.iter().filter_map(piste_de_mpv).collect()
}

fn piste_de_mpv(v: &Value) -> Option<Piste> {
    let genre = match v.get("type")?.as_str()? {
        "video" => GenrePiste::Video,
        "audio" => GenrePiste::Audio,
        "sub" => GenrePiste::SousTitres,
        _ => return None,
    };
    let texte = |cle: &str| {
        v.get(cle)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
    };
    let drapeau = |cle: &str| v.get(cle).and_then(Value::as_bool).unwrap_or(false);
    Some(Piste {
        id: v.get("id")?.as_i64()?,
        genre,
        titre: texte("title"),
        langue: texte("lang"),
        codec: texte("codec"),
        defaut: drapeau("default"),
        forcee: drapeau("forced"),
        externe: drapeau("external"),
        image_fixe: drapeau("albumart"),
        selectionnee: drapeau("selected"),
    })
}

pub fn nom_de_langue(code: &str) -> Option<&'static str> {
    Some(match code.to_ascii_lowercase().as_str() {
        "fr" | "fre" | "fra" | "fr-fr" => "Français",
        "fr-ca" => "Français (Québec)",
        "en" | "eng" | "en-us" | "en-gb" => "Anglais",
        "es" | "spa" => "Espagnol",
        "de" | "ger" | "deu" => "Allemand",
        "it" | "ita" => "Italien",
        "pt" | "por" => "Portugais",
        "nl" | "dut" | "nld" => "Néerlandais",
        "ja" | "jpn" => "Japonais",
        "ko" | "kor" => "Coréen",
        "zh" | "chi" | "zho" => "Chinois",
        "ru" | "rus" => "Russe",
        "ar" | "ara" => "Arabe",
        _ => return None,
    })
}

impl Piste {
    pub fn libelle(&self) -> String {
        let langue = self
            .langue
            .as_deref()
            .map(|l| nom_de_langue(l).unwrap_or(l));
        let mut libelle = match (&self.titre, langue) {
            (Some(titre), Some(langue)) if !titre.contains(langue) => format!("{titre} ({langue})"),
            (Some(titre), _) => titre.clone(),
            (None, Some(langue)) => langue.to_string(),
            (None, None) => format!("Piste {}", self.id),
        };
        if self.forcee && self.genre == GenrePiste::SousTitres {
            libelle.push_str(" · forcés");
        }
        libelle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTE: &str = r#"[
        {"id":1,"type":"video","src-id":0,"albumart":false,"default":true,"forced":false,"external":false,"selected":true,"codec":"h264"},
        {"id":1,"type":"audio","lang":"fre","title":"VFF","default":true,"selected":true,"codec":"aac"},
        {"id":2,"type":"audio","lang":"eng","selected":false},
        {"id":1,"type":"sub","lang":"fr","forced":true,"external":true,"title":"  "},
        {"id":2,"type":"sub"},
        {"id":3,"type":"inconnu"},
        {"type":"audio"}
    ]"#;

    #[test]
    fn les_pistes_de_mpv_sont_lues_et_les_entrees_invalides_ignorees() {
        let pistes = pistes_de_mpv(LISTE);
        assert_eq!(pistes.len(), 5);
        assert_eq!(pistes[0].genre, GenrePiste::Video);
        assert_eq!(pistes[0].codec.as_deref(), Some("h264"));
        assert!(pistes[0].defaut && pistes[0].selectionnee && !pistes[0].image_fixe);
        assert_eq!(pistes[1].titre.as_deref(), Some("VFF"));
        assert_eq!(pistes[3].titre, None);
        assert!(pistes[3].forcee && pistes[3].externe);
        assert!(pistes_de_mpv("pas du json").is_empty());
        assert!(pistes_de_mpv("{}").is_empty());
    }

    #[test]
    fn libelles_des_pistes() {
        let pistes = pistes_de_mpv(LISTE);
        assert_eq!(pistes[1].libelle(), "VFF (Français)");
        assert_eq!(pistes[2].libelle(), "Anglais");
        assert_eq!(pistes[3].libelle(), "Français · forcés");
        assert_eq!(pistes[4].libelle(), "Piste 2");
        let mut titre = pistes[2].clone();
        titre.titre = Some("Anglais (commentaires)".into());
        assert_eq!(titre.libelle(), "Anglais (commentaires)");
        titre.langue = Some("tlh".into());
        assert_eq!(titre.libelle(), "Anglais (commentaires) (tlh)");
    }
}
