use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use rand::RngExt;
use sha2::Sha256;
use uuid::Uuid;

pub const PLAFOND: Duration = Duration::from_secs(12 * 3600);

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
    pub inactivite: Duration,
}

#[derive(Clone, Debug)]
pub struct Ouverture {
    pub sub: String,
    pub titre: Uuid,
    pub fichier: Uuid,
    pub mode: Mode,
}

#[derive(Clone, Debug)]
pub struct Acces {
    pub flux: String,
    pub ouverture: Ouverture,
}

struct Session {
    ouverture: Ouverture,
    cree: Instant,
    vu: Instant,
    en_cours: usize,
}

impl Session {
    fn vivante(&self, now: Instant, inactivite: Duration) -> bool {
        now.saturating_duration_since(self.cree) < PLAFOND
            && (self.en_cours > 0 || now.saturating_duration_since(self.vu) < inactivite)
    }
}

#[derive(Default)]
pub struct Flux {
    sessions: Mutex<HashMap<String, Session>>,
    inactivite: Mutex<Option<Duration>>,
}

fn signature(secret: &str, sub: &str, flux: &str, exp: u64) -> Hmac<Sha256> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("clé HMAC");
    mac.update(format!("{sub}|{flux}|{exp}").as_bytes());
    mac
}

pub fn jeton(secret: &str, sub: &str, flux: &str, exp: u64) -> String {
    let code = signature(secret, sub, flux, exp).finalize().into_bytes();
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
        self.sessions.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn inactivite(&self) -> Duration {
        self.inactivite
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .unwrap_or(Duration::from_secs(300))
    }

    pub fn purger(&self, now: Instant) -> Vec<(String, Mode)> {
        let inactivite = self.inactivite();
        let mut m = self.verrou();
        let mortes: Vec<String> = m
            .iter()
            .filter(|(_, s)| !s.vivante(now, inactivite))
            .map(|(id, _)| id.clone())
            .collect();
        mortes
            .into_iter()
            .filter_map(|id| m.remove(&id).map(|s| (id, s.ouverture.mode)))
            .collect()
    }

    pub fn ouvrir(
        &self,
        ouverture: Ouverture,
        limites: Limites,
        secret: &str,
        now: Instant,
        unix: u64,
    ) -> Result<(String, String, Vec<String>), Refus> {
        *self.inactivite.lock().unwrap_or_else(|e| e.into_inner()) = Some(limites.inactivite);
        let mut morts: Vec<String> = self.purger(now).into_iter().map(|(id, _)| id).collect();
        let mut m = self.verrou();
        let remplaces: Vec<String> = m
            .iter()
            .filter(|(_, s)| {
                s.ouverture.sub == ouverture.sub && s.ouverture.titre == ouverture.titre
            })
            .map(|(id, _)| id.clone())
            .collect();
        let du_meme = m
            .values()
            .filter(|s| s.ouverture.sub == ouverture.sub && s.ouverture.titre != ouverture.titre)
            .count();
        if du_meme >= limites.flux {
            return Err(Refus::TropDeFlux);
        }
        let hls_restants = m
            .iter()
            .filter(|(id, s)| s.ouverture.mode == Mode::Hls && !remplaces.contains(id))
            .count();
        if ouverture.mode == Mode::Hls && hls_restants >= limites.transcodages {
            return Err(Refus::ServeurCharge);
        }
        for id in &remplaces {
            m.remove(id);
        }
        morts.extend(remplaces);
        let octets: [u8; 16] = rand::rng().random();
        let id: String = octets.iter().map(|o| format!("{o:02x}")).collect();
        let exp = unix + PLAFOND.as_secs();
        let signe = jeton(secret, &ouverture.sub, &id, exp);
        m.insert(
            id.clone(),
            Session {
                ouverture,
                cree: now,
                vu: now,
                en_cours: 0,
            },
        );
        Ok((id, signe, morts))
    }

    pub fn valider(&self, signe: &str, secret: &str, now: Instant, unix: u64) -> Option<Acces> {
        let mut morceaux = signe.split('.');
        let (flux, exp, code) = (morceaux.next()?, morceaux.next()?, morceaux.next()?);
        if morceaux.next().is_some() {
            return None;
        }
        let exp: u64 = exp.parse().ok()?;
        let code = URL_SAFE_NO_PAD.decode(code).ok()?;
        let inactivite = self.inactivite();
        let mut m = self.verrou();
        let session = m.get_mut(flux)?;
        if !session.vivante(now, inactivite) {
            return None;
        }
        signature(secret, &session.ouverture.sub, flux, exp)
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

    pub fn fermer(&self, flux: &str, sub: &str) -> Option<Mode> {
        let mut m = self.verrou();
        if m.get(flux)?.ouverture.sub != sub {
            return None;
        }
        m.remove(flux).map(|s| s.ouverture.mode)
    }

    pub fn retirer(&self, flux: &str) -> Option<Mode> {
        self.verrou().remove(flux).map(|s| s.ouverture.mode)
    }

    pub fn existe(&self, flux: &str) -> bool {
        self.verrou().contains_key(flux)
    }

    pub fn nombre(&self) -> usize {
        self.verrou().len()
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::LazyLock;

    fn cle_aleatoire() -> String {
        let octets: [u8; 32] = rand::rng().random();
        octets.iter().map(|o| format!("{o:02x}")).collect()
    }

    static SECRET: LazyLock<String> = LazyLock::new(cle_aleatoire);
    const UNIX: u64 = 1_000_000;
    const INACTIVITE: Duration = Duration::from_secs(60);
    const LARGES: Limites = Limites {
        flux: 100,
        transcodages: 100,
        inactivite: INACTIVITE,
    };

    fn demande(sub: &str, mode: Mode, titre: u128) -> Ouverture {
        Ouverture {
            sub: sub.into(),
            titre: Uuid::from_u128(titre),
            fichier: Uuid::from_u128(titre),
            mode,
        }
    }

    fn ouvrir(f: &Flux, t: Instant) -> (String, String) {
        let (id, signe, _) = f
            .ouvrir(demande("alice", Mode::Direct, 1), LARGES, &SECRET, t, UNIX)
            .unwrap();
        (id, signe)
    }

    #[test]
    fn jeton_valide_et_alteres() {
        let (f, t) = (Flux::default(), Instant::now());
        let (id, signe) = ouvrir(&f, t);
        assert_eq!(f.valider(&signe, &SECRET, t, UNIX).unwrap().flux, id);
        let (debut, code) = signe.rsplit_once('.').unwrap();
        let autre = if code.starts_with('A') { "B" } else { "A" };
        for faux in [
            format!("{debut}.{autre}{}", &code[1..]),
            format!("{id}.{}.{code}", UNIX + 999_999),
            format!("{signe}.x"),
            String::new(),
        ] {
            assert!(f.valider(&faux, &SECRET, t, UNIX).is_none(), "{faux}");
        }
        assert!(f.valider(&signe, &cle_aleatoire(), t, UNIX).is_none());
        assert!(
            f.valider(&jeton(&SECRET, "mallory", &id, UNIX + 10), &SECRET, t, UNIX)
                .is_none()
        );
        assert!(
            f.valider(&signe, &SECRET, t, UNIX + PLAFOND.as_secs())
                .is_none()
        );
    }

    #[test]
    fn inactivite_et_lecture_en_cours() {
        let f = Arc::new(Flux::default());
        let t = Instant::now();
        let (id, signe) = ouvrir(&f, t);
        assert!(f.valider(&signe, &SECRET, t + INACTIVITE, UNIX).is_none());
        let (id2, signe2) = ouvrir(&f, t);
        assert_ne!(id, id2);
        let lecture = f.lecture(&id2);
        assert!(
            f.valider(&signe2, &SECRET, t + INACTIVITE * 3, UNIX)
                .is_some()
        );
        drop(lecture);
        assert_eq!(f.purger(Instant::now() + INACTIVITE).len(), 1);
    }

    #[test]
    fn fermeture_reservee_au_proprietaire() {
        let (f, t) = (Flux::default(), Instant::now());
        let (id, signe) = ouvrir(&f, t);
        assert_eq!(f.fermer(&id, "bob"), None);
        assert_eq!(f.fermer(&id, "alice"), Some(Mode::Direct));
        assert!(f.valider(&signe, &SECRET, t, UNIX).is_none());
    }

    #[test]
    fn limites() {
        let (f, t) = (Flux::default(), Instant::now());
        let lim = Limites {
            flux: 2,
            transcodages: 1,
            inactivite: INACTIVITE,
        };
        let ouvre =
            |qui: &str, mode, titre| f.ouvrir(demande(qui, mode, titre), lim, &SECRET, t, UNIX);
        let (premier, _, _) = ouvre("alice", Mode::Hls, 1).unwrap();
        ouvre("alice", Mode::Direct, 2).unwrap();
        assert_eq!(
            ouvre("alice", Mode::Direct, 3).unwrap_err(),
            Refus::TropDeFlux
        );
        assert_eq!(
            ouvre("bob", Mode::Hls, 1).unwrap_err(),
            Refus::ServeurCharge
        );
        let (_, _, remplaces) = ouvre("alice", Mode::Hls, 1).unwrap();
        assert_eq!(remplaces, vec![premier.clone()]);
        assert!(!f.existe(&premier));
        f.purger(t + INACTIVITE);
        ouvre("bob", Mode::Hls, 1).unwrap();
    }
}
