const MARQUES: &[&[u8; 4]] = &[
    b"isom", b"iso2", b"iso4", b"iso5", b"iso6", b"mp41", b"mp42", b"avc1", b"M4V ", b"dash",
];
const VIDEO: &[&[u8; 4]] = &[b"avc1", b"avc3", b"hvc1", b"hev1", b"av01"];
const AUDIO: &[&[u8; 4]] = &[b"mp4a", b"Opus"];

struct Boite {
    typ: [u8; 4],
    corps: usize,
    fin: usize,
}

fn boite(b: &[u8], at: usize) -> Option<Boite> {
    if at.checked_add(8)? > b.len() {
        return None;
    }
    let taille = u32::from_be_bytes(b[at..at + 4].try_into().ok()?) as usize;
    let typ: [u8; 4] = b[at + 4..at + 8].try_into().ok()?;
    let (taille, corps) = match taille {
        1 => {
            if at + 16 > b.len() {
                return None;
            }
            let grand = u64::from_be_bytes(b[at + 8..at + 16].try_into().ok()?);
            if grand > b.len() as u64 {
                return None;
            }
            (grand as usize, at + 16)
        }
        0 => (b.len() - at, at + 8),
        n => (n, at + 8),
    };
    if taille < corps - at || at + taille > b.len() {
        return None;
    }
    Some(Boite {
        typ,
        corps,
        fin: at + taille,
    })
}

fn enfants(b: &[u8], de: usize, a: usize, mut f: impl FnMut(&Boite) -> bool) {
    let mut at = de;
    while at < a {
        let Some(bo) = boite(&b[..a], at) else {
            return;
        };
        if f(&bo) {
            return;
        }
        at = bo.fin;
    }
}

fn duree_mvhd(b: &[u8]) -> u64 {
    let (echelle, duree) = if b.len() >= 20 && b[0] == 0 {
        (
            u64::from(u32::from_be_bytes(b[12..16].try_into().unwrap())),
            u64::from(u32::from_be_bytes(b[16..20].try_into().unwrap())),
        )
    } else if b.len() >= 32 && b[0] == 1 {
        (
            u64::from(u32::from_be_bytes(b[20..24].try_into().unwrap())),
            u64::from_be_bytes(b[24..32].try_into().unwrap()),
        )
    } else {
        return 0;
    };
    if echelle == 0 || duree / echelle > 1 << 20 {
        return 0;
    }
    duree.wrapping_mul(1000) / echelle
}

fn codec_piste(b: &[u8], de: usize, a: usize) -> (bool, bool, bool) {
    let (mut bas, mut haut) = (de, a);
    for voulu in [b"mdia", b"minf", b"stbl", b"stsd"] {
        let mut trouve = false;
        enfants(b, bas, haut, |bo| {
            if &bo.typ == voulu {
                (bas, haut, trouve) = (bo.corps, bo.fin, true);
            }
            trouve
        });
        if !trouve {
            return (false, false, false);
        }
    }
    if bas + 8 > haut {
        return (false, false, false);
    }
    let (mut video, mut audio, mut valide) = (false, false, true);
    enfants(b, bas + 8, haut, |bo| {
        if VIDEO.contains(&&bo.typ) {
            video = true;
        } else if AUDIO.contains(&&bo.typ) {
            audio = true;
        } else {
            valide = false;
        }
        false
    });
    (video, audio, valide)
}

pub fn analyser(b: &[u8]) -> Option<i64> {
    let ftyp = boite(b, 0)?;
    if &ftyp.typ != b"ftyp"
        || ftyp.corps + 4 > b.len()
        || !MARQUES.contains(&&<[u8; 4]>::try_from(&b[ftyp.corps..ftyp.corps + 4]).ok()?)
    {
        return None;
    }
    let (mut video, mut mauvais, mut moov, mut duree) = (false, false, false, 0u64);
    enfants(b, 0, b.len(), |bo| {
        if &bo.typ != b"moov" {
            return false;
        }
        moov = true;
        enfants(b, bo.corps, bo.fin, |bo| {
            match &bo.typ {
                b"mvhd" => duree = duree_mvhd(&b[bo.corps..bo.fin]),
                b"trak" => {
                    let (v, a, valide) = codec_piste(b, bo.corps, bo.fin);
                    video = video || v;
                    mauvais = mauvais || !valide || (!v && !a);
                }
                _ => {}
            }
            false
        });
        true
    });
    (moov && video && !mauvais && duree > 0).then_some(duree as i64)
}

pub fn est_jpeg(b: &[u8]) -> bool {
    b.len() > 4 && b[0] == 0xFF && b[1] == 0xD8 && b[2] == 0xFF
}

#[cfg(test)]
pub mod tests {
    use super::{analyser, est_jpeg};

    fn boite(typ: &[u8; 4], corps: &[u8]) -> Vec<u8> {
        let mut v = ((corps.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(typ);
        v.extend_from_slice(corps);
        v
    }

    fn mvhd(echelle: u32, duree: u32) -> Vec<u8> {
        let mut c = vec![0u8; 12];
        c.extend_from_slice(&echelle.to_be_bytes());
        c.extend_from_slice(&duree.to_be_bytes());
        c.extend_from_slice(&[0; 80]);
        boite(b"mvhd", &c)
    }

    fn piste(codec: &[u8; 4]) -> Vec<u8> {
        let entree = boite(codec, &[0; 8]);
        let mut stsd = vec![0, 0, 0, 0, 0, 0, 0, 1];
        stsd.extend(entree);
        let stbl = boite(b"stbl", &boite(b"stsd", &stsd));
        let minf = boite(b"minf", &stbl);
        let mdia = boite(b"mdia", &minf);
        boite(b"trak", &mdia)
    }

    pub fn mp4_valide(pistes: &[&[u8; 4]], echelle: u32, duree: u32) -> Vec<u8> {
        let mut v = boite(b"ftyp", b"isom\0\0\0\0isom");
        let mut moov = mvhd(echelle, duree);
        for p in pistes {
            moov.extend(piste(p));
        }
        v.extend(boite(b"moov", &moov));
        v
    }

    #[test]
    fn video_et_audio_acceptes() {
        assert_eq!(
            analyser(&mp4_valide(&[b"avc1", b"mp4a"], 1000, 12_345)),
            Some(12_345)
        );
        assert_eq!(
            analyser(&mp4_valide(&[b"hev1", b"Opus"], 600, 1200)),
            Some(2000)
        );
        assert_eq!(analyser(&mp4_valide(&[b"av01"], 1, 3)), Some(3000));
    }

    #[test]
    fn formats_refuses() {
        assert_eq!(analyser(&mp4_valide(&[b"mp4a"], 1000, 1000)), None);
        assert_eq!(analyser(&mp4_valide(&[b"vp09", b"mp4a"], 1000, 1000)), None);
        assert_eq!(analyser(&mp4_valide(&[b"avc1"], 0, 1000)), None);
        assert_eq!(analyser(&mp4_valide(&[b"avc1"], 1000, 0)), None);
        assert_eq!(analyser(&mp4_valide(&[b"avc1"], 1, u32::MAX)), None);
        assert_eq!(analyser(b""), None);
        assert_eq!(analyser(b"\x00\x00\x00\x18ftypmp42\0\0\0\0mp42isom"), None);
        let mut sans_moov = boite(b"ftyp", b"isom\0\0\0\0isom");
        sans_moov.extend(boite(b"free", &[0; 16]));
        assert_eq!(analyser(&sans_moov), None);
        let mut v = mp4_valide(&[b"avc1"], 1000, 1000);
        v[8..12].copy_from_slice(b"zzzz");
        assert_eq!(analyser(&v), None);
        let mut coupe = mp4_valide(&[b"avc1"], 1000, 1000);
        coupe.truncate(coupe.len() - 5);
        assert_eq!(analyser(&coupe), None);
    }

    #[test]
    fn signature_jpeg() {
        assert!(est_jpeg(b"\xFF\xD8\xFF\xE0x"));
        assert!(!est_jpeg(b"\xFF\xD8\xFF"));
        assert!(!est_jpeg(b"GIF89a"));
    }
}
