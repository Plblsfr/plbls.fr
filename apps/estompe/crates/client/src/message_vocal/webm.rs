use super::ErreurVocal;

const MAITRES: [u32; 5] = [0x1853_8067, 0x1F43_B675, 0x1654_AE6B, 0xAE, 0xA0];
const NUMERO_DE_PISTE: u32 = 0xD7;
const CODEC: u32 = 0x86;
const DONNEES_DU_CODEC: u32 = 0x63A2;
const BLOC_SIMPLE: u32 = 0xA3;
const BLOC: u32 = 0xA1;

fn entier_variable(octets: &[u8], p: usize, garder_marque: bool) -> Option<(u64, usize, bool)> {
    let premier = *octets.get(p)?;
    let longueur = premier.leading_zeros() as usize + 1;
    if longueur > 8 {
        return None;
    }
    let mut valeur = if garder_marque {
        u64::from(premier)
    } else {
        u64::from(premier) & (0xFF >> longueur)
    };
    let mut inconnue = valeur == (0xFF >> longueur);
    for i in 1..longueur {
        let o = *octets.get(p + i)?;
        inconnue &= o == 0xFF;
        valeur = (valeur << 8) | u64::from(o);
    }
    Some((valeur, longueur, inconnue && !garder_marque))
}

pub struct PisteOpus {
    pub preskip: usize,
    pub paquets: Vec<Vec<u8>>,
}

pub fn paquets_webm(octets: &[u8]) -> Result<PisteOpus, ErreurVocal> {
    let mut p = 0;
    let mut piste_courante = None;
    let mut codec_courant = String::new();
    let mut piste_opus = None;
    let mut preskip = 0;
    let mut blocs: Vec<(u64, &[u8])> = Vec::new();
    while p < octets.len() {
        let (id, l1, _) = entier_variable(octets, p, true).ok_or(ErreurVocal::Illisible)?;
        let (taille, l2, inconnue) =
            entier_variable(octets, p + l1, false).ok_or(ErreurVocal::Illisible)?;
        let debut = p + l1 + l2;
        let id = id as u32;
        if MAITRES.contains(&id) {
            if id == 0xAE {
                piste_courante = None;
                codec_courant.clear();
            }
            p = debut;
            continue;
        }
        if inconnue {
            return Err(ErreurVocal::Illisible);
        }
        let fin = debut.saturating_add(taille as usize).min(octets.len());
        let contenu = &octets[debut..fin];
        match id {
            NUMERO_DE_PISTE => {
                piste_courante = Some(contenu.iter().fold(0u64, |v, o| (v << 8) | u64::from(*o)));
            }
            CODEC => codec_courant = String::from_utf8_lossy(contenu).into_owned(),
            DONNEES_DU_CODEC if codec_courant == "A_OPUS" || contenu.starts_with(b"OpusHead") => {
                preskip = super::opus::preskip_de_la_tete(contenu).unwrap_or(0);
            }
            BLOC_SIMPLE | BLOC => {
                if let Some((piste, l, _)) = entier_variable(contenu, 0, false) {
                    if contenu
                        .get(l + 2)
                        .is_some_and(|drapeaux| drapeaux & 0x06 != 0)
                    {
                        return Err(ErreurVocal::Illisible);
                    }
                    if let Some(donnees) = contenu.get(l + 3..) {
                        blocs.push((piste, donnees));
                    }
                }
            }
            _ => {}
        }
        if codec_courant == "A_OPUS" && piste_opus.is_none() {
            piste_opus = piste_courante;
        }
        p = fin;
    }
    let piste = piste_opus.ok_or(ErreurVocal::FormatInconnu)?;
    Ok(PisteOpus {
        preskip,
        paquets: blocs
            .into_iter()
            .filter(|(n, _)| *n == piste)
            .map(|(_, d)| d.to_vec())
            .collect(),
    })
}
