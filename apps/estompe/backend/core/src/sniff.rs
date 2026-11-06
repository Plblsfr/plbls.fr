const LONGUEUR: usize = 512;

const HTML: &[&[u8]] = &[
    b"<!DOCTYPE HTML",
    b"<HTML",
    b"<HEAD",
    b"<SCRIPT",
    b"<IFRAME",
    b"<H1",
    b"<DIV",
    b"<FONT",
    b"<TABLE",
    b"<A",
    b"<STYLE",
    b"<TITLE",
    b"<B",
    b"<BODY",
    b"<BR",
    b"<P",
    b"<!--",
];

fn est_blanc(b: u8) -> bool {
    matches!(b, b'\t' | b'\n' | 0x0c | b'\r' | b' ')
}

fn masque(donnees: &[u8], masque: &[u8], motif: &[u8]) -> bool {
    donnees.len() >= motif.len()
        && motif
            .iter()
            .zip(masque)
            .zip(donnees)
            .all(|((p, m), d)| d & m == *p)
}

fn html(donnees: &[u8], motif: &[u8]) -> bool {
    if donnees.len() < motif.len() + 1 {
        return false;
    }
    motif.iter().zip(donnees).all(|(&b, &d)| {
        let d = if b.is_ascii_uppercase() { d & 0xDF } else { d };
        b == d
    }) && matches!(donnees[motif.len()], b' ' | b'>')
}

fn mp4(donnees: &[u8]) -> bool {
    if donnees.len() < 12 {
        return false;
    }
    let taille = u32::from_be_bytes([donnees[0], donnees[1], donnees[2], donnees[3]]) as usize;
    if donnees.len() < taille || !taille.is_multiple_of(4) || &donnees[4..8] != b"ftyp" {
        return false;
    }
    (8..taille)
        .step_by(4)
        .filter(|&st| st != 12)
        .any(|st| &donnees[st..st + 3] == b"mp4")
}

pub fn detecter(donnees: &[u8]) -> &'static str {
    let donnees = &donnees[..donnees.len().min(LONGUEUR)];
    let debut = donnees
        .iter()
        .position(|&b| !est_blanc(b))
        .unwrap_or(donnees.len());
    let sans_blancs = &donnees[debut..];
    if HTML.iter().any(|m| html(sans_blancs, m)) {
        return "text/html; charset=utf-8";
    }
    if masque(sans_blancs, b"\xFF\xFF\xFF\xFF\xFF", b"<?xml") {
        return "text/xml; charset=utf-8";
    }
    let exact = |motif: &[u8]| donnees.starts_with(motif);
    let m = |ms: &[u8], motif: &[u8]| masque(donnees, ms, motif);
    if exact(b"%PDF-") {
        return "application/pdf";
    }
    if exact(b"%!PS-Adobe-") {
        return "application/postscript";
    }
    if m(b"\xFF\xFF\x00\x00", b"\xFE\xFF\x00\x00") {
        return "text/plain; charset=utf-16be";
    }
    if m(b"\xFF\xFF\x00\x00", b"\xFF\xFE\x00\x00") {
        return "text/plain; charset=utf-16le";
    }
    if m(b"\xFF\xFF\xFF\x00", b"\xEF\xBB\xBF\x00") {
        return "text/plain; charset=utf-8";
    }
    if exact(b"\x00\x00\x01\x00") || exact(b"\x00\x00\x02\x00") {
        return "image/x-icon";
    }
    if exact(b"BM") {
        return "image/bmp";
    }
    if exact(b"GIF87a") || exact(b"GIF89a") {
        return "image/gif";
    }
    if m(
        b"\xFF\xFF\xFF\xFF\x00\x00\x00\x00\xFF\xFF\xFF\xFF\xFF\xFF",
        b"RIFF\x00\x00\x00\x00WEBPVP",
    ) {
        return "image/webp";
    }
    if exact(b"\x89PNG\x0D\x0A\x1A\x0A") {
        return "image/png";
    }
    if exact(b"\xFF\xD8\xFF") {
        return "image/jpeg";
    }
    let riff = b"\xFF\xFF\xFF\xFF\x00\x00\x00\x00\xFF\xFF\xFF\xFF";
    if m(riff, b"FORM\x00\x00\x00\x00AIFF") {
        return "audio/aiff";
    }
    if m(b"\xFF\xFF\xFF", b"ID3") {
        return "audio/mpeg";
    }
    if m(b"\xFF\xFF\xFF\xFF\xFF", b"OggS\x00") {
        return "application/ogg";
    }
    if m(b"\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF", b"MThd\x00\x00\x00\x06") {
        return "audio/midi";
    }
    if m(riff, b"RIFF\x00\x00\x00\x00AVI ") {
        return "video/avi";
    }
    if m(riff, b"RIFF\x00\x00\x00\x00WAVE") {
        return "audio/wave";
    }
    if mp4(donnees) {
        return "video/mp4";
    }
    if exact(b"\x1A\x45\xDF\xA3") {
        return "video/webm";
    }
    let mut ms = [0u8; 36];
    ms[34] = 0xFF;
    ms[35] = 0xFF;
    let mut motif = [0u8; 36];
    motif[34..].copy_from_slice(b"LP");
    if m(&ms, &motif) {
        return "application/vnd.ms-fontobject";
    }
    for (motif, ct) in [
        (&b"\x00\x01\x00\x00"[..], "font/ttf"),
        (b"OTTO", "font/otf"),
        (b"ttcf", "font/collection"),
        (b"wOFF", "font/woff"),
        (b"wOF2", "font/woff2"),
        (b"\x1F\x8B\x08", "application/x-gzip"),
        (b"PK\x03\x04", "application/zip"),
        (b"Rar!\x1A\x07\x00", "application/x-rar-compressed"),
        (b"Rar!\x1A\x07\x01\x00", "application/x-rar-compressed"),
        (b"\x00\x61\x73\x6D", "application/wasm"),
    ] {
        if exact(motif) {
            return ct;
        }
    }
    let texte = sans_blancs.iter().all(|&b| {
        !(b <= 0x08 || b == 0x0B || (0x0E..=0x1A).contains(&b) || (0x1C..=0x1F).contains(&b))
    });
    if texte {
        "text/plain; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

pub fn crc32(octets: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &o in octets {
        crc ^= u32::from(o);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::detecter;

    #[test]
    fn types_courants_comme_le_go() {
        let cas: &[(&[u8], &str)] = &[
            (b"\x89PNG\r\n\x1a\n\0\0", "image/png"),
            (b"\xFF\xD8\xFF\xE0xx", "image/jpeg"),
            (b"GIF89a....", "image/gif"),
            (b"RIFF\x01\x02\x03\x04WEBPVP8 ", "image/webp"),
            (b"\x1aE\xdf\xa3\0\0", "video/webm"),
            (b"OggS\0\x02\0\0", "application/ogg"),
            (b"%PDF-1.7", "application/pdf"),
            (b"PK\x03\x04abc", "application/zip"),
            (b"  <html><script>", "text/html; charset=utf-8"),
            (b"<!DOCTYPE html>", "text/html; charset=utf-8"),
            (b"<?xml version", "text/xml; charset=utf-8"),
            (b"bonjour", "text/plain; charset=utf-8"),
            (b"", "text/plain; charset=utf-8"),
            (b"\x00\x01binaire", "application/octet-stream"),
            (b"<scriptx", "text/plain; charset=utf-8"),
            (
                b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00mp42isom",
                "video/mp4",
            ),
            (
                b"\x00\x00\x00\x18ftypisom\x00\x00\x00\x00isomiso2",
                "application/octet-stream",
            ),
        ];
        for (donnees, attendu) in cas {
            assert_eq!(detecter(donnees), *attendu, "{donnees:?}");
        }
    }

    #[test]
    fn seuls_les_512_premiers_octets_comptent() {
        let mut d = b"texte".to_vec();
        d.resize(512, b'a');
        d.push(0);
        assert_eq!(detecter(&d), "text/plain; charset=utf-8");
    }
}
