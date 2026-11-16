use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime},
};

use tokio::sync::watch;

const DUREE_MAX: Duration = Duration::from_secs(1800);
const SUFFIXE: &str = ".part";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Opus,
    Aac,
}

impl Format {
    pub fn codec(self) -> &'static str {
        match self {
            Format::Opus => "opus",
            Format::Aac => "aac",
        }
    }

    pub fn conteneur(self) -> &'static str {
        match self {
            Format::Opus => "ogg",
            Format::Aac => "aac",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Opus => "audio/ogg; codecs=opus",
            Format::Aac => "audio/aac",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Opus => "opus",
            Format::Aac => "aac",
        }
    }

    pub fn delai(self) -> i32 {
        match self {
            Format::Opus => 0,
            Format::Aac => 1024,
        }
    }

    pub fn frequence(self, source: Option<i32>) -> i32 {
        match self {
            Format::Opus => 48_000,
            Format::Aac => source.filter(|f| *f > 0).unwrap_or(48_000).min(48_000),
        }
    }

    pub fn depuis(texte: &str) -> Option<Self> {
        match texte {
            "opus" => Some(Format::Opus),
            "aac" => Some(Format::Aac),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Qualite {
    Originale,
    Haute,
    Normale,
    Basse,
}

impl Qualite {
    pub const TOUTES: [Qualite; 4] = [
        Qualite::Originale,
        Qualite::Haute,
        Qualite::Normale,
        Qualite::Basse,
    ];

    pub fn nom(self) -> &'static str {
        match self {
            Qualite::Originale => "originale",
            Qualite::Haute => "haute",
            Qualite::Normale => "normale",
            Qualite::Basse => "basse",
        }
    }

    pub fn depuis(texte: &str) -> Option<Self> {
        Qualite::TOUTES.into_iter().find(|q| q.nom() == texte)
    }

    pub fn kbps(self) -> u32 {
        match self {
            Qualite::Originale | Qualite::Haute => 256,
            Qualite::Normale => 160,
            Qualite::Basse => 96,
        }
    }

    pub fn depuis_kbps(kbps: u32) -> Option<Self> {
        [Qualite::Haute, Qualite::Normale, Qualite::Basse]
            .into_iter()
            .find(|q| q.kbps() == kbps)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cible {
    pub format: Format,
    pub kbps: u32,
}

impl Cible {
    pub fn texte(self) -> String {
        format!("{}{}", self.format.codec(), self.kbps)
    }

    pub fn depuis(texte: &str) -> Option<Self> {
        let i = texte.find(|c: char| c.is_ascii_digit())?;
        let format = Format::depuis(&texte[..i])?;
        let kbps: u32 = texte[i..].parse().ok()?;
        Qualite::depuis_kbps(kbps)?;
        Some(Cible { format, kbps })
    }
}

pub fn arguments(
    source: &Path,
    sortie: &Path,
    cible: Cible,
    canaux: Option<i32>,
    frequence: Option<i32>,
) -> Vec<String> {
    let mut a: Vec<String> = ["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.push(source.to_string_lossy().into_owned());
    a.extend(
        ["-map", "0:a:0", "-vn", "-sn", "-dn", "-map_metadata", "-1"]
            .iter()
            .map(|s| s.to_string()),
    );
    if canaux.is_some_and(|c| c > 2) {
        a.extend(["-ac".into(), "2".into()]);
    }
    let (codec, conteneur) = match cible.format {
        Format::Opus => ("libopus", "ogg"),
        Format::Aac => ("aac", "adts"),
    };
    a.extend(["-c:a".into(), codec.into()]);
    a.extend(["-b:a".into(), format!("{}k", cible.kbps)]);
    if cible.format == Format::Aac {
        a.extend(["-ar".into(), cible.format.frequence(frequence).to_string()]);
    }
    a.extend(["-f".into(), conteneur.into()]);
    a.push(sortie.to_string_lossy().into_owned());
    a
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Etape {
    EnCours,
    Fini,
    Echec,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Occupe;

#[derive(Default)]
struct Interne {
    encours: HashMap<String, watch::Receiver<Etape>>,
    lectures: HashMap<String, usize>,
}

pub struct GardeLecture {
    interne: Arc<Mutex<Interne>>,
    cle: String,
}

impl Drop for GardeLecture {
    fn drop(&mut self) {
        let mut i = self.interne.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = i.lectures.get_mut(&self.cle) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                i.lectures.remove(&self.cle);
            }
        }
    }
}

pub enum Disponible {
    Pret(PathBuf, GardeLecture),
    EnCours(std::fs::File, watch::Receiver<Etape>, GardeLecture),
}

pub struct Travail {
    pub ffmpeg: String,
    pub source: PathBuf,
    pub cible: Cible,
    pub canaux: Option<i32>,
    pub frequence: Option<i32>,
}

pub struct Transcodeur {
    racine: PathBuf,
    max_octets: u64,
    max_simultanes: usize,
    interne: Arc<Mutex<Interne>>,
    lancements: AtomicU64,
}

fn garde(interne: &Arc<Mutex<Interne>>, i: &mut Interne, cle: &str) -> GardeLecture {
    *i.lectures.entry(cle.to_string()).or_default() += 1;
    GardeLecture {
        interne: interne.clone(),
        cle: cle.to_string(),
    }
}

impl Transcodeur {
    pub fn nouveau(racine: PathBuf, max_octets: u64, max_simultanes: usize) -> Self {
        Self {
            racine,
            max_octets,
            max_simultanes: max_simultanes.max(1),
            interne: Arc::default(),
            lancements: AtomicU64::new(0),
        }
    }

    fn verrou(&self) -> MutexGuard<'_, Interne> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn chemin(&self, cle: &str) -> PathBuf {
        self.racine.join(cle)
    }

    fn partiel(&self, cle: &str) -> PathBuf {
        self.racine.join(format!("{cle}{SUFFIXE}"))
    }

    pub fn lancements(&self) -> u64 {
        self.lancements.load(Ordering::SeqCst)
    }

    pub fn en_cours(&self) -> usize {
        self.verrou().encours.len()
    }

    pub fn lecteurs(&self) -> usize {
        self.verrou().lectures.values().sum()
    }

    pub async fn preparer(&self) {
        if let Err(e) = tokio::fs::create_dir_all(&self.racine).await {
            tracing::error!("cache de transcodage {} : {e}", self.racine.display());
            return;
        }
        if let Ok(mut l) = tokio::fs::read_dir(&self.racine).await {
            while let Ok(Some(e)) = l.next_entry().await {
                if e.file_name().to_string_lossy().ends_with(SUFFIXE) {
                    let _ = tokio::fs::remove_file(e.path()).await;
                }
            }
        }
        self.plafonner().await;
    }

    pub fn obtenir(&self, cle: &str) -> Option<Disponible> {
        let mut i = self.verrou();
        if let Some(rx) = i.encours.get(cle).cloned() {
            let f = std::fs::File::open(self.partiel(cle)).ok()?;
            return Some(Disponible::EnCours(
                f,
                rx,
                garde(&self.interne, &mut i, cle),
            ));
        }
        let chemin = self.chemin(cle);
        let f = std::fs::File::options().append(true).open(&chemin).ok()?;
        let _ = f.set_modified(SystemTime::now());
        Some(Disponible::Pret(chemin, garde(&self.interne, &mut i, cle)))
    }

    pub fn lancer(self: &Arc<Self>, cle: &str, t: Travail) -> Result<(), Occupe> {
        let mut i = self.verrou();
        if i.encours.contains_key(cle) || self.chemin(cle).is_file() {
            return Ok(());
        }
        if i.encours.len() >= self.max_simultanes {
            return Err(Occupe);
        }
        let partiel = self.partiel(cle);
        if let Err(e) = std::fs::create_dir_all(&self.racine)
            .and_then(|_| std::fs::File::create(&partiel).map(|_| ()))
        {
            tracing::error!("cache de transcodage : {e}");
            return Err(Occupe);
        }
        let (tx, rx) = watch::channel(Etape::EnCours);
        i.encours.insert(cle.to_string(), rx);
        self.lancements.fetch_add(1, Ordering::SeqCst);
        drop(i);
        let moi = self.clone();
        let cle = cle.to_string();
        tokio::spawn(async move {
            let args = arguments(&t.source, &partiel, t.cible, t.canaux, t.frequence);
            let mut c = tokio::process::Command::new(&t.ffmpeg);
            c.args(&args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            let reussi = match tokio::time::timeout(DUREE_MAX, c.status()).await {
                Ok(Ok(s)) => s.success(),
                Ok(Err(e)) => {
                    tracing::warn!("ffmpeg : {e}");
                    false
                }
                Err(_) => false,
            };
            let etape = {
                let mut i = moi.verrou();
                i.encours.remove(&cle);
                if reussi && std::fs::rename(&partiel, moi.chemin(&cle)).is_ok() {
                    Etape::Fini
                } else {
                    let _ = std::fs::remove_file(&partiel);
                    Etape::Echec
                }
            };
            let _ = tx.send(etape);
            if etape == Etape::Fini {
                moi.plafonner().await;
            } else {
                tracing::warn!("transcodage échoué");
            }
        });
        Ok(())
    }

    pub async fn taille(&self) -> u64 {
        self.fichiers().await.iter().map(|f| f.1).sum()
    }

    async fn fichiers(&self) -> Vec<(String, u64, SystemTime)> {
        let mut v = Vec::new();
        let Ok(mut l) = tokio::fs::read_dir(&self.racine).await else {
            return v;
        };
        while let Ok(Some(e)) = l.next_entry().await {
            if let Ok(m) = e.metadata().await
                && m.is_file()
            {
                let date = m.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                v.push((e.file_name().to_string_lossy().into_owned(), m.len(), date));
            }
        }
        v
    }

    pub async fn plafonner(&self) -> u64 {
        let mut fichiers = self.fichiers().await;
        let mut total: u64 = fichiers.iter().map(|f| f.1).sum();
        fichiers.retain(|f| !f.0.ends_with(SUFFIXE));
        fichiers.sort_by_key(|f| f.2);
        for (nom, taille, _) in fichiers {
            if total <= self.max_octets {
                break;
            }
            let i = self.verrou();
            if i.lectures.contains_key(&nom) || i.encours.contains_key(&nom) {
                continue;
            }
            if std::fs::remove_file(self.chemin(&nom)).is_ok() {
                total = total.saturating_sub(taille);
            }
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lignes_ffmpeg() {
        let l = |c: Cible, canaux, f| {
            arguments(Path::new("/m/a.flac"), Path::new("/c/k.part"), c, canaux, f).join(" ")
        };
        assert_eq!(
            l(
                Cible {
                    format: Format::Opus,
                    kbps: 160
                },
                Some(2),
                Some(44_100)
            ),
            "-nostdin -hide_banner -loglevel error -y -i /m/a.flac -map 0:a:0 -vn -sn -dn -map_metadata -1 -c:a libopus -b:a 160k -f ogg /c/k.part"
        );
        assert_eq!(
            l(
                Cible {
                    format: Format::Aac,
                    kbps: 96
                },
                Some(6),
                Some(96_000)
            ),
            "-nostdin -hide_banner -loglevel error -y -i /m/a.flac -map 0:a:0 -vn -sn -dn -map_metadata -1 -ac 2 -c:a aac -b:a 96k -ar 48000 -f adts /c/k.part"
        );
    }

    #[test]
    fn qualites_et_cibles() {
        assert_eq!(Qualite::depuis("normale"), Some(Qualite::Normale));
        assert_eq!(Qualite::depuis("Normale"), None);
        assert_eq!(
            [Qualite::Basse, Qualite::Normale, Qualite::Haute].map(Qualite::kbps),
            [96, 160, 256]
        );
        let c = Cible {
            format: Format::Aac,
            kbps: 160,
        };
        assert_eq!(Cible::depuis(&c.texte()), Some(c));
        for faux in ["opus", "opus128", "flac160", "", "aac1600"] {
            assert_eq!(Cible::depuis(faux), None, "{faux}");
        }
    }

    fn ecrire(t: &Transcodeur, nom: &str, taille: usize, age: u64) {
        let f = std::fs::File::create(t.chemin(nom)).unwrap();
        f.set_len(taille as u64).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(age))
            .unwrap();
    }

    #[tokio::test]
    async fn plafond_respecte_sans_evincer_un_fichier_lu() {
        let racine = tempfile::tempdir().unwrap();
        let t = Transcodeur::nouveau(racine.path().to_path_buf(), 1000, 1);
        t.preparer().await;
        ecrire(&t, "lu.opus", 600, 300);
        ecrire(&t, "ancien.opus", 600, 200);
        ecrire(&t, "recent.opus", 300, 100);
        ecrire(&t, "x.opus.part", 50, 400);
        let lu = t.obtenir("lu.opus");
        assert!(matches!(lu, Some(Disponible::Pret(..))));
        ecrire(&t, "lu.opus", 600, 300);
        assert_eq!(t.plafonner().await, 950);
        assert!(t.chemin("lu.opus").exists());
        assert!(!t.chemin("ancien.opus").exists());
        assert!(t.chemin("recent.opus").exists() && t.chemin("x.opus.part").exists());
        assert!(t.taille().await <= 1000);
        drop(lu);
        let t = Transcodeur::nouveau(racine.path().to_path_buf(), 400, 1);
        t.preparer().await;
        assert!(!t.chemin("x.opus.part").exists());
        assert!(!t.chemin("lu.opus").exists() && t.chemin("recent.opus").exists());
        assert_eq!(t.taille().await, 300);
    }
}
