pub const CLE_MAX: usize = 120;

const FORMES: [&str; 14] = [
    "SA", "SAS", "SASU", "SARL", "EURL", "SCI", "SNC", "SCOP", "GMBH", "LTD", "INC", "BV", "FR",
    "FRA",
];
const TECHNIQUES_PRELEVEMENT: [&str; 3] = ["PRLV", "PRELEVEMENT", "SEPA"];
const TECHNIQUES_VIREMENT: [&str; 5] = ["VIR", "VIREMENT", "INST", "INSTANTANE", "SEPA"];
const DIRECTIONS: [&str; 3] = ["VERS", "DE", "POUR"];
const CIVILITES: [&str; 5] = ["M", "MR", "MME", "MLLE", "MLE"];
const PETITS_MOTS: [&str; 9] = ["DE", "DU", "DES", "LA", "LE", "LES", "ET", "AU", "AUX"];

fn plier(c: char, sortie: &mut String) {
    let simple = match c {
        'à' | 'â' | 'ä' | 'á' | 'ã' | 'À' | 'Â' | 'Ä' | 'Á' | 'Ã' => 'A',
        'ç' | 'Ç' => 'C',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'î' | 'ï' | 'í' | 'Î' | 'Ï' | 'Í' => 'I',
        'ñ' | 'Ñ' => 'N',
        'ô' | 'ö' | 'ó' | 'õ' | 'Ô' | 'Ö' | 'Ó' | 'Õ' => 'O',
        'ù' | 'û' | 'ü' | 'ú' | 'Ù' | 'Û' | 'Ü' | 'Ú' => 'U',
        'ÿ' | 'Ÿ' => 'Y',
        'œ' | 'Œ' => {
            sortie.push_str("OE");
            return;
        }
        'æ' | 'Æ' => {
            sortie.push_str("AE");
            return;
        }
        c if c.is_alphanumeric() => {
            sortie.extend(c.to_uppercase());
            return;
        }
        _ => ' ',
    };
    sortie.push(simple);
}

pub fn mots(texte: &str) -> Vec<String> {
    let mut plie = String::with_capacity(texte.len());
    for c in texte.chars() {
        plier(c, &mut plie);
    }
    plie.split_whitespace().map(str::to_string).collect()
}

pub fn texte(source: &str) -> String {
    mots(source).join(" ")
}

pub fn contient(texte_normalise: &str, mot_normalise: &str) -> bool {
    !mot_normalise.is_empty()
        && texte_normalise.match_indices(mot_normalise).any(|(i, _)| {
            let fin = i + mot_normalise.len();
            (i == 0 || texte_normalise.as_bytes()[i - 1] == b' ')
                && (fin == texte_normalise.len() || texte_normalise.as_bytes()[fin] == b' ')
        })
}

fn est_reference(mot: &str, premier: bool) -> bool {
    let chiffres = mot.chars().filter(char::is_ascii_digit).count();
    if chiffres == mot.chars().count() {
        return !(premier && chiffres <= 3);
    }
    chiffres >= 2
}

fn sans_tete<'a>(mut mots: &'a [String], tetes: &[&str]) -> &'a [String] {
    while let [premier, reste @ ..] = mots
        && tetes.contains(&premier.as_str())
    {
        mots = reste;
    }
    mots
}

fn sans_une_tete<'a>(mots: &'a [String], tetes: &[&str]) -> &'a [String] {
    match mots {
        [premier, reste @ ..] if tetes.contains(&premier.as_str()) && !reste.is_empty() => reste,
        _ => mots,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tiers {
    pub cle: String,
    pub nom: String,
}

pub fn nom(cle: &str) -> String {
    cle.split(' ')
        .enumerate()
        .map(|(i, mot)| {
            if i > 0 && PETITS_MOTS.contains(&mot) {
                return mot.to_lowercase();
            }
            let sigle = mot.chars().count() > 1 && !mot.chars().any(|c| "AEIOUY".contains(c));
            if sigle {
                return mot.to_string();
            }
            let mut lettres = mot.chars();
            match lettres.next() {
                Some(p) => p.to_string() + &lettres.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn tiers(genre: &str, libelle: &str, commercant_brut: Option<&str>) -> Option<Tiers> {
    let tous = match (genre, commercant_brut) {
        ("carte", Some(brut)) => mots(brut),
        _ => mots(libelle),
    };
    let utiles: &[String] = match genre {
        "prelevement" => sans_tete(&tous, &TECHNIQUES_PRELEVEMENT),
        "virement" | "virement_instantane" => sans_une_tete(
            sans_une_tete(sans_tete(&tous, &TECHNIQUES_VIREMENT), &DIRECTIONS),
            &CIVILITES,
        ),
        _ => &tous,
    };
    let mut gardes: Vec<&str> = Vec::new();
    for mot in utiles {
        if !est_reference(mot, gardes.is_empty()) {
            gardes.push(mot);
        }
    }
    while gardes.len() > 1 && gardes.last().is_some_and(|m| FORMES.contains(m)) {
        gardes.pop();
    }
    let mut cle = String::new();
    for mot in gardes {
        if cle.chars().count() + mot.chars().count() + 1 > CLE_MAX {
            break;
        }
        if !cle.is_empty() {
            cle.push(' ');
        }
        cle.push_str(mot);
    }
    (!cle.is_empty()).then(|| Tiers {
        nom: nom(&cle),
        cle,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cle(genre: &str, libelle: &str, brut: Option<&str>) -> Option<String> {
        tiers(genre, libelle, brut).map(|t| t.cle)
    }

    #[test]
    fn texte_plie_la_casse_les_accents_et_la_ponctuation() {
        for (source, attendu) in [
            ("Épicerie du Cœur", "EPICERIE DU COEUR"),
            ("  l'atelier--méridien ", "L ATELIER MERIDIEN"),
            ("LIVREO*CMD", "LIVREO CMD"),
            ("Café.Tamis/Villeneuve", "CAFE TAMIS VILLENEUVE"),
            ("", ""),
        ] {
            assert_eq!(texte(source), attendu, "{source}");
        }
    }

    #[test]
    fn mot_cle_cherche_par_mots_entiers() {
        assert!(contient("PRLV SEPA LOYER MARS", "LOYER"));
        assert!(contient("LOYER", "LOYER"));
        assert!(contient("VIR LIVRET BLEU", "LIVRET BLEU"));
        assert!(!contient("EMPLOYER FICTIF", "LOYER"));
        assert!(!contient("LOYERS", "LOYER"));
        assert!(!contient("LOYER", ""));
    }

    #[test]
    fn commercants_de_carte_regroupes() {
        let cas = [
            ("MARCHE COLLINE VILLENEUVE", "MARCHE COLLINE VILLENEUVE"),
            ("Marché Colline Villeneuve", "MARCHE COLLINE VILLENEUVE"),
            (
                "MARCHE COLLINE 0042 VILLENEUVE",
                "MARCHE COLLINE VILLENEUVE",
            ),
            ("MARCHE-COLLINE VILLENEUVE.", "MARCHE COLLINE VILLENEUVE"),
            ("MARCHE COLLINE VILLENEUVE FR", "MARCHE COLLINE VILLENEUVE"),
            ("LIVREO*CMD 88213 REF0KX72", "LIVREO CMD"),
            ("BISTROT LUNE SARL", "BISTROT LUNE"),
            ("BISTROT LUNE SAS FR", "BISTROT LUNE"),
            ("TPE X4471 CAFE TAMIS", "TPE CAFE TAMIS"),
            ("3 PETITS POIS SAINT-ELOI", "3 PETITS POIS SAINT ELOI"),
            ("SA", "SA"),
        ];
        for (brut, attendu) in cas {
            assert_eq!(
                cle("carte", &format!("CARTE 01/02 {brut}"), Some(brut)).as_deref(),
                Some(attendu),
                "{brut}"
            );
        }
        assert_ne!(
            cle("carte", "", Some("MARCHE COLLINE VILLENEUVE")),
            cle("carte", "", Some("MARCHE COLLINE SAINT-ELOI"))
        );
        assert_eq!(cle("carte", "", Some("0042 88213")), None);
    }

    #[test]
    fn tiers_des_prelevements_et_des_virements() {
        let cas = [
            (
                "prelevement",
                "PRLV SEPA TELECOM IMAGINAIRE SA",
                "TELECOM IMAGINAIRE",
            ),
            (
                "prelevement",
                "PRLV STREAMO ABONNEMENT",
                "STREAMO ABONNEMENT",
            ),
            (
                "virement_instantane",
                "VIR INST vers BERNARD PAUL",
                "BERNARD PAUL",
            ),
            ("virement", "VIR SEPA M BERNARD PAUL", "BERNARD PAUL"),
            ("virement", "VIR DE MME LEGRAND CAMILLE", "LEGRAND CAMILLE"),
            (
                "virement",
                "VIR ATELIER MERIDIEN SALAIRE 112026",
                "ATELIER MERIDIEN SALAIRE",
            ),
            ("virement", "VIR VERS LIVRET", "LIVRET"),
            ("virement", "VIR DE", "DE"),
            ("echeance_pret", "ECH PRET 00012345678", "ECH PRET"),
            (
                "cotisation",
                "F COTISATION FORMULE FICTIVE",
                "F COTISATION FORMULE FICTIVE",
            ),
            (
                "autre",
                "RETRAIT DAB 12/03 VILLENEUVE",
                "RETRAIT DAB VILLENEUVE",
            ),
        ];
        for (genre, libelle, attendu) in cas {
            assert_eq!(
                cle(genre, libelle, None).as_deref(),
                Some(attendu),
                "{libelle}"
            );
        }
        assert_eq!(cle("virement", "VIR", None), None);
    }

    #[test]
    fn nom_lisible() {
        for (cle, attendu) in [
            ("MARCHE COLLINE VILLENEUVE", "Marche Colline Villeneuve"),
            ("SNCF GARE DE VILLENEUVE", "SNCF Gare de Villeneuve"),
            ("LE BISTROT ET LA LUNE", "Le Bistrot et la Lune"),
            ("3 PETITS POIS", "3 Petits Pois"),
        ] {
            assert_eq!(nom(cle), attendu);
            assert_eq!(tiers("carte", "", Some(cle)).unwrap().nom, attendu);
        }
    }

    #[test]
    fn cle_bornee_et_stable() {
        let long = "COMMERCE ".repeat(40);
        let t = tiers("carte", "", Some(&long)).unwrap();
        assert!(t.cle.chars().count() <= CLE_MAX);
        assert_eq!(tiers("carte", "", Some(&t.cle)).unwrap().cle, t.cle);
    }
}
