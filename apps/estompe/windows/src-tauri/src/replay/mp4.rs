use std::io::{self, Write};

use super::tampon::{Unite, UNITE_TEMPS};
use super::types::Codec;

const ECHELLE_FILM: u64 = 1000;
const ECHELLE_VIDEO: u64 = 90_000;
const TRANCHE: i64 = UNITE_TEMPS;

pub struct PisteVideo<'a> {
    pub codec: Codec,
    pub largeur: u32,
    pub hauteur: u32,
    pub unites: &'a [Unite],
}

pub struct PisteAudio<'a> {
    pub nom: &'static str,
    pub frequence: u32,
    pub canaux: u8,
    pub unites: &'a [Unite],
}

pub fn nal(donnees: &[u8]) -> Vec<&[u8]> {
    let mut debuts = Vec::new();
    let mut i = 0;
    while i + 3 <= donnees.len() {
        if donnees[i] == 0 && donnees[i + 1] == 0 && donnees[i + 2] == 1 {
            debuts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    let fin = |k: usize| debuts.get(k + 1).map_or(donnees.len(), |d| d - 3);
    debuts.iter().enumerate().map(|(k, d)| sans_zeros(&donnees[*d..fin(k)])).filter(|n| !n.is_empty()).collect()
}

fn sans_zeros(mut s: &[u8]) -> &[u8] {
    while let [reste @ .., 0] = s {
        s = reste;
    }
    s
}

pub fn en_longueurs(donnees: &[u8]) -> Vec<u8> {
    let mut sortie = Vec::with_capacity(donnees.len() + 16);
    for n in nal(donnees) {
        sortie.extend((n.len() as u32).to_be_bytes());
        sortie.extend(n);
    }
    sortie
}

fn type_nal(codec: Codec, n: &[u8]) -> u8 {
    match codec {
        Codec::H264 => n[0] & 0x1F,
        Codec::Hevc => (n[0] >> 1) & 0x3F,
    }
}

fn prefixes(mut donnees: &[u8]) -> Vec<&[u8]> {
    let mut unites = Vec::new();
    while let Some((taille, reste)) = donnees.split_first_chunk::<4>() {
        let taille = (u32::from_be_bytes(*taille) as usize).min(reste.len());
        unites.push(&reste[..taille]);
        donnees = &reste[taille..];
    }
    unites
}

fn jeu_de_parametres(codec: Codec, donnees: &[u8], genre: u8) -> Option<&[u8]> {
    prefixes(donnees).into_iter().find(|n| !n.is_empty() && type_nal(codec, n) == genre)
}

fn est_parametre(codec: Codec, n: &[u8]) -> bool {
    match codec {
        Codec::H264 => matches!(type_nal(codec, n), 7 | 8),
        Codec::Hevc => matches!(type_nal(codec, n), 32..=34),
    }
}

pub fn parametres(codec: Codec, donnees: &[u8]) -> Vec<u8> {
    let mut sortie = Vec::new();
    for n in prefixes(donnees).into_iter().filter(|n| !n.is_empty() && est_parametre(codec, n)) {
        sortie.extend((n.len() as u32).to_be_bytes());
        sortie.extend(n);
    }
    sortie
}

pub fn avec_parametres(codec: Codec, image: Vec<u8>, entete: &mut Vec<u8>) -> Vec<u8> {
    let presents = parametres(codec, &image);
    if !presents.is_empty() {
        *entete = presents;
        image
    } else {
        [entete.as_slice(), &image].concat()
    }
}

fn sans_emulation(n: &[u8]) -> Vec<u8> {
    let mut sortie = Vec::with_capacity(n.len());
    for (i, o) in n.iter().enumerate() {
        if !(*o == 3 && i >= 2 && n[i - 1] == 0 && n[i - 2] == 0) {
            sortie.push(*o);
        }
    }
    sortie
}

struct Bits<'a> {
    octets: &'a [u8],
    position: usize,
}

impl Bits<'_> {
    fn lire(&mut self, n: u32) -> Option<u64> {
        let mut v = 0u64;
        for _ in 0..n {
            let octet = *self.octets.get(self.position / 8)?;
            v = v << 1 | (octet >> (7 - self.position % 8) & 1) as u64;
            self.position += 1;
        }
        Some(v)
    }

    fn golomb(&mut self) -> Option<u64> {
        let mut zeros = 0;
        while self.lire(1)? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        Some((1u64 << zeros) - 1 + self.lire(zeros)?)
    }
}

pub fn avcc(image_cle: &[u8]) -> Option<Vec<u8>> {
    let (sps, pps) = (jeu_de_parametres(Codec::H264, image_cle, 7)?, jeu_de_parametres(Codec::H264, image_cle, 8)?);
    if sps.len() < 4 {
        return None;
    }
    let mut c = vec![1, sps[1], sps[2], sps[3], 0xFF, 0xE1];
    c.extend((sps.len() as u16).to_be_bytes());
    c.extend(sps);
    c.push(1);
    c.extend((pps.len() as u16).to_be_bytes());
    c.extend(pps);
    if [100, 110, 122, 144].contains(&sps[1]) {
        c.extend([0xFD, 0xF8, 0xF8, 0]);
    }
    Some(c)
}

pub fn hvcc(image_cle: &[u8]) -> Option<Vec<u8>> {
    let jeux = [32u8, 33, 34].map(|g| jeu_de_parametres(Codec::Hevc, image_cle, g));
    let [Some(_), Some(sps), Some(_)] = jeux else { return None };
    let brut = sans_emulation(sps.get(2..)?);
    let mut b = Bits { octets: &brut, position: 0 };
    b.lire(4)?;
    let sous_couches = b.lire(3)?;
    let imbrique = b.lire(1)? as u8;
    let profil = b.lire(8)? as u8;
    let compatibilite = b.lire(32)? as u32;
    let contraintes = b.lire(48)?;
    let niveau = b.lire(8)? as u8;
    let drapeaux: Vec<(u64, u64)> = (0..sous_couches).map(|_| Some((b.lire(1)?, b.lire(1)?))).collect::<Option<_>>()?;
    if sous_couches > 0 {
        b.lire(2 * (8 - sous_couches as u32))?;
    }
    for (profil_present, niveau_present) in drapeaux {
        b.lire(88 * profil_present as u32)?;
        b.lire(8 * niveau_present as u32)?;
    }
    b.golomb()?;
    let chroma = b.golomb()? as u8;
    if chroma == 3 {
        b.lire(1)?;
    }
    b.golomb()?;
    b.golomb()?;
    if b.lire(1)? == 1 {
        for _ in 0..4 {
            b.golomb()?;
        }
    }
    let (luma, chrominance) = (b.golomb()? as u8, b.golomb()? as u8);
    let mut c = vec![1, profil];
    c.extend(compatibilite.to_be_bytes());
    c.extend(&contraintes.to_be_bytes()[2..]);
    c.extend([niveau, 0xF0, 0, 0xFC, 0xFC | chroma & 3, 0xF8 | luma & 7, 0xF8 | chrominance & 7, 0, 0]);
    c.extend([((sous_couches as u8 + 1) << 3) | imbrique << 2 | 3, 3]);
    for (genre, jeu) in [32u8, 33, 34].into_iter().zip(jeux) {
        let jeu = jeu?;
        c.extend([0x80 | genre, 0, 1]);
        c.extend((jeu.len() as u16).to_be_bytes());
        c.extend(jeu);
    }
    Some(c)
}

pub fn config_aac(frequence: u32, canaux: u8) -> Option<[u8; 2]> {
    const FREQUENCES: [u32; 13] = [96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350];
    let indice = FREQUENCES.iter().position(|f| *f == frequence)? as u16;
    Some((2u16 << 11 | indice << 7 | (canaux as u16 & 15) << 3).to_be_bytes())
}

fn boite(sortie: &mut Vec<u8>, nom: &[u8; 4], contenu: impl FnOnce(&mut Vec<u8>)) {
    let debut = sortie.len();
    sortie.extend([0; 4]);
    sortie.extend(nom);
    contenu(sortie);
    let taille = (sortie.len() - debut) as u32;
    sortie[debut..debut + 4].copy_from_slice(&taille.to_be_bytes());
}

fn complete(sortie: &mut Vec<u8>, nom: &[u8; 4], drapeaux: u32, contenu: impl FnOnce(&mut Vec<u8>)) {
    boite(sortie, nom, |s| {
        s.extend(drapeaux.to_be_bytes());
        contenu(s)
    })
}

fn u32be(s: &mut Vec<u8>, v: u64) {
    s.extend((v.min(u32::MAX as u64) as u32).to_be_bytes());
}

const MATRICE: [u32; 9] = [0x10000, 0, 0, 0, 0x10000, 0, 0, 0, 0x4000_0000];

struct Piste<'a> {
    echelle: u64,
    unites: &'a [Unite],
    entree: Vec<u8>,
    genre: &'static [u8; 4],
    nom: &'static str,
    dimensions: (u32, u32),
    morceaux: Vec<(usize, usize)>,
}

impl Piste<'_> {
    fn tic(&self, pts: i64) -> u64 {
        ((pts.max(0) as u128 * self.echelle as u128 + UNITE_TEMPS as u128 / 2) / UNITE_TEMPS as u128) as u64
    }

    fn durees(&self) -> Vec<u64> {
        let fin = self.unites.last().map_or(0, |u| self.tic(u.pts + u.duree));
        let suivant = |i: usize| self.unites.get(i + 1).map_or(fin, |u| self.tic(u.pts));
        self.unites.iter().enumerate().map(|(i, u)| suivant(i).saturating_sub(self.tic(u.pts)).max(1)).collect()
    }

    fn decouper(&mut self, origine: i64) {
        let mut debut = 0;
        while debut < self.unites.len() {
            let tranche = (self.unites[debut].pts - origine) / TRANCHE;
            let n = self.unites[debut..].iter().take_while(|u| (u.pts - origine) / TRANCHE == tranche).count();
            self.morceaux.push((debut, n));
            debut += n;
        }
    }
}

fn compresser(valeurs: impl Iterator<Item = u64>) -> Vec<(u32, u64)> {
    let mut series: Vec<(u32, u64)> = Vec::new();
    for v in valeurs {
        match series.last_mut() {
            Some((n, d)) if *d == v => *n += 1,
            _ => series.push((1, v)),
        }
    }
    series
}

fn trak(sortie: &mut Vec<u8>, p: &Piste, id: u32, origine: i64, positions: &[u64]) {
    let durees = p.durees();
    let total: u64 = durees.iter().sum();
    let retard = p.unites.first().map_or(0, |u| (u.pts - origine).max(0) as u64 * ECHELLE_FILM / UNITE_TEMPS as u64);
    let duree_film = total * ECHELLE_FILM / p.echelle;
    boite(sortie, b"trak", |s| {
        complete(s, b"tkhd", 3, |s| {
            s.extend([0; 8]);
            u32be(s, id as u64);
            s.extend([0; 4]);
            u32be(s, retard + duree_film);
            s.extend([0; 8]);
            s.extend([0, 0, 0, 0]);
            s.extend(if p.genre == b"soun" { [1, 0, 0, 0] } else { [0; 4] });
            MATRICE.iter().for_each(|m| s.extend(m.to_be_bytes()));
            u32be(s, (p.dimensions.0 as u64) << 16);
            u32be(s, (p.dimensions.1 as u64) << 16);
        });
        if retard > 0 {
            boite(s, b"edts", |s| {
                complete(s, b"elst", 0, |s| {
                    u32be(s, 2);
                    u32be(s, retard);
                    s.extend((-1i32).to_be_bytes());
                    s.extend([0, 1, 0, 0]);
                    u32be(s, duree_film);
                    s.extend([0, 0, 0, 0, 0, 1, 0, 0]);
                })
            });
        }
        boite(s, b"mdia", |s| {
            complete(s, b"mdhd", 0, |s| {
                s.extend([0; 8]);
                u32be(s, p.echelle);
                u32be(s, total);
                s.extend([0x55, 0xC4, 0, 0]);
            });
            complete(s, b"hdlr", 0, |s| {
                s.extend([0; 4]);
                s.extend(p.genre);
                s.extend([0; 12]);
                s.extend(p.nom.as_bytes());
                s.push(0);
            });
            boite(s, b"minf", |s| {
                if p.genre == b"soun" {
                    complete(s, b"smhd", 0, |s| s.extend([0; 4]));
                } else {
                    complete(s, b"vmhd", 1, |s| s.extend([0; 8]));
                }
                boite(s, b"dinf", |s| {
                    complete(s, b"dref", 0, |s| {
                        u32be(s, 1);
                        complete(s, b"url ", 1, |_| {});
                    })
                });
                boite(s, b"stbl", |s| {
                    complete(s, b"stsd", 0, |s| {
                        u32be(s, 1);
                        s.extend(&p.entree);
                    });
                    complete(s, b"stts", 0, |s| {
                        let series = compresser(durees.iter().copied());
                        u32be(s, series.len() as u64);
                        for (n, d) in series {
                            u32be(s, n as u64);
                            u32be(s, d);
                        }
                    });
                    if p.genre == b"vide" {
                        complete(s, b"stss", 0, |s| {
                            let cles: Vec<usize> = p.unites.iter().enumerate().filter(|(_, u)| u.cle).map(|(i, _)| i + 1).collect();
                            u32be(s, cles.len() as u64);
                            cles.iter().for_each(|c| u32be(s, *c as u64));
                        });
                    }
                    complete(s, b"stsc", 0, |s| {
                        let mut series: Vec<(usize, usize)> = Vec::new();
                        for (i, (_, n)) in p.morceaux.iter().enumerate() {
                            if series.last().map_or(true, |(_, m)| m != n) {
                                series.push((i + 1, *n));
                            }
                        }
                        u32be(s, series.len() as u64);
                        for (premier, n) in series {
                            u32be(s, premier as u64);
                            u32be(s, n as u64);
                            u32be(s, 1);
                        }
                    });
                    complete(s, b"stsz", 0, |s| {
                        u32be(s, 0);
                        u32be(s, p.unites.len() as u64);
                        p.unites.iter().for_each(|u| u32be(s, u.donnees.len() as u64));
                    });
                    complete(s, b"co64", 0, |s| {
                        u32be(s, p.morceaux.len() as u64);
                        positions.iter().for_each(|o| s.extend(o.to_be_bytes()));
                    });
                });
            });
        });
    });
}

fn entree_video(v: &PisteVideo, config: &[u8]) -> Vec<u8> {
    let (nom, boite_config): (&[u8; 4], &[u8; 4]) = match v.codec {
        Codec::H264 => (b"avc1", b"avcC"),
        Codec::Hevc => (b"hvc1", b"hvcC"),
    };
    let mut e = Vec::new();
    boite(&mut e, nom, |s| {
        s.extend([0, 0, 0, 0, 0, 0, 0, 1]);
        s.extend([0; 16]);
        s.extend((v.largeur as u16).to_be_bytes());
        s.extend((v.hauteur as u16).to_be_bytes());
        s.extend([0, 0x48, 0, 0, 0, 0x48, 0, 0, 0, 0, 0, 0, 0, 1]);
        s.extend([0; 32]);
        s.extend([0, 0x18, 0xFF, 0xFF]);
        boite(s, boite_config, |s| s.extend(config));
    });
    e
}

fn entree_audio(a: &PisteAudio, config: [u8; 2], debit: u32) -> Vec<u8> {
    let mut e = Vec::new();
    boite(&mut e, b"mp4a", |s| {
        s.extend([0, 0, 0, 0, 0, 0, 0, 1]);
        s.extend([0; 8]);
        s.extend([0, a.canaux, 0, 16, 0, 0, 0, 0]);
        u32be(s, (a.frequence as u64) << 16);
        complete(s, b"esds", 0, |s| {
            s.extend([3, 25, 0, 0, 0, 4, 17, 0x40, 0x15, 0, 0, 0]);
            s.extend(debit.to_be_bytes());
            s.extend(debit.to_be_bytes());
            s.extend([5, 2, config[0], config[1], 6, 1, 2]);
        });
    });
    e
}

pub fn ecrire(sortie: &mut impl Write, video: &PisteVideo, audios: &[PisteAudio]) -> io::Result<u64> {
    let invalide = |m: &str| io::Error::new(io::ErrorKind::InvalidData, m.to_string());
    let premiere = video.unites.first().filter(|u| u.cle).ok_or_else(|| invalide("le clip ne commence pas sur une image clé"))?;
    let config = match video.codec {
        Codec::H264 => avcc(&premiere.donnees),
        Codec::Hevc => hvcc(&premiere.donnees),
    };
    let config = config.ok_or_else(|| invalide("paramètres de l'encodeur absents de la première image"))?;
    let origine = premiere.pts;
    let mut pistes = vec![Piste {
        echelle: ECHELLE_VIDEO,
        unites: video.unites,
        entree: entree_video(video, &config),
        genre: b"vide",
        nom: "Vidéo",
        dimensions: (video.largeur, video.hauteur),
        morceaux: Vec::new(),
    }];
    for a in audios.iter().filter(|a| !a.unites.is_empty()) {
        let config = config_aac(a.frequence, a.canaux).ok_or_else(|| invalide("format audio non pris en charge"))?;
        let octets: u64 = a.unites.iter().map(|u| u.donnees.len() as u64).sum();
        let duree: i64 = a.unites.iter().map(|u| u.duree).sum();
        let debit = (octets * 8 * UNITE_TEMPS as u64 / duree.max(1) as u64) as u32;
        pistes.push(Piste {
            echelle: a.frequence as u64,
            unites: a.unites,
            entree: entree_audio(a, config, debit),
            genre: b"soun",
            nom: a.nom,
            dimensions: (0, 0),
            morceaux: Vec::new(),
        });
    }
    pistes.iter_mut().for_each(|p| p.decouper(origine));

    let mut ordre: Vec<(i64, usize, usize)> = Vec::new();
    for (i, p) in pistes.iter().enumerate() {
        ordre.extend(p.morceaux.iter().enumerate().map(|(k, (debut, _))| ((p.unites[*debut].pts - origine) / TRANCHE, i, k)));
    }
    ordre.sort();
    let taille_morceau = |i: usize, k: usize| -> u64 {
        let (debut, n) = pistes[i].morceaux[k];
        pistes[i].unites[debut..debut + n].iter().map(|u| u.donnees.len() as u64).sum()
    };

    let mut entete = Vec::new();
    boite(&mut entete, b"ftyp", |s| {
        s.extend(b"isom");
        s.extend([0, 0, 2, 0]);
        s.extend(b"isomiso2mp41");
    });
    let moov = |base: u64| {
        let mut positions: Vec<Vec<u64>> = pistes.iter().map(|p| vec![0; p.morceaux.len()]).collect();
        let mut position = base;
        for (_, i, k) in &ordre {
            positions[*i][*k] = position;
            position += taille_morceau(*i, *k);
        }
        let duree = pistes.iter().map(|p| p.unites.last().map_or(0, |u| (u.pts + u.duree - origine).max(0) as u64)).max().unwrap_or(0);
        let mut m = Vec::new();
        boite(&mut m, b"moov", |s| {
            complete(s, b"mvhd", 0, |s| {
                s.extend([0; 8]);
                u32be(s, ECHELLE_FILM);
                u32be(s, duree * ECHELLE_FILM / UNITE_TEMPS as u64);
                s.extend([0, 1, 0, 0, 1, 0, 0, 0]);
                s.extend([0; 8]);
                MATRICE.iter().for_each(|m| s.extend(m.to_be_bytes()));
                s.extend([0; 24]);
                u32be(s, pistes.len() as u64 + 1);
            });
            for (i, p) in pistes.iter().enumerate() {
                trak(s, p, i as u32 + 1, origine, &positions[i]);
            }
        });
        m
    };
    let taille_moov = moov(0).len() as u64;
    let donnees: u64 = ordre.iter().map(|(_, i, k)| taille_morceau(*i, *k)).sum();
    let base = entete.len() as u64 + taille_moov + 16;
    sortie.write_all(&entete)?;
    sortie.write_all(&moov(base))?;
    sortie.write_all(&1u32.to_be_bytes())?;
    sortie.write_all(b"mdat")?;
    sortie.write_all(&(donnees + 16).to_be_bytes())?;
    for (_, i, k) in &ordre {
        let (debut, n) = pistes[*i].morceaux[*k];
        for u in &pistes[*i].unites[debut..debut + n] {
            sortie.write_all(&u.donnees)?;
        }
    }
    Ok(base + donnees)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const SPS: [u8; 10] = [0x67, 0x64, 0x00, 0x28, 0xAC, 0xD9, 0x40, 0x78, 0x02, 0x27];
    pub const PPS: [u8; 4] = [0x68, 0xEB, 0xE3, 0xCB];

    pub fn image_h264(cle: bool, taille: usize) -> Vec<u8> {
        let mut a = Vec::new();
        if cle {
            a.extend([0, 0, 0, 1]);
            a.extend(SPS);
            a.extend([0, 0, 0, 1]);
            a.extend(PPS);
        }
        a.extend([0, 0, 1, if cle { 0x65 } else { 0x41 }]);
        a.extend(vec![0xAB; taille]);
        en_longueurs(&a)
    }

    fn boites(donnees: &[u8]) -> Vec<(String, &[u8])> {
        let mut liste = Vec::new();
        let mut reste = donnees;
        while reste.len() >= 8 {
            let mut taille = u32::from_be_bytes(reste[..4].try_into().unwrap()) as usize;
            let mut entete = 8;
            if taille == 1 {
                taille = u64::from_be_bytes(reste[8..16].try_into().unwrap()) as usize;
                entete = 16;
            }
            liste.push((String::from_utf8_lossy(&reste[4..8]).into_owned(), &reste[entete..taille]));
            reste = &reste[taille..];
        }
        assert!(reste.is_empty());
        liste
    }

    fn trouver<'a>(donnees: &'a [u8], chemin: &[&str]) -> Vec<&'a [u8]> {
        let Some((nom, suite)) = chemin.split_first() else { return vec![donnees] };
        boites(donnees).into_iter().filter(|(n, _)| n == nom).flat_map(|(_, c)| trouver(c, suite)).collect()
    }

    fn u32s(d: &[u8]) -> Vec<u32> {
        d.chunks_exact(4).map(|c| u32::from_be_bytes(c.try_into().unwrap())).collect()
    }

    #[test]
    fn decoupe_les_unites_annexe_b() {
        let flux = [0, 0, 0, 1, 0x67, 1, 2, 0, 0, 1, 0x68, 3, 0, 0, 0, 1, 0x65, 9, 9, 0];
        assert_eq!(nal(&flux), [&[0x67, 1, 2][..], &[0x68, 3], &[0x65, 9, 9]]);
        let l = en_longueurs(&flux);
        assert_eq!(l, [0, 0, 0, 3, 0x67, 1, 2, 0, 0, 0, 2, 0x68, 3, 0, 0, 0, 3, 0x65, 9, 9]);
        assert_eq!(prefixes(&l).len(), 3);
        assert!(nal(&[1, 2, 3]).is_empty());
    }

    #[test]
    fn configurations_des_codecs() {
        let c = avcc(&image_h264(true, 4)).unwrap();
        assert_eq!(&c[..8], [1, 0x64, 0x00, 0x28, 0xFF, 0xE1, 0, 10]);
        assert_eq!(&c[18..21], [1, 0, 4]);
        assert_eq!(&c[c.len() - 4..], [0xFD, 0xF8, 0xF8, 0]);
        assert!(avcc(&image_h264(false, 4)).is_none());
        assert_eq!(config_aac(48_000, 2), Some([0x11, 0x90]));
        assert_eq!(config_aac(48_000, 1), Some([0x11, 0x88]));
        assert_eq!(config_aac(44_100, 2), Some([0x12, 0x10]));
        assert_eq!(config_aac(1234, 2), None);

        let vps = [64, 1, 12, 1, 255, 255, 1, 96, 0, 0, 3, 0, 144, 0, 0, 3, 0, 0, 3, 0, 63, 146, 128, 144];
        let sps = [66, 1, 1, 1, 96, 0, 0, 3, 0, 144, 0, 0, 3, 0, 0, 3, 0, 63, 160, 5, 2, 1, 105, 101, 146, 164, 147, 43, 192, 90, 2, 0, 0, 3, 0, 2, 0, 0, 3, 0, 60, 16];
        let pps = [68, 1, 193, 114, 180, 98, 64];
        let mut flux = Vec::new();
        for n in [&vps[..], &sps, &pps, &[0x26, 0x01, 0xAF]] {
            flux.extend([0, 0, 0, 1]);
            flux.extend(n);
        }
        let h = hvcc(&en_longueurs(&flux)).unwrap();
        assert_eq!(&h[..13], [1, 0x01, 0x60, 0, 0, 0, 0x90, 0, 0, 0, 0, 0, 63]);
        assert_eq!(&h[13..23], [0xF0, 0, 0xFC, 0xFD, 0xF8, 0xF8, 0, 0, 0x0F, 3]);
        assert_eq!(&h[23..28], [0xA0, 0, 1, 0, 24]);
        assert_eq!(h.len(), 23 + 5 * 3 + vps.len() + sps.len() + pps.len());
    }

    #[test]
    fn remet_les_parametres_sur_chaque_image_cle() {
        let mut entete = Vec::new();
        let complete = image_h264(true, 4);
        assert_eq!(avec_parametres(Codec::H264, complete.clone(), &mut entete), complete);
        assert_eq!(entete.len(), 4 + SPS.len() + 4 + PPS.len());
        let nue = en_longueurs(&[0, 0, 1, 0x65, 1, 2, 3]);
        let reparee = avec_parametres(Codec::H264, nue.clone(), &mut entete);
        assert!(avcc(&nue).is_none() && avcc(&reparee).is_some());
        assert!(reparee.ends_with(&nue));
        assert_eq!(avec_parametres(Codec::Hevc, nue.clone(), &mut Vec::new()), nue);
    }

    pub fn unites(n: i64, pas: i64, taille: usize, video: bool, decalage: i64) -> Vec<Unite> {
        let donnees = |i: i64| if video { image_h264(i % 30 == 0, taille) } else { vec![0x21; taille] };
        (0..n).map(|i| Unite { pts: decalage + i * pas, duree: pas, cle: !video || i % 30 == 0, donnees: donnees(i).into() }).collect()
    }

    #[test]
    fn ecrit_un_mp4_coherent() {
        let image = UNITE_TEMPS / 25;
        let bloc = UNITE_TEMPS * 1024 / 48_000;
        let (video, jeu, micro) = (unites(90, image, 500, true, 7 * UNITE_TEMPS), unites(140, bloc, 60, false, 7 * UNITE_TEMPS + 100_000), unites(140, bloc, 30, false, 7 * UNITE_TEMPS));
        let piste = PisteVideo { codec: Codec::H264, largeur: 1920, hauteur: 1080, unites: &video };
        let audios = [PisteAudio { nom: "Jeu", frequence: 48_000, canaux: 2, unites: &jeu }, PisteAudio { nom: "Micro", frequence: 48_000, canaux: 1, unites: &micro }];
        let mut fichier = Vec::new();
        let taille = ecrire(&mut fichier, &piste, &audios).unwrap();
        assert_eq!(taille, fichier.len() as u64);
        let noms: Vec<String> = boites(&fichier).into_iter().map(|(n, _)| n).collect();
        assert_eq!(noms, ["ftyp", "moov", "mdat"]);
        let pistes = trouver(&fichier, &["moov", "trak"]);
        assert_eq!(pistes.len(), 3);
        let mvhd = u32s(&trouver(&fichier, &["moov", "mvhd"])[0][4..]);
        assert_eq!((mvhd[2], mvhd[3]), (1000, 3600));

        for (i, (piste, source)) in pistes.iter().zip([&video, &jeu, &micro]).enumerate() {
            let table = |nom: &str| trouver(piste, &["mdia", "minf", "stbl", nom])[0];
            let tailles = u32s(&table("stsz")[4..]);
            assert_eq!(tailles[1] as usize, source.len());
            let positions: Vec<u64> = table("co64")[8..].chunks_exact(8).map(|c| u64::from_be_bytes(c.try_into().unwrap())).collect();
            let series = u32s(&table("stsc")[8..]);
            let par_morceau = |m: usize| series.chunks_exact(3).rev().find(|s| s[0] as usize <= m + 1).unwrap()[1] as usize;
            let mut echantillon = 0;
            for (m, position) in positions.iter().enumerate() {
                let mut position = *position as usize;
                for _ in 0..par_morceau(m) {
                    let taille = tailles[2 + echantillon] as usize;
                    assert_eq!(&fichier[position..position + taille], &source[echantillon].donnees[..]);
                    position += taille;
                    echantillon += 1;
                }
            }
            assert_eq!(echantillon, source.len());
            let durees = u32s(&table("stts")[8..]);
            let total: u64 = durees.chunks_exact(2).map(|d| d[0] as u64 * d[1] as u64).sum();
            let echelle = u32s(&trouver(piste, &["mdia", "mdhd"])[0][4..])[2] as u64;
            assert_eq!(echelle, if i == 0 { 90_000 } else { 48_000 });
            assert_eq!(total, if i == 0 { 324_000 } else { 140 * 1024 });
            assert_eq!(trouver(piste, &["edts", "elst"]).len(), usize::from(i == 1));
        }
        assert_eq!(u32s(&trouver(pistes[0], &["mdia", "minf", "stbl", "stss"])[0][4..]), [3, 1, 31, 61]);
        assert_eq!(u32s(&trouver(pistes[1], &["edts", "elst"])[0][4..])[1], 10);
        let stsd = trouver(pistes[0], &["mdia", "minf", "stbl", "stsd"])[0];
        assert_eq!(&stsd[12..16], b"avc1");
        assert!(stsd.windows(4).any(|f| f == b"avcC"));
    }

    #[test]
    fn refuse_un_clip_sans_image_cle() {
        let video = unites(10, 1000, 10, true, 0);
        let piste = PisteVideo { codec: Codec::H264, largeur: 2, hauteur: 2, unites: &video[1..] };
        assert!(ecrire(&mut Vec::new(), &piste, &[]).is_err());
    }
}
