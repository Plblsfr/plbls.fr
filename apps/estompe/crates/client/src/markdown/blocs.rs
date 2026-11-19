use super::{en_ligne, Bloc, Contenu, Etat, Resolveur, Segment, PROFONDEUR_BLOCS_MAX};

const BALISES_BLOC: &[&str] = &[
    "address",
    "article",
    "aside",
    "base",
    "basefont",
    "blockquote",
    "body",
    "caption",
    "center",
    "col",
    "colgroup",
    "dd",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "frame",
    "frameset",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hr",
    "html",
    "iframe",
    "legend",
    "li",
    "link",
    "main",
    "menu",
    "menuitem",
    "nav",
    "noframes",
    "ol",
    "optgroup",
    "option",
    "p",
    "param",
    "search",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "title",
    "tr",
    "track",
    "ul",
];
const BALISES_BRUTES: &[&str] = &["script", "pre", "style", "textarea"];

#[derive(Clone, Copy, PartialEq)]
enum Html {
    Brut(&'static str),
    Commentaire,
    Bloc,
}

struct Marqueur {
    ordonne: bool,
    car: char,
    numero: u64,
    largeur: usize,
    debut_contenu: usize,
    colonnes_apres: usize,
    vide: bool,
}

struct Cloture {
    car: char,
    longueur: usize,
    indentation: usize,
    langage: Option<String>,
}

fn vide(l: &str) -> bool {
    l.trim().is_empty()
}

fn indentation(l: &str) -> usize {
    let mut colonnes = 0;
    for c in l.chars() {
        match c {
            ' ' => colonnes += 1,
            '\t' => colonnes += 4 - colonnes % 4,
            _ => break,
        }
    }
    colonnes
}

fn retirer_indentation(l: &str, n: usize) -> String {
    let mut colonnes = 0;
    for (i, c) in l.char_indices() {
        if colonnes >= n {
            return l[i..].to_string();
        }
        match c {
            ' ' => colonnes += 1,
            '\t' => {
                let suivant = colonnes + 4 - colonnes % 4;
                if suivant > n {
                    return format!("{}{}", " ".repeat(suivant - n), &l[i + 1..]);
                }
                colonnes = suivant;
            }
            _ => return l[i..].to_string(),
        }
    }
    String::new()
}

fn cloture(l: &str) -> Option<Cloture> {
    let ind = indentation(l);
    if ind > 3 {
        return None;
    }
    let reste = l.trim_start();
    let car = reste.chars().next()?;
    if car != '`' && car != '~' {
        return None;
    }
    let longueur = reste.chars().take_while(|&c| c == car).count();
    if longueur < 3 {
        return None;
    }
    let info = reste[longueur..].trim();
    if car == '`' && info.contains('`') {
        return None;
    }
    let langage = info.split_whitespace().next().map(String::from);
    Some(Cloture {
        car,
        longueur,
        indentation: ind,
        langage,
    })
}

fn ferme_cloture(l: &str, ouverture: &Cloture) -> bool {
    if indentation(l) > 3 {
        return false;
    }
    let reste = l.trim();
    let longueur = reste.chars().take_while(|&c| c == ouverture.car).count();
    longueur >= ouverture.longueur && longueur == reste.chars().count()
}

fn titre_atx(l: &str) -> Option<(u8, String)> {
    if indentation(l) > 3 {
        return None;
    }
    let reste = l.trim_start();
    let niveau = reste.chars().take_while(|&c| c == '#').count();
    if !(1..=6).contains(&niveau) {
        return None;
    }
    let apres = &reste[niveau..];
    if !apres.is_empty() && !apres.starts_with([' ', '\t']) {
        return None;
    }
    let mut contenu = apres.trim();
    let sans_dieses = contenu.trim_end_matches('#');
    if sans_dieses.is_empty() {
        contenu = "";
    } else if sans_dieses.ends_with([' ', '\t']) {
        contenu = sans_dieses.trim_end();
    }
    Some((niveau as u8, contenu.to_string()))
}

fn est_separateur(l: &str) -> bool {
    if indentation(l) > 3 {
        return false;
    }
    let reste = l.trim();
    let Some(car) = reste.chars().next() else {
        return false;
    };
    if !matches!(car, '-' | '_' | '*') {
        return false;
    }
    reste.chars().all(|c| c == car || c == ' ' || c == '\t')
        && reste.chars().filter(|&c| c == car).count() >= 3
}

fn souligne_setext(l: &str) -> Option<u8> {
    if indentation(l) > 3 {
        return None;
    }
    let reste = l.trim();
    let car = reste.chars().next()?;
    if !matches!(car, '=' | '-') || !reste.chars().all(|c| c == car) {
        return None;
    }
    Some(if car == '=' { 1 } else { 2 })
}

fn marqueur_liste(l: &str) -> Option<Marqueur> {
    let ind = indentation(l);
    if ind > 3 {
        return None;
    }
    let reste = l.trim_start_matches([' ', '\t']);
    let (ordonne, car, numero, longueur_marqueur) = match reste.chars().next()? {
        c @ ('-' | '+' | '*') => (false, c, 0, 1),
        c if c.is_ascii_digit() => {
            let chiffres = reste.chars().take_while(|c| c.is_ascii_digit()).count();
            if chiffres > 9 {
                return None;
            }
            let delimiteur = reste[chiffres..].chars().next()?;
            if delimiteur != '.' && delimiteur != ')' {
                return None;
            }
            (
                true,
                delimiteur,
                reste[..chiffres].parse().ok()?,
                chiffres + 1,
            )
        }
        _ => return None,
    };
    let apres = &reste[longueur_marqueur..];
    if !apres.is_empty() && !apres.starts_with([' ', '\t']) {
        return None;
    }
    let vide = vide(apres);
    let espaces = indentation(apres);
    let colonnes_apres = if vide || espaces > 4 { 1 } else { espaces };
    Some(Marqueur {
        ordonne,
        car,
        numero,
        largeur: ind + longueur_marqueur + colonnes_apres,
        debut_contenu: l.len() - reste.len() + longueur_marqueur,
        colonnes_apres,
        vide,
    })
}

fn retirer_citation(l: &str) -> Option<String> {
    if indentation(l) > 3 {
        return None;
    }
    let reste = l.trim_start().strip_prefix('>')?;
    Some(
        reste
            .strip_prefix(' ')
            .or_else(|| reste.strip_prefix('\t'))
            .unwrap_or(reste)
            .to_string(),
    )
}

fn debut_html(l: &str) -> Option<Html> {
    if indentation(l) > 3 {
        return None;
    }
    let reste = l.trim_start().strip_prefix('<')?;
    if reste.starts_with("!--") {
        return Some(Html::Commentaire);
    }
    let (fermante, nom_debut) = match reste.strip_prefix('/') {
        Some(r) => (true, r),
        None => (false, reste),
    };
    let nom: String = nom_debut
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    let suite = &nom_debut[nom.len()..];
    if !fermante {
        if let Some(brut) = BALISES_BRUTES.iter().find(|b| **b == nom) {
            if suite.is_empty() || suite.starts_with([' ', '\t', '>']) {
                return Some(Html::Brut(brut));
            }
        }
    }
    if BALISES_BLOC.contains(&nom.as_str())
        && (suite.is_empty() || suite.starts_with([' ', '\t', '>']) || suite.starts_with("/>"))
    {
        return Some(Html::Bloc);
    }
    None
}

fn fin_html(l: &str, genre: Html) -> bool {
    match genre {
        Html::Brut(nom) => l.to_ascii_lowercase().contains(&format!("</{nom}>")),
        Html::Commentaire => l.contains("-->"),
        Html::Bloc => false,
    }
}

fn interrompt_paragraphe(l: &str) -> bool {
    cloture(l).is_some()
        || titre_atx(l).is_some()
        || est_separateur(l)
        || retirer_citation(l).is_some()
        || debut_html(l).is_some()
        || marqueur_liste(l).is_some_and(|m| !m.vide && (!m.ordonne || m.numero == 1))
}

fn segments_bruts(lignes: &[String]) -> Vec<Segment> {
    let mut segments = Vec::new();
    for (i, ligne) in lignes.iter().enumerate() {
        if i > 0 {
            segments.push(Segment {
                contenu: Contenu::SautDeLigne,
                ..Segment::texte("")
            });
        }
        if !ligne.is_empty() {
            segments.push(Segment::texte(ligne));
        }
    }
    segments
}

fn paragraphe_en_ligne(lignes: &[String], resolveur: &Resolveur, etat: &mut Etat) -> Vec<Segment> {
    let texte = lignes
        .iter()
        .enumerate()
        .map(|(i, l)| if i == 0 { l.as_str() } else { l.trim_start() })
        .collect::<Vec<_>>()
        .join("\n");
    let mut segments = en_ligne::analyser(texte.trim_end(), resolveur, etat);
    if let Some(Segment {
        contenu: Contenu::Texte(t),
        ..
    }) = segments.first_mut()
    {
        let reste = t.trim_start().to_string();
        *t = reste;
    }
    if segments
        .first()
        .is_some_and(|s| s.contenu == Contenu::Texte(String::new()))
    {
        segments.remove(0);
    }
    etat.borner_segments(&mut segments);
    segments
}

fn contenu_imbrique(
    lignes: &[String],
    profondeur: usize,
    resolveur: &Resolveur,
    etat: &mut Etat,
) -> Vec<Bloc> {
    if profondeur + 1 < PROFONDEUR_BLOCS_MAX {
        return analyser(lignes, profondeur + 1, resolveur, etat);
    }
    let non_vides: Vec<String> = lignes.iter().filter(|l| !vide(l)).cloned().collect();
    if non_vides.is_empty() || !etat.reserver_bloc() {
        return Vec::new();
    }
    vec![Bloc::Paragraphe(paragraphe_en_ligne(
        &non_vides, resolveur, etat,
    ))]
}

fn meme_liste(a: &Marqueur, b: &Marqueur) -> bool {
    a.ordonne == b.ordonne && a.car == b.car
}

pub(super) fn analyser(
    lignes: &[String],
    profondeur: usize,
    resolveur: &Resolveur,
    etat: &mut Etat,
) -> Vec<Bloc> {
    let n = lignes.len();
    let mut blocs = Vec::new();
    let mut i = 0;
    while i < n {
        if etat.tronque {
            break;
        }
        let l = &lignes[i];
        if vide(l) {
            i += 1;
            continue;
        }
        if est_separateur(l) {
            i += 1;
            continue;
        }
        if !etat.reserver_bloc() {
            break;
        }
        if indentation(l) >= 4 {
            let mut contenu = Vec::new();
            while i < n && (vide(&lignes[i]) || indentation(&lignes[i]) >= 4) {
                contenu.push(retirer_indentation(&lignes[i], 4));
                i += 1;
            }
            while contenu.last().is_some_and(|d| vide(d)) {
                contenu.pop();
            }
            blocs.push(Bloc::Code {
                langage: None,
                texte: contenu.join("\n"),
            });
            continue;
        }
        if let Some(ouverture) = cloture(l) {
            i += 1;
            let mut contenu = Vec::new();
            while i < n && !ferme_cloture(&lignes[i], &ouverture) {
                contenu.push(retirer_indentation(&lignes[i], ouverture.indentation));
                i += 1;
            }
            i += 1;
            blocs.push(Bloc::Code {
                langage: ouverture.langage,
                texte: contenu.join("\n"),
            });
            continue;
        }
        if let Some((niveau, contenu)) = titre_atx(l) {
            i += 1;
            let mut segments = en_ligne::analyser(&contenu, resolveur, etat);
            etat.borner_segments(&mut segments);
            for s in &mut segments {
                s.styles.gras = true;
            }
            blocs.push(Bloc::Titre { niveau, segments });
            continue;
        }
        if retirer_citation(l).is_some() {
            let mut interieur: Vec<String> = Vec::new();
            while i < n {
                if let Some(reste) = retirer_citation(&lignes[i]) {
                    interieur.push(reste);
                } else if !vide(&lignes[i])
                    && interieur
                        .last()
                        .is_some_and(|d| !vide(d) && cloture(d).is_none() && indentation(d) < 4)
                    && !interrompt_paragraphe(&lignes[i])
                {
                    interieur.push(lignes[i].clone());
                } else {
                    break;
                }
                i += 1;
            }
            blocs.push(Bloc::Citation(contenu_imbrique(
                &interieur, profondeur, resolveur, etat,
            )));
            continue;
        }
        if let Some(premier) = marqueur_liste(l) {
            let mut elements = Vec::new();
            let mut marqueur = premier;
            loop {
                let ligne = &lignes[i];
                let mut contenu_element = vec![if marqueur.vide {
                    String::new()
                } else {
                    retirer_indentation(&ligne[marqueur.debut_contenu..], marqueur.colonnes_apres)
                }];
                if let Some(premiere) = contenu_element.first_mut() {
                    for case in ["[ ] ", "[x] ", "[X] "] {
                        if let Some(reste) = premiere.strip_prefix(case) {
                            if !vide(reste) {
                                *premiere = reste.trim_start().to_string();
                            }
                            break;
                        }
                    }
                }
                i += 1;
                while i < n {
                    let suivante = &lignes[i];
                    if vide(suivante) {
                        contenu_element.push(String::new());
                    } else if indentation(suivante) >= marqueur.largeur {
                        contenu_element.push(retirer_indentation(suivante, marqueur.largeur));
                    } else if marqueur_liste(suivante).is_some() || est_separateur(suivante) {
                        break;
                    } else if contenu_element.last().is_some_and(|d| !vide(d))
                        && !interrompt_paragraphe(suivante)
                    {
                        contenu_element.push(suivante.clone());
                    } else {
                        break;
                    }
                    i += 1;
                }
                while contenu_element.last().is_some_and(|d| vide(d)) {
                    contenu_element.pop();
                }
                elements.push(contenu_imbrique(
                    &contenu_element,
                    profondeur,
                    resolveur,
                    etat,
                ));
                if etat.tronque || i >= n {
                    break;
                }
                match marqueur_liste(&lignes[i]) {
                    Some(m) if meme_liste(&m, &marqueur) && !est_separateur(&lignes[i]) => {
                        marqueur = m
                    }
                    _ => break,
                }
            }
            blocs.push(Bloc::Liste {
                ordonnee: marqueur.ordonne,
                elements,
            });
            continue;
        }
        if let Some(genre) = debut_html(l) {
            let mut contenu = Vec::new();
            while i < n {
                let ligne = &lignes[i];
                if genre == Html::Bloc && vide(ligne) {
                    break;
                }
                contenu.push(ligne.clone());
                i += 1;
                if fin_html(ligne, genre) {
                    break;
                }
            }
            let mut segments = segments_bruts(&contenu);
            etat.borner_segments(&mut segments);
            blocs.push(Bloc::Paragraphe(segments));
            continue;
        }
        let mut contenu = vec![l.clone()];
        i += 1;
        let mut titre = None;
        while i < n && !vide(&lignes[i]) {
            if let Some(niveau) = souligne_setext(&lignes[i]) {
                titre = Some(niveau);
                i += 1;
                break;
            }
            if interrompt_paragraphe(&lignes[i]) {
                break;
            }
            contenu.push(lignes[i].clone());
            i += 1;
        }
        let mut segments = paragraphe_en_ligne(&contenu, resolveur, etat);
        match titre {
            Some(niveau) => {
                for s in &mut segments {
                    s.styles.gras = true;
                }
                blocs.push(Bloc::Titre { niveau, segments });
            }
            None => blocs.push(Bloc::Paragraphe(segments)),
        }
    }
    blocs
}
