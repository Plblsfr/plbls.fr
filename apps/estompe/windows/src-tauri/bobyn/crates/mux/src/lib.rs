//! Muxeur MP4 minimal : une piste vidéo (H.264, HEVC ou AV1) et des pistes AAC, entrelacées par seconde, moov en tête de fichier (lecture en streaming).
use bobyn_buffer::{Codec, Packet};
use std::io::{self, Write};

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(body.len() + 8);
    v.extend(((body.len() + 8) as u32).to_be_bytes());
    v.extend(kind);
    v.extend(body);
    v
}

/// Boîte avec version 0 et drapeaux.
fn full(kind: &[u8; 4], flags: u32, body: &[u8]) -> Vec<u8> {
    let mut b = flags.to_be_bytes().to_vec();
    b.extend(body);
    bx(kind, &b)
}

/// Découpe un flux Annex-B en NAL (sans codes de début).
pub fn nals(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new(); // (début du code, début du NAL)
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            starts.push((i, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::new();
    for (k, &(_, s)) in starts.iter().enumerate() {
        let mut e = starts.get(k + 1).map_or(data.len(), |n| n.0);
        while e > s && data[e - 1] == 0 {
            e -= 1; // zéros de fin / préfixe 4 octets du NAL suivant
        }
        if e > s {
            out.push(&data[s..e]);
        }
    }
    out
}

/// Piste vidéo : paquets Annex-B (H.264, HEVC) ou OBU (AV1) sortis de l'encodeur.
pub struct Video<'a> {
    pub codec: Codec,
    pub width: u32,
    pub height: u32,
    pub packets: &'a [Packet],
}

/// Piste AAC-LC 48 kHz stéréo : une trame brute de 1024 échantillons par élément, démarrant à l'heure de la 1re image vidéo.
pub struct Audio {
    pub name: String,
    pub frames: Vec<Vec<u8>>,
}

/// Chapitre (marqueur) : instant en microsecondes depuis le début du fichier.
pub struct Chapter {
    pub start_us: i64,
    pub title: String,
}

pub const AAC_FRAME: u32 = 1024;
const AAC_RATE: u32 = 48_000;
/// AudioSpecificConfig : AAC-LC (2), 48 kHz (index 3), 2 canaux.
const AAC_ASC: [u8; 2] = [0x11, 0x90];
/// shortcut: retard de décodage AAC-LC fixé à une trame (1024), jamais mesuré sur l'encodeur MF ; à ajuster avec un test de claquement.
const AAC_PRIMING: u32 = AAC_FRAME;

struct Trk<'a> {
    timescale: u32,
    handler: &'static [u8; 4],
    name: String,
    stsd: Vec<u8>,
    media_header: Vec<u8>,
    dims: (u32, u32),
    samples: Vec<Vec<&'a [u8]>>,
    /// Préfixe de longueur sur 4 octets devant chaque élément (NAL) ; absent pour les OBU AV1 et l'AAC.
    prefix: bool,
    durs: Vec<u32>,
    keys: Option<Vec<u32>>,
    priming: u32,
}

impl Trk<'_> {
    fn size(&self, i: usize) -> u32 {
        self.samples[i].iter().map(|n| n.len() as u32 + if self.prefix { 4 } else { 0 }).sum()
    }
    fn duration(&self) -> u32 {
        self.durs.iter().sum()
    }
}

fn be(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_be_bytes()).collect()
}

/// Retire les octets d'échappement 00 00 03 d'un NAL pour lire ses champs.
fn unescape(n: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(n.len());
    let mut zeros = 0;
    for &b in n {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

/// Découpe un flux d'OBU AV1 (format « low overhead », champ de taille présent) : renvoie (type, OBU complet).
fn obus(mut d: &[u8]) -> io::Result<Vec<(u8, &[u8])>> {
    let mut out = Vec::new();
    while !d.is_empty() {
        let h = d[0];
        if h & 2 == 0 {
            return Err(bad("OBU AV1 sans champ de taille"));
        }
        let mut i = 1 + (h >> 2 & 1) as usize;
        let (mut size, mut shift) = (0usize, 0);
        loop {
            let b = *d.get(i).ok_or_else(|| bad("OBU AV1 tronqué"))?;
            i += 1;
            size |= ((b & 0x7f) as usize) << shift;
            shift += 7;
            if b & 0x80 == 0 || shift > 28 {
                break;
            }
        }
        let end = i + size;
        if end > d.len() {
            return Err(bad("OBU AV1 tronqué"));
        }
        out.push((h >> 3 & 0xf, &d[..end]));
        d = &d[end..];
    }
    Ok(out)
}

struct Bits<'a>(&'a [u8], usize);

impl Bits<'_> {
    fn get(&mut self, n: usize) -> u32 {
        let mut v = 0;
        for _ in 0..n {
            let byte = self.0.get(self.1 / 8).copied().unwrap_or(0);
            v = v << 1 | (byte >> (7 - self.1 % 8) & 1) as u32;
            self.1 += 1;
        }
        v
    }
}

fn visual_entry(kind: &[u8; 4], width: u32, height: u32, config: Vec<u8>) -> Vec<u8> {
    let mut e = vec![0; 6];
    e.extend([0, 1, 0, 0, 0, 0]);
    e.extend([0; 12]);
    e.extend((width as u16).to_be_bytes());
    e.extend((height as u16).to_be_bytes());
    e.extend([0, 0x48, 0, 0, 0, 0x48, 0, 0, 0, 0, 0, 0, 0, 1]);
    e.extend([0; 32]);
    e.extend([0, 0x18, 0xFF, 0xFF]);
    e.extend(config);
    bx(kind, &e)
}

fn video_track<'a>(v: &Video<'a>) -> io::Result<Trk<'a>> {
    let first = v.packets.first().ok_or_else(|| bad("aucun paquet"))?.pts_us;
    let mut samples: Vec<Vec<&[u8]>> = Vec::new();
    let (mut ps, mut prefix) = ([None; 3], true);
    for p in v.packets {
        let mut keep = Vec::new();
        match v.codec {
            Codec::H264 => {
                for n in nals(&p.data) {
                    match n[0] & 0x1f {
                        7 => ps[0] = ps[0].or(Some(n)),
                        8 => ps[1] = ps[1].or(Some(n)),
                        9 => {} // délimiteur d'unité d'accès
                        _ => keep.push(n),
                    }
                }
            }
            Codec::Hevc => {
                for n in nals(&p.data) {
                    match n[0] >> 1 & 0x3f {
                        32 => ps[0] = ps[0].or(Some(n)),
                        33 => ps[1] = ps[1].or(Some(n)),
                        34 => ps[2] = ps[2].or(Some(n)),
                        35 => {}
                        _ => keep.push(n),
                    }
                }
            }
            Codec::Av1 => {
                prefix = false;
                for (t, o) in obus(&p.data)? {
                    match t {
                        1 => {
                            ps[0] = ps[0].or(Some(o));
                            keep.push(o);
                        }
                        2 => {} // délimiteur temporel : implicite dans MP4
                        _ => keep.push(o),
                    }
                }
            }
        }
        samples.push(keep);
    }
    let entry = match v.codec {
        Codec::H264 => {
            let (sps, pps) = ps[0].zip(ps[1]).ok_or_else(|| bad("SPS/PPS absents du flux"))?;
            if sps.len() < 4 {
                return Err(bad("SPS trop court"));
            }
            // shortcut: pas des champs chroma/bit-depth d'avcC pour les profils High, tolérés par VLC et Windows.
            let mut c = vec![1, sps[1], sps[2], sps[3], 0xFF, 0xE1];
            c.extend((sps.len() as u16).to_be_bytes());
            c.extend(sps);
            c.push(1);
            c.extend((pps.len() as u16).to_be_bytes());
            c.extend(pps);
            visual_entry(b"avc1", v.width, v.height, bx(b"avcC", &c))
        }
        Codec::Hevc => {
            let (vps, sps, pps) = (ps[0], ps[1], ps[2]);
            let (vps, sps, pps) = vps.zip(sps).zip(pps).map(|((a, b), c)| (a, b, c)).ok_or_else(|| bad("VPS/SPS/PPS absents du flux"))?;
            // SPS : 2 octets d'en-tête NAL, 1 octet (sous-couches), puis profile_tier_level sur 12 octets.
            let r = unescape(sps);
            if r.len() < 15 {
                return Err(bad("SPS HEVC trop court"));
            }
            let mut c = vec![1];
            c.extend(&r[3..15]); // profil/niveau : profile_space..profile_idc, compatibilité, contraintes, level_idc
            c.extend([0xF0, 0, 0xFC, 0xFD, 0xF8, 0xF8, 0, 0, 0x0F, 3]); // chroma 4:2:0, 8 bits, longueur NAL = 4
            for (t, n) in [(32u8, vps), (33, sps), (34, pps)] {
                c.extend([0x80 | t, 0, 1]);
                c.extend((n.len() as u16).to_be_bytes());
                c.extend(n);
            }
            visual_entry(b"hvc1", v.width, v.height, bx(b"hvcC", &c))
        }
        Codec::Av1 => {
            let seq = ps[0].ok_or_else(|| bad("séquence AV1 absente du flux"))?;
            let ext = seq[0] >> 2 & 1;
            let payload = &seq[1 + ext as usize..];
            let size_len = payload.iter().take_while(|b| *b & 0x80 != 0).count() + 1;
            let mut b = Bits(&payload[size_len.min(payload.len())..], 0);
            let profile = b.get(3);
            b.get(1); // still_picture
            let reduced = b.get(1) == 1;
            // shortcut: niveau lu seulement sans timing_info (cas d'AMF) ; sinon niveau 4.0 par défaut, purement informatif.
            let level = if reduced {
                b.get(5)
            } else if b.get(1) == 0 {
                b.get(1); // initial_display_delay_present
                b.get(5); // operating_points_cnt_minus_1
                b.get(12);
                b.get(5)
            } else {
                8
            };
            let mut c = vec![0x81, (profile << 5 | level) as u8, 0x0C, 0]; // 8 bits, 4:2:0
            c.extend(seq);
            visual_entry(b"av01", v.width, v.height, bx(b"av1C", &c))
        }
    };
    // Durées d'après les pts ; la dernière image reprend celle de la précédente.
    let ticks = |us: i64| ((us - first) as u64 * 90_000 / 1_000_000) as u32;
    let mut durs: Vec<u32> = v.packets.windows(2).map(|p| ticks(p[1].pts_us) - ticks(p[0].pts_us)).collect();
    durs.push(durs.last().copied().unwrap_or(90_000 / 60));
    let keys = v.packets.iter().enumerate().filter(|(_, p)| p.keyframe).map(|(i, _)| i as u32 + 1).collect();
    Ok(Trk {
        timescale: 90_000,
        handler: b"vide",
        name: "VideoHandler".into(),
        stsd: full(b"stsd", 0, &[be(&[1]), entry].concat()),
        media_header: full(b"vmhd", 1, &[0; 8]),
        dims: (v.width, v.height),
        samples,
        prefix,
        durs,
        keys: Some(keys),
        priming: 0,
    })
}

fn audio_track(a: &Audio) -> Trk<'_> {
    // Descripteurs MPEG-4 (esds) : longueurs toutes < 128, donc sur un octet.
    let dsi = [&[5u8, 2][..], &AAC_ASC].concat();
    let dcd = [&[4u8, 13 + dsi.len() as u8, 0x40, 0x15, 0, 0, 0, 0, 0, 0, 0, 0, 0][..], &dsi].concat();
    let esds = full(b"esds", 0, &[&[3u8, 3 + dcd.len() as u8 + 3, 0, 0, 0][..], &dcd, &[6, 1, 2]].concat());
    let mut e = vec![0; 6];
    e.extend([0, 1]);
    e.extend([0; 8]);
    e.extend([0, 2, 0, 16, 0, 0, 0, 0]);
    e.extend((AAC_RATE << 16).to_be_bytes());
    e.extend(esds);
    Trk {
        timescale: AAC_RATE,
        handler: b"soun",
        name: a.name.clone(),
        stsd: full(b"stsd", 0, &[be(&[1]), bx(b"mp4a", &e)].concat()),
        media_header: full(b"smhd", 0, &[0; 4]),
        dims: (0, 0),
        samples: a.frames.iter().map(|f| vec![f.as_slice()]).collect(),
        prefix: false,
        durs: vec![AAC_FRAME; a.frames.len()],
        keys: None,
        priming: AAC_PRIMING,
    }
}

/// Chapitres au format Nero (`udta/chpl`), lu par VLC, ffmpeg et la plupart des lecteurs : au plus 255 chapitres, titres de 255 octets.
fn chpl(chapters: &[Chapter]) -> Vec<u8> {
    let list = &chapters[..chapters.len().min(255)];
    let mut body = vec![0, 0, 0, 0, list.len() as u8];
    for c in list {
        let title = c.title.char_indices().map(|(i, ch)| i + ch.len_utf8()).take_while(|&e| e <= 255).last().map_or("", |e| &c.title[..e]);
        body.extend((c.start_us.max(0) as u64 * 10).to_be_bytes());
        body.push(title.len() as u8);
        body.extend(title.as_bytes());
    }
    bx(b"udta", &full(b"chpl", 1 << 24, &body))
}

pub fn write_mp4<W: Write>(mut w: W, video: &Video, audio: &[Audio], chapters: &[Chapter]) -> io::Result<()> {
    let mut trks = vec![video_track(video)?];
    trks.extend(audio.iter().filter(|a| !a.frames.is_empty()).map(audio_track));

    // Entrelacement : un bloc par piste et par seconde, dans l'ordre du temps.
    let mut chunks: Vec<(u64, usize, usize, usize)> = Vec::new(); // (seconde, piste, 1er échantillon, nombre)
    for (t, k) in trks.iter().enumerate() {
        let mut start = 0u64;
        for (i, d) in k.durs.iter().enumerate() {
            let sec = start / k.timescale as u64;
            match chunks.last_mut() {
                Some(c) if c.1 == t && c.0 == sec && c.2 + c.3 == i => c.3 += 1,
                _ => chunks.push((sec, t, i, 1)),
            }
            start += *d as u64;
        }
    }
    chunks.sort_by_key(|c| (c.0, c.1, c.2));
    // shortcut: mdat 32 bits, refuse au-delà de 4 Gio (≈ 10 min à 50 Mb/s), à passer en largesize si besoin.
    let mdat_len: u64 = trks.iter().map(|k| (0..k.samples.len()).map(|i| k.size(i) as u64).sum::<u64>()).sum();
    if mdat_len + 4096 > u32::MAX as u64 {
        return Err(bad("replay trop gros pour un mdat 32 bits"));
    }

    let ftyp = bx(b"ftyp", &[b"isom".as_slice(), &512u32.to_be_bytes(), b"isomiso2avc1mp41"].concat());
    // Le moov précède le mdat : sa taille ne dépend pas des décalages (stco 32 bits), on le construit donc deux fois.
    let build_moov = |base: u32| -> Vec<u8> {
    let mut offset = base;
    let mut offsets: Vec<Vec<u32>> = vec![Vec::new(); trks.len()];
    for &(_, t, first, n) in &chunks {
        offsets[t].push(offset);
        offset += (first..first + n).map(|i| trks[t].size(i)).sum::<u32>();
    }

    let matrix: Vec<u8> = [0x10000u32, 0, 0, 0, 0x10000, 0, 0, 0, 0x4000_0000].iter().flat_map(|x| x.to_be_bytes()).collect();
    let movie_dur = |k: &Trk| (k.duration().saturating_sub(k.priming) as u64 * 90_000 / k.timescale as u64) as u32;
    let mut traks = Vec::new();
    for (t, k) in trks.iter().enumerate() {
        let mut runs: Vec<(u32, u32)> = Vec::new();
        for &d in &k.durs {
            match runs.last_mut() {
                Some((n, rd)) if *rd == d => *n += 1,
                _ => runs.push((1, d)),
            }
        }
        let stts = full(b"stts", 0, &[be(&[runs.len() as u32]), runs.iter().flat_map(|&(n, d)| [n, d]).flat_map(|x| x.to_be_bytes()).collect()].concat());
        let mut stsc_runs: Vec<(u32, u32)> = Vec::new(); // (1er bloc, échantillons par bloc)
        for (ci, &(_, _, _, n)) in chunks.iter().filter(|c| c.1 == t).enumerate() {
            if stsc_runs.last().map(|r| r.1) != Some(n as u32) {
                stsc_runs.push((ci as u32 + 1, n as u32));
            }
        }
        let stsc = full(b"stsc", 0, &[be(&[stsc_runs.len() as u32]), stsc_runs.iter().flat_map(|&(f, n)| [f, n, 1]).flat_map(|x| x.to_be_bytes()).collect()].concat());
        let sizes: Vec<u32> = (0..k.samples.len()).map(|i| k.size(i)).collect();
        let stsz = full(b"stsz", 0, &[be(&[0, sizes.len() as u32]), be(&sizes)].concat());
        let stco = full(b"stco", 0, &[be(&[offsets[t].len() as u32]), be(&offsets[t])].concat());
        let mut stbl = vec![k.stsd.clone(), stts];
        if let Some(keys) = &k.keys {
            stbl.push(full(b"stss", 0, &[be(&[keys.len() as u32]), be(keys)].concat()));
        }
        stbl.extend([stsc, stsz, stco]);
        let dinf = bx(b"dinf", &full(b"dref", 0, &[be(&[1]), full(b"url ", 1, &[])].concat()));
        let minf = bx(b"minf", &[k.media_header.clone(), dinf, bx(b"stbl", &stbl.concat())].concat());
        let mdhd = full(b"mdhd", 0, &[be(&[0, 0, k.timescale, k.duration()]), vec![0x55, 0xC4, 0, 0]].concat());
        let hdlr = full(b"hdlr", 0, &[be(&[0]), k.handler.to_vec(), vec![0; 12], k.name.as_bytes().to_vec(), vec![0]].concat());
        let mdia = bx(b"mdia", &[mdhd, hdlr, minf].concat());
        let volume = if k.handler == b"soun" { vec![1, 0] } else { vec![0, 0] };
        let tkhd = full(b"tkhd", 3, &[be(&[0, 0, t as u32 + 1, 0, movie_dur(k)]), vec![0; 8], vec![0; 4], volume, vec![0; 2], matrix.clone(), be(&[k.dims.0 << 16, k.dims.1 << 16])].concat());
        let mut trak = vec![tkhd];
        if k.priming > 0 {
            // Montage : saute le retard de l'encodeur pour que l'audio reste calé sur la vidéo.
            let elst = full(b"elst", 0, &be(&[1, movie_dur(k), k.priming, 0x10000]));
            trak.push(bx(b"edts", &elst));
        }
        trak.push(mdia);
        traks.push(bx(b"trak", &trak.concat()));
    }
    let duration = trks.iter().map(movie_dur).max().unwrap_or(0);
    let mvhd = full(b"mvhd", 0, &[be(&[0, 0, 90_000, duration, 0x10000]), vec![1, 0], vec![0; 10], matrix, vec![0; 24], be(&[trks.len() as u32 + 1])].concat());
    let udta = if chapters.is_empty() { Vec::new() } else { chpl(chapters) };
    bx(b"moov", &[mvhd, traks.concat(), udta].concat())
    };
    let moov = build_moov(ftyp.len() as u32 + build_moov(0).len() as u32 + 8);

    w.write_all(&ftyp)?;
    w.write_all(&moov)?;
    w.write_all(&(mdat_len as u32 + 8).to_be_bytes())?;
    w.write_all(b"mdat")?;
    for &(_, t, first, n) in &chunks {
        let k = &trks[t];
        for s in &k.samples[first..first + n] {
            for part in s {
                if k.prefix {
                    w.write_all(&(part.len() as u32).to_be_bytes())?;
                }
                w.write_all(part)?;
            }
        }
    }
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn pkt(pts_us: i64, key: bool, nals: &[&[u8]]) -> Packet {
        let mut d = Vec::new();
        for n in nals {
            d.extend([0, 0, 0, 1]);
            d.extend(*n);
        }
        Packet { pts_us, keyframe: key, data: Arc::from(d) }
    }

    fn boxes(mut d: &[u8]) -> Vec<(String, &[u8])> {
        let mut v = Vec::new();
        while !d.is_empty() {
            let n = u32::from_be_bytes(d[..4].try_into().unwrap()) as usize;
            v.push((String::from_utf8_lossy(&d[4..8]).into(), &d[8..n]));
            d = &d[n..];
        }
        v
    }

    #[test]
    fn splits_three_and_four_byte_start_codes() {
        let d = [0, 0, 0, 1, 0x67, 1, 2, 0, 0, 1, 0x68, 3, 0, 0, 0, 1, 0x65, 9];
        assert_eq!(nals(&d), vec![&[0x67, 1, 2][..], &[0x68, 3][..], &[0x65, 9][..]]);
    }

    #[test]
    fn writes_well_formed_file() {
        let (sps, pps) = ([0x67u8, 0x42, 0xC0, 0x1E, 0xAA], [0x68u8, 0xCE]);
        let pk = vec![
            pkt(0, true, &[&sps, &pps, &[0x65, 1, 1, 1]]),
            pkt(16_667, false, &[&[0x09, 0x30], &[0x41, 2, 2]]),
            pkt(33_333, false, &[&[0x41, 3]]),
        ];
        let mut out = Vec::new();
        write_mp4(&mut out, &Video { codec: Codec::H264, width: 1920, height: 1080, packets: &pk }, &[], &[]).unwrap();
        let top = boxes(&out);
        assert_eq!(top.iter().map(|b| b.0.as_str()).collect::<Vec<_>>(), ["ftyp", "moov", "mdat"]);
        // 1er échantillon = NAL IDR seul (SPS/PPS retirés) : longueur 4 puis 65 01 01 01
        assert_eq!(&top[2].1[..8], &[0, 0, 0, 4, 0x65, 1, 1, 1]);
        assert_eq!(top[2].1.len(), 8 + 7 + 6);
        // stco pointe sur le début des données du mdat
        let off = out.windows(4).position(|w| w == b"stco").unwrap() + 4 + 4 + 4;
        let off = u32::from_be_bytes(out[off..off + 4].try_into().unwrap()) as usize;
        assert_eq!(&out[off..off + 8], &[0, 0, 0, 4, 0x65, 1, 1, 1]);
    }

    #[test]
    fn writes_chapters_as_chpl() {
        let pk = vec![pkt(0, true, &[&[0x67, 0x42, 0xC0, 0x1E, 0xAA], &[0x68, 0xCE], &[0x65, 1]])];
        let chapters = [Chapter { start_us: 1_500_000, title: "Marqueur 1".into() }, Chapter { start_us: 3_000_000, title: "é".repeat(200) }];
        let mut out = Vec::new();
        write_mp4(&mut out, &Video { codec: Codec::H264, width: 1920, height: 1080, packets: &pk }, &[], &chapters).unwrap();
        let moov = boxes(&out).into_iter().find(|b| b.0 == "moov").unwrap().1;
        let udta = boxes(moov).into_iter().find(|b| b.0 == "udta").unwrap().1;
        let chpl = boxes(udta).into_iter().find(|b| b.0 == "chpl").unwrap().1;
        assert_eq!(&chpl[..4], &[1, 0, 0, 0]); // version 1, drapeaux 0
        assert_eq!(chpl[8], 2);
        assert_eq!(u64::from_be_bytes(chpl[9..17].try_into().unwrap()), 15_000_000); // 100 ns
        assert_eq!(&chpl[17..28], b"\x0aMarqueur 1");
        let second = &chpl[28 + 8..];
        assert_eq!(second[0] as usize, 254); // titre tronqué sur une frontière de caractère
        assert!(std::str::from_utf8(&second[1..255]).is_ok());
        let mut plain = Vec::new();
        write_mp4(&mut plain, &Video { codec: Codec::H264, width: 1920, height: 1080, packets: &pk }, &[], &[]).unwrap();
        let moov = boxes(&plain).into_iter().find(|b| b.0 == "moov").unwrap().1;
        assert!(!boxes(moov).iter().any(|b| b.0 == "udta"));
    }

    #[test]
    fn rejects_stream_without_sps() {
        let pk = vec![pkt(0, true, &[&[0x65, 1]])];
        assert!(write_mp4(Vec::new(), &Video { codec: Codec::H264, width: 16, height: 16, packets: &pk }, &[], &[]).is_err());
    }

    #[test]
    fn hevc_keeps_profile_and_strips_parameter_sets() {
        // SPS : en-tête NAL (0x42 0x01), octet de sous-couches, puis profile_tier_level (Main, niveau 120).
        let sps = [0x42, 0x01, 0x01, 0x01, 0x60, 0, 0, 0, 0x90, 0, 0, 0, 0, 0, 0x78, 0xA0];
        let pk = vec![pkt(0, true, &[&[0x40, 1, 0xAA], &sps, &[0x44, 1, 0xBB], &[0x26, 1, 9]]), pkt(16_667, false, &[&[0x02, 1, 7]])];
        let mut out = Vec::new();
        write_mp4(&mut out, &Video { codec: Codec::Hevc, width: 64, height: 64, packets: &pk }, &[], &[]).unwrap();
        let i = out.windows(4).position(|w| w == b"hvcC").unwrap() + 4;
        assert_eq!(&out[i..i + 4], &[1, 1, 0x60, 0]); // version, Main, compat 60 00 00 00 (échappements retirés)
        assert_eq!(out[i + 12], 0x78); // level_idc
        let m = out.windows(4).position(|w| w == b"mdat").unwrap() + 4;
        assert_eq!(&out[m..m + 7], &[0, 0, 0, 3, 0x26, 1, 9]); // seul le NAL IDR reste dans l'échantillon
    }

    #[test]
    fn av1_drops_temporal_delimiter_and_keeps_sequence_header() {
        let td = [0x12, 0];
        let seq = [0x0A, 3, 0x00, 0x00, 0x0C]; // profil 0, pas de timing, niveau 0 (bits arbitraires)
        let frame = [0x32, 2, 0xDE, 0xAD];
        let pk = vec![Packet { pts_us: 0, keyframe: true, data: Arc::from([&td[..], &seq, &frame].concat()) }];
        let mut out = Vec::new();
        write_mp4(&mut out, &Video { codec: Codec::Av1, width: 64, height: 64, packets: &pk }, &[], &[]).unwrap();
        assert!(out.windows(4).any(|w| w == b"av1C"));
        let m = out.windows(4).position(|w| w == b"mdat").unwrap() + 4;
        assert_eq!(&out[m..m + 9], &[&seq[..], &frame].concat()[..]);
    }

    #[test]
    fn audio_track_is_interleaved_and_offsets_are_valid() {
        let pk = vec![pkt(0, true, &[&[0x67, 0x42, 0xC0, 0x1E], &[0x68, 1], &[0x65, 1]]), pkt(1_500_000, false, &[&[0x41, 1]])];
        let audio = Audio { name: "Discord".into(), frames: vec![vec![0xAA; 10]; 100] };
        let mut out = Vec::new();
        write_mp4(&mut out, &Video { codec: Codec::H264, width: 64, height: 64, packets: &pk }, &[audio], &[]).unwrap();
        assert_eq!(out.windows(4).filter(|w| *w == b"trak").count(), 2);
        // Bloc audio de la 1re seconde (46 trames de 1024 à 48 kHz) placé juste après le NAL vidéo de la 1re seconde.
        let m = out.windows(4).position(|w| w == b"mdat").unwrap() + 4;
        assert_eq!(&out[m + 8..m + 18], &[0xAA; 10]);
        assert!(out.windows(4).any(|w| w == b"elst"));
    }
}
