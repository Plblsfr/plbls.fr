use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use regex::Regex;
use tokio::sync::{OnceCell, Semaphore};
use uuid::Uuid;

pub const TAILLE_MAX: usize = 5 << 20;
pub const EXTRACTIONS_MAX: usize = 2;
pub const DUREE_ECHEC: Duration = Duration::from_secs(60);

type Cle = (Uuid, String);
type Partage = Arc<OnceCell<Result<String, Echec>>>;

#[derive(Clone, Debug, PartialEq)]
pub enum Echec {
    Extraction(String),
    Base(String),
}

pub struct Extractions {
    en_cours: Mutex<HashMap<Cle, Partage>>,
    echecs: Mutex<HashMap<Cle, (Instant, String)>>,
    places: Semaphore,
    duree_echec: Duration,
}

impl Extractions {
    pub fn nouveau(places: usize, duree_echec: Duration) -> Self {
        Self {
            en_cours: Mutex::default(),
            echecs: Mutex::default(),
            places: Semaphore::new(places.max(1)),
            duree_echec,
        }
    }

    fn echec_recent(&self, cle: &Cle) -> Option<String> {
        let mut echecs = self.echecs.lock().expect("échecs de sous-titres");
        let maintenant = Instant::now();
        echecs.retain(|_, (quand, _)| maintenant.duration_since(*quand) < self.duree_echec);
        echecs.get(cle).map(|(_, e)| e.clone())
    }

    pub async fn obtenir<F, Fut>(
        &self,
        fichier: Uuid,
        piste: &str,
        extraire: F,
    ) -> Result<String, Echec>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<String, Echec>>,
    {
        let cle = (fichier, piste.to_string());
        if let Some(e) = self.echec_recent(&cle) {
            return Err(Echec::Extraction(e));
        }
        let cellule = self
            .en_cours
            .lock()
            .expect("extractions de sous-titres")
            .entry(cle.clone())
            .or_default()
            .clone();
        let resultat = cellule
            .get_or_init(|| async {
                if let Some(e) = self.echec_recent(&cle) {
                    return Err(Echec::Extraction(e));
                }
                let _place = self
                    .places
                    .acquire()
                    .await
                    .map_err(|e| Echec::Extraction(e.to_string()))?;
                let r = extraire().await;
                if let Err(Echec::Extraction(e)) = &r {
                    self.echecs
                        .lock()
                        .expect("échecs de sous-titres")
                        .insert(cle.clone(), (Instant::now(), e.clone()));
                }
                r
            })
            .await
            .clone();
        let mut en_cours = self.en_cours.lock().expect("extractions de sous-titres");
        if en_cours.get(&cle).is_some_and(|c| Arc::ptr_eq(c, &cellule)) {
            en_cours.remove(&cle);
        }
        resultat
    }

    #[cfg(test)]
    fn en_attente(&self, fichier: Uuid, piste: &str) -> usize {
        self.en_cours
            .lock()
            .expect("extractions de sous-titres")
            .get(&(fichier, piste.to_string()))
            .map_or(0, |c| Arc::strong_count(c) - 1)
    }
}

fn motif(cellule: &'static OnceLock<Regex>, source: &str) -> &'static Regex {
    cellule.get_or_init(|| Regex::new(source).expect("motif de sous-titres"))
}

fn balise_autorisee(balise: &str) -> bool {
    let nom = balise
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim_start_matches('/')
        .to_ascii_lowercase();
    matches!(nom.as_str(), "b" | "i" | "u")
}

fn retirer_blocs(vtt: &str) -> String {
    let mut sortie = Vec::new();
    for bloc in vtt.replace("\r\n", "\n").split("\n\n") {
        let debut = bloc.trim_start();
        if debut.starts_with("STYLE") || debut.starts_with("NOTE") || debut.starts_with("REGION") {
            continue;
        }
        sortie.push(bloc.to_string());
    }
    sortie.join("\n\n")
}

fn nettoyer_texte(texte: &str) -> String {
    static SCRIPT: OnceLock<Regex> = OnceLock::new();
    static BALISE: OnceLock<Regex> = OnceLock::new();
    static LIEN: OnceLock<Regex> = OnceLock::new();
    let script = motif(&SCRIPT, r"(?is)<script\b.*?(</script\s*>|$)");
    let balise = motif(&BALISE, r"</?[a-zA-Z][^<>]*>");
    let lien = motif(
        &LIEN,
        r"(?i)\b(?:https?|ftp|file|javascript|data):\S*|\bwww\.\S+",
    );
    let mut courant = script.replace_all(texte, "").into_owned();
    loop {
        let suivant = balise
            .replace_all(&courant, |c: &regex::Captures<'_>| {
                if balise_autorisee(&c[0]) {
                    c[0].to_ascii_lowercase()
                } else {
                    String::new()
                }
            })
            .into_owned();
        let stable = suivant == courant;
        courant = suivant;
        if stable {
            break;
        }
    }
    let courant = lien.replace_all(&courant, "").into_owned();
    balise
        .replace_all(&courant, "\u{0}$0\u{1}")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace("\u{0}&lt;", "<")
        .replace("&gt;\u{1}", ">")
        .replace(['\u{0}', '\u{1}'], "")
}

pub fn nettoyer(vtt: &str) -> String {
    let sans_entete = vtt
        .trim_start_matches('\u{feff}')
        .trim_start()
        .strip_prefix("WEBVTT")
        .map(|reste| reste.split_once('\n').map_or("", |(_, r)| r))
        .unwrap_or(vtt);
    let mut sortie = String::from("WEBVTT\n\n");
    let blocs = retirer_blocs(sans_entete);
    let mut temps_vu = false;
    for ligne in blocs.lines() {
        if ligne.contains("-->") {
            temps_vu = true;
            sortie.push_str(ligne);
        } else if temps_vu {
            sortie.push_str(&nettoyer_texte(ligne));
        }
        sortie.push('\n');
    }
    sortie
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPERE: &str = "00:00:01.000 --> 00:00:02.000";

    fn cue(texte: &str) -> String {
        nettoyer(&format!("WEBVTT\n\n{REPERE}\n{texte}\n"))
    }

    #[test]
    fn garde_le_texte_et_les_balises_simples() {
        let s = cue("Bonjour <i>toi</i> <B>ici</B> <u>là</u>");
        assert!(s.starts_with("WEBVTT\n\n"), "{s}");
        assert!(s.contains(REPERE), "{s}");
        assert!(s.contains("Bonjour <i>toi</i> <b>ici</b> <u>là</u>"), "{s}");
    }

    #[test]
    fn retire_script_liens_et_autres_balises() {
        for (entree, interdit) in [
            ("<script>alert(1)</script>ok", "alert"),
            ("<SCRIPT src=x>alert(1)", "alert"),
            ("<img src=x onerror=alert(1)>ok", "onerror"),
            ("<a href='https://x.test'>ok</a>", "href"),
            ("voir https://x.test/a", "x.test"),
            ("voir www.x.test", "x.test"),
            ("javascript:alert(1)", "alert"),
            ("<c.yellow>ok</c>", "yellow"),
        ] {
            let s = cue(entree);
            assert!(!s.contains(interdit), "{entree} : {s}");
        }
    }

    #[test]
    fn balise_imbriquee_neutralisee() {
        let s = cue("<scr<b>ipt>alert(1)</scr</b>ipt>");
        assert!(!s.to_ascii_lowercase().contains("<script"), "{s}");
        assert!(!s.contains("<scr"), "{s}");
    }

    #[test]
    fn chevron_isole_echappe() {
        let s = cue("1 < 2 et 3 > 2");
        assert!(s.contains("1 &lt; 2 et 3 &gt; 2"), "{s}");
    }

    #[test]
    fn retire_style_et_notes() {
        let s = nettoyer(&format!(
            "WEBVTT\n\nSTYLE\n::cue {{ background: url(https://x.test) }}\n\nNOTE secret\n\n{REPERE}\nok\n"
        ));
        assert!(
            !s.contains("STYLE") && !s.contains("x.test") && !s.contains("secret"),
            "{s}"
        );
        assert!(s.contains("ok"), "{s}");
    }

    #[test]
    fn entete_ajoute_si_absent() {
        let s = nettoyer(&format!("{REPERE}\nok\n"));
        assert!(s.starts_with("WEBVTT\n\n"), "{s}");
        assert!(s.contains("ok"), "{s}");
    }

    async fn attendre(condition: impl Fn() -> bool) {
        while !condition() {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn une_seule_extraction_par_piste() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let extractions = Arc::new(Extractions::nouveau(EXTRACTIONS_MAX, DUREE_ECHEC));
        let lancements = Arc::new(AtomicUsize::new(0));
        let feu = Arc::new(tokio::sync::Notify::new());
        let fichier = Uuid::new_v4();
        let requetes: Vec<_> = (0..8)
            .map(|_| {
                let (x, l, f) = (extractions.clone(), lancements.clone(), feu.clone());
                tokio::spawn(async move {
                    x.obtenir(fichier, "0", || async move {
                        l.fetch_add(1, Ordering::SeqCst);
                        f.notified().await;
                        Ok("WEBVTT\n".to_string())
                    })
                    .await
                })
            })
            .collect();
        attendre(|| {
            extractions.en_attente(fichier, "0") == 8 && lancements.load(Ordering::SeqCst) == 1
        })
        .await;
        feu.notify_one();
        for r in requetes {
            assert_eq!(r.await.unwrap(), Ok("WEBVTT\n".to_string()));
        }
        assert_eq!(lancements.load(Ordering::SeqCst), 1);
        assert_eq!(extractions.en_attente(fichier, "0"), 0);
    }

    #[tokio::test]
    async fn extractions_simultanees_plafonnees() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let extractions = Arc::new(Extractions::nouveau(2, DUREE_ECHEC));
        let actives = Arc::new(AtomicUsize::new(0));
        let pic = Arc::new(AtomicUsize::new(0));
        let feu = Arc::new(tokio::sync::Semaphore::new(0));
        let requetes: Vec<_> = (0..5)
            .map(|i| {
                let (x, a, p, f) = (
                    extractions.clone(),
                    actives.clone(),
                    pic.clone(),
                    feu.clone(),
                );
                tokio::spawn(async move {
                    x.obtenir(Uuid::new_v4(), &i.to_string(), || async move {
                        let n = a.fetch_add(1, Ordering::SeqCst) + 1;
                        p.fetch_max(n, Ordering::SeqCst);
                        f.acquire().await.unwrap().forget();
                        a.fetch_sub(1, Ordering::SeqCst);
                        Ok(String::new())
                    })
                    .await
                })
            })
            .collect();
        attendre(|| actives.load(Ordering::SeqCst) == 2).await;
        assert_eq!(extractions.places.available_permits(), 0);
        feu.add_permits(5);
        for r in requetes {
            assert!(r.await.unwrap().is_ok());
        }
        assert_eq!(pic.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn echec_memorise_temporairement() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let lancements = AtomicUsize::new(0);
        let echoue = || async {
            lancements.fetch_add(1, Ordering::SeqCst);
            Err(Echec::Extraction("illisible".into()))
        };
        let fichier = Uuid::new_v4();
        let x = Extractions::nouveau(1, DUREE_ECHEC);
        for _ in 0..3 {
            assert_eq!(
                x.obtenir(fichier, "1", echoue).await,
                Err(Echec::Extraction("illisible".into()))
            );
        }
        assert_eq!(lancements.load(Ordering::SeqCst), 1);
        assert!(
            x.obtenir(fichier, "2", || async { Ok(String::new()) })
                .await
                .is_ok()
        );
        let x = Extractions::nouveau(1, Duration::ZERO);
        let _ = x.obtenir(fichier, "1", echoue).await;
        let _ = x.obtenir(fichier, "1", echoue).await;
        assert_eq!(lancements.load(Ordering::SeqCst), 3);
        let base = AtomicUsize::new(0);
        let x = Extractions::nouveau(1, DUREE_ECHEC);
        for _ in 0..2 {
            let r = x
                .obtenir(fichier, "1", || async {
                    base.fetch_add(1, Ordering::SeqCst);
                    Err(Echec::Base("bd".into()))
                })
                .await;
            assert_eq!(r, Err(Echec::Base("bd".into())));
        }
        assert_eq!(base.load(Ordering::SeqCst), 2);
    }
}
