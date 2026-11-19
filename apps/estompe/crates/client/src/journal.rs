use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

pub const NOM_FICHIER: &str = "estompe-debug.log";
pub const NOM_FICHIER_PRECEDENT: &str = "estompe-debug.1.log";
pub const TAILLE_MAX: u64 = 512 * 1024;

const JETON: &str = "[jeton]";
const MASQUE: &str = "[masqué]";
const CLES_SECRETES: [&str; 9] = [
    "token",
    "password",
    "passwd",
    "mot_de_passe",
    "secret",
    "code",
    "authorization",
    "apikey",
    "api_key",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Niveau {
    Info,
    Avertissement,
    Erreur,
}

impl Niveau {
    pub fn code(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Avertissement => "warn",
            Self::Erreur => "error",
        }
    }
}

#[derive(Serialize)]
struct Ligne<'a> {
    t: u64,
    l: &'a str,
    s: &'a str,
    m: &'a str,
}

#[derive(Debug)]
pub struct Journal {
    courant: PathBuf,
    precedent: PathBuf,
    taille_max: u64,
    actif: AtomicBool,
    verrou: Mutex<()>,
}

impl Journal {
    pub fn ouvrir(dossier: &Path) -> Self {
        Self::avec_taille_max(dossier, TAILLE_MAX)
    }

    pub fn avec_taille_max(dossier: &Path, taille_max: u64) -> Self {
        let _ = fs::create_dir_all(dossier);
        Self {
            courant: dossier.join(NOM_FICHIER),
            precedent: dossier.join(NOM_FICHIER_PRECEDENT),
            taille_max,
            actif: AtomicBool::new(false),
            verrou: Mutex::new(()),
        }
    }

    pub fn activer(&self, actif: bool) {
        if actif {
            self.actif.store(true, Ordering::Relaxed);
            self.ecrire(Niveau::Info, "debug", "mode debug activé");
        } else {
            self.ecrire(Niveau::Info, "debug", "mode debug désactivé");
            self.actif.store(false, Ordering::Relaxed);
        }
    }

    pub fn est_actif(&self) -> bool {
        self.actif.load(Ordering::Relaxed)
    }

    pub fn ecrire(&self, niveau: Niveau, source: &str, message: &str) {
        if !self.est_actif() {
            return;
        }
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let ligne = Ligne {
            t,
            l: niveau.code(),
            s: &expurger(source),
            m: &expurger(message),
        };
        if let Ok(texte) = serde_json::to_string(&ligne) {
            self.ajouter(&texte);
        }
    }

    pub fn lire(&self) -> String {
        let _garde = self.verrouiller();
        fs::read_to_string(&self.precedent).unwrap_or_default()
            + &fs::read_to_string(&self.courant).unwrap_or_default()
    }

    pub fn exporter(&self) -> String {
        self.lire()
    }

    pub fn exporter_vers(&self, destination: &Path) -> io::Result<()> {
        fs::write(destination, self.lire())
    }

    pub fn vider(&self) {
        let _garde = self.verrouiller();
        let _ = fs::remove_file(&self.courant);
        let _ = fs::remove_file(&self.precedent);
    }

    fn verrouiller(&self) -> MutexGuard<'_, ()> {
        self.verrou.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn ajouter(&self, ligne: &str) {
        let _garde = self.verrouiller();
        if fs::metadata(&self.courant)
            .map(|m| m.len() >= self.taille_max)
            .unwrap_or(false)
        {
            let _ = fs::rename(&self.courant, &self.precedent);
        }
        if let Ok(mut fichier) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.courant)
        {
            let _ = writeln!(fichier, "{ligne}");
        }
    }
}

pub fn expurger(texte: &str) -> String {
    let caracteres: Vec<char> = texte.chars().collect();
    let sans_jwt = remplacer(&caracteres, jwt);
    let sans_bearer = remplacer(&sans_jwt, bearer);
    remplacer(&sans_bearer, champ_secret).into_iter().collect()
}

type Motif = fn(&[char], usize) -> Option<(usize, String)>;

fn remplacer(c: &[char], motif: Motif) -> Vec<char> {
    let mut sortie = Vec::with_capacity(c.len());
    let mut i = 0;
    while i < c.len() {
        match motif(c, i) {
            Some((fin, remplacement)) if fin > i => {
                sortie.extend(remplacement.chars());
                i = fin;
            }
            _ => {
                sortie.push(c[i]);
                i += 1;
            }
        }
    }
    sortie
}

fn est_mot(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn suite(c: &[char], debut: usize, admis: impl Fn(char) -> bool) -> usize {
    debut
        + c[debut.min(c.len())..]
            .iter()
            .take_while(|x| admis(**x))
            .count()
}

fn commence_par(c: &[char], debut: usize, mot: &str) -> bool {
    let mut restant = c[debut..].iter();
    mot.chars().all(|attendu| {
        restant
            .next()
            .is_some_and(|recu| recu.eq_ignore_ascii_case(&attendu))
    })
}

fn jwt(c: &[char], i: usize) -> Option<(usize, String)> {
    let segment = |x: char| est_mot(x) || x == '-';
    if c[i..].len() < 3 || c[i] != 'e' || c[i + 1] != 'y' || c[i + 2] != 'J' {
        return None;
    }
    let entete = suite(c, i + 3, segment);
    if entete == i + 3 || c.get(entete) != Some(&'.') {
        return None;
    }
    let charge = suite(c, entete + 1, segment);
    if charge == entete + 1 || c.get(charge) != Some(&'.') {
        return None;
    }
    Some((suite(c, charge + 1, segment), JETON.to_string()))
}

fn bearer(c: &[char], i: usize) -> Option<(usize, String)> {
    if (i > 0 && est_mot(c[i - 1])) || !commence_par(c, i, "bearer") {
        return None;
    }
    let espaces = suite(c, i + 6, char::is_whitespace);
    let fin = suite(c, espaces, |x| est_mot(x) || ".~+/=-".contains(x));
    (espaces > i + 6 && fin > espaces).then(|| (fin, format!("Bearer {JETON}")))
}

fn champ_secret(c: &[char], i: usize) -> Option<(usize, String)> {
    CLES_SECRETES.iter().find_map(|cle| {
        if !commence_par(c, i, cle) {
            return None;
        }
        let mut j = i + cle.len();
        if matches!(c.get(j), Some('"' | '\'')) {
            j += 1;
        }
        j = suite(c, j, char::is_whitespace);
        if !matches!(c.get(j), Some(':' | '=')) {
            return None;
        }
        j = suite(c, j + 1, char::is_whitespace);
        let fin = valeur_entre(c, j, '"')
            .or_else(|| valeur_entre(c, j, '\''))
            .unwrap_or_else(|| suite(c, j, |x| !x.is_whitespace() && !"&,;}".contains(x)));
        (fin > j).then(|| (fin, c[i..j].iter().collect::<String>() + MASQUE))
    })
}

fn valeur_entre(c: &[char], debut: usize, guillemet: char) -> Option<usize> {
    if c.get(debut) != Some(&guillemet) {
        return None;
    }
    c[debut + 1..]
        .iter()
        .position(|x| *x == guillemet)
        .map(|p| debut + p + 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    const JWT: &str = "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJhbGljZSJ9.c2lnbmF0dXJl-_x";

    fn journal_actif(dossier: &Path) -> Journal {
        let journal = Journal::ouvrir(dossier);
        journal.activer(true);
        journal
    }

    #[test]
    fn un_jeton_passe_au_journal_y_est_masque() {
        let dossier = tempfile::tempdir().unwrap();
        let journal = journal_actif(dossier.path());
        journal.ecrire(
            Niveau::Info,
            "réseau",
            &format!("GET /me avec Authorization: Bearer {JWT}"),
        );
        journal.ecrire(
            Niveau::Avertissement,
            "connexion",
            &format!("retour https://app/cb?code=abc123&state=x jeton {JWT}"),
        );
        journal.ecrire(
            Niveau::Erreur,
            "connexion",
            r#"corps {"refresh_token":"rt-secret-1","password": "mot de passe"}"#,
        );
        let contenu = fs::read_to_string(dossier.path().join(NOM_FICHIER)).unwrap();
        let interdits = [JWT, "eyJ", "abc123", "rt-secret-1", "mot de passe"];
        for (rang, interdit) in interdits.iter().enumerate() {
            assert!(
                !contenu.contains(interdit),
                "la valeur sensible n°{rang} est restée dans le journal"
            );
        }
        assert!(contenu.contains("[jeton]"));
        assert!(contenu.contains("code=[masqué]&state=x"));
        assert_eq!(journal.exporter(), contenu);
    }

    #[test]
    fn expurge_comme_le_journal_web() {
        assert_eq!(expurger(&format!("jeton {JWT} reçu")), "jeton [jeton] reçu");
        assert_eq!(expurger("eyJabc.def."), "[jeton]");
        assert_eq!(expurger("eyJabc.def"), "eyJabc.def");
        assert_eq!(
            expurger("en-tête bearer   abc.DEF~+/=- fin"),
            "en-tête Bearer [jeton] fin"
        );
        assert_eq!(expurger("Bearer"), "Bearer");
        assert_eq!(expurger("xBearer abc"), "xBearer abc");
        assert_eq!(
            expurger("/cb?access_token=abc&next=1"),
            "/cb?access_token=[masqué]&next=1"
        );
        assert_eq!(expurger("TOKEN : abc, suite"), "TOKEN : [masqué], suite");
        assert_eq!(
            expurger(r#"{"password":"a b","x":1}"#),
            r#"{"password":[masqué],"x":1}"#
        );
        assert_eq!(expurger("secret='a b' c"), "secret=[masqué] c");
        assert_eq!(
            expurger(r#"mot_de_passe="ouvert"#),
            r#"mot_de_passe=[masqué]"#
        );
        assert_eq!(
            expurger("api_key=k1;apikey=k2}"),
            "api_key=[masqué];apikey=[masqué]}"
        );
        assert_eq!(expurger("passwd=é€ fin"), "passwd=[masqué] fin");
        assert_eq!(expurger("token="), "token=");
        assert_eq!(expurger("le code postal"), "le code postal");
        assert_eq!(expurger("GET /servers → 200"), "GET /servers → 200");
        assert_eq!(
            expurger("Authorization: Bearer abc"),
            "Authorization: [masqué] [jeton]"
        );
        assert_eq!(expurger(""), "");
    }

    #[test]
    fn n_ecrit_rien_tant_que_le_mode_est_desactive() {
        let dossier = tempfile::tempdir().unwrap();
        let journal = Journal::ouvrir(dossier.path());
        journal.ecrire(Niveau::Info, "test", "ignoré");
        assert!(!journal.est_actif());
        assert!(!dossier.path().join(NOM_FICHIER).exists());
        assert_eq!(journal.exporter(), "");
        journal.activer(true);
        journal.ecrire(Niveau::Avertissement, "test", "gardé");
        journal.activer(false);
        journal.ecrire(Niveau::Info, "test", "ignoré");
        let lignes: Vec<serde_json::Value> = journal
            .lire()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let messages: Vec<&str> = lignes.iter().map(|l| l["m"].as_str().unwrap()).collect();
        assert_eq!(
            messages,
            ["mode debug activé", "gardé", "mode debug désactivé"]
        );
        assert_eq!(
            (lignes[1]["l"].as_str(), lignes[1]["s"].as_str()),
            (Some("warn"), Some("test"))
        );
        assert!(lignes[1]["t"].as_u64().unwrap() > 0);
    }

    #[test]
    fn rotation_au_dela_du_plafond() {
        let dossier = tempfile::tempdir().unwrap();
        let journal = Journal::avec_taille_max(dossier.path(), 64);
        let courant = dossier.path().join(NOM_FICHIER);
        let precedent = dossier.path().join(NOM_FICHIER_PRECEDENT);
        fs::write(&precedent, "très ancien\n").unwrap();
        fs::write(&courant, vec![b'x'; 64]).unwrap();
        journal.ajouter("{\"m\":\"neuf\"}");
        assert_eq!(fs::metadata(&precedent).unwrap().len(), 64);
        assert_eq!(fs::read_to_string(&courant).unwrap(), "{\"m\":\"neuf\"}\n");
        journal.ajouter("{\"m\":\"suivant\"}");
        assert_eq!(fs::metadata(&precedent).unwrap().len(), 64);
        assert_eq!(fs::read_to_string(&courant).unwrap().lines().count(), 2);
    }

    #[test]
    fn exporte_le_fichier_precedent_puis_le_courant() {
        let dossier = tempfile::tempdir().unwrap();
        let journal = Journal::ouvrir(dossier.path());
        fs::write(dossier.path().join(NOM_FICHIER_PRECEDENT), "ancien\n").unwrap();
        fs::write(dossier.path().join(NOM_FICHIER), "récent\n").unwrap();
        assert_eq!(journal.exporter(), "ancien\nrécent\n");
        let destination = dossier.path().join("export.log");
        journal.exporter_vers(&destination).unwrap();
        assert_eq!(
            fs::read_to_string(&destination).unwrap(),
            "ancien\nrécent\n"
        );
        assert!(journal
            .exporter_vers(&dossier.path().join("absent").join("export.log"))
            .is_err());
        journal.vider();
        assert_eq!(journal.exporter(), "");
        assert!(!dossier.path().join(NOM_FICHIER).exists());
    }

    #[test]
    fn cree_le_dossier_des_journaux() {
        let racine = tempfile::tempdir().unwrap();
        let dossier = racine.path().join("journaux").join("estompe");
        let journal = journal_actif(&dossier);
        journal.ecrire(Niveau::Info, "test", "ligne");
        assert!(dossier.join(NOM_FICHIER).is_file());
    }
}
