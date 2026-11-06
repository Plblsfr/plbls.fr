use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use rand::RngExt;
use reqwest::Url;
use sha2::Sha256;

pub const INACTIVITE: Duration = Duration::from_secs(5 * 60);
pub const PLAFOND: Duration = Duration::from_secs(12 * 3600);
const URIS_MAX: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Direct,
    Hls,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Refus {
    TropDeFlux,
    ServeurCharge,
}

#[derive(Clone, Copy)]
pub struct Limites {
    pub flux: usize,
    pub transcodages: usize,
}

#[derive(Clone)]
pub struct Ouverture {
    pub utilisateur: String,
    pub sub: String,
    pub jellyfin_id: String,
    pub source_id: String,
    pub mode: Mode,
    pub audio: Option<i64>,
    pub sous_titres: Vec<i64>,
}

#[derive(Clone)]
pub struct Acces {
    pub flux: String,
    pub ouverture: Ouverture,
}

struct Session {
    ouverture: Ouverture,
    cree: Instant,
    vu: Instant,
    en_cours: usize,
    uris: Vec<String>,
    index: HashMap<String, usize>,
}

impl Session {
    fn vivante(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.cree) < PLAFOND
            && (self.en_cours > 0 || now.saturating_duration_since(self.vu) < INACTIVITE)
    }
}

#[derive(Default)]
pub struct Flux(Mutex<HashMap<String, Session>>);

fn signature(secret: &str, utilisateur: &str, flux: &str, exp: u64) -> Hmac<Sha256> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("clé HMAC");
    mac.update(format!("{utilisateur}|{flux}|{exp}").as_bytes());
    mac
}

fn jeton(secret: &str, utilisateur: &str, flux: &str, exp: u64) -> String {
    let code = signature(secret, utilisateur, flux, exp)
        .finalize()
        .into_bytes();
    format!("{flux}.{exp}.{}", URL_SAFE_NO_PAD.encode(code))
}

pub fn maintenant_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

impl Flux {
    fn verrou(&self) -> MutexGuard<'_, HashMap<String, Session>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn purger(m: &mut HashMap<String, Session>, now: Instant) {
        m.retain(|_, s| s.vivante(now));
    }

    pub fn ouvrir(
        &self,
        ouverture: Ouverture,
        limites: Limites,
        secret: &str,
        now: Instant,
        unix: u64,
    ) -> Result<(String, String, Vec<String>), Refus> {
        let mut m = self.verrou();
        Self::purger(&mut m, now);
        let remplaces: Vec<String> = m
            .iter()
            .filter(|(_, s)| {
                s.ouverture.utilisateur == ouverture.utilisateur
                    && s.ouverture.jellyfin_id == ouverture.jellyfin_id
            })
            .map(|(id, _)| id.clone())
            .collect();
        let transcodages: Vec<String> = remplaces
            .iter()
            .filter(|id| m[*id].ouverture.mode == Mode::Hls)
            .cloned()
            .collect();
        for id in &remplaces {
            m.remove(id);
        }
        let du_meme = m
            .values()
            .filter(|s| s.ouverture.utilisateur == ouverture.utilisateur)
            .count();
        if du_meme >= limites.flux {
            return Err(Refus::TropDeFlux);
        }
        if ouverture.mode == Mode::Hls
            && m.values().filter(|s| s.ouverture.mode == Mode::Hls).count() >= limites.transcodages
        {
            return Err(Refus::ServeurCharge);
        }
        let octets: [u8; 16] = rand::rng().random();
        let id: String = octets.iter().map(|o| format!("{o:02x}")).collect();
        let exp = unix + PLAFOND.as_secs();
        let signe = jeton(secret, &ouverture.utilisateur, &id, exp);
        m.insert(
            id.clone(),
            Session {
                ouverture,
                cree: now,
                vu: now,
                en_cours: 0,
                uris: Vec::new(),
                index: HashMap::new(),
            },
        );
        Ok((id, signe, transcodages))
    }

    pub fn valider(&self, signe: &str, secret: &str, now: Instant, unix: u64) -> Option<Acces> {
        let mut morceaux = signe.split('.');
        let (flux, exp, code) = (morceaux.next()?, morceaux.next()?, morceaux.next()?);
        if morceaux.next().is_some() {
            return None;
        }
        let exp: u64 = exp.parse().ok()?;
        let code = URL_SAFE_NO_PAD.decode(code).ok()?;
        let mut m = self.verrou();
        Self::purger(&mut m, now);
        let session = m.get_mut(flux)?;
        signature(secret, &session.ouverture.utilisateur, flux, exp)
            .verify_slice(&code)
            .ok()?;
        if exp <= unix {
            return None;
        }
        session.vu = now;
        Some(Acces {
            flux: flux.to_string(),
            ouverture: session.ouverture.clone(),
        })
    }

    pub fn fermer(&self, flux: &str, utilisateur: &str) -> Option<Mode> {
        let mut m = self.verrou();
        if m.get(flux)?.ouverture.utilisateur != utilisateur {
            return None;
        }
        m.remove(flux).map(|s| s.ouverture.mode)
    }

    pub fn enregistrer_uri(&self, flux: &str, uri: String) -> Option<usize> {
        let mut m = self.verrou();
        let s = m.get_mut(flux)?;
        if let Some(n) = s.index.get(&uri) {
            return Some(*n);
        }
        if s.uris.len() >= URIS_MAX {
            return None;
        }
        let n = s.uris.len();
        s.uris.push(uri.clone());
        s.index.insert(uri, n);
        Some(n)
    }

    pub fn uri(&self, flux: &str, n: usize) -> Option<String> {
        self.verrou().get(flux)?.uris.get(n).cloned()
    }

    pub fn lecture(self: &Arc<Self>, flux: &str) -> Lecture {
        if let Some(s) = self.verrou().get_mut(flux) {
            s.en_cours += 1;
        }
        Lecture {
            flux: self.clone(),
            id: flux.to_string(),
        }
    }
}

pub struct Lecture {
    flux: Arc<Flux>,
    id: String,
}

impl Drop for Lecture {
    fn drop(&mut self) {
        if let Some(s) = self.flux.verrou().get_mut(&self.id) {
            s.en_cours = s.en_cours.saturating_sub(1);
            s.vu = Instant::now();
        }
    }
}

fn meme_origine(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

fn reecrire_uri(
    uri: &str,
    base: &Url,
    prefixe: &str,
    enregistrer: &mut impl FnMut(String) -> Option<usize>,
) -> Option<String> {
    let cible = base.join(uri.trim()).ok()?;
    if !meme_origine(&cible, base) {
        return None;
    }
    Some(format!("{prefixe}{}", enregistrer(cible.to_string())?))
}

pub fn reecrire_playlist(
    playlist: &str,
    base: &Url,
    prefixe: &str,
    mut enregistrer: impl FnMut(String) -> Option<usize>,
) -> Option<String> {
    let mut sortie = String::with_capacity(playlist.len());
    for ligne in playlist.lines() {
        let nue = ligne.trim_end_matches('\r');
        if nue.trim().is_empty() {
            sortie.push('\n');
            continue;
        }
        if !nue.starts_with('#') {
            sortie.push_str(&reecrire_uri(nue, base, prefixe, &mut enregistrer)?);
            sortie.push('\n');
            continue;
        }
        if !nue.starts_with("#EXT") {
            continue;
        }
        let mut reste = nue;
        while let Some((avant, apres)) = reste.split_once("URI=\"") {
            let (uri, suite) = apres.split_once('"')?;
            sortie.push_str(avant);
            sortie.push_str("URI=\"");
            sortie.push_str(&reecrire_uri(uri, base, prefixe, &mut enregistrer)?);
            sortie.push('"');
            reste = suite;
        }
        sortie.push_str(reste);
        sortie.push('\n');
    }
    Some(sortie)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "secret-de-test-assez-long-pour-hmac";
    const UNIX: u64 = 1_000_000;
    const LARGES: Limites = Limites {
        flux: 100,
        transcodages: 100,
    };

    fn demande(utilisateur: &str, mode: Mode) -> Ouverture {
        Ouverture {
            utilisateur: utilisateur.into(),
            sub: format!("sub-{utilisateur}"),
            jellyfin_id: "jf".into(),
            source_id: "src".into(),
            mode,
            audio: None,
            sous_titres: vec![],
        }
    }

    fn ouvrir(f: &Flux, t: Instant) -> (String, String) {
        let (id, signe, _) = f
            .ouvrir(demande("alice", Mode::Direct), LARGES, SECRET, t, UNIX)
            .unwrap();
        (id, signe)
    }

    #[test]
    fn jeton_valide() {
        let (f, t) = (Flux::default(), Instant::now());
        let (id, signe) = ouvrir(&f, t);
        let a = f.valider(&signe, SECRET, t, UNIX).unwrap();
        assert_eq!(a.flux, id);
        assert_eq!(a.ouverture.utilisateur, "alice");
    }

    #[test]
    fn signature_alteree_refusee() {
        let (f, t) = (Flux::default(), Instant::now());
        let (id, signe) = ouvrir(&f, t);
        let (debut, code) = signe.rsplit_once('.').unwrap();
        let autre = if code.starts_with('A') { "B" } else { "A" };
        for faux in [
            format!("{debut}.{autre}{}", &code[1..]),
            format!("{id}.{}.{code}", UNIX + 999_999),
            format!("{id}.{}", UNIX + 1),
            format!("{signe}.x"),
            "n.importe.quoi".to_string(),
            String::new(),
        ] {
            assert!(f.valider(&faux, SECRET, t, UNIX).is_none(), "{faux}");
        }
        assert!(f.valider(&signe, "autre-secret", t, UNIX).is_none());
        assert!(f.valider(&signe, SECRET, t, UNIX).is_some());
    }

    #[test]
    fn jeton_d_un_autre_utilisateur_refuse() {
        let (f, t) = (Flux::default(), Instant::now());
        let (id, _) = ouvrir(&f, t);
        let forge = jeton(SECRET, "mallory", &id, UNIX + 10);
        assert!(f.valider(&forge, SECRET, t, UNIX).is_none());
    }

    #[test]
    fn expire_par_inactivite() {
        let (f, t) = (Flux::default(), Instant::now());
        let (_, signe) = ouvrir(&f, t);
        assert!(f.valider(&signe, SECRET, t + INACTIVITE, UNIX).is_none());
    }

    #[test]
    fn prolonge_par_une_requete() {
        let (f, t) = (Flux::default(), Instant::now());
        let (_, signe) = ouvrir(&f, t);
        let presque = INACTIVITE - Duration::from_secs(1);
        assert!(f.valider(&signe, SECRET, t + presque, UNIX).is_some());
        assert!(f.valider(&signe, SECRET, t + presque * 2, UNIX).is_some());
        assert!(f.valider(&signe, SECRET, t + presque * 4, UNIX).is_none());
    }

    #[test]
    fn plafond_absolu() {
        let (f, t) = (Flux::default(), Instant::now());
        let (_, signe) = ouvrir(&f, t);
        let mut courant = t;
        while courant.duration_since(t) + INACTIVITE < PLAFOND {
            courant += INACTIVITE - Duration::from_secs(1);
            assert!(f.valider(&signe, SECRET, courant, UNIX).is_some());
        }
        assert!(f.valider(&signe, SECRET, t + PLAFOND, UNIX).is_none());
        let (f, t) = (Flux::default(), Instant::now());
        let (_, signe) = ouvrir(&f, t);
        assert!(
            f.valider(&signe, SECRET, t, UNIX + PLAFOND.as_secs())
                .is_none()
        );
    }

    #[test]
    fn lecture_en_cours_garde_la_session() {
        let f = Arc::new(Flux::default());
        let t = Instant::now();
        let (id, signe) = ouvrir(&f, t);
        let lecture = f.lecture(&id);
        assert!(
            f.valider(&signe, SECRET, t + INACTIVITE * 3, UNIX)
                .is_some()
        );
        drop(lecture);
        assert!(
            f.valider(&signe, SECRET, Instant::now() + INACTIVITE, UNIX)
                .is_none()
        );
    }

    #[test]
    fn fermeture_reservee_au_proprietaire() {
        let (f, t) = (Flux::default(), Instant::now());
        let (id, signe) = ouvrir(&f, t);
        assert_eq!(f.fermer(&id, "bob"), None);
        assert!(f.valider(&signe, SECRET, t, UNIX).is_some());
        assert_eq!(f.fermer(&id, "alice"), Some(Mode::Direct));
        assert!(f.valider(&signe, SECRET, t, UNIX).is_none());
        assert_eq!(f.fermer(&id, "alice"), None);
    }

    #[test]
    fn limite_de_flux_par_utilisateur() {
        let (f, t) = (Flux::default(), Instant::now());
        let lim = Limites {
            flux: 2,
            transcodages: 100,
        };
        let ouvre = |qui: &str, jf: &str, t| {
            let mut o = demande(qui, Mode::Direct);
            o.jellyfin_id = jf.into();
            f.ouvrir(o, lim, SECRET, t, UNIX)
        };
        let (premier, _, _) = ouvre("alice", "a", t).unwrap();
        ouvre("alice", "b", t).unwrap();
        assert_eq!(ouvre("alice", "c", t).unwrap_err(), Refus::TropDeFlux);
        ouvre("bob", "a", t).unwrap();
        f.fermer(&premier, "alice");
        ouvre("alice", "c", t).unwrap();
        assert_eq!(ouvre("alice", "d", t).unwrap_err(), Refus::TropDeFlux);
        ouvre("alice", "d", t + INACTIVITE).unwrap();
    }

    #[test]
    fn rouvrir_le_meme_element_remplace_l_ancien_flux() {
        let (f, t) = (Flux::default(), Instant::now());
        let lim = Limites {
            flux: 1,
            transcodages: 1,
        };
        let ouvre = |qui: &str, mode| f.ouvrir(demande(qui, mode), lim, SECRET, t, UNIX);
        let (ancien, signe, remplaces) = ouvre("alice", Mode::Hls).unwrap();
        assert!(remplaces.is_empty());
        let (nouveau, _, remplaces) = ouvre("alice", Mode::Hls).unwrap();
        assert_eq!(remplaces, vec![ancien.clone()]);
        assert!(f.valider(&signe, SECRET, t, UNIX).is_none());
        assert_eq!(f.fermer(&ancien, "alice"), None);
        assert_eq!(ouvre("bob", Mode::Hls).unwrap_err(), Refus::ServeurCharge);
        assert_eq!(f.fermer(&nouveau, "alice"), Some(Mode::Hls));
        ouvre("bob", Mode::Hls).unwrap();
    }

    #[test]
    fn limite_de_transcodages_globale() {
        let (f, t) = (Flux::default(), Instant::now());
        let lim = Limites {
            flux: 100,
            transcodages: 2,
        };
        let ouvre = |qui: &str, mode| f.ouvrir(demande(qui, mode), lim, SECRET, t, UNIX);
        ouvre("alice", Mode::Hls).unwrap();
        ouvre("bob", Mode::Hls).unwrap();
        assert_eq!(
            ouvre("carole", Mode::Hls).unwrap_err(),
            Refus::ServeurCharge
        );
        ouvre("carole", Mode::Direct).unwrap();
    }

    #[test]
    fn table_des_uris_propre_au_flux() {
        let (f, t) = (Flux::default(), Instant::now());
        let (id, _) = ouvrir(&f, t);
        assert_eq!(f.enregistrer_uri(&id, "a".into()), Some(0));
        assert_eq!(f.enregistrer_uri(&id, "b".into()), Some(1));
        assert_eq!(f.enregistrer_uri(&id, "a".into()), Some(0));
        assert_eq!(f.uri(&id, 1).as_deref(), Some("b"));
        assert_eq!(f.uri(&id, 2), None);
        assert_eq!(f.uri("autre", 0), None);
    }

    #[test]
    fn playlist_reecrite_sans_identifiant_ni_chemin() {
        let base = Url::parse("http://jellyfin:8096/Videos/abc123/master.m3u8?ApiKey=CLE").unwrap();
        let brut = "#EXTM3U\r\n# commentaire abc123\n#EXT-X-MEDIA:TYPE=SUBTITLES,URI=\"subs/abc123.m3u8?ApiKey=CLE\",NAME=\"fr\"\n#EXT-X-STREAM-INF:BANDWIDTH=1000\nmain.m3u8?DeviceId=d&ApiKey=CLE\n\n/Videos/abc123/hls1/seg0.ts\n";
        let mut vues = Vec::new();
        let s = reecrire_playlist(brut, &base, "u/", |u| {
            vues.push(u);
            Some(vues.len() - 1)
        })
        .unwrap();
        for interdit in [
            "abc123",
            "CLE",
            "ApiKey",
            "jellyfin",
            "Videos",
            "commentaire",
        ] {
            assert!(!s.contains(interdit), "{interdit} : {s}");
        }
        assert!(s.starts_with("#EXTM3U\n"), "{s}");
        assert!(s.contains("URI=\"u/0\",NAME=\"fr\""), "{s}");
        assert!(s.contains("\nu/1\n"), "{s}");
        assert!(s.contains("\nu/2\n"), "{s}");
        assert_eq!(
            vues[1],
            "http://jellyfin:8096/Videos/abc123/main.m3u8?DeviceId=d&ApiKey=CLE"
        );
        assert_eq!(vues[2], "http://jellyfin:8096/Videos/abc123/hls1/seg0.ts");
    }

    #[test]
    fn playlist_vers_une_autre_origine_refusee() {
        let base = Url::parse("http://jellyfin:8096/Videos/a/master.m3u8").unwrap();
        for brut in [
            "#EXTM3U\nhttp://ailleurs.test/seg.ts\n",
            "#EXTM3U\n//ailleurs.test/seg.ts\n",
            "#EXTM3U\nhttp://jellyfin:9999/seg.ts\n",
            "#EXT-X-KEY:METHOD=AES-128,URI=\"https://ailleurs.test/k\"\n",
        ] {
            assert_eq!(
                reecrire_playlist(brut, &base, "", |_| Some(0)),
                None,
                "{brut}"
            );
        }
    }
}
