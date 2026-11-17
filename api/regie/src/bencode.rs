use std::ops::Range;

const PROFONDEUR_MAX: usize = 32;
pub const FICHIERS_MAX: usize = 5000;

#[derive(Debug, PartialEq)]
enum Valeur<'a> {
    Entier(i64),
    Octets(&'a [u8]),
    Liste(Vec<Valeur<'a>>),
    Dict(Vec<(&'a [u8], Valeur<'a>, Range<usize>)>),
}

impl<'a> Valeur<'a> {
    fn cle(&self, nom: &str) -> Option<&Valeur<'a>> {
        match self {
            Valeur::Dict(d) => d
                .iter()
                .find(|(k, _, _)| *k == nom.as_bytes())
                .map(|(_, v, _)| v),
            _ => None,
        }
    }

    fn etendue(&self, nom: &str) -> Option<Range<usize>> {
        match self {
            Valeur::Dict(d) => d
                .iter()
                .find(|(k, _, _)| *k == nom.as_bytes())
                .map(|(_, _, e)| e.clone()),
            _ => None,
        }
    }

    fn octets(&self) -> Option<&'a [u8]> {
        match self {
            Valeur::Octets(o) => Some(o),
            _ => None,
        }
    }

    fn texte(&self) -> Option<&'a str> {
        self.octets().and_then(|o| std::str::from_utf8(o).ok())
    }

    fn entier(&self) -> Option<i64> {
        match self {
            Valeur::Entier(n) => Some(*n),
            _ => None,
        }
    }

    fn liste(&self) -> Option<&[Valeur<'a>]> {
        match self {
            Valeur::Liste(l) => Some(l),
            _ => None,
        }
    }
}

struct Lecteur<'a> {
    donnees: &'a [u8],
    position: usize,
}

impl<'a> Lecteur<'a> {
    fn jusqu_a(&mut self, fin: u8) -> Option<&'a [u8]> {
        let reste = &self.donnees[self.position..];
        let n = reste.iter().position(|b| *b == fin)?;
        self.position += n + 1;
        Some(&reste[..n])
    }

    fn nombre<T: std::str::FromStr>(&mut self, fin: u8) -> Option<T> {
        std::str::from_utf8(self.jusqu_a(fin)?).ok()?.parse().ok()
    }

    fn chaine(&mut self) -> Option<&'a [u8]> {
        let n: usize = self.nombre(b':')?;
        let fin = self.position.checked_add(n)?;
        let o = self.donnees.get(self.position..fin)?;
        self.position = fin;
        Some(o)
    }

    fn valeur(&mut self, profondeur: usize) -> Option<Valeur<'a>> {
        if profondeur > PROFONDEUR_MAX {
            return None;
        }
        match *self.donnees.get(self.position)? {
            b'i' => {
                self.position += 1;
                Some(Valeur::Entier(self.nombre(b'e')?))
            }
            b'l' => {
                self.position += 1;
                let mut l = Vec::new();
                while *self.donnees.get(self.position)? != b'e' {
                    l.push(self.valeur(profondeur + 1)?);
                }
                self.position += 1;
                Some(Valeur::Liste(l))
            }
            b'd' => {
                self.position += 1;
                let mut d = Vec::new();
                while *self.donnees.get(self.position)? != b'e' {
                    let cle = self.chaine()?;
                    let debut = self.position;
                    let v = self.valeur(profondeur + 1)?;
                    d.push((cle, v, debut..self.position));
                }
                self.position += 1;
                Some(Valeur::Dict(d))
            }
            b'0'..=b'9' => Some(Valeur::Octets(self.chaine()?)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fichier {
    pub chemin: Vec<String>,
    pub taille: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Torrent {
    pub hash: String,
    pub nom: String,
    pub fichiers: Vec<Fichier>,
    pub prive: bool,
    pub tracker: Option<String>,
}

fn composant(v: &Valeur) -> Result<String, String> {
    let t = v.texte().ok_or("nom de fichier illisible")?;
    let interdit = t.is_empty()
        || t.len() > 255
        || t == "."
        || t == ".."
        || t.chars().any(|c| c == '/' || c == '\\' || c.is_control());
    if interdit {
        return Err("nom de fichier refusé".into());
    }
    Ok(t.to_string())
}

fn hote(url: &str) -> Option<String> {
    let reste = url.split_once("://")?.1;
    let autorite = reste.split(['/', '?', '#']).next()?;
    let hote = autorite.rsplit('@').next()?;
    let hote = hote.rsplit_once(':').map_or(hote, |(h, port)| {
        if port.bytes().all(|b| b.is_ascii_digit()) {
            h
        } else {
            hote
        }
    });
    let valide = !hote.is_empty()
        && hote.len() <= 253
        && hote
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    valide.then(|| hote.to_ascii_lowercase())
}

impl Torrent {
    pub fn lire(donnees: &[u8]) -> Result<Self, String> {
        let mut lecteur = Lecteur {
            donnees,
            position: 0,
        };
        let racine = lecteur
            .valeur(0)
            .filter(|_| lecteur.position == donnees.len())
            .ok_or("fichier .torrent illisible")?;
        let (info, etendue) = racine
            .cle("info")
            .zip(racine.etendue("info"))
            .ok_or("fichier .torrent sans section info")?;
        if info.cle("pieces").and_then(Valeur::octets).is_none() {
            return Err("torrent sans empreintes v1 (torrent v2 seul) : non géré".into());
        }
        let nom = composant(info.cle("name").ok_or("torrent sans nom")?)?;
        let taille = |v: Option<&Valeur>| {
            v.and_then(Valeur::entier)
                .and_then(|n| u64::try_from(n).ok())
                .ok_or("taille de fichier invalide")
        };
        let fichiers = match info.cle("files") {
            None => vec![Fichier {
                chemin: vec![nom.clone()],
                taille: taille(info.cle("length"))?,
            }],
            Some(liste) => {
                let liste = liste.liste().ok_or("liste de fichiers illisible")?;
                if liste.is_empty() || liste.len() > FICHIERS_MAX {
                    return Err(format!("torrent de 1 à {FICHIERS_MAX} fichiers attendu"));
                }
                let mut fichiers = Vec::with_capacity(liste.len());
                for f in liste {
                    let composants = f
                        .cle("path")
                        .and_then(Valeur::liste)
                        .filter(|p| !p.is_empty() && p.len() <= 32)
                        .ok_or("chemin de fichier illisible")?;
                    let mut chemin = vec![nom.clone()];
                    for c in composants {
                        chemin.push(composant(c)?);
                    }
                    fichiers.push(Fichier {
                        chemin,
                        taille: taille(f.cle("length"))?,
                    });
                }
                fichiers
            }
        };
        let mut vus = std::collections::HashSet::new();
        for f in &fichiers {
            for n in 1..=f.chemin.len() {
                let dossier = n < f.chemin.len();
                if !vus.insert((f.chemin[..n].join("/").to_lowercase(), dossier)) && !dossier {
                    return Err("deux fichiers du torrent portent le même chemin".into());
                }
            }
        }
        if fichiers
            .iter()
            .any(|f| vus.contains(&(f.chemin.join("/").to_lowercase(), true)))
        {
            return Err("un fichier du torrent porte le nom d'un dossier".into());
        }
        fichiers
            .iter()
            .try_fold(0u64, |total, f| total.checked_add(f.taille))
            .ok_or("taille totale invalide")?;
        let empreinte =
            ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, &donnees[etendue]);
        let hash = empreinte
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(Self {
            hash,
            nom,
            fichiers,
            prive: info.cle("private").and_then(Valeur::entier) == Some(1),
            tracker: racine
                .cle("announce")
                .and_then(Valeur::texte)
                .and_then(hote),
        })
    }

    pub fn total(&self) -> u64 {
        self.fichiers.iter().map(|f| f.taille).sum()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn c(t: &str) -> String {
        format!("{}:{t}", t.len())
    }

    pub fn multi(tracker: &str, premier: &str, second: &str, nom: &str, extra: &str) -> String {
        format!(
            "d{}{}{}d{}l{premier}{second}e{}{}{}i16384e{extra}{}i1eee",
            c("announce"),
            c(tracker),
            c("info"),
            c("files"),
            c("name"),
            c(nom),
            c("piece length"),
            c("private"),
        )
    }

    pub fn fichier(taille: i64, chemin: &[&str]) -> String {
        let composants: String = chemin.iter().map(|x| c(x)).collect();
        format!("d{}i{taille}e{}l{composants}ee", c("length"), c("path"))
    }

    const PIECES: &str = "6:pieces20:aaaaaaaaaaaaaaaaaaaa";
    const TRACKER: &str = "https://user:cle@tracker.exemple.fr:443/a";

    fn variante(premier: &str, second: &str, nom: &str, extra: &str) -> String {
        multi(TRACKER, premier, second, nom, extra)
    }

    pub fn torrent_multi() -> Vec<u8> {
        variante(
            &fichier(5, &["film.mkv"]),
            &fichier(3, &["Subs", "fr.srt"]),
            "Film.2024",
            PIECES,
        )
        .into_bytes()
    }

    pub fn torrent_simple() -> Vec<u8> {
        b"d4:infod6:lengthi4e4:name8:seul.mkv12:piece lengthi16384e6:pieces20:bbbbbbbbbbbbbbbbbbbbee".to_vec()
    }

    #[test]
    fn torrent_a_plusieurs_fichiers() {
        let t = Torrent::lire(&torrent_multi()).unwrap();
        assert_eq!(t.nom, "Film.2024");
        assert_eq!(
            t.fichiers,
            vec![
                Fichier {
                    chemin: vec!["Film.2024".into(), "film.mkv".into()],
                    taille: 5
                },
                Fichier {
                    chemin: vec!["Film.2024".into(), "Subs".into(), "fr.srt".into()],
                    taille: 3
                },
            ]
        );
        assert_eq!(t.total(), 8);
        assert!(t.prive);
        assert_eq!(t.tracker.as_deref(), Some("tracker.exemple.fr"));
        assert_eq!(t.hash.len(), 40);
        assert!(t.hash.bytes().all(|b| b.is_ascii_hexdigit()));
        let autre = multi(
            "udp://tracker.ailleurs.fr:6969",
            &fichier(5, &["film.mkv"]),
            &fichier(3, &["Subs", "fr.srt"]),
            "Film.2024",
            PIECES,
        );
        assert_eq!(
            Torrent::lire(autre.as_bytes()).unwrap().hash,
            t.hash,
            "l'empreinte ne dépend que de la section info"
        );
    }

    #[test]
    fn torrent_a_un_fichier() {
        let t = Torrent::lire(&torrent_simple()).unwrap();
        assert_eq!(
            t.fichiers,
            vec![Fichier {
                chemin: vec!["seul.mkv".into()],
                taille: 4
            }]
        );
        assert!(!t.prive && t.tracker.is_none());
    }

    #[test]
    fn torrents_refuses() {
        let (film, sous_titre) = (fichier(5, &["film.mkv"]), fichier(3, &["Subs", "fr.srt"]));
        let bon = String::from_utf8(torrent_multi()).unwrap();
        let refuses = [
            ("pas du bencode".to_string(), "illisible"),
            ("d3:fooi1ee".to_string(), "sans section info"),
            (format!("{bon}x"), "illisible"),
            (
                variante(&fichier(5, &[".."]), &sous_titre, "Film.2024", PIECES),
                "nom de fichier refusé",
            ),
            (
                variante(&fichier(5, &["a/../b.x"]), &sous_titre, "Film.2024", PIECES),
                "nom de fichier refusé",
            ),
            (
                variante(&film, &sous_titre, "/etc/pass", PIECES),
                "nom de fichier refusé",
            ),
            (
                variante(
                    &fichier(-5, &["film.mkv"]),
                    &sous_titre,
                    "Film.2024",
                    PIECES,
                ),
                "taille",
            ),
            (
                variante(&film, &fichier(3, &["FILM.MKV"]), "Film.2024", PIECES),
                "même chemin",
            ),
            (
                variante(&film, &fichier(3, &["film.mkv", "x"]), "Film.2024", PIECES),
                "nom d'un dossier",
            ),
            (variante(&film, &sous_titre, "Film.2024", ""), "v2"),
            (format!("{}i1e", "l".repeat(100)), "illisible"),
        ];
        for (texte, raison) in refuses {
            let e = Torrent::lire(texte.as_bytes()).unwrap_err();
            assert!(e.contains(raison), "{texte} : {e}");
        }
    }

    #[test]
    fn hote_du_tracker() {
        assert_eq!(hote("udp://t.fr:6969/announce").as_deref(), Some("t.fr"));
        assert_eq!(hote("https://T.FR/a?passkey=x").as_deref(), Some("t.fr"));
        assert_eq!(hote("sans-schema"), None);
        assert_eq!(hote("http://<script>/"), None);
    }
}
