pub const TOLERANCE_LIGNE: f64 = 3.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Mot {
    pub texte: String,
    pub x: f64,
    pub y: f64,
    pub largeur: f64,
    pub vertical: bool,
}

impl Mot {
    pub fn fin(&self) -> f64 {
        self.x + self.largeur
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub numero: u32,
    pub mots: Vec<Mot>,
}

#[derive(Clone, Debug)]
pub struct Ligne<'a> {
    pub page: u32,
    pub y: f64,
    pub mots: Vec<&'a Mot>,
}

impl Ligne<'_> {
    pub fn texte(&self) -> String {
        joindre(&self.mots)
    }
}

pub fn joindre(mots: &[&Mot]) -> String {
    mots.iter()
        .map(|m| m.texte.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn lignes(page: &Page) -> Vec<Ligne<'_>> {
    let mut mots: Vec<&Mot> = page.mots.iter().filter(|m| !m.vertical).collect();
    mots.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let mut lignes: Vec<Ligne> = Vec::new();
    for m in mots {
        match lignes.last_mut() {
            Some(l) if m.y - l.y <= TOLERANCE_LIGNE => l.mots.push(m),
            _ => lignes.push(Ligne {
                page: page.numero,
                y: m.y,
                mots: vec![m],
            }),
        }
    }
    for l in &mut lignes {
        l.mots.sort_by(|a, b| a.x.total_cmp(&b.x));
    }
    lignes
}

pub fn lire(texte: &str) -> Result<Vec<Page>, String> {
    let mut pages: Vec<Page> = Vec::new();
    for (i, brute) in texte.lines().enumerate() {
        let ligne = brute.trim();
        if ligne.is_empty() || ligne.starts_with('#') {
            continue;
        }
        let erreur = |quoi: &str| format!("ligne {} : {quoi}", i + 1);
        if let Some(n) = ligne.strip_prefix("page ") {
            pages.push(Page {
                numero: n.trim().parse().map_err(|_| erreur("numéro de page"))?,
                mots: Vec::new(),
            });
            continue;
        }
        let (vertical, reste) = match ligne.strip_prefix("v ") {
            Some(r) => (true, r),
            None => (false, ligne),
        };
        let mut champs = reste.splitn(4, ' ');
        let mut nombre = |quoi: &str| -> Result<f64, String> {
            champs
                .next()
                .and_then(|c| c.parse().ok())
                .ok_or_else(|| erreur(quoi))
        };
        let (x, y, largeur) = (nombre("x")?, nombre("y")?, nombre("largeur")?);
        let texte = champs.next().ok_or_else(|| erreur("texte"))?.to_string();
        pages
            .last_mut()
            .ok_or_else(|| erreur("mot avant la première page"))?
            .mots
            .push(Mot {
                texte,
                x,
                y,
                largeur,
                vertical,
            });
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_des_fixtures() {
        let pages =
            lire("# commentaire\npage 1\n10 20.5 30 Date\nv 5 700 12 Émis\n\npage 2\n1 2 3 a b\n")
                .unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(
            pages[0].mots[0],
            Mot {
                texte: "Date".into(),
                x: 10.0,
                y: 20.5,
                largeur: 30.0,
                vertical: false
            }
        );
        assert!(pages[0].mots[1].vertical);
        assert_eq!(pages[1].mots[0].texte, "a b");
        assert!(lire("1 2 3 seul").unwrap_err().contains("ligne 1"));
        assert!(lire("page 1\n1 x 3 mot").unwrap_err().contains("ligne 2"));
        assert!(lire("page 1\n1 2 3").is_err());
    }

    #[test]
    fn lignes_regroupees_et_triees() {
        let pages = lire(
            "page 1\n200 101 10 c\n10 100 10 a\n100 100.9 10 b\n10 110 10 d\nv 1 100 5 marge\n",
        )
        .unwrap();
        let l = lignes(&pages[0]);
        assert_eq!(
            l.iter().map(Ligne::texte).collect::<Vec<_>>(),
            ["a b c", "d"]
        );
        assert_eq!(l[0].page, 1);
    }
}
