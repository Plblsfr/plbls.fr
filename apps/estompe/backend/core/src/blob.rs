use std::{
    io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use rand::Rng;
use tokio::{fs, io::AsyncWriteExt};

#[derive(Debug)]
pub enum ErreurBlob {
    CleInvalide,
    Introuvable,
    Io(io::Error),
}

impl From<io::Error> for ErreurBlob {
    fn from(e: io::Error) -> Self {
        if e.kind() == io::ErrorKind::NotFound {
            ErreurBlob::Introuvable
        } else {
            ErreurBlob::Io(e)
        }
    }
}

pub struct Disque {
    racine: PathBuf,
}

pub fn cle_valide(cle: &str) -> bool {
    cle.strip_prefix("files/").is_some_and(|n| {
        n.len() == 32
            && n.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

pub fn nouvelle_cle() -> String {
    let mut octets = [0u8; 16];
    rand::rng().fill_bytes(&mut octets);
    let hex: String = octets.iter().map(|b| format!("{b:02x}")).collect();
    format!("files/{hex}")
}

impl Disque {
    pub fn new(racine: impl Into<PathBuf>) -> Self {
        Disque {
            racine: racine.into(),
        }
    }

    fn chemin(&self, cle: &str) -> Result<PathBuf, ErreurBlob> {
        if !cle_valide(cle) {
            return Err(ErreurBlob::CleInvalide);
        }
        let nom = &cle["files/".len()..];
        Ok(self.racine.join(&nom[..2]).join(nom))
    }

    pub async fn put(&self, cle: &str, donnees: &[u8]) -> Result<(), ErreurBlob> {
        let chemin = self.chemin(cle)?;
        let dossier = chemin.parent().expect("dossier parent");
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o750)
            .create(dossier)
            .await?;
        let mut alea = [0u8; 8];
        rand::rng().fill_bytes(&mut alea);
        let temporaire = dossier.join(format!(
            ".tmp-{}",
            alea.iter().map(|b| format!("{b:02x}")).collect::<String>()
        ));
        let ecrit = ecrire(&temporaire, donnees).await;
        let fini = match ecrit {
            Ok(()) => fs::rename(&temporaire, &chemin).await,
            Err(e) => Err(e),
        };
        if let Err(e) = fini {
            let _ = fs::remove_file(&temporaire).await;
            return Err(e.into());
        }
        Ok(())
    }

    pub async fn ouvrir(&self, cle: &str) -> Result<fs::File, ErreurBlob> {
        Ok(fs::File::open(self.chemin(cle)?).await?)
    }

    pub async fn supprimer(&self, cle: &str) -> Result<(), ErreurBlob> {
        match fs::remove_file(self.chemin(cle)?).await {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    }
}

async fn ecrire(chemin: &Path, donnees: &[u8]) -> io::Result<()> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(chemin)
        .await?;
    f.write_all(donnees).await?;
    f.flush().await?;
    drop(f);
    fs::set_permissions(chemin, std::fs::Permissions::from_mode(0o640)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLE: &str = "files/0123456789abcdef0123456789abcdef";

    fn fichiers(racine: &Path) -> Vec<PathBuf> {
        let mut v = vec![];
        for d in std::fs::read_dir(racine).into_iter().flatten().flatten() {
            if d.path().is_dir() {
                v.extend(fichiers(&d.path()));
            } else {
                v.push(d.path());
            }
        }
        v
    }

    #[tokio::test]
    async fn cycle_complet_sans_fichier_temporaire() {
        let tmp = tempfile::tempdir().unwrap();
        let racine = tmp.path().join("data");
        let d = Disque::new(&racine);
        d.put(CLE, b"bonjour").await.unwrap();
        let chemin = racine.join("01").join(&CLE["files/".len()..]);
        assert_eq!(std::fs::read(&chemin).unwrap(), b"bonjour");
        assert_eq!(
            std::fs::metadata(&chemin).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(
            std::fs::metadata(racine.join("01"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o750
        );
        d.put(CLE, b"v2").await.unwrap();
        let tous = fichiers(&racine);
        assert_eq!(tous, vec![chemin.clone()]);
        assert_eq!(std::fs::read(&chemin).unwrap(), b"v2");
        d.ouvrir(CLE).await.unwrap();
        d.supprimer(CLE).await.unwrap();
        assert!(matches!(d.ouvrir(CLE).await, Err(ErreurBlob::Introuvable)));
        d.supprimer(CLE).await.unwrap();
    }

    #[tokio::test]
    async fn cles_invalides_refusees() {
        let tmp = tempfile::tempdir().unwrap();
        let d = Disque::new(tmp.path());
        for cle in [
            "",
            "../../etc/passwd",
            "files/../../x",
            "files/ABCDEF0123456789ABCDEF0123456789",
            "files/0123",
            "/etc/passwd",
            &format!("{CLE}/x"),
            &format!("{CLE}\n"),
        ] {
            assert!(matches!(
                d.put(cle, b"x").await,
                Err(ErreurBlob::CleInvalide)
            ));
            assert!(matches!(d.ouvrir(cle).await, Err(ErreurBlob::CleInvalide)));
            assert!(matches!(
                d.supprimer(cle).await,
                Err(ErreurBlob::CleInvalide)
            ));
        }
    }

    #[test]
    fn nouvelle_cle_est_valide_et_unique() {
        let (a, b) = (nouvelle_cle(), nouvelle_cle());
        assert!(cle_valide(&a));
        assert_ne!(a, b);
    }
}
