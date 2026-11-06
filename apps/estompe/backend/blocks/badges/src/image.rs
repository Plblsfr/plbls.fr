fn u16_be(b: &[u8]) -> usize {
    usize::from(u16::from_be_bytes([b[0], b[1]]))
}

fn u32_be(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

pub(crate) fn crc32(octets: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &o in octets {
        crc ^= u32::from(o);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

pub fn dimensions_png(b: &[u8]) -> Option<(usize, usize)> {
    if b.len() < 8 || &b[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut pos = 8;
    let mut dims = None;
    loop {
        let entete = b.get(pos..pos + 8)?;
        let longueur = u32_be(entete) as usize;
        let type_ = &entete[4..8];
        let corps = b.get(pos + 8..pos + 8 + longueur)?;
        let crc = b.get(pos + 8 + longueur..pos + 12 + longueur)?;
        if crc32(&b[pos + 4..pos + 8 + longueur]) != u32_be(crc) {
            return None;
        }
        pos += 12 + longueur;
        match (dims, type_) {
            (None, b"IHDR") => {
                if longueur != 13 {
                    return None;
                }
                let (profondeur, couleur) = (corps[8], corps[9]);
                let valide = match couleur {
                    0 => matches!(profondeur, 1 | 2 | 4 | 8 | 16),
                    3 => matches!(profondeur, 1 | 2 | 4 | 8),
                    2 | 4 | 6 => matches!(profondeur, 8 | 16),
                    _ => false,
                };
                let (l, h) = (u32_be(corps), u32_be(&corps[4..]));
                if !valide
                    || corps[10] != 0
                    || corps[11] != 0
                    || corps[12] > 1
                    || l == 0
                    || h == 0
                    || l > i32::MAX as u32
                    || h > i32::MAX as u32
                {
                    return None;
                }
                dims = Some((l as usize, h as usize));
                if couleur != 3 {
                    return dims;
                }
            }
            (None, _) => return None,
            (Some(_), b"PLTE") => {
                if !longueur.is_multiple_of(3) || longueur == 0 || longueur > 768 {
                    return None;
                }
                return dims;
            }
            (Some(_), b"IDAT") => return None,
            _ => {}
        }
    }
}

pub fn dimensions_jpeg(b: &[u8]) -> Option<(usize, usize)> {
    if b.len() < 2 || b[..2] != [0xFF, 0xD8] {
        return None;
    }
    let mut pos = 2;
    loop {
        while *b.get(pos)? != 0xFF {
            pos += 1;
        }
        while *b.get(pos)? == 0xFF {
            pos += 1;
        }
        let marqueur = *b.get(pos)?;
        pos += 1;
        match marqueur {
            0x00 | 0x01 | 0xD0..=0xD7 => continue,
            0xD8 | 0xD9 => return None,
            _ => {}
        }
        let longueur = u16_be(b.get(pos..pos + 2)?);
        if longueur < 2 {
            return None;
        }
        let corps = b.get(pos + 2..pos + longueur)?;
        match marqueur {
            0xC0..=0xC2 => {
                if corps.len() < 6 || corps[0] != 8 {
                    return None;
                }
                return Some((u16_be(&corps[3..]), u16_be(&corps[1..])));
            }
            0xC3..=0xCF if !matches!(marqueur, 0xC4 | 0xC8 | 0xCC) => return None,
            0xDA => return None,
            _ => {}
        }
        pos += longueur;
    }
}

pub fn dimensions_webp(b: &[u8]) -> Option<(usize, usize)> {
    if b.len() < 30 || &b[0..4] != b"RIFF" || &b[8..12] != b"WEBP" {
        return None;
    }
    let u16_le = |i: usize| usize::from(u16::from_le_bytes([b[i], b[i + 1]]));
    let u24_le =
        |i: usize| usize::from(b[i]) | usize::from(b[i + 1]) << 8 | usize::from(b[i + 2]) << 16;
    match &b[12..16] {
        b"VP8 " => {
            if b[23] != 0x9d || b[24] != 0x01 || b[25] != 0x2a {
                return None;
            }
            Some((u16_le(26) & 0x3fff, u16_le(28) & 0x3fff))
        }
        b"VP8L" => {
            if b[20] != 0x2f {
                return None;
            }
            let v = u32::from_le_bytes([b[21], b[22], b[23], b[24]]);
            Some(((v & 0x3fff) as usize + 1, (v >> 14 & 0x3fff) as usize + 1))
        }
        b"VP8X" => {
            if b[20] & 0x02 != 0 {
                return None;
            }
            Some((u24_le(24) + 1, u24_le(27) + 1))
        }
        _ => None,
    }
}
