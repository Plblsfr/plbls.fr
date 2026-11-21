use crate::{Dossiers, Erreur, Resultat, IDENTIFIANT, VARIABLE_DOSSIER};
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{GenericFilePath, ListenerOptions, Name};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const TAILLE_MAX_MESSAGE: usize = 64 * 1024;
pub const DELAI_INSTANCE: Duration = Duration::from_secs(2);
const ACCUSE: &[u8] = b"ok\n";
#[cfg(not(windows))]
const FICHIER_SOCKET: &str = "instance.sock";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageInstance {
    pub arguments: Vec<String>,
    pub jeton_activation: Option<String>,
}

impl MessageInstance {
    pub fn de_l_environnement(arguments: Vec<String>) -> Self {
        let jeton = ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"]
            .iter()
            .filter_map(|nom| std::env::var(nom).ok())
            .find(|valeur| !valeur.is_empty());
        Self {
            arguments,
            jeton_activation: jeton,
        }
    }

    fn encoder(&self) -> Resultat<Vec<u8>> {
        let mut octets = serde_json::to_vec(self).map_err(|e| Erreur::Echec(e.to_string()))?;
        octets.push(b'\n');
        if octets.len() > TAILLE_MAX_MESSAGE {
            return Err(Erreur::Refuse(format!(
                "message d'instance de {} octets, {TAILLE_MAX_MESSAGE} au plus",
                octets.len()
            )));
        }
        Ok(octets)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NomInstance {
    chemin: PathBuf,
}

impl NomInstance {
    pub fn de(dossiers: &Dossiers) -> Self {
        let dossier = std::env::var_os(VARIABLE_DOSSIER)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        Self::selon(dossiers, dossier.as_deref())
    }

    pub fn selon(dossiers: &Dossiers, dossier_essai: Option<&Path>) -> Self {
        Self {
            chemin: chemin_de_l_os(dossiers, dossier_essai),
        }
    }

    pub fn chemin(&self) -> &Path {
        &self.chemin
    }

    fn nom(&self) -> Resultat<Name<'static>> {
        self.chemin
            .clone()
            .to_fs_name::<GenericFilePath>()
            .map_err(|e| Erreur::Echec(format!("nom d'instance {} : {e}", self.chemin.display())))
    }
}

#[cfg(windows)]
fn chemin_de_l_os(dossiers: &Dossiers, dossier_essai: Option<&Path>) -> PathBuf {
    let base = dossier_essai.unwrap_or(&dossiers.donnees);
    PathBuf::from(format!(
        r"\\.\pipe\{IDENTIFIANT}-{:016x}",
        empreinte(&base.to_string_lossy().to_lowercase())
    ))
}

#[cfg(not(windows))]
fn chemin_de_l_os(dossiers: &Dossiers, dossier_essai: Option<&Path>) -> PathBuf {
    if let Some(dossier) = dossier_essai {
        return dossier.join(FICHIER_SOCKET);
    }
    match std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|d| d.is_absolute())
    {
        Some(execution) => execution.join(IDENTIFIANT).join(FICHIER_SOCKET),
        None => dossiers.donnees.join(FICHIER_SOCKET),
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn empreinte(texte: &str) -> u64 {
    texte.bytes().fold(0xcbf2_9ce4_8422_2325, |h, o| {
        (h ^ u64::from(o)).wrapping_mul(0x0100_0000_01b3)
    })
}

pub enum Instance {
    Premiere(EcouteInstance),
    Seconde,
}

pub struct EcouteInstance {
    ecoute: LocalSocketListener,
}

pub type RecepteurInstance = Box<dyn Fn(MessageInstance) + Send>;

impl EcouteInstance {
    pub fn ecouter(self, recepteur: RecepteurInstance) -> Resultat<()> {
        let recepteur = std::sync::Arc::new(std::sync::Mutex::new(recepteur));
        std::thread::Builder::new()
            .name("estompe-instance".into())
            .spawn(move || {
                for connexion in self.ecoute.incoming().filter_map(Result::ok) {
                    let recepteur = recepteur.clone();
                    let _ = std::thread::Builder::new()
                        .name("estompe-instance-message".into())
                        .spawn(move || {
                            if let Some(message) = lire_message(connexion) {
                                let recepteur = recepteur.lock().unwrap_or_else(|e| e.into_inner());
                                recepteur(message);
                            }
                        });
                }
            })
            .map(|_| ())
            .map_err(|e| Erreur::Echec(format!("fil de l'instance : {e}")))
    }
}

fn lire_message(connexion: LocalSocketStream) -> Option<MessageInstance> {
    let _ = connexion.set_recv_timeout(Some(DELAI_INSTANCE));
    let _ = connexion.set_send_timeout(Some(DELAI_INSTANCE));
    let mut lecteur = BufReader::new(&connexion).take(TAILLE_MAX_MESSAGE as u64 + 1);
    let mut ligne = Vec::new();
    lecteur.read_until(b'\n', &mut ligne).ok()?;
    if ligne.len() > TAILLE_MAX_MESSAGE || ligne.last() != Some(&b'\n') {
        return None;
    }
    let message = serde_json::from_slice(&ligne).ok()?;
    let _ = (&connexion).write_all(ACCUSE);
    Some(message)
}

fn envoyer(connexion: LocalSocketStream, octets: &[u8]) -> Resultat<()> {
    let _ = connexion.set_recv_timeout(Some(DELAI_INSTANCE));
    let _ = connexion.set_send_timeout(Some(DELAI_INSTANCE));
    (&connexion)
        .write_all(octets)
        .map_err(|e| Erreur::Echec(format!("envoi à la première instance : {e}")))?;
    let mut accuse = [0u8; 3];
    (&connexion)
        .read_exact(&mut accuse)
        .map_err(|e| Erreur::Echec(format!("accusé de la première instance : {e}")))?;
    Ok(())
}

pub fn acquerir(nom: &NomInstance, message: &MessageInstance) -> Resultat<Instance> {
    let octets = message.encoder()?;
    preparer_dossier(nom.chemin())?;
    let nom_socket = nom.nom()?;
    for _ in 0..3 {
        if let Ok(connexion) = LocalSocketStream::connect(nom_socket.borrow()) {
            autoriser_le_premier_plan();
            envoyer(connexion, &octets)?;
            return Ok(Instance::Seconde);
        }
        match ListenerOptions::new()
            .name(nom_socket.borrow())
            .reclaim_name(true)
            .create_sync()
        {
            Ok(ecoute) => return Ok(Instance::Premiere(EcouteInstance { ecoute })),
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => retirer_orphelin(nom.chemin()),
            Err(e) => {
                return Err(Erreur::Echec(format!(
                    "instance {} : {e}",
                    nom.chemin().display()
                )))
            }
        }
    }
    Err(Erreur::Echec(format!(
        "instance {} : ni joignable ni créable",
        nom.chemin().display()
    )))
}

#[cfg(windows)]
fn preparer_dossier(_: &Path) -> Resultat<()> {
    Ok(())
}

#[cfg(not(windows))]
fn preparer_dossier(chemin: &Path) -> Resultat<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let Some(dossier) = chemin.parent() else {
        return Ok(());
    };
    if dossier.is_dir() {
        if dossier.file_name().and_then(|n| n.to_str()) == Some(IDENTIFIANT) {
            let _ = std::fs::set_permissions(dossier, std::fs::Permissions::from_mode(0o700));
        }
        return Ok(());
    }
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dossier)
        .map_err(|e| Erreur::Echec(format!("{} : {e}", dossier.display())))
}

#[cfg(windows)]
fn retirer_orphelin(_: &Path) {}

#[cfg(not(windows))]
fn retirer_orphelin(chemin: &Path) {
    let _ = std::fs::remove_file(chemin);
}

#[cfg(windows)]
fn autoriser_le_premier_plan() {
    use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
}

#[cfg(not(windows))]
fn autoriser_le_premier_plan() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn nom_dans(dossier: &Path) -> NomInstance {
        NomInstance::selon(&Dossiers::sous(dossier), Some(dossier))
    }

    fn message(arguments: &[&str]) -> MessageInstance {
        MessageInstance {
            arguments: arguments.iter().map(|a| a.to_string()).collect(),
            jeton_activation: None,
        }
    }

    fn ecouter(instance: Instance) -> mpsc::Receiver<MessageInstance> {
        let Instance::Premiere(ecoute) = instance else {
            panic!("première instance attendue");
        };
        let (envoi, reception) = mpsc::channel();
        ecoute
            .ecouter(Box::new(move |m| {
                let _ = envoi.send(m);
            }))
            .unwrap();
        reception
    }

    #[test]
    fn la_seconde_instance_livre_ses_arguments_a_la_premiere() {
        let tmp = tempfile::tempdir().unwrap();
        let nom = nom_dans(tmp.path());
        let recus = ecouter(acquerir(&nom, &message(&[])).unwrap());
        let envoye = MessageInstance {
            arguments: vec!["--invitation".into(), "ABCDE-23456".into()],
            jeton_activation: Some("jeton".into()),
        };
        assert!(matches!(acquerir(&nom, &envoye), Ok(Instance::Seconde)));
        assert_eq!(recus.recv_timeout(Duration::from_secs(10)).unwrap(), envoye);
        assert!(matches!(
            acquerir(&nom, &message(&["--x"])),
            Ok(Instance::Seconde)
        ));
        assert_eq!(
            recus.recv_timeout(Duration::from_secs(10)).unwrap(),
            message(&["--x"])
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn un_socket_orphelin_est_repris() {
        let tmp = tempfile::tempdir().unwrap();
        let nom = nom_dans(tmp.path());
        let orphelin = std::os::unix::net::UnixListener::bind(nom.chemin()).unwrap();
        drop(orphelin);
        assert!(nom.chemin().exists());
        let recus = ecouter(acquerir(&nom, &message(&[])).unwrap());
        assert!(matches!(
            acquerir(&nom, &message(&["a"])),
            Ok(Instance::Seconde)
        ));
        assert_eq!(
            recus.recv_timeout(Duration::from_secs(10)).unwrap(),
            message(&["a"])
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn le_dossier_du_socket_n_est_ouvert_qu_au_compte() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let chemin = tmp.path().join(IDENTIFIANT).join(FICHIER_SOCKET);
        preparer_dossier(&chemin).unwrap();
        let mode = std::fs::metadata(chemin.parent().unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn un_message_trop_long_est_refuse() {
        let tmp = tempfile::tempdir().unwrap();
        let nom = nom_dans(tmp.path());
        let recus = ecouter(acquerir(&nom, &message(&[])).unwrap());
        let long = "x".repeat(TAILLE_MAX_MESSAGE);
        assert!(matches!(
            acquerir(&nom, &message(&[&long])),
            Err(Erreur::Refuse(_))
        ));
        let connexion = LocalSocketStream::connect(nom.nom().unwrap()).unwrap();
        let mut brut = vec![b' '; TAILLE_MAX_MESSAGE + 10];
        brut.push(b'\n');
        assert!(envoyer(connexion, &brut).is_err());
        assert!(matches!(
            acquerir(&nom, &message(&["court"])),
            Ok(Instance::Seconde)
        ));
        assert_eq!(
            recus.recv_timeout(Duration::from_secs(10)).unwrap(),
            message(&["court"]),
            "seul le message court est livré"
        );
    }

    #[test]
    fn deux_dossiers_differents_donnent_deux_instances() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (nom_a, nom_b) = (nom_dans(a.path()), nom_dans(b.path()));
        assert_ne!(nom_a, nom_b);
        let _premiere = acquerir(&nom_a, &message(&[])).unwrap();
        assert!(matches!(
            acquerir(&nom_b, &message(&[])),
            Ok(Instance::Premiere(_))
        ));
    }

    #[test]
    fn l_empreinte_est_stable() {
        assert_eq!(empreinte(""), 0xcbf2_9ce4_8422_2325);
        assert_ne!(empreinte("c:\\a"), empreinte("c:\\b"));
    }
}
