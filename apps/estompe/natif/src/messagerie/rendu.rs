use std::collections::BTreeSet;

use estompe_client::markdown::{lien_ouvrable, Bloc, Contenu, Rendu, Segment, Styles};
use estompe_client::mention::GenreMention;

use super::emoji::{self, Morceau, EMOJI_GRANDS_MAX};

pub const PREFIXE_LIEN: &str = "u:";
pub const PREFIXE_MENTION: &str = "m:";
pub const PREFIXE_MASQUE: &str = "s:";
pub const BLOCS_MASQUES_MAX: usize = 60;

const SEPARATEUR_INVISIBLE: char = '\u{200B}';
const CARACTERE_MASQUE: char = '\u{2588}';

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GenreBlocAffiche {
    Paragraphe,
    Titre,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GenreSegmentAffiche {
    Texte,
    Code,
    Emoji,
    EmojiServeur,
    Masque,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SegmentAffiche {
    pub genre: GenreSegmentAffiche,
    pub balisage: String,
    pub texte: String,
    pub cible: String,
    pub espace: bool,
    pub joint: bool,
}

impl SegmentAffiche {
    fn nouveau(genre: GenreSegmentAffiche, texte: &str) -> Self {
        Self {
            genre,
            balisage: String::new(),
            texte: texte.to_string(),
            cible: String::new(),
            espace: false,
            joint: false,
        }
    }

    pub fn est_emoji(&self) -> bool {
        matches!(
            self.genre,
            GenreSegmentAffiche::Emoji | GenreSegmentAffiche::EmojiServeur
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlocAffiche {
    pub genre: GenreBlocAffiche,
    pub balisage: String,
    pub code: String,
    pub niveau: u8,
    pub citation: u8,
    pub retrait: u8,
    pub puce: String,
    pub segments: Vec<SegmentAffiche>,
    pub grand: bool,
    pub espace_avant: bool,
}

impl BlocAffiche {
    pub fn en_segments(&self) -> bool {
        !self.segments.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Palette {
    pub mention: String,
    pub masque: String,
    pub discret: String,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            mention: "#a83e63".into(),
            masque: "#16130f".into(),
            discret: "#6a635c".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cible {
    Lien(String),
    Mention(String),
    Masque(u32),
}

pub fn echapper(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len() + 8);
    for c in texte.chars() {
        match c {
            '\n' | '\r' => sortie.push(' '),
            c if c.is_ascii_punctuation() => {
                sortie.push('\\');
                sortie.push(c);
            }
            c => sortie.push(c),
        }
    }
    sortie
}

fn encoder_cible(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    for c in texte.chars() {
        match c {
            ' ' | '<' | '>' | '%' | '\\' | '\n' | '\r' | '\t' => {
                let mut tampon = [0u8; 4];
                for octet in c.encode_utf8(&mut tampon).bytes() {
                    sortie.push_str(&format!("%{octet:02X}"));
                }
            }
            c => sortie.push(c),
        }
    }
    sortie
}

fn decoder_cible(texte: &str) -> String {
    let octets = texte.as_bytes();
    let mut sortie = Vec::with_capacity(octets.len());
    let mut i = 0;
    while i < octets.len() {
        if octets[i] == b'%' {
            if let Some(valeur) = texte
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                sortie.push(valeur);
                i += 3;
                continue;
            }
        }
        sortie.push(octets[i]);
        i += 1;
    }
    String::from_utf8_lossy(&sortie).into_owned()
}

pub fn analyser_cible(cible: &str) -> Option<Cible> {
    if let Some(url) = cible.strip_prefix(PREFIXE_LIEN) {
        return lien_ouvrable(url).map(Cible::Lien);
    }
    if let Some(nom) = cible.strip_prefix(PREFIXE_MENTION) {
        let nom = decoder_cible(nom);
        return (!nom.is_empty()).then_some(Cible::Mention(nom));
    }
    cible
        .strip_prefix(PREFIXE_MASQUE)
        .and_then(|n| n.parse().ok())
        .map(Cible::Masque)
}

fn entourer(interieur: String, ouvrant: &str, fermant: &str) -> String {
    format!("{ouvrant}{SEPARATEUR_INVISIBLE}{interieur}{SEPARATEUR_INVISIBLE}{fermant}")
}

fn styler(texte: String, styles: Styles) -> String {
    let mut sortie = texte;
    if styles.barre {
        sortie = entourer(sortie, "~~", "~~");
    }
    if styles.italique {
        sortie = entourer(sortie, "*", "*");
    }
    if styles.gras {
        sortie = entourer(sortie, "**", "**");
    }
    sortie
}

fn code_en_ligne(texte: &str) -> String {
    let propre: String = texte
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let mut plus_longue = 0;
    let mut courante = 0;
    for c in propre.chars() {
        if c == '`' {
            courante += 1;
            plus_longue = plus_longue.max(courante);
        } else {
            courante = 0;
        }
    }
    let barrieres = "`".repeat(plus_longue + 1);
    format!("{barrieres} {propre} {barrieres}")
}

fn couleur(texte: String, couleur: &str) -> String {
    format!("<font color=\"{couleur}\">{texte}</font>")
}

fn lien(texte: String, cible: &str) -> String {
    format!("[{texte}](<{}>)", encoder_cible(cible))
}

fn texte_du_segment(segment: &Segment) -> String {
    match &segment.contenu {
        Contenu::Texte(t) => t.clone(),
        Contenu::SautDeLigne => String::new(),
        Contenu::Mention { texte, .. } => format!("@{texte}"),
        Contenu::Emoji(nom) => format!(":{nom}:"),
    }
}

fn balisage_du_segment(segment: &Segment, reveles: &BTreeSet<u32>, palette: &Palette) -> String {
    if let Some(masque) = segment.masque.filter(|m| !reveles.contains(m)) {
        let longueur = texte_du_segment(segment)
            .chars()
            .count()
            .clamp(1, BLOCS_MASQUES_MAX);
        let blocs: String = std::iter::repeat_n(CARACTERE_MASQUE, longueur).collect();
        return lien(
            couleur(blocs, &palette.masque),
            &format!("{PREFIXE_MASQUE}{masque}"),
        );
    }
    let texte = texte_du_segment(segment);
    if texte.trim().is_empty() {
        return echapper(&texte);
    }
    if let Contenu::Mention { texte: nom, genre } = &segment.contenu {
        let corps = couleur(
            styler(
                echapper(&texte),
                Styles {
                    gras: true,
                    ..segment.styles
                },
            ),
            &palette.mention,
        );
        return match genre {
            GenreMention::Personne => lien(corps, &format!("{PREFIXE_MENTION}{nom}")),
            GenreMention::Role | GenreMention::Groupe => corps,
        };
    }
    let debut = texte.len() - texte.trim_start().len();
    let fin = texte.trim_end().len();
    let (avant, coeur, apres) = (&texte[..debut], &texte[debut..fin], &texte[fin..]);
    let mut corps = if segment.styles.code {
        code_en_ligne(coeur)
    } else {
        echapper(coeur)
    };
    corps = styler(corps, segment.styles);
    if let Some(url) = segment.lien.as_deref().and_then(lien_ouvrable) {
        corps = lien(corps, &format!("{PREFIXE_LIEN}{url}"));
    }
    format!("{}{corps}{}", echapper(avant), echapper(apres))
}

pub fn balisage(segments: &[Segment], reveles: &BTreeSet<u32>, palette: &Palette) -> String {
    let mut lignes: Vec<String> = vec![String::new()];
    for segment in segments {
        if segment.contenu == Contenu::SautDeLigne {
            lignes.push(String::new());
            continue;
        }
        if let Some(ligne) = lignes.last_mut() {
            ligne.push_str(&balisage_du_segment(segment, reveles, palette));
        }
    }
    lignes
        .into_iter()
        .map(|l| {
            if l.trim().is_empty() {
                SEPARATEUR_INVISIBLE.to_string()
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\\\n")
}

fn masque_cache(segment: &Segment, reveles: &BTreeSet<u32>) -> Option<u32> {
    segment.masque.filter(|m| !reveles.contains(m))
}

pub fn besoin_de_segments(segments: &[Segment], reveles: &BTreeSet<u32>) -> bool {
    segments.iter().any(|s| {
        masque_cache(s, reveles).is_some()
            || match &s.contenu {
                Contenu::Emoji(_) => true,
                Contenu::Texte(t) => s.styles.code || emoji::contient_un_emoji(t),
                Contenu::Mention { .. } | Contenu::SautDeLigne => false,
            }
    })
}

#[derive(Default)]
struct Flux {
    lignes: Vec<Vec<SegmentAffiche>>,
}

impl Flux {
    fn ligne(&mut self) -> &mut Vec<SegmentAffiche> {
        if self.lignes.is_empty() {
            self.lignes.push(Vec::new());
        }
        let n = self.lignes.len() - 1;
        &mut self.lignes[n]
    }

    fn pousser(&mut self, segment: SegmentAffiche) {
        self.ligne().push(segment);
    }

    fn espace(&mut self) {
        if let Some(dernier) = self.ligne().last_mut() {
            dernier.espace = true;
        }
    }

    fn saut(&mut self) {
        self.ligne();
        self.lignes.push(Vec::new());
    }

    fn mots(&mut self, texte: &str, mut creer: impl FnMut(&str) -> SegmentAffiche) {
        let mut reste = texte;
        while !reste.is_empty() {
            let blanc = reste.len() - reste.trim_start().len();
            if blanc > 0 {
                self.espace();
                reste = &reste[blanc..];
                continue;
            }
            let fin = reste.find(char::is_whitespace).unwrap_or(reste.len());
            self.pousser(creer(&reste[..fin]));
            reste = &reste[fin..];
        }
    }
}

fn mot_balise(mot: &str, segment: &Segment) -> String {
    let mut corps = styler(echapper(mot), segment.styles);
    if let Some(url) = segment.lien.as_deref().and_then(lien_ouvrable) {
        corps = lien(corps, &format!("{PREFIXE_LIEN}{url}"));
    }
    corps
}

pub fn segments_affiches(
    segments: &[Segment],
    reveles: &BTreeSet<u32>,
    palette: &Palette,
) -> Vec<Vec<SegmentAffiche>> {
    let mut flux = Flux::default();
    for segment in segments {
        if segment.contenu == Contenu::SautDeLigne {
            flux.saut();
            continue;
        }
        if let Some(masque) = masque_cache(segment, reveles) {
            let cible = format!("{PREFIXE_MASQUE}{masque}");
            flux.mots(&texte_du_segment(segment), |mot| SegmentAffiche {
                cible: cible.clone(),
                ..SegmentAffiche::nouveau(GenreSegmentAffiche::Masque, mot)
            });
            continue;
        }
        match &segment.contenu {
            Contenu::Emoji(nom) => flux.pousser(SegmentAffiche {
                cible: nom.clone(),
                ..SegmentAffiche::nouveau(GenreSegmentAffiche::EmojiServeur, &format!(":{nom}:"))
            }),
            Contenu::Mention { .. } => flux.pousser(SegmentAffiche {
                balisage: balisage_du_segment(segment, reveles, palette),
                ..SegmentAffiche::nouveau(GenreSegmentAffiche::Texte, &texte_du_segment(segment))
            }),
            Contenu::Texte(t) if segment.styles.code => {
                if t.starts_with(char::is_whitespace) {
                    flux.espace();
                }
                let code = t.trim();
                if !code.is_empty() {
                    flux.pousser(SegmentAffiche::nouveau(GenreSegmentAffiche::Code, code));
                }
                if t.ends_with(char::is_whitespace) {
                    flux.espace();
                }
            }
            Contenu::Texte(t) => {
                for morceau in emoji::decouper(t) {
                    match morceau {
                        Morceau::Emoji(e) => {
                            flux.pousser(SegmentAffiche::nouveau(GenreSegmentAffiche::Emoji, e))
                        }
                        Morceau::Texte(x) => flux.mots(x, |mot| SegmentAffiche {
                            balisage: mot_balise(mot, segment),
                            ..SegmentAffiche::nouveau(GenreSegmentAffiche::Texte, mot)
                        }),
                    }
                }
            }
            Contenu::SautDeLigne => {}
        }
    }
    for ligne in &mut flux.lignes {
        for i in 1..ligne.len() {
            let (avant, apres) = ligne.split_at_mut(i);
            let (precedent, courant) = (&mut avant[i - 1], &apres[0]);
            precedent.joint = precedent.espace
                && precedent.genre == GenreSegmentAffiche::Masque
                && courant.genre == GenreSegmentAffiche::Masque
                && precedent.cible == courant.cible;
        }
    }
    flux.lignes
}

#[derive(Clone, Copy)]
struct Position {
    citation: u8,
    retrait: u8,
}

struct Aplatissement<'a> {
    reveles: &'a BTreeSet<u32>,
    palette: &'a Palette,
    sortie: Vec<BlocAffiche>,
}

impl Aplatissement<'_> {
    fn pousser(
        &mut self,
        position: Position,
        puce: &mut Option<String>,
        genre: GenreBlocAffiche,
        niveau: u8,
    ) -> &mut BlocAffiche {
        self.sortie.push(BlocAffiche {
            genre,
            balisage: String::new(),
            code: String::new(),
            niveau,
            citation: position.citation,
            retrait: position.retrait,
            puce: puce.take().unwrap_or_default(),
            segments: Vec::new(),
            grand: false,
            espace_avant: true,
        });
        let n = self.sortie.len() - 1;
        &mut self.sortie[n]
    }

    fn texte(
        &mut self,
        position: Position,
        puce: &mut Option<String>,
        genre: GenreBlocAffiche,
        segments: &[Segment],
        niveau: u8,
    ) {
        if !besoin_de_segments(segments, self.reveles) {
            let balise = balisage(segments, self.reveles, self.palette);
            self.pousser(position, puce, genre, niveau).balisage = balise;
            return;
        }
        let lignes = segments_affiches(segments, self.reveles, self.palette);
        for (i, ligne) in lignes.into_iter().enumerate() {
            let bloc = self.pousser(position, puce, genre, niveau);
            bloc.espace_avant = i == 0;
            if ligne.is_empty() {
                bloc.balisage = SEPARATEUR_INVISIBLE.to_string();
            } else {
                bloc.segments = ligne;
            }
        }
    }

    fn aplatir(&mut self, blocs: &[Bloc], position: Position, puce: &mut Option<String>) {
        for bloc in blocs {
            match bloc {
                Bloc::Paragraphe(segments) => {
                    self.texte(position, puce, GenreBlocAffiche::Paragraphe, segments, 0)
                }
                Bloc::Titre { niveau, segments } => self.texte(
                    position,
                    puce,
                    GenreBlocAffiche::Titre,
                    segments,
                    (*niveau).clamp(1, 3),
                ),
                Bloc::Code { texte, .. } => {
                    self.pousser(position, puce, GenreBlocAffiche::Code, 0).code = texte.clone()
                }
                Bloc::Citation(interieur) => self.aplatir(
                    interieur,
                    Position {
                        citation: position.citation.saturating_add(1),
                        ..position
                    },
                    puce,
                ),
                Bloc::Liste { ordonnee, elements } => {
                    for (i, element) in elements.iter().enumerate() {
                        let mut marque = Some(if *ordonnee {
                            format!("{}.", i + 1)
                        } else {
                            "•".to_string()
                        });
                        let interieure = Position {
                            retrait: position.retrait.saturating_add(1),
                            ..position
                        };
                        if element.is_empty() {
                            self.aplatir(&[Bloc::Paragraphe(Vec::new())], interieure, &mut marque);
                        } else {
                            self.aplatir(element, interieure, &mut marque);
                        }
                    }
                }
            }
        }
    }
}

pub fn seulement_des_emoji(blocs: &[BlocAffiche]) -> bool {
    let mut nombre = 0;
    for bloc in blocs {
        if bloc.genre == GenreBlocAffiche::Code {
            return false;
        }
        if !bloc.en_segments() {
            if bloc.balisage.trim_matches(SEPARATEUR_INVISIBLE).trim().is_empty() {
                continue;
            }
            return false;
        }
        for segment in &bloc.segments {
            if !segment.est_emoji() {
                return false;
            }
            nombre += 1;
        }
    }
    (1..=EMOJI_GRANDS_MAX).contains(&nombre)
}

pub fn blocs_affiches(
    rendu: &Rendu,
    reveles: &BTreeSet<u32>,
    palette: &Palette,
    modifie: bool,
) -> Vec<BlocAffiche> {
    let mut a = Aplatissement {
        reveles,
        palette,
        sortie: Vec::new(),
    };
    a.aplatir(
        &rendu.blocs,
        Position {
            citation: 0,
            retrait: 0,
        },
        &mut None,
    );
    let mut sortie = a.sortie;
    if let Some(premier) = sortie.first_mut() {
        premier.espace_avant = false;
    }
    if seulement_des_emoji(&sortie) {
        for bloc in &mut sortie {
            bloc.grand = true;
        }
    }
    if modifie {
        let marque = couleur(echapper("(modifié)"), &palette.discret);
        match sortie.last_mut() {
            Some(dernier) if dernier.genre == GenreBlocAffiche::Paragraphe && dernier.en_segments() => {
                if let Some(precedent) = dernier.segments.last_mut() {
                    precedent.espace = true;
                }
                dernier.segments.push(SegmentAffiche {
                    balisage: marque,
                    ..SegmentAffiche::nouveau(GenreSegmentAffiche::Texte, "(modifié)")
                });
            }
            Some(dernier) if dernier.genre == GenreBlocAffiche::Paragraphe => {
                dernier.balisage = format!("{} {marque}", dernier.balisage);
            }
            _ => sortie.push(BlocAffiche {
                genre: GenreBlocAffiche::Paragraphe,
                balisage: marque,
                code: String::new(),
                niveau: 0,
                citation: 0,
                retrait: 0,
                puce: String::new(),
                segments: Vec::new(),
                grand: false,
                espace_avant: true,
            }),
        }
    }
    sortie
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_client::markdown::analyser;
    use estompe_client::mention::Mentionnables;

    fn rendre(texte: &str) -> Vec<BlocAffiche> {
        let contexte = Mentionnables::nouveau(
            ["carol", "bob"],
            ["Modération"],
            ["everyone"],
            Vec::<String>::new(),
        );
        blocs_affiches(
            &analyser(texte, &contexte),
            &BTreeSet::new(),
            &Palette::default(),
            false,
        )
    }

    fn styled(balisage: &str) -> slint::StyledText {
        slint::StyledText::from_markdown(balisage)
            .unwrap_or_else(|e| panic!("balisage refusé « {balisage} » : {e}"))
    }

    fn texte_de(styled: &slint::StyledText) -> String {
        format!("{styled:?}")
    }

    #[test]
    fn echappe_toute_la_ponctuation_et_garde_le_texte_lisible() {
        let brut = "a*b_c `d` [e](f) <g> & # 1. ~h~ \\";
        let rendu = styled(&echapper(brut));
        let affiche = texte_de(&rendu);
        assert!(
            affiche.contains("a*b_c `d` [e](f) <g> & # 1. ~h~ \\\\"),
            "{affiche}"
        );
        assert!(!affiche.contains("Strong") && !affiche.contains("Link"));
    }

    #[test]
    fn chaque_style_devient_un_balisage_accepte() {
        for (texte, style) in [
            ("**gras**", "Strong"),
            ("*italique*", "Emphasis"),
            ("~~barré~~", "Strikethrough"),
            ("[site](https://plbls.fr)", "Link"),
            ("**gras *et italique***", "Emphasis"),
            ("a **\"cité\"**, b", "Strong"),
            ("**(parenthèses)**", "Strong"),
        ] {
            let blocs = rendre(texte);
            assert_eq!(blocs.len(), 1, "{texte}");
            let rendu = texte_de(&styled(&blocs[0].balisage));
            assert!(rendu.contains(style), "{texte} : {rendu}");
            assert!(
                !rendu.contains("**") && !rendu.contains("~~"),
                "{texte} : {rendu}"
            );
        }
    }

    #[test]
    fn les_espaces_et_la_ponctuation_aux_bords_d_un_style_ne_le_cassent_pas() {
        let mut segment = Segment::texte(" \"gras\", ");
        segment.styles.gras = true;
        segment.styles.italique = true;
        let suite = Segment::texte("fin");
        let balise = balisage(
            &[Segment::texte("x"), segment, suite],
            &BTreeSet::new(),
            &Palette::default(),
        );
        let rendu = texte_de(&styled(&balise));
        assert!(
            rendu.contains("Strong") && rendu.contains("Emphasis"),
            "{rendu}"
        );
        assert!(!rendu.contains('*'), "{rendu}");
    }

    #[test]
    fn les_liens_ne_portent_que_des_adresses_ouvrables() {
        let blocs = rendre("[x](javascript:alert(1)) et https://plbls.fr/a b");
        let rendu = texte_de(&styled(&blocs[0].balisage));
        assert!(!rendu.contains("javascript"), "{rendu}");
        assert!(rendu.contains("u:https://plbls.fr/a"), "{rendu}");
        assert_eq!(analyser_cible("u:javascript:alert(1)"), None);
        assert_eq!(analyser_cible("u:file:///etc/passwd"), None);
        assert_eq!(
            analyser_cible("u:https://plbls.fr/"),
            Some(Cible::Lien("https://plbls.fr/".into()))
        );
        assert_eq!(analyser_cible("autre:x"), None);
    }

    #[test]
    fn mentions_de_personne_cliquables_et_de_role_colorees() {
        let blocs = rendre("salut @carol et @Modération et @everyone");
        let rendu = texte_de(&styled(&blocs[0].balisage));
        assert!(rendu.contains("m:carol"), "{rendu}");
        assert!(!rendu.contains("m:Mod"), "{rendu}");
        assert_eq!(
            analyser_cible("m:Gu%C3%A9%20x"),
            Some(Cible::Mention("Gué x".into()))
        );
        assert_eq!(
            analyser_cible(&format!("m:{}", encoder_cible("a b<c>%"))),
            Some(Cible::Mention("a b<c>%".into()))
        );
    }

    #[test]
    fn un_passage_masque_est_un_pave_cliquable_jusqu_au_clic() {
        let contexte = Mentionnables::default();
        let rendu = analyser("avant ||secret bien gardé|| après", &contexte);
        let cache = blocs_affiches(&rendu, &BTreeSet::new(), &Palette::default(), false);
        assert!(cache[0].balisage.is_empty());
        let masques: Vec<(&str, &str, bool, bool)> = cache[0]
            .segments
            .iter()
            .filter(|s| s.genre == GenreSegmentAffiche::Masque)
            .map(|s| (s.texte.as_str(), s.cible.as_str(), s.espace, s.joint))
            .collect();
        assert_eq!(
            masques,
            [
                ("secret", "s:1", true, true),
                ("bien", "s:1", true, true),
                ("gardé", "s:1", true, false)
            ]
        );
        let revele = blocs_affiches(&rendu, &BTreeSet::from([1]), &Palette::default(), false);
        assert!(!revele[0].en_segments());
        let texte = texte_de(&styled(&revele[0].balisage));
        assert!(
            texte.contains("secret") && !texte.contains("s:1"),
            "{texte}"
        );
        assert_eq!(analyser_cible("s:1"), Some(Cible::Masque(1)));
    }

    #[test]
    fn le_code_en_ligne_passe_par_des_segments_a_chasse_fixe() {
        let blocs = rendre("lance `cargo test --locked` puis **relis**");
        assert_eq!(blocs.len(), 1);
        let resume: Vec<(GenreSegmentAffiche, &str, bool)> = blocs[0]
            .segments
            .iter()
            .map(|s| (s.genre, s.texte.as_str(), s.espace))
            .collect();
        assert_eq!(
            resume,
            [
                (GenreSegmentAffiche::Texte, "lance", true),
                (GenreSegmentAffiche::Code, "cargo test --locked", true),
                (GenreSegmentAffiche::Texte, "puis", true),
                (GenreSegmentAffiche::Texte, "relis", false),
            ]
        );
        assert!(texte_de(&styled(&blocs[0].segments[3].balisage)).contains("Strong"));
        for segment in &blocs[0].segments {
            styled(&segment.balisage);
        }
    }

    #[test]
    fn les_emoji_deviennent_des_segments_et_un_message_d_emoji_s_affiche_en_grand() {
        let contexte = Mentionnables::nouveau(
            Vec::<String>::new(),
            Vec::<String>::new(),
            Vec::<String>::new(),
            ["chat"],
        );
        let blocs = |texte: &str| {
            blocs_affiches(
                &analyser(texte, &contexte),
                &BTreeSet::new(),
                &Palette::default(),
                false,
            )
        };
        let phrase = blocs("Bravo 🎉 [site](https://plbls.fr)👍🏽 :chat: :inconnu:");
        let genres: Vec<(GenreSegmentAffiche, &str)> = phrase[0]
            .segments
            .iter()
            .map(|s| (s.genre, s.texte.as_str()))
            .collect();
        assert_eq!(
            genres,
            [
                (GenreSegmentAffiche::Texte, "Bravo"),
                (GenreSegmentAffiche::Emoji, "🎉"),
                (GenreSegmentAffiche::Texte, "site"),
                (GenreSegmentAffiche::Emoji, "👍🏽"),
                (GenreSegmentAffiche::EmojiServeur, ":chat:"),
                (GenreSegmentAffiche::Texte, ":inconnu:"),
            ]
        );
        assert!(phrase[0].segments[2].balisage.contains("u:https://plbls.fr"));
        assert!(!phrase[0].grand);
        let seuls = blocs("🇫🇷 👩🏾‍💻\n:chat:");
        assert_eq!(seuls.len(), 2);
        assert!(seuls.iter().all(|b| b.grand));
        assert!(!seuls[1].espace_avant && !seuls[0].espace_avant);
        let deux = blocs("a\n\n😀");
        assert!(!deux[0].espace_avant && deux[1].espace_avant);
        assert!(!blocs(&"😀".repeat(EMOJI_GRANDS_MAX + 1))[0].grand);
        assert!(blocs(&"😀".repeat(EMOJI_GRANDS_MAX))[0].grand);
        assert!(!blocs("😀 a")[0].grand);
        assert!(!blocs("> 😀\n```\nx\n```")[0].grand);
        assert!(!blocs("bonjour")[0].en_segments());
        let modifie = blocs_affiches(
            &analyser("😀", &contexte),
            &BTreeSet::new(),
            &Palette::default(),
            true,
        );
        assert!(modifie[0].grand);
        assert_eq!(modifie[0].segments.last().unwrap().texte, "(modifié)");
    }

    #[test]
    fn titres_citations_listes_et_code_sont_aplatis_en_blocs() {
        let blocs = rendre("# Titre\n> cité\n> > profond\n- un\n- deux\n  - sous\n1. premier\n```\nlet x = 1;\n```\nfin");
        let resume: Vec<(GenreBlocAffiche, u8, u8, u8, &str)> = blocs
            .iter()
            .map(|b| (b.genre, b.niveau, b.citation, b.retrait, b.puce.as_str()))
            .collect();
        assert_eq!(
            resume,
            [
                (GenreBlocAffiche::Titre, 1, 0, 0, ""),
                (GenreBlocAffiche::Paragraphe, 0, 1, 0, ""),
                (GenreBlocAffiche::Paragraphe, 0, 2, 0, ""),
                (GenreBlocAffiche::Paragraphe, 0, 0, 1, "•"),
                (GenreBlocAffiche::Paragraphe, 0, 0, 1, "•"),
                (GenreBlocAffiche::Paragraphe, 0, 0, 2, "•"),
                (GenreBlocAffiche::Paragraphe, 0, 0, 1, "1."),
                (GenreBlocAffiche::Code, 0, 0, 0, ""),
                (GenreBlocAffiche::Paragraphe, 0, 0, 0, ""),
            ]
        );
        assert_eq!(blocs[7].code, "let x = 1;");
        for bloc in &blocs {
            styled(&bloc.balisage);
        }
    }

    #[test]
    fn sauts_de_ligne_et_marque_de_modification() {
        let contexte = Mentionnables::default();
        let rendu = analyser("ligne 1\nligne 2", &contexte);
        let blocs = blocs_affiches(&rendu, &BTreeSet::new(), &Palette::default(), true);
        assert_eq!(blocs.len(), 1);
        let texte = texte_de(&styled(&blocs[0].balisage));
        assert!(
            texte.contains("ligne 1") && texte.contains("ligne 2"),
            "{texte}"
        );
        assert!(texte.contains("(modifié)"), "{texte}");
        let code = analyser("```\nx\n```", &contexte);
        let blocs = blocs_affiches(&code, &BTreeSet::new(), &Palette::default(), true);
        assert_eq!(blocs.len(), 2);
    }

    #[test]
    fn un_texte_hostile_ne_produit_jamais_de_balisage_refuse() {
        let contexte = Mentionnables::personnes(["carol"]);
        for texte in [
            "<font color=\"red\">x</font>",
            "</font></u>",
            "``` `` ` ",
            "***a**b*c***",
            "[a](<b>)",
            "\\\\\\",
            "<u>",
            "||**gras masqué**||",
            "@carol**@carol**",
            "- [ ] tâche",
            "---",
            "| a | b |\n|---|---|",
            "$$x$$",
            "[^1]",
            "<script>alert(1)</script>",
            "&amp; &#65;",
            "```\n```",
            "\n\n\n",
        ] {
            for bloc in blocs_affiches(
                &analyser(texte, &contexte),
                &BTreeSet::new(),
                &Palette::default(),
                false,
            ) {
                styled(&bloc.balisage);
            }
        }
    }
}
