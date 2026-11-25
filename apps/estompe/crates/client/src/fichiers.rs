use crate::types::{Attachment, AttachmentKind};
use std::collections::HashMap;

pub const TAILLE_MAXIMALE: u64 = 10 * 1024 * 1024;
pub const VIDEO_MAXIMALE_PAR_DEFAUT: u64 = 100 * 1024 * 1024;
pub const COTE_MAXIMAL: u32 = 1920;
pub const DUREE_MAXIMALE_VOCAL_MS: u64 = 5 * 60 * 1000;
pub const MIMES_COMPRESSIBLES: [&str; 3] = ["image/jpeg", "image/png", "image/webp"];
pub const MIMES_IMAGE: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];
pub const MIMES_VIDEO: [&str; 2] = ["video/mp4", "video/webm"];
pub const PIECES_PAR_MESSAGE: usize = 10;
pub const MIME_PAR_DEFAUT: &str = "application/octet-stream";

const SEUIL_DE_COMPRESSION: u64 = 300 * 1024;
const LONGUEUR_MAXIMALE_DE_NOM: usize = 200;
const LONGUEUR_MAXIMALE_D_EXTENSION: usize = 20;
const NOM_PAR_DEFAUT: &str = "fichier";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dimensions {
    pub largeur: u32,
    pub hauteur: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeEnregistrement {
    pub mime: &'static str,
    pub extension: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategoriePiece {
    Vocal,
    Expiree,
    Replay,
    Image,
    Video,
    Audio,
    Fichier,
}

fn arrondi(x: f64) -> f64 {
    (x + 0.5).floor()
}

pub fn est_compressible(mime: &str) -> bool {
    MIMES_COMPRESSIBLES.contains(&mime)
}

pub fn ajuster_dans(largeur: u32, hauteur: u32, cote_max: u32) -> Dimensions {
    let plus_grand = f64::from(largeur.max(hauteur));
    let k = (f64::from(cote_max) / plus_grand).min(1.0);
    Dimensions {
        largeur: arrondi(f64::from(largeur) * k).max(1.0) as u32,
        hauteur: arrondi(f64::from(hauteur) * k).max(1.0) as u32,
    }
}

pub fn ajuster(largeur: u32, hauteur: u32) -> Dimensions {
    ajuster_dans(largeur, hauteur, COTE_MAXIMAL)
}

pub fn changer_extension(nom: &str, extension: &str) -> String {
    let radical = match nom.rfind('.') {
        Some(i) if i > 0 => &nom[..i],
        _ => nom,
    };
    format!("{radical}.{extension}")
}

pub fn dimensions_a_envoyer(
    mime: &str,
    taille: u64,
    largeur: u32,
    hauteur: u32,
) -> Option<Dimensions> {
    if !est_compressible(mime) {
        return None;
    }
    let cible = ajuster(largeur, hauteur);
    let inchangee = cible.largeur == largeur && cible.hauteur == hauteur;
    if taille < SEUIL_DE_COMPRESSION && inchangee {
        return None;
    }
    Some(cible)
}

pub fn garder_la_version_compressee(taille_compressee: u64, taille_originale: u64) -> bool {
    taille_compressee < taille_originale
}

pub fn verifier_taille(taille: u64) -> Option<&'static str> {
    if taille > TAILLE_MAXIMALE {
        Some("Fichier trop gros (10 Mo maximum)")
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limites {
    pub fichier: u64,
    pub video: u64,
    pub nombre: usize,
}

impl Limites {
    pub fn du_compte(max_video_size: Option<i64>) -> Self {
        Self {
            fichier: TAILLE_MAXIMALE,
            video: max_video_size
                .and_then(|v| u64::try_from(v).ok())
                .filter(|v| *v > 0)
                .unwrap_or(VIDEO_MAXIMALE_PAR_DEFAUT),
            nombre: PIECES_PAR_MESSAGE,
        }
    }

    pub fn taille_pour(&self, mime: &str) -> u64 {
        if est_une_video(mime) {
            self.video
        } else {
            self.fichier
        }
    }
}

impl Default for Limites {
    fn default() -> Self {
        Self::du_compte(None)
    }
}

pub fn est_une_video(mime: &str) -> bool {
    mime == "video/mp4"
}

pub fn type_mime_du_nom(nom: &str) -> &'static str {
    let extension = nom
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "ogg" | "opus" => "audio/ogg",
        "m4a" => "audio/mp4",
        "wav" => "audio/wav",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "json" => "application/json",
        "txt" | "md" | "log" => "text/plain",
        _ => MIME_PAR_DEFAUT,
    }
}

pub fn megaoctets(octets: u64) -> String {
    format!("{} Mo", (octets + 524_288) / 1_048_576)
}

pub fn refus_de_piece(nom: &str, taille: u64, mime: &str, limites: &Limites) -> Option<String> {
    let limite = limites.taille_pour(mime);
    (taille > limite).then(|| {
        let genre = if est_une_video(mime) {
            "Vidéo trop grosse"
        } else {
            "Fichier trop gros"
        };
        format!(
            "{genre} : « {nom} » fait {}, {} maximum.",
            crate::format::taille_lisible(taille),
            megaoctets(limite)
        )
    })
}

pub fn refus_du_nombre(deja: usize, limites: &Limites) -> Option<String> {
    (deja >= limites.nombre).then(|| format!("{} fichiers maximum par message.", limites.nombre))
}

pub fn type_d_enregistrement(pris_en_charge: impl Fn(&str) -> bool) -> Option<TypeEnregistrement> {
    const CANDIDATS: [TypeEnregistrement; 3] = [
        TypeEnregistrement {
            mime: "audio/webm;codecs=opus",
            extension: "webm",
        },
        TypeEnregistrement {
            mime: "audio/ogg;codecs=opus",
            extension: "ogg",
        },
        TypeEnregistrement {
            mime: "audio/mp4",
            extension: "m4a",
        },
    ];
    CANDIDATS.into_iter().find(|c| pris_en_charge(c.mime))
}

pub fn ratio_de_position(x: f64, gauche: f64, largeur: f64) -> f64 {
    if largeur > 0.0 {
        ((x - gauche) / largeur).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub fn niveau_sonore(echantillons: &[f32]) -> u8 {
    let somme: f64 = echantillons
        .iter()
        .map(|&s| f64::from(s) * f64::from(s))
        .sum();
    let moyenne = somme / echantillons.len().max(1) as f64;
    let db = 10.0 * (moyenne + 1e-10).log10();
    arrondi(((db + 60.0) * 100.0 / 60.0).clamp(0.0, 100.0)) as u8
}

pub fn forme_d_onde(niveaux: &[i64], barres: usize) -> Vec<i64> {
    if niveaux.is_empty() {
        return vec![0];
    }
    let n = barres.min(niveaux.len());
    (0..n)
        .map(|i| {
            let debut = i * niveaux.len() / n;
            let fin = ((i + 1) * niveaux.len() / n).max(debut + 1);
            let tranche = &niveaux[debut..fin];
            let somme: i64 = tranche.iter().sum();
            arrondi(somme as f64 / tranche.len() as f64) as i64
        })
        .collect()
}

pub fn categorie(piece: &Attachment) -> CategoriePiece {
    let mime = piece.mime.as_deref().unwrap_or_default();
    let genre = piece.kind;
    if genre == Some(AttachmentKind::Voice) {
        CategoriePiece::Vocal
    } else if piece.expired == Some(true) {
        CategoriePiece::Expiree
    } else if genre == Some(AttachmentKind::Video) {
        CategoriePiece::Replay
    } else if MIMES_IMAGE.contains(&mime) {
        CategoriePiece::Image
    } else if MIMES_VIDEO.contains(&mime) {
        CategoriePiece::Video
    } else if mime.starts_with("audio/") {
        CategoriePiece::Audio
    } else {
        CategoriePiece::Fichier
    }
}

pub type Ecouteur = Box<dyn Fn(SourceMedia)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceMedia {
    Film,
    Vocal,
    Sillon,
}

#[derive(Default)]
pub struct Medias {
    ecouteurs: HashMap<u64, Ecouteur>,
    ordre: Vec<u64>,
    suivant: u64,
}

impl Medias {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn demarre(&self, source: SourceMedia) {
        for id in &self.ordre {
            if let Some(ecouteur) = self.ecouteurs.get(id) {
                ecouteur(source);
            }
        }
    }

    pub fn ecouter(&mut self, ecouteur: impl Fn(SourceMedia) + 'static) -> u64 {
        let id = self.suivant;
        self.suivant += 1;
        self.ecouteurs.insert(id, Box::new(ecouteur));
        self.ordre.push(id);
        id
    }

    pub fn ne_plus_ecouter(&mut self, id: u64) {
        self.ecouteurs.remove(&id);
        self.ordre.retain(|i| *i != id);
    }
}

fn est_invisible(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00AD}'
                | '\u{200B}'..='\u{200F}'
                | '\u{2028}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{FEFF}'
                | '\u{E0000}'..='\u{E007F}'
        )
}

fn est_reserve_de_windows(nom: &str) -> bool {
    let radical = nom.split('.').next().unwrap_or_default();
    let majuscules = radical.trim_end_matches(' ').to_uppercase();
    if matches!(
        majuscules.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    let Some(numero) = majuscules
        .strip_prefix("COM")
        .or_else(|| majuscules.strip_prefix("LPT"))
    else {
        return false;
    };
    let mut caracteres = numero.chars();
    matches!(
        (caracteres.next(), caracteres.next()),
        (Some('0'..='9' | '¹' | '²' | '³'), None)
    )
}

fn separer_extension(nom: &str) -> (&str, &str) {
    match nom.rfind('.') {
        Some(i) if i > 0 => nom.split_at(i),
        _ => (nom, ""),
    }
}

fn couper_a(texte: &str, octets: usize) -> &str {
    let mut fin = octets.min(texte.len());
    while !texte.is_char_boundary(fin) {
        fin -= 1;
    }
    &texte[..fin]
}

fn sans_fin_genante(texte: &str) -> &str {
    texte.trim_end_matches(['.', ' '])
}

fn tronquer(nom: &str) -> String {
    if nom.len() <= LONGUEUR_MAXIMALE_DE_NOM {
        return nom.to_string();
    }
    let (radical, extension) = separer_extension(nom);
    if extension.len() > LONGUEUR_MAXIMALE_D_EXTENSION {
        return sans_fin_genante(couper_a(nom, LONGUEUR_MAXIMALE_DE_NOM)).to_string();
    }
    let radical = sans_fin_genante(couper_a(
        radical,
        LONGUEUR_MAXIMALE_DE_NOM - extension.len(),
    ));
    if radical.is_empty() {
        return NOM_PAR_DEFAUT.to_string();
    }
    format!("{radical}{extension}")
}

pub fn nom_de_fichier_sur(nom: &str) -> String {
    let composant = nom
        .rsplit(['/', '\\'])
        .find(|c| !c.is_empty())
        .unwrap_or_default();
    let nettoye: String = composant
        .chars()
        .filter(|c| !est_invisible(*c))
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let rogne = sans_fin_genante(nettoye.trim_start_matches(' '));
    if rogne.is_empty() {
        return NOM_PAR_DEFAUT.to_string();
    }
    let mut resultat = tronquer(rogne);
    if est_reserve_de_windows(&resultat) {
        resultat = tronquer(&format!("_{resultat}"));
    }
    if resultat.is_empty() {
        NOM_PAR_DEFAUT.to_string()
    } else {
        resultat
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn dim(largeur: u32, hauteur: u32) -> Dimensions {
        Dimensions { largeur, hauteur }
    }

    #[test]
    fn ajuster_reduit_en_gardant_les_proportions() {
        assert_eq!(ajuster(4000, 2000), dim(1920, 960));
        assert_eq!(ajuster(1000, 3000), dim(640, 1920));
    }

    #[test]
    fn ajuster_n_agrandit_jamais() {
        assert_eq!(ajuster(800, 600), dim(800, 600));
    }

    #[test]
    fn ajuster_reste_au_moins_a_un_pixel() {
        assert_eq!(ajuster(100_000, 1).hauteur, 1);
        assert_eq!(ajuster(0, 0), dim(1, 1));
    }

    #[test]
    fn ajuster_dans_un_carre_de_miniature() {
        assert_eq!(ajuster_dans(1600, 900, 320), dim(320, 180));
        assert_eq!(ajuster_dans(200, 100, 320), dim(200, 100));
    }

    #[test]
    fn changer_extension_remplace_l_extension() {
        assert_eq!(changer_extension("photo.png", "webp"), "photo.webp");
        assert_eq!(changer_extension("a.b.jpeg", "webp"), "a.b.webp");
        assert_eq!(changer_extension("sans", "webp"), "sans.webp");
        assert_eq!(changer_extension(".cache", "webp"), ".cache.webp");
    }

    #[test]
    fn verifier_taille_refuse_au_dela_de_10_mo() {
        assert_eq!(verifier_taille(10 * 1024 * 1024), None);
        assert!(verifier_taille(10 * 1024 * 1024 + 1)
            .unwrap()
            .contains("10 Mo"));
    }

    #[test]
    fn une_piece_au_dessus_de_la_limite_est_refusee_avec_la_limite() {
        let limites = Limites::du_compte(Some(50 * 1024 * 1024));
        assert_eq!(
            refus_de_piece("a.bin", 10 * 1024 * 1024, MIME_PAR_DEFAUT, &limites),
            None
        );
        assert_eq!(
            refus_de_piece("gros.bin", 12 * 1024 * 1024, MIME_PAR_DEFAUT, &limites).as_deref(),
            Some("Fichier trop gros : « gros.bin » fait 12.0 Mo, 10 Mo maximum.")
        );
        assert_eq!(
            refus_de_piece("clip.mp4", 40 * 1024 * 1024, "video/mp4", &limites),
            None
        );
        assert!(
            refus_de_piece("clip.mp4", 60 * 1024 * 1024, "video/mp4", &limites)
                .unwrap()
                .starts_with("Vidéo trop grosse")
        );
        assert_eq!(Limites::du_compte(None).video, VIDEO_MAXIMALE_PAR_DEFAUT);
        assert_eq!(Limites::du_compte(Some(0)).video, VIDEO_MAXIMALE_PAR_DEFAUT);
        assert_eq!(refus_du_nombre(9, &limites), None);
        assert_eq!(
            refus_du_nombre(10, &limites).as_deref(),
            Some("10 fichiers maximum par message.")
        );
    }

    #[test]
    fn le_type_d_une_piece_vient_de_son_extension() {
        assert_eq!(type_mime_du_nom("Photo.JPG"), "image/jpeg");
        assert_eq!(type_mime_du_nom("capture.png"), "image/png");
        assert_eq!(type_mime_du_nom("clip.mp4"), "video/mp4");
        assert_eq!(type_mime_du_nom("sans"), MIME_PAR_DEFAUT);
        assert_eq!(type_mime_du_nom("archive.tar.inconnu"), MIME_PAR_DEFAUT);
        assert!(est_une_video("video/mp4") && !est_une_video("video/webm"));
    }

    #[test]
    fn type_d_enregistrement_prefere_opus_webm_puis_ogg_puis_aac() {
        let webm = TypeEnregistrement {
            mime: "audio/webm;codecs=opus",
            extension: "webm",
        };
        let ogg = TypeEnregistrement {
            mime: "audio/ogg;codecs=opus",
            extension: "ogg",
        };
        let mp4 = TypeEnregistrement {
            mime: "audio/mp4",
            extension: "m4a",
        };
        assert_eq!(type_d_enregistrement(|_| true), Some(webm));
        assert_eq!(
            type_d_enregistrement(|t| t == "audio/ogg;codecs=opus"),
            Some(ogg)
        );
        assert_eq!(type_d_enregistrement(|t| t == "audio/mp4"), Some(mp4));
        assert_eq!(type_d_enregistrement(|_| false), None);
    }

    #[test]
    fn niveau_sonore_mesure_le_silence_a_0_et_un_signal_plein_a_100() {
        assert_eq!(niveau_sonore(&[0.0; 256]), 0);
        assert_eq!(niveau_sonore(&[1.0; 256]), 100);
        assert_eq!(niveau_sonore(&[]), 0);
        assert_eq!(niveau_sonore(&[0.1; 64]), 67);
    }

    #[test]
    fn forme_d_onde_reduit_par_moyenne_de_tranches() {
        assert_eq!(forme_d_onde(&[10, 30, 50, 70], 2), vec![20, 60]);
        assert_eq!(forme_d_onde(&[40, 40], 40), vec![40, 40]);
        assert_eq!(forme_d_onde(&[1, 2], 1), vec![2]);
    }

    #[test]
    fn forme_d_onde_renvoie_toujours_au_moins_une_valeur() {
        assert_eq!(forme_d_onde(&[], 40), vec![0]);
    }

    #[test]
    fn ratio_de_position_se_place_selon_le_clic_dans_la_barre() {
        assert_eq!(ratio_de_position(150.0, 100.0, 200.0), 0.25);
        assert_eq!(ratio_de_position(100.0, 100.0, 200.0), 0.0);
        assert_eq!(ratio_de_position(300.0, 100.0, 200.0), 1.0);
    }

    #[test]
    fn ratio_de_position_borne_entre_0_et_1_et_gere_une_largeur_nulle() {
        assert_eq!(ratio_de_position(50.0, 100.0, 200.0), 0.0);
        assert_eq!(ratio_de_position(900.0, 100.0, 200.0), 1.0);
        assert_eq!(ratio_de_position(150.0, 100.0, 0.0), 0.0);
    }

    #[test]
    fn la_compression_ne_vise_que_les_images_non_animees() {
        assert!(est_compressible("image/png"));
        assert!(est_compressible("image/jpeg"));
        assert!(est_compressible("image/webp"));
        assert!(!est_compressible("image/gif"));
        assert!(!est_compressible("video/mp4"));
        assert_eq!(
            dimensions_a_envoyer("image/gif", 5_000_000, 4000, 2000),
            None
        );
        assert_eq!(
            dimensions_a_envoyer("image/png", 5_000_000, 4000, 2000),
            Some(dim(1920, 960))
        );
        assert_eq!(dimensions_a_envoyer("image/png", 100_000, 800, 600), None);
        assert_eq!(
            dimensions_a_envoyer("image/png", 400_000, 800, 600),
            Some(dim(800, 600))
        );
        assert!(garder_la_version_compressee(10, 11));
        assert!(!garder_la_version_compressee(11, 11));
    }

    #[test]
    fn les_limites_du_front_sont_reprises() {
        assert_eq!(TAILLE_MAXIMALE, 10_485_760);
        assert_eq!(VIDEO_MAXIMALE_PAR_DEFAUT, 104_857_600);
        assert_eq!(DUREE_MAXIMALE_VOCAL_MS, 300_000);
    }

    fn piece(mime: &str, genre: AttachmentKind, expiree: bool) -> Attachment {
        Attachment {
            mime: Some(mime.to_string()),
            kind: Some(genre),
            expired: Some(expiree),
            ..Attachment::default()
        }
    }

    #[test]
    fn la_categorie_d_une_piece_jointe() {
        use AttachmentKind::{File, Video, Voice};
        for m in ["image/png", "image/jpeg", "image/gif", "image/webp"] {
            assert_eq!(categorie(&piece(m, File, false)), CategoriePiece::Image);
        }
        for m in ["video/mp4", "video/webm"] {
            assert_eq!(categorie(&piece(m, File, false)), CategoriePiece::Video);
        }
        assert_eq!(
            categorie(&piece("application/pdf", File, false)),
            CategoriePiece::Fichier
        );
        assert_eq!(
            categorie(&piece("image/svg+xml", File, false)),
            CategoriePiece::Fichier
        );
        assert_eq!(
            categorie(&piece("video/mp4", Video, false)),
            CategoriePiece::Replay
        );
        assert_eq!(
            categorie(&piece("video/mp4", Video, true)),
            CategoriePiece::Expiree
        );
        assert_eq!(
            categorie(&piece("image/png", File, true)),
            CategoriePiece::Expiree
        );
        assert_eq!(
            categorie(&piece("audio/webm", Voice, false)),
            CategoriePiece::Vocal
        );
        assert_eq!(
            categorie(&piece("audio/webm", Voice, true)),
            CategoriePiece::Vocal
        );
        assert_eq!(categorie(&Attachment::default()), CategoriePiece::Fichier);
        for m in ["audio/mpeg", "audio/ogg", "audio/wav", "audio/webm"] {
            assert_eq!(categorie(&piece(m, File, false)), CategoriePiece::Audio);
        }
        assert_eq!(
            categorie(&piece("audio/mpeg", File, true)),
            CategoriePiece::Expiree
        );
    }

    #[test]
    fn les_medias_previennent_les_ecouteurs_dans_l_ordre() {
        let recu: Rc<RefCell<Vec<(u8, SourceMedia)>>> = Rc::default();
        let mut medias = Medias::new();
        let a = recu.clone();
        let premier = medias.ecouter(move |s| a.borrow_mut().push((1, s)));
        let b = recu.clone();
        medias.ecouter(move |s| b.borrow_mut().push((2, s)));
        medias.demarre(SourceMedia::Film);
        medias.ne_plus_ecouter(premier);
        medias.demarre(SourceMedia::Sillon);
        assert_eq!(
            *recu.borrow(),
            vec![
                (1, SourceMedia::Film),
                (2, SourceMedia::Film),
                (2, SourceMedia::Sillon)
            ]
        );
    }

    fn est_sur(sortie: &str) -> bool {
        !sortie.is_empty()
            && sortie.len() <= 200
            && !sortie.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*'])
            && !sortie.chars().any(|c| c.is_control())
            && !sortie.ends_with(['.', ' '])
            && !sortie.starts_with(' ')
            && sortie != "."
            && sortie != ".."
            && !est_reserve_de_windows(sortie)
    }

    #[test]
    fn un_nom_ordinaire_reste_inchange() {
        for nom in [
            "photo.png",
            "rapport final (v2).pdf",
            "archive.tar.gz",
            ".gitignore",
            "été 2026 — plage.jpg",
            "日本語.txt",
            "a",
            "sans_extension",
            "Rocket_League_2026-10-13_21-05-09.mp4",
        ] {
            assert_eq!(nom_de_fichier_sur(nom), nom);
        }
    }

    #[test]
    fn les_chemins_sont_reduits_au_dernier_composant() {
        assert_eq!(nom_de_fichier_sur("../../etc/passwd"), "passwd");
        assert_eq!(
            nom_de_fichier_sur("..\\..\\Windows\\system.ini"),
            "system.ini"
        );
        assert_eq!(nom_de_fichier_sur("/etc/shadow"), "shadow");
        assert_eq!(nom_de_fichier_sur("C:\\Windows\\win.ini"), "win.ini");
        assert_eq!(
            nom_de_fichier_sur("dossier/sous/fichier.txt"),
            "fichier.txt"
        );
        assert_eq!(nom_de_fichier_sur("a/b\\c/d.txt"), "d.txt");
        assert_eq!(nom_de_fichier_sur("//serveur/partage/x.bin"), "x.bin");
        assert_eq!(nom_de_fichier_sur("dossier/"), "dossier");
        assert_eq!(nom_de_fichier_sur("../x.txt"), "x.txt");
    }

    #[test]
    fn les_noms_de_navigation_deviennent_un_nom_par_defaut() {
        for nom in [
            "", ".", "..", "...", "../", "..\\", "/", "\\", "//", "./", ".\\", "../..", " ", "   ",
            ". .", ". . .",
        ] {
            assert_eq!(nom_de_fichier_sur(nom), "fichier", "entrée {nom:?}");
        }
    }

    #[test]
    fn les_caracteres_reserves_de_windows_sont_remplaces() {
        assert_eq!(
            nom_de_fichier_sur("a<b>c:d\"e|f?g*h.txt"),
            "a_b_c_d_e_f_g_h.txt"
        );
        assert_eq!(nom_de_fichier_sur("C:evil.txt"), "C_evil.txt");
        assert_eq!(nom_de_fichier_sur("fichier.txt:flux"), "fichier.txt_flux");
        assert_eq!(
            nom_de_fichier_sur("fichier.txt::$DATA"),
            "fichier.txt__$DATA"
        );
        assert_eq!(nom_de_fichier_sur("???"), "___");
    }

    #[test]
    fn les_noms_reserves_de_windows_sont_prefixes() {
        for nom in [
            "CON",
            "con",
            "Con.txt",
            "PRN",
            "AUX.log",
            "NUL",
            "nul.tar.gz",
            "COM1",
            "com9.txt",
            "COM0",
            "LPT1",
            "lpt9.dat",
            "LPT0",
            "COM¹",
            "LPT².txt",
            "COM³.bin",
            "CONIN$",
            "CONOUT$",
            "CON .txt",
            "CON ",
            "NUL. ",
            "CON.",
            "con...",
        ] {
            let sortie = nom_de_fichier_sur(nom);
            assert!(sortie.starts_with('_'), "{nom:?} -> {sortie:?}");
            assert!(est_sur(&sortie), "{nom:?} -> {sortie:?}");
        }
        assert_eq!(nom_de_fichier_sur("CON"), "_CON");
        assert_eq!(nom_de_fichier_sur("NUL.txt"), "_NUL.txt");
        assert_eq!(nom_de_fichier_sur("COM1.txt"), "_COM1.txt");
    }

    #[test]
    fn les_noms_qui_ressemblent_a_un_peripherique_sans_en_etre_un_sont_gardes() {
        for nom in [
            "CONSOLE.txt",
            "COM10",
            "COM",
            "LPT",
            "LPT10.txt",
            "NULL",
            "AUXILIAIRE",
            "ACON",
            ".CON",
            "COMX",
        ] {
            assert_eq!(nom_de_fichier_sur(nom), nom);
        }
        assert_eq!(nom_de_fichier_sur("CON.CON_x"), "_CON.CON_x");
    }

    #[test]
    fn les_points_et_espaces_de_fin_sont_retires() {
        assert_eq!(nom_de_fichier_sur("fichier.txt."), "fichier.txt");
        assert_eq!(nom_de_fichier_sur("fichier.txt..."), "fichier.txt");
        assert_eq!(nom_de_fichier_sur("fichier.txt "), "fichier.txt");
        assert_eq!(nom_de_fichier_sur("fichier.txt . . "), "fichier.txt");
        assert_eq!(nom_de_fichier_sur("  fichier.txt"), "fichier.txt");
        assert_eq!(nom_de_fichier_sur("a ."), "a");
    }

    #[test]
    fn les_caracteres_de_controle_et_invisibles_sont_retires() {
        assert_eq!(nom_de_fichier_sur("a\0b.txt"), "ab.txt");
        assert_eq!(nom_de_fichier_sur("a\nb\r\tc.txt"), "abc.txt");
        assert_eq!(nom_de_fichier_sur("a\u{1b}[31mb.txt"), "a[31mb.txt");
        assert_eq!(nom_de_fichier_sur("a\u{7f}b"), "ab");
        assert_eq!(nom_de_fichier_sur("a\u{85}b"), "ab");
        assert_eq!(nom_de_fichier_sur("\0\0\0"), "fichier");
        assert_eq!(
            nom_de_fichier_sur("fichier\u{202e}gpj.exe"),
            "fichiergpj.exe"
        );
        assert_eq!(nom_de_fichier_sur("a\u{200b}b\u{feff}c"), "abc");
        assert_eq!(nom_de_fichier_sur("a\u{2028}b\u{2029}c"), "abc");
        assert_eq!(nom_de_fichier_sur("..\u{0}/x.txt"), "x.txt");
        assert_eq!(nom_de_fichier_sur("..\u{0}"), "fichier");
    }

    #[test]
    fn un_nom_trop_long_est_coupe_en_gardant_l_extension() {
        let long = format!("{}.pdf", "a".repeat(500));
        let sortie = nom_de_fichier_sur(&long);
        assert_eq!(sortie.len(), 200);
        assert!(sortie.ends_with(".pdf"));
        assert!(sortie.starts_with("aaaa"));
        let juste = format!("{}.pdf", "b".repeat(196));
        assert_eq!(nom_de_fichier_sur(&juste), juste);
        let trop = format!("{}.pdf", "b".repeat(197));
        assert_eq!(nom_de_fichier_sur(&trop).len(), 200);
    }

    #[test]
    fn la_coupe_respecte_les_caracteres_multi_octets() {
        let long = format!("{}.txt", "é".repeat(300));
        let sortie = nom_de_fichier_sur(&long);
        assert!(sortie.len() <= 200);
        assert!(sortie.ends_with(".txt"));
        assert!(sortie.trim_end_matches(".txt").chars().all(|c| c == 'é'));
        let emojis = "😀".repeat(100);
        assert!(nom_de_fichier_sur(&emojis).len() <= 200);
    }

    #[test]
    fn une_extension_demesuree_est_coupee_avec_le_nom() {
        let nom = format!("a.{}", "x".repeat(400));
        let sortie = nom_de_fichier_sur(&nom);
        assert!(est_sur(&sortie));
        assert_eq!(sortie.len(), 200);
    }

    #[test]
    fn la_coupe_ne_laisse_ni_point_ni_espace_a_la_fin() {
        let nom = format!("{} .txt", "a".repeat(195));
        let sortie = nom_de_fichier_sur(&nom);
        assert!(est_sur(&sortie), "{sortie:?}");
        let nom = format!("{}{}.t", "a".repeat(197), "...");
        assert!(est_sur(&nom_de_fichier_sur(&nom)));
        let nom = format!("{}. {}", "a".repeat(198), "b".repeat(300));
        assert!(est_sur(&nom_de_fichier_sur(&nom)));
    }

    #[test]
    fn la_coupe_ne_fabrique_pas_de_nom_reserve() {
        let nom = format!("CON{}.txt", " ".repeat(300));
        assert!(est_sur(&nom_de_fichier_sur(&nom)));
        let nom = format!("CON{}x.txt", " ".repeat(300));
        assert!(est_sur(&nom_de_fichier_sur(&nom)));
        let nom = format!("{}{}", "CON", "m".repeat(300));
        assert!(est_sur(&nom_de_fichier_sur(&nom)));
        let nom = format!("NUL{}", ".".repeat(300));
        assert!(est_sur(&nom_de_fichier_sur(&nom)));
        let nom = format!("{}.txt", "a".repeat(196));
        let sortie = nom_de_fichier_sur(&format!("CON.{nom}"));
        assert!(est_sur(&sortie));
    }

    #[test]
    fn jamais_de_separateur_ni_de_remontee_quelle_que_soit_l_entree() {
        let entrees = [
            "..\\",
            "../",
            "..//..//x",
            "....//....//x",
            "..;/x",
            "%2e%2e%2fx",
            "..%5cx",
            "/",
            "\\\\?\\C:\\x",
            "\\\\.\\COM1",
            "\\\\.\\pipe\\x",
            "~/x",
            "$HOME/x",
            "`id`",
            "$(id).txt",
            "-rf",
            "--help",
            "a/../../b",
            "x\\..\\..\\y",
            "con/con",
            "aux\\nul",
            "NUL:",
            "COM1:",
            "LPT1:.txt",
            "C:",
            "C:\\",
            "file.txt:Zone.Identifier",
            "<script>.html",
            " . ",
            "\u{202e}\u{202e}",
            "\0",
            "ab\u{0}",
        ];
        for entree in entrees {
            let sortie = nom_de_fichier_sur(entree);
            assert!(est_sur(&sortie), "{entree:?} -> {sortie:?}");
        }
    }

    #[test]
    fn la_sortie_est_stable() {
        for entree in [
            "../CON.txt",
            "a<b",
            "x.txt.",
            "  CON  ",
            "é\u{0}.txt",
            &"z".repeat(400),
        ] {
            let une_fois = nom_de_fichier_sur(entree);
            assert_eq!(nom_de_fichier_sur(&une_fois), une_fois, "{entree:?}");
        }
    }

    #[test]
    fn l_extension_est_conservee_apres_nettoyage() {
        assert_eq!(nom_de_fichier_sur("rapport<1>.pdf"), "rapport_1_.pdf");
        assert_eq!(nom_de_fichier_sur("CON.pdf"), "_CON.pdf");
        assert_eq!(nom_de_fichier_sur("../photo.JPG"), "photo.JPG");
        assert_eq!(nom_de_fichier_sur("fichier.tar.gz "), "fichier.tar.gz");
    }
}
