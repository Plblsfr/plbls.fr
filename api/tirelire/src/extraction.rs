use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
};

use lopdf::{Dictionary, Document, Encoding, Object, ObjectId, content::Content};

use crate::{
    metriques::{self, Standard},
    mots::{Mot, Page},
};

pub const PAGES_MAX: usize = 60;
pub const GLYPHES_MAX: usize = 400_000;
pub const CONTENU_MAX: usize = 16 * 1024 * 1024;
const HAUTEUR_A4: f64 = 842.0;
const ECART_MOT: f64 = 0.12;
const ECART_LIGNE: f64 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Erreur {
    PasUnPdf,
    Illisible,
    TropGrand,
}

struct Glyphe {
    texte: String,
    x: f64,
    y: f64,
    position: f64,
    rangee: f64,
    avance: f64,
    taille: f64,
    vertical: bool,
}

type Matrice = [f64; 6];

const IDENTITE: Matrice = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn produit(m: &Matrice, n: &Matrice) -> Matrice {
    [
        m[0] * n[0] + m[1] * n[2],
        m[0] * n[1] + m[1] * n[3],
        m[2] * n[0] + m[3] * n[2],
        m[2] * n[1] + m[3] * n[3],
        m[4] * n[0] + m[5] * n[2] + n[4],
        m[4] * n[1] + m[5] * n[3] + n[5],
    ]
}

fn translation(x: f64, y: f64) -> Matrice {
    [1.0, 0.0, 0.0, 1.0, x, y]
}

fn nombre(o: &Object) -> Option<f64> {
    o.as_float().ok().map(f64::from)
}

fn nombres<const N: usize>(operandes: &[Object]) -> Option<[f64; N]> {
    let valeurs: Vec<f64> = operandes.iter().filter_map(nombre).collect();
    valeurs.try_into().ok()
}

enum Largeurs {
    Table { premier: usize, valeurs: Vec<f64> },
    Standard(Standard),
}

struct Police<'a> {
    composite: Option<Encoding<'a>>,
    table: [char; 256],
    largeurs: Largeurs,
    octets: usize,
}

fn table_des_caracteres(dict: &Dictionary, doc: &Document) -> [char; 256] {
    let mut table: [char; 256] = std::array::from_fn(|i| metriques::win_ansi(i as u8));
    let differences = dict
        .get_deref(b"Encoding", doc)
        .and_then(Object::as_dict)
        .and_then(|e| e.get_deref(b"Differences", doc))
        .and_then(Object::as_array);
    let mut code = 0usize;
    for element in differences.into_iter().flatten() {
        match element {
            Object::Integer(n) => code = usize::try_from(*n).unwrap_or(usize::MAX),
            Object::Name(nom) => {
                if let Some(case) = table.get_mut(code) {
                    *case = std::str::from_utf8(nom)
                        .ok()
                        .and_then(metriques::par_nom)
                        .unwrap_or('\u{fffd}');
                }
                code = code.saturating_add(1);
            }
            _ => {}
        }
    }
    table
}

impl<'a> Police<'a> {
    fn de(dict: &'a Dictionary, doc: &'a Document) -> Self {
        let est_composite = dict
            .get(b"Subtype")
            .and_then(Object::as_name)
            .is_ok_and(|n| n == b"Type0");
        let composite = est_composite
            .then(|| dict.get_font_encoding(doc).ok())
            .flatten();
        let table_largeurs = dict
            .get_deref(b"Widths", doc)
            .and_then(Object::as_array)
            .ok()
            .map(|t| {
                t.iter()
                    .map(|o| nombre(o).unwrap_or(0.0))
                    .collect::<Vec<_>>()
            })
            .filter(|t| !t.is_empty());
        let largeurs = match table_largeurs {
            Some(valeurs) if !est_composite => Largeurs::Table {
                premier: dict
                    .get_deref(b"FirstChar", doc)
                    .and_then(Object::as_i64)
                    .ok()
                    .and_then(|n| usize::try_from(n).ok())
                    .unwrap_or(0),
                valeurs,
            },
            _ => {
                let nom = dict
                    .get(b"BaseFont")
                    .and_then(Object::as_name)
                    .map(|n| String::from_utf8_lossy(n).into_owned())
                    .unwrap_or_default();
                Largeurs::Standard(metriques::standard(&nom))
            }
        };
        Self {
            composite,
            table: table_des_caracteres(dict, doc),
            largeurs,
            octets: if est_composite { 2 } else { 1 },
        }
    }

    fn texte(&self, code: &[u8]) -> String {
        if self.octets == 1 {
            return self.table[usize::from(code[0])].to_string();
        }
        self.composite
            .as_ref()
            .and_then(|e| e.bytes_to_string(code).ok())
            .unwrap_or_else(|| "\u{fffd}".to_string())
    }

    fn largeur(&self, code: &[u8], texte: &str) -> f64 {
        match &self.largeurs {
            Largeurs::Table { premier, valeurs } => usize::from(code[0])
                .checked_sub(*premier)
                .and_then(|i| valeurs.get(i))
                .copied()
                .unwrap_or(metriques::LARGEUR_PAR_DEFAUT),
            Largeurs::Standard(s) => texte
                .chars()
                .next()
                .map_or(metriques::LARGEUR_PAR_DEFAUT, |c| metriques::largeur(*s, c)),
        }
    }
}

#[derive(Clone, Copy)]
struct Etat<'p, 'a> {
    ctm: Matrice,
    police: Option<&'p Police<'a>>,
    taille: f64,
    espace_caractere: f64,
    espace_mot: f64,
    echelle: f64,
    interligne: f64,
    elevation: f64,
}

struct Lecteur<'p, 'a> {
    polices: &'p BTreeMap<Vec<u8>, Police<'a>>,
    hauteur: f64,
    etat: Etat<'p, 'a>,
    pile: Vec<Etat<'p, 'a>>,
    tm: Matrice,
    tlm: Matrice,
    glyphes: Vec<Glyphe>,
    reste: usize,
}

impl<'p, 'a> Lecteur<'p, 'a> {
    fn nouveau(polices: &'p BTreeMap<Vec<u8>, Police<'a>>, hauteur: f64, reste: usize) -> Self {
        Self {
            polices,
            hauteur,
            etat: Etat {
                ctm: IDENTITE,
                police: None,
                taille: 0.0,
                espace_caractere: 0.0,
                espace_mot: 0.0,
                echelle: 1.0,
                interligne: 0.0,
                elevation: 0.0,
            },
            pile: Vec::new(),
            tm: IDENTITE,
            tlm: IDENTITE,
            glyphes: Vec::new(),
            reste,
        }
    }

    fn ligne_suivante(&mut self, x: f64, y: f64) {
        self.tlm = produit(&translation(x, y), &self.tlm);
        self.tm = self.tlm;
    }

    fn avancer(&mut self, x: f64) {
        self.tm = produit(&translation(x, 0.0), &self.tm);
    }

    fn montrer(&mut self, octets: &[u8]) -> Result<(), Erreur> {
        let Some(police) = self.etat.police else {
            return Ok(());
        };
        for code in octets.chunks(police.octets) {
            if self.reste == 0 {
                return Err(Erreur::TropGrand);
            }
            self.reste -= 1;
            let texte = police.texte(code);
            let largeur = police.largeur(code, &texte) / 1000.0;
            let e = &self.etat;
            let m = produit(&produit(&translation(0.0, e.elevation), &self.tm), &e.ctm);
            let facteur = m[0].hypot(m[1]);
            let vertical = m[1].abs() > m[0].abs();
            let (x, y) = (m[4], self.hauteur - m[5]);
            let (position, rangee) = if vertical {
                (m[5] * m[1].signum(), x)
            } else {
                (x, y)
            };
            self.glyphes.push(Glyphe {
                texte,
                x,
                y,
                position,
                rangee,
                avance: largeur * e.taille * e.echelle * facteur,
                taille: e.taille * facteur,
                vertical,
            });
            let espace = if code == b" " { e.espace_mot } else { 0.0 };
            let pas = (largeur * e.taille + e.espace_caractere + espace) * e.echelle;
            self.avancer(pas);
        }
        Ok(())
    }

    fn operation(&mut self, operateur: &str, operandes: &[Object]) -> Result<(), Erreur> {
        match operateur {
            "q" => self.pile.push(self.etat),
            "Q" => {
                if let Some(e) = self.pile.pop() {
                    self.etat = e;
                }
            }
            "cm" => {
                if let Some(m) = nombres::<6>(operandes) {
                    self.etat.ctm = produit(&m, &self.etat.ctm);
                }
            }
            "BT" => {
                self.tm = IDENTITE;
                self.tlm = IDENTITE;
            }
            "Tf" => {
                if let [nom, taille] = operandes {
                    self.etat.police = nom.as_name().ok().and_then(|n| self.polices.get(n));
                    self.etat.taille = nombre(taille).unwrap_or(0.0);
                }
            }
            "Td" | "TD" => {
                if let Some([x, y]) = nombres::<2>(operandes) {
                    if operateur == "TD" {
                        self.etat.interligne = -y;
                    }
                    self.ligne_suivante(x, y);
                }
            }
            "Tm" => {
                if let Some(m) = nombres::<6>(operandes) {
                    self.tm = m;
                    self.tlm = m;
                }
            }
            "T*" => self.ligne_suivante(0.0, -self.etat.interligne),
            "Tc" | "Tw" | "Tz" | "TL" | "Ts" => {
                if let Some([v]) = nombres::<1>(operandes) {
                    match operateur {
                        "Tc" => self.etat.espace_caractere = v,
                        "Tw" => self.etat.espace_mot = v,
                        "Tz" => self.etat.echelle = v / 100.0,
                        "TL" => self.etat.interligne = v,
                        _ => self.etat.elevation = v,
                    }
                }
            }
            "Tj" | "'" | "\"" => {
                if operateur != "Tj" {
                    self.ligne_suivante(0.0, -self.etat.interligne);
                }
                if let [mot, caractere, _] = operandes {
                    self.etat.espace_mot = nombre(mot).unwrap_or(0.0);
                    self.etat.espace_caractere = nombre(caractere).unwrap_or(0.0);
                }
                if let Some(chaine) = operandes.last().and_then(|o| o.as_str().ok()) {
                    self.montrer(chaine)?;
                }
            }
            "TJ" => {
                for element in operandes
                    .first()
                    .and_then(|o| o.as_array().ok())
                    .into_iter()
                    .flatten()
                {
                    match element {
                        Object::String(chaine, _) => self.montrer(chaine)?,
                        autre => {
                            if let Some(n) = nombre(autre) {
                                self.avancer(-n / 1000.0 * self.etat.taille * self.etat.echelle);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn hauteur(doc: &Document, page: ObjectId) -> f64 {
    let mut courant = Some(page);
    for _ in 0..16 {
        let Some(dict) = courant.and_then(|id| doc.get_dictionary(id).ok()) else {
            break;
        };
        let boite = dict
            .get_deref(b"MediaBox", doc)
            .and_then(Object::as_array)
            .ok()
            .and_then(|b| nombres::<4>(b));
        if let Some([_, bas, _, haut]) = boite {
            return haut - bas;
        }
        courant = dict.get(b"Parent").and_then(Object::as_reference).ok();
    }
    HAUTEUR_A4
}

fn lire_page(doc: &Document, page: ObjectId, reste: usize) -> Result<Vec<Glyphe>, Erreur> {
    let polices: BTreeMap<Vec<u8>, Police> = doc
        .get_page_fonts(page)
        .unwrap_or_default()
        .into_iter()
        .map(|(nom, dict)| (nom, Police::de(dict, doc)))
        .collect();
    let contenu = doc
        .get_page_content_with_limit(page, CONTENU_MAX)
        .map_err(|_| Erreur::TropGrand)?;
    let contenu = Content::decode(&contenu).map_err(|_| Erreur::Illisible)?;
    let mut lecteur = Lecteur::nouveau(&polices, hauteur(doc, page), reste);
    for op in &contenu.operations {
        lecteur.operation(&op.operator, &op.operands)?;
    }
    Ok(lecteur.glyphes)
}

fn lire_document(octets: &[u8]) -> Result<Vec<Page>, Erreur> {
    let doc = Document::load_mem(octets).map_err(|_| Erreur::Illisible)?;
    if doc.is_encrypted() {
        return Err(Erreur::Illisible);
    }
    let pages = doc.get_pages();
    if pages.is_empty() {
        return Err(Erreur::Illisible);
    }
    if pages.len() > PAGES_MAX {
        return Err(Erreur::TropGrand);
    }
    let mut reste = GLYPHES_MAX;
    let mut lues = Vec::new();
    for (numero, id) in pages {
        let glyphes = lire_page(&doc, id, reste)?;
        reste -= glyphes.len();
        lues.push(Page {
            numero,
            mots: assembler(glyphes),
        });
    }
    Ok(lues)
}

fn assembler(mut glyphes: Vec<Glyphe>) -> Vec<Mot> {
    glyphes.sort_by(|a, b| {
        a.vertical
            .cmp(&b.vertical)
            .then(a.rangee.total_cmp(&b.rangee))
            .then(a.position.total_cmp(&b.position))
    });
    let mut mots: Vec<Mot> = Vec::new();
    let mut ouvert: Option<(f64, f64, bool)> = None;
    for g in glyphes {
        if g.texte.trim().is_empty() {
            ouvert = None;
            continue;
        }
        let suite = ouvert.is_some_and(|(rangee, fin, vertical)| {
            vertical == g.vertical
                && (g.rangee - rangee).abs() <= ECART_LIGNE
                && g.position - fin <= ECART_MOT * g.taille
                && g.position - fin >= -g.taille
        });
        match mots.last_mut() {
            Some(m) if suite => {
                m.texte.push_str(&g.texte);
                m.largeur += g.position + g.avance - (ouvert.map_or(0.0, |o| o.1));
            }
            _ => mots.push(Mot {
                texte: g.texte.clone(),
                x: g.x,
                y: g.y,
                largeur: g.avance,
                vertical: g.vertical,
            }),
        }
        ouvert = Some((g.rangee, g.position + g.avance, g.vertical));
    }
    mots.sort_by(|a, b| {
        a.vertical
            .cmp(&b.vertical)
            .then(a.y.total_cmp(&b.y))
            .then(a.x.total_cmp(&b.x))
    });
    mots
}

pub fn mots_du_pdf(octets: &[u8]) -> Result<Vec<Page>, Erreur> {
    let tete = &octets[..octets.len().min(1024)];
    if !tete.windows(5).any(|f| f == b"%PDF-") {
        return Err(Erreur::PasUnPdf);
    }
    catch_unwind(AssertUnwindSafe(|| lire_document(octets))).map_err(|_| Erreur::Illisible)?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyphe(texte: &str, x: f64, y: f64, avance: f64) -> Glyphe {
        Glyphe {
            texte: texte.into(),
            x,
            y,
            position: x,
            rangee: y,
            avance,
            taille: 8.0,
            vertical: false,
        }
    }

    #[test]
    fn glyphes_assembles_en_mots() {
        let mots = assembler(vec![
            glyphe("B", 15.0, 100.0, 5.0),
            glyphe("A", 10.0, 100.0, 5.0),
            glyphe("C", 22.5, 100.0, 5.0),
            glyphe(" ", 27.5, 100.0, 2.0),
            glyphe("D", 29.5, 100.0, 5.0),
            glyphe("E", 10.0, 111.0, 5.0),
        ]);
        let vus: Vec<(&str, f64, f64)> = mots
            .iter()
            .map(|m| (m.texte.as_str(), m.x, m.largeur))
            .collect();
        assert_eq!(
            vus,
            [
                ("AB", 10.0, 10.0),
                ("C", 22.5, 5.0),
                ("D", 29.5, 5.0),
                ("E", 10.0, 5.0)
            ]
        );
    }

    #[test]
    fn texte_vertical_marque() {
        let mut g = glyphe("M", 34.0, 700.0, 5.0);
        g.vertical = true;
        g.position = 142.0;
        g.rangee = 34.0;
        let mots = assembler(vec![g, glyphe("a", 50.0, 700.0, 4.0)]);
        assert!(!mots[0].vertical);
        assert!(mots[1].vertical);
        assert_eq!(mots[1].texte, "M");
    }

    #[test]
    fn refuse_ce_qui_n_est_pas_un_pdf() {
        assert_eq!(mots_du_pdf(b"bonjour"), Err(Erreur::PasUnPdf));
        assert_eq!(mots_du_pdf(b""), Err(Erreur::PasUnPdf));
        assert_eq!(
            mots_du_pdf(b"%PDF-1.6\nn'importe quoi"),
            Err(Erreur::Illisible)
        );
    }
}
