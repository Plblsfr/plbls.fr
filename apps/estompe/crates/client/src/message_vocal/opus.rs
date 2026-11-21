use unsafe_libopus::{
    opus_decode_float, opus_decoder_create, opus_decoder_destroy, opus_encode_float,
    opus_encoder_create, opus_encoder_ctl, opus_encoder_destroy, OpusDecoder, OpusEncoder,
    OPUS_APPLICATION_VOIP, OPUS_GET_LOOKAHEAD_REQUEST, OPUS_OK, OPUS_SET_BITRATE_REQUEST,
};

use super::{ErreurVocal, DEBIT, TAUX};

pub const TRAME: usize = 960;
const TRAME_MAX: usize = 5760;
const PAQUET_MAX: usize = 1500;
const PAQUETS_PAR_PAGE: usize = 50;
const VENDEUR: &[u8] = b"Estompe natif";

pub struct Encodeur {
    etat: *mut OpusEncoder,
    tampon: Vec<f32>,
    ogg: EcrivainOgg,
    preskip: u64,
    recus: u64,
    encodes: u64,
}

unsafe impl Send for Encodeur {}

impl Encodeur {
    pub fn nouveau() -> Result<Self, ErreurVocal> {
        let mut erreur = 0;
        let etat =
            unsafe { opus_encoder_create(TAUX as i32, 1, OPUS_APPLICATION_VOIP, &mut erreur) };
        if etat.is_null() || erreur != OPUS_OK {
            return Err(ErreurVocal::Codec(format!("encodeur Opus : {erreur}")));
        }
        let mut avance = 0i32;
        unsafe {
            opus_encoder_ctl!(etat, OPUS_SET_BITRATE_REQUEST, DEBIT);
            opus_encoder_ctl!(etat, OPUS_GET_LOOKAHEAD_REQUEST, &mut avance);
        }
        let preskip = avance.max(0) as u16;
        let mut tete = b"OpusHead".to_vec();
        tete.extend_from_slice(&[1, 1]);
        tete.extend_from_slice(&preskip.to_le_bytes());
        tete.extend_from_slice(&TAUX.to_le_bytes());
        tete.extend_from_slice(&[0, 0, 0]);
        let mut etiquettes = b"OpusTags".to_vec();
        etiquettes.extend_from_slice(&(VENDEUR.len() as u32).to_le_bytes());
        etiquettes.extend_from_slice(VENDEUR);
        etiquettes.extend_from_slice(&0u32.to_le_bytes());
        let mut serie = [0u8; 4];
        let _ = getrandom::fill(&mut serie);
        let mut ogg = EcrivainOgg::nouveau(u32::from_le_bytes(serie));
        ogg.page(&[&tete], 0, 0x02);
        ogg.page(&[&etiquettes], 0, 0);
        Ok(Self {
            etat,
            tampon: Vec::with_capacity(TRAME),
            ogg,
            preskip: u64::from(preskip),
            recus: 0,
            encodes: 0,
        })
    }

    pub fn echantillons(&self) -> u64 {
        self.recus
    }

    pub fn pousser(&mut self, echantillons: &[f32]) {
        self.recus += echantillons.len() as u64;
        for &e in echantillons {
            self.tampon.push(e);
            if self.tampon.len() == TRAME {
                self.encoder_la_trame();
            }
        }
    }

    fn encoder_la_trame(&mut self) {
        let mut paquet = [0u8; PAQUET_MAX];
        let longueur = unsafe {
            opus_encode_float(
                self.etat,
                self.tampon.as_ptr(),
                TRAME as i32,
                paquet.as_mut_ptr(),
                PAQUET_MAX as i32,
            )
        };
        self.tampon.clear();
        self.encodes += TRAME as u64;
        if longueur > 0 {
            self.ogg.paquet(&paquet[..longueur as usize], self.encodes);
        }
    }

    pub fn terminer(mut self) -> Vec<u8> {
        while self.encodes < self.recus + self.preskip || !self.tampon.is_empty() {
            self.tampon.resize(TRAME, 0.0);
            self.encoder_la_trame();
        }
        let fin = self.recus + self.preskip;
        std::mem::replace(&mut self.ogg, EcrivainOgg::nouveau(0)).terminer(fin)
    }
}

impl Drop for Encodeur {
    fn drop(&mut self) {
        unsafe { opus_encoder_destroy(self.etat) };
    }
}

struct EcrivainOgg {
    serie: u32,
    sequence: u32,
    octets: Vec<u8>,
    attente: Vec<Vec<u8>>,
    granule: u64,
}

impl EcrivainOgg {
    fn nouveau(serie: u32) -> Self {
        Self {
            serie,
            sequence: 0,
            octets: Vec::new(),
            attente: Vec::new(),
            granule: 0,
        }
    }

    fn paquet(&mut self, paquet: &[u8], granule: u64) {
        self.attente.push(paquet.to_vec());
        self.granule = granule;
        let segments: usize = self.attente.iter().map(|p| p.len() / 255 + 1).sum();
        if self.attente.len() >= PAQUETS_PAR_PAGE || segments > 200 {
            self.vider(0);
        }
    }

    fn vider(&mut self, drapeaux: u8) {
        let paquets = std::mem::take(&mut self.attente);
        let refs: Vec<&[u8]> = paquets.iter().map(Vec::as_slice).collect();
        self.page(&refs, self.granule, drapeaux);
    }

    fn terminer(mut self, granule: u64) -> Vec<u8> {
        self.granule = granule;
        self.vider(0x04);
        self.octets
    }

    fn page(&mut self, paquets: &[&[u8]], granule: u64, drapeaux: u8) {
        let mut segments = Vec::new();
        for p in paquets {
            segments.extend(std::iter::repeat_n(255u8, p.len() / 255));
            segments.push((p.len() % 255) as u8);
        }
        let debut = self.octets.len();
        self.octets.extend_from_slice(b"OggS\0");
        self.octets.push(drapeaux);
        self.octets.extend_from_slice(&granule.to_le_bytes());
        self.octets.extend_from_slice(&self.serie.to_le_bytes());
        self.octets.extend_from_slice(&self.sequence.to_le_bytes());
        self.octets.extend_from_slice(&[0; 4]);
        self.octets.push(segments.len() as u8);
        self.octets.extend_from_slice(&segments);
        for p in paquets {
            self.octets.extend_from_slice(p);
        }
        let somme = crc_ogg(&self.octets[debut..]);
        self.octets[debut + 22..debut + 26].copy_from_slice(&somme.to_le_bytes());
        self.sequence += 1;
    }
}

fn crc_ogg(octets: &[u8]) -> u32 {
    let mut crc = 0u32;
    for &o in octets {
        crc ^= u32::from(o) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    crc
}

pub struct Decodeur(*mut OpusDecoder);

impl Decodeur {
    pub fn nouveau() -> Result<Self, ErreurVocal> {
        let mut erreur = 0;
        let etat = unsafe { opus_decoder_create(TAUX as i32, 1, &mut erreur) };
        if etat.is_null() || erreur != OPUS_OK {
            return Err(ErreurVocal::Codec(format!("décodeur Opus : {erreur}")));
        }
        Ok(Self(etat))
    }

    pub fn decoder(&mut self, paquet: &[u8], sortie: &mut Vec<f32>) {
        let mut trame = [0f32; TRAME_MAX];
        let n = unsafe {
            opus_decode_float(
                self.0,
                paquet.as_ptr(),
                paquet.len() as i32,
                trame.as_mut_ptr(),
                TRAME_MAX as i32,
                0,
            )
        };
        if n > 0 {
            sortie.extend_from_slice(&trame[..n as usize]);
        }
    }
}

impl Drop for Decodeur {
    fn drop(&mut self) {
        unsafe { opus_decoder_destroy(self.0) };
    }
}

pub fn preskip_de_la_tete(tete: &[u8]) -> Option<usize> {
    (tete.len() >= 19 && tete.starts_with(b"OpusHead"))
        .then(|| usize::from(u16::from_le_bytes([tete[10], tete[11]])))
}

pub fn paquets_ogg(octets: &[u8]) -> Result<(Vec<Vec<u8>>, Option<u64>), ErreurVocal> {
    let mut paquets = Vec::new();
    let mut courant = Vec::new();
    let mut granule = None;
    let mut serie = None;
    let mut p = 0;
    while p + 27 <= octets.len() {
        if &octets[p..p + 4] != b"OggS" {
            return Err(ErreurVocal::Illisible);
        }
        let s = u32::from_le_bytes([
            octets[p + 14],
            octets[p + 15],
            octets[p + 16],
            octets[p + 17],
        ]);
        let n = usize::from(octets[p + 26]);
        let table = octets
            .get(p + 27..p + 27 + n)
            .ok_or(ErreurVocal::Illisible)?;
        let mut q = p + 27 + n;
        let ma_serie = *serie.get_or_insert(s) == s;
        for &taille in table {
            let morceau = octets
                .get(q..q + usize::from(taille))
                .ok_or(ErreurVocal::Illisible)?;
            q += usize::from(taille);
            if ma_serie {
                courant.extend_from_slice(morceau);
                if taille < 255 {
                    paquets.push(std::mem::take(&mut courant));
                }
            }
        }
        if ma_serie {
            let g = i64::from_le_bytes(octets[p + 6..p + 14].try_into().unwrap_or_default());
            if g >= 0 {
                granule = Some(g as u64);
            }
        }
        p = q;
    }
    Ok((paquets, granule))
}
