pub struct Analyse {
    pub type_mime: &'static str,
    pub largeur: u32,
    pub hauteur: u32,
}

const SIGNATURE_PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

pub fn analyser(o: &[u8]) -> Option<Analyse> {
    let (type_mime, (largeur, hauteur)) = if o.starts_with(SIGNATURE_PNG) {
        ("image/png", png(o)?)
    } else if o.starts_with(&[0xFF, 0xD8, 0xFF]) {
        ("image/jpeg", jpeg(o)?)
    } else {
        return None;
    };
    Some(Analyse {
        type_mime,
        largeur,
        hauteur,
    })
}

fn u32_be(o: &[u8]) -> u32 {
    u32::from_be_bytes([o[0], o[1], o[2], o[3]])
}

fn crc32(octets: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in octets {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn png(o: &[u8]) -> Option<(u32, u32)> {
    let c = o.get(8..33)?;
    if u32_be(c) != 13 || &c[4..8] != b"IHDR" || crc32(&c[4..21]) != u32_be(&c[21..]) {
        return None;
    }
    let (largeur, hauteur) = (u32_be(&c[8..]), u32_be(&c[12..]));
    let (profondeur, couleur) = (c[16], c[17]);
    let profondeurs: &[u8] = match couleur {
        0 => &[1, 2, 4, 8, 16],
        3 => &[1, 2, 4, 8],
        2 | 4 | 6 => &[8, 16],
        _ => return None,
    };
    let valide = profondeurs.contains(&profondeur)
        && c[18] == 0
        && c[19] == 0
        && c[20] <= 1
        && (1..=i32::MAX as u32).contains(&largeur)
        && (1..=i32::MAX as u32).contains(&hauteur);
    valide.then_some((largeur, hauteur))
}

fn sof_valide(s: &[u8]) -> bool {
    let composantes = usize::from(s[5]);
    if s[0] != 8 || !matches!(composantes, 1 | 3 | 4) || s.len() != 6 + 3 * composantes {
        return false;
    }
    let mut hv_precedents: Vec<(u8, u8)> = vec![];
    for i in 0..composantes {
        let (c, hv, tq) = (s[6 + 3 * i], s[7 + 3 * i], s[8 + 3 * i]);
        if (0..i).any(|j| s[6 + 3 * j] == c) || tq > 3 {
            return false;
        }
        let (mut h, mut v) = (hv >> 4, hv & 0x0f);
        if !(1..=4).contains(&h) || !(1..=4).contains(&v) || h == 3 || v == 3 {
            return false;
        }
        let ok = match composantes {
            1 => {
                (h, v) = (1, 1);
                true
            }
            3 => match i {
                0 => v != 4,
                1 => hv_precedents[0].0.is_multiple_of(h) && hv_precedents[0].1.is_multiple_of(v),
                _ => hv_precedents[1] == (h, v),
            },
            _ => match i {
                0 => hv == 0x11 || hv == 0x22,
                1 | 2 => hv == 0x11,
                _ => hv_precedents[0] == (h, v),
            },
        };
        if !ok {
            return false;
        }
        hv_precedents.push((h, v));
    }
    true
}

fn jpeg(o: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    let mut jfif = false;
    let mut dimensions = None;
    loop {
        while *o.get(i)? != 0xFF {
            i += 1;
        }
        while *o.get(i)? == 0xFF {
            i += 1;
        }
        let marqueur = *o.get(i)?;
        i += 1;
        match marqueur {
            0 | 0xD0..=0xD7 => continue,
            0xD9 => return None,
            _ => {}
        }
        let longueur = usize::from(u16::from_be_bytes([*o.get(i)?, *o.get(i + 1)?]));
        if longueur < 2 {
            return None;
        }
        if marqueur == 0xDA {
            return dimensions;
        }
        let segment = o.get(i + 2..i + longueur)?;
        match marqueur {
            0xC0..=0xC2 => {
                if dimensions.is_some() || segment.len() < 6 || !sof_valide(segment) {
                    return None;
                }
                let hauteur = u32::from(u16::from_be_bytes([segment[1], segment[2]]));
                let largeur = u32::from(u16::from_be_bytes([segment[3], segment[4]]));
                dimensions = Some((largeur, hauteur));
                if jfif {
                    return dimensions;
                }
            }
            0xE0 => jfif = segment.starts_with(b"JFIF\0"),
            0xC4 | 0xDB | 0xDD | 0xE1..=0xEF | 0xFE => {}
            _ => return None,
        }
        i += longueur;
    }
}
