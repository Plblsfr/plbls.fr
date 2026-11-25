use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use bytes::Bytes;
use futures_util::future::{BoxFuture, FutureExt, Shared};
use tokio::task::{AbortHandle, JoinHandle};

use crate::http::{Client, ErreurApi};

use super::depot::{partager, DepotPartage, Memoire};
use super::film::PrefTampon;
use super::fournisseurs::{fusionner_plages, Plage};

pub const GIO: u64 = 1 << 30;
pub const PARALLELE: usize = 3;
pub const GARDER_DERRIERE: f64 = 30.0;
pub const REESSAI: Duration = Duration::from_secs(5);
pub const ECHECS_MAX: u32 = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct Fragment {
    pub url: String,
    pub debut: f64,
    pub duree: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budget {
    pub secondes: f64,
    pub octets: u64,
}

pub fn budget(tampon: PrefTampon) -> Option<Budget> {
    match tampon {
        PrefTampon::Normal => None,
        PrefTampon::Long => Some(Budget {
            secondes: 15.0 * 60.0,
            octets: 2 * GIO,
        }),
        PrefTampon::Tout => Some(Budget {
            secondes: f64::INFINITY,
            octets: 16 * GIO,
        }),
    }
}

pub fn nom_de_segment(url: &str) -> String {
    let chemin = url.split(['?', '#']).next().unwrap_or(url);
    match chemin.rsplit('/').next() {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => url.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErreurSegment {
    PasPret,
    Echec(String),
}

pub type Telechargement = BoxFuture<'static, Result<Bytes, ErreurSegment>>;

pub trait Telechargeur: Send + Sync + 'static {
    fn telecharger(&self, url: &str, avance: bool) -> Telechargement;
}

impl<F> Telechargeur for F
where
    F: Fn(&str, bool) -> Telechargement + Send + Sync + 'static,
{
    fn telecharger(&self, url: &str, avance: bool) -> Telechargement {
        self(url, avance)
    }
}

pub struct TelechargeurHttp {
    client: Client,
}

impl TelechargeurHttp {
    pub fn nouveau(client: Client) -> Self {
        Self { client }
    }
}

impl Telechargeur for TelechargeurHttp {
    fn telecharger(&self, url: &str, avance: bool) -> Telechargement {
        let client = self.client.clone();
        let chemin = if avance {
            let sep = if url.contains('?') { '&' } else { '?' };
            format!("{url}{sep}avance=1")
        } else {
            url.to_string()
        };
        async move {
            match client.octets(&chemin).await {
                Ok(o) => Ok(o),
                Err(ErreurApi::Introuvable(_)) if avance => Err(ErreurSegment::PasPret),
                Err(e) => Err(ErreurSegment::Echec(e.to_string())),
            }
        }
        .boxed()
    }
}

type Partage = Shared<BoxFuture<'static, Result<Bytes, ErreurSegment>>>;

struct Vol {
    id: u64,
    partage: Partage,
    arret: AbortHandle,
}

#[derive(Default)]
struct EtatAvance {
    fragments: Vec<Fragment>,
    en_vol: HashMap<String, Vol>,
    sain: bool,
    en_avance: usize,
    parti: bool,
    echecs: u32,
    reessai: Option<JoinHandle<()>>,
    attendu: Option<String>,
    retenue: Option<JoinHandle<()>>,
    prochain_id: u64,
}

type FonctionBudget = Box<dyn Fn() -> Option<Budget> + Send + Sync>;
type FonctionPosition = Box<dyn Fn() -> f64 + Send + Sync>;

struct Interne {
    etat: Mutex<EtatAvance>,
    depot: DepotPartage,
    possede: bool,
    budget: FonctionBudget,
    position: FonctionPosition,
    telechargeur: Arc<dyn Telechargeur>,
    parallele: usize,
}

#[derive(Clone)]
pub struct Avance(Arc<Interne>);

#[derive(Default)]
pub struct OptionsAvance {
    pub depot: Option<DepotPartage>,
    pub parallele: Option<usize>,
}

impl Avance {
    pub fn nouvelle(
        budget: impl Fn() -> Option<Budget> + Send + Sync + 'static,
        position: impl Fn() -> f64 + Send + Sync + 'static,
        telechargeur: Arc<dyn Telechargeur>,
        options: OptionsAvance,
    ) -> Self {
        let possede = options.depot.is_none();
        Self(Arc::new(Interne {
            etat: Mutex::new(EtatAvance::default()),
            depot: options
                .depot
                .unwrap_or_else(|| partager(Box::new(Memoire::default()))),
            possede,
            budget: Box::new(budget),
            position: Box::new(position),
            telechargeur,
            parallele: options.parallele.unwrap_or(PARALLELE),
        }))
    }

    pub fn octets(&self) -> u64 {
        self.0.depot().octets()
    }

    pub fn active(&self) -> bool {
        !self.0.etat().parti && (self.0.budget)().is_some()
    }

    pub fn contient(&self, url: &str) -> bool {
        self.0.depot().contient(&nom_de_segment(url))
    }

    pub fn definir_fragments(&self, fragments: Vec<Fragment>) {
        self.0.etat().fragments = fragments;
        self.reveiller();
    }

    pub async fn obtenir(&self, url: &str) -> Result<Bytes, ErreurSegment> {
        let nom = nom_de_segment(url);
        let garde = {
            let mut d = self.0.depot();
            if d.contient(&nom) {
                d.lire(&nom)
            } else {
                None
            }
        };
        if let Some(o) = garde {
            return Ok(o);
        }
        let vol = self.0.etat().en_vol.get(&nom).map(|v| v.partage.clone());
        match vol {
            Some(v) => match v.await {
                Ok(o) => Ok(o),
                Err(_) => self.0.charger(url, &nom, false).await,
            },
            None => self.0.charger(url, &nom, false).await,
        }
    }

    pub fn plages(&self) -> Vec<Plage> {
        let fragments = self.0.etat().fragments.clone();
        let d = self.0.depot();
        let plages: Vec<Plage> = fragments
            .iter()
            .filter(|f| d.contient(&nom_de_segment(&f.url)))
            .map(|f| (f.debut, f.debut + f.duree))
            .collect();
        fusionner_plages(&plages, 0.5)
    }

    pub fn reveiller(&self) {
        {
            let e = self.0.etat();
            if e.parti || e.reessai.is_some() || e.echecs >= ECHECS_MAX {
                return;
            }
        }
        self.0.pomper();
    }

    pub fn detruire(&self) {
        let mut e = self.0.etat();
        e.parti = true;
        if let Some(t) = e.reessai.take() {
            t.abort();
        }
        if let Some(t) = e.retenue.take() {
            t.abort();
        }
        for (_, v) in e.en_vol.drain() {
            v.arret.abort();
        }
        e.fragments.clear();
        drop(e);
        if self.0.possede {
            self.0.depot().fermer(true);
        }
    }
}

impl Interne {
    fn etat(&self) -> MutexGuard<'_, EtatAvance> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn depot(&self) -> MutexGuard<'_, Box<dyn super::depot::Depot>> {
        self.depot.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn active(&self) -> bool {
        !self.etat().parti && (self.budget)().is_some()
    }

    fn charger(self: &Arc<Self>, url: &str, nom: &str, avance: bool) -> Partage {
        let telechargement = self.telechargeur.telecharger(url, avance);
        let moi = self.clone();
        let nom_tache = nom.to_string();
        let id = {
            let mut e = self.etat();
            e.prochain_id += 1;
            e.prochain_id
        };
        let tache = tokio::spawn(async move {
            let r = telechargement.await;
            if let Ok(o) = &r {
                if moi.active() {
                    moi.depot().ecrire(&nom_tache, o.clone());
                }
            }
            let mut e = moi.etat();
            if e.en_vol.get(&nom_tache).is_some_and(|v| v.id == id) {
                e.en_vol.remove(&nom_tache);
            }
            r
        });
        let arret = tache.abort_handle();
        let partage = async move {
            tache
                .await
                .unwrap_or_else(|_| Err(ErreurSegment::Echec("interrompu".into())))
        }
        .boxed()
        .shared();
        self.etat().en_vol.insert(
            nom.to_string(),
            Vol {
                id,
                partage: partage.clone(),
                arret,
            },
        );
        partage
    }

    fn suivant(&self, e: &EtatAvance, b: Budget) -> Option<Fragment> {
        let pos = (self.position)();
        let d = self.depot();
        e.fragments
            .iter()
            .find(|f| {
                let nom = nom_de_segment(&f.url);
                f.debut + f.duree > pos
                    && f.debut <= pos + b.secondes
                    && !d.contient(&nom)
                    && !e.en_vol.contains_key(&nom)
            })
            .cloned()
    }

    fn evincer(&self, e: &EtatAvance, b: Budget) -> bool {
        let mut d = self.depot();
        let gardes = d.noms();
        let reserve = if gardes.is_empty() {
            0
        } else {
            d.octets() / gardes.len() as u64 * e.en_avance as u64
        };
        let place = |d: &dyn super::depot::Depot| d.octets() + reserve < b.octets;
        if place(d.as_ref()) {
            return true;
        }
        let listes: HashSet<String> = e.fragments.iter().map(|f| nom_de_segment(&f.url)).collect();
        for nom in gardes {
            if place(d.as_ref()) {
                return true;
            }
            if !listes.contains(&nom) {
                d.retirer(&nom);
            }
        }
        let limite = (self.position)() - GARDER_DERRIERE;
        for f in &e.fragments {
            if place(d.as_ref()) {
                return true;
            }
            if f.debut + f.duree > limite {
                break;
            }
            d.retirer(&nom_de_segment(&f.url));
        }
        place(d.as_ref())
    }

    fn pomper(self: &Arc<Self>) {
        let Some(b) = (self.budget)() else {
            return;
        };
        loop {
            let mut e = self.etat();
            if e.parti {
                return;
            }
            let limite = if e.sain { self.parallele } else { 1 };
            if e.en_avance >= limite {
                return;
            }
            let Some(f) = self.suivant(&e, b) else {
                return;
            };
            if !self.evincer(&e, b) {
                return;
            }
            let nom = nom_de_segment(&f.url);
            if e.attendu.as_deref() == Some(nom.as_str()) {
                return;
            }
            e.en_avance += 1;
            drop(e);
            let partage = self.charger(&f.url, &nom, true);
            let moi = self.clone();
            tokio::spawn(async move {
                let r = partage.await;
                moi.apres(nom, r);
            });
        }
    }

    fn apres(self: &Arc<Self>, nom: String, r: Result<Bytes, ErreurSegment>) {
        let mut e = self.etat();
        e.en_avance = e.en_avance.saturating_sub(1);
        if e.parti {
            return;
        }
        match r {
            Ok(_) => {
                e.echecs = 0;
                e.sain = true;
                drop(e);
                Avance(self.clone()).reveiller();
            }
            Err(ErreurSegment::PasPret) => {
                e.attendu = Some(nom);
                if let Some(t) = e.retenue.take() {
                    t.abort();
                }
                let moi = self.clone();
                e.retenue = Some(tokio::spawn(async move {
                    tokio::time::sleep(REESSAI).await;
                    {
                        let mut e = moi.etat();
                        e.attendu = None;
                        e.retenue = None;
                    }
                    Avance(moi).reveiller();
                }));
            }
            Err(ErreurSegment::Echec(_)) => {
                e.sain = false;
                if e.reessai.is_some() {
                    return;
                }
                e.echecs += 1;
                if e.echecs < ECHECS_MAX {
                    let moi = self.clone();
                    e.reessai = Some(tokio::spawn(async move {
                        tokio::time::sleep(REESSAI).await;
                        moi.etat().reessai = None;
                        Avance(moi).reveiller();
                    }));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::oneshot;

    fn fragments(n: usize, d: f64) -> Vec<Fragment> {
        (0..n)
            .map(|i| Fragment {
                url: format!("seg{i}"),
                debut: i as f64 * d,
                duree: d,
            })
            .collect()
    }

    fn octets(n: usize) -> Bytes {
        Bytes::from(vec![0u8; n])
    }

    async fn regler() {
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
    }

    type Journal = Arc<Mutex<Vec<String>>>;

    fn servi(taille: usize) -> (Arc<dyn Telechargeur>, Journal) {
        let journal: Journal = Arc::default();
        let j = journal.clone();
        let t = move |url: &str, _: bool| -> Telechargement {
            j.lock().unwrap().push(url.to_string());
            async move { Ok(octets(taille)) }.boxed()
        };
        (Arc::new(t), journal)
    }

    fn fixe(secondes: f64, octets: u64) -> impl Fn() -> Option<Budget> + Send + Sync {
        move || Some(Budget { secondes, octets })
    }

    fn noms(j: &Journal) -> Vec<String> {
        j.lock().unwrap().clone()
    }

    #[test]
    fn rien_de_plus_au_reglage_normal_plus_au_reglage_long_qu_au_complet_borne() {
        assert_eq!(budget(PrefTampon::Normal), None);
        assert_eq!(budget(PrefTampon::Long).unwrap().secondes, 900.0);
        assert_eq!(budget(PrefTampon::Tout).unwrap().secondes, f64::INFINITY);
        assert!(
            budget(PrefTampon::Tout).unwrap().octets > budget(PrefTampon::Long).unwrap().octets
        );
    }

    #[tokio::test(start_paused = true)]
    async fn charge_dans_l_ordre_depuis_la_position_jusqu_a_la_duree_prevue() {
        let (t, j) = servi(10);
        let a = Avance::nouvelle(
            fixe(10.0, 1_000_000_000),
            || 9.0,
            t,
            OptionsAvance::default(),
        );
        a.definir_fragments(fragments(10, 4.0));
        regler().await;
        assert_eq!(noms(&j), ["seg2", "seg3", "seg4"]);
        assert_eq!(a.plages(), [(8.0, 20.0)]);
        assert_eq!(a.octets(), 30);
    }

    #[tokio::test(start_paused = true)]
    async fn charge_tout_le_film_sans_borne_et_suit_la_liste_qui_s_allonge() {
        let (t, j) = servi(10);
        let a = Avance::nouvelle(
            fixe(f64::INFINITY, 1_000_000_000),
            || 0.0,
            t,
            OptionsAvance::default(),
        );
        a.definir_fragments(fragments(3, 4.0));
        regler().await;
        assert_eq!(a.plages(), [(0.0, 12.0)]);
        a.definir_fragments(fragments(6, 4.0));
        regler().await;
        assert_eq!(noms(&j).len(), 6);
        assert_eq!(a.plages(), [(0.0, 24.0)]);
    }

    #[tokio::test(start_paused = true)]
    async fn repart_de_la_nouvelle_position_apres_un_saut_et_montre_les_deux_plages() {
        let pos = Arc::new(Mutex::new(0.0));
        let p = pos.clone();
        let (t, _) = servi(10);
        let a = Avance::nouvelle(
            fixe(8.0, 1_000_000_000),
            move || *p.lock().unwrap(),
            t,
            OptionsAvance::default(),
        );
        a.definir_fragments(fragments(20, 4.0));
        regler().await;
        assert_eq!(a.plages(), [(0.0, 12.0)]);
        *pos.lock().unwrap() = 40.0;
        a.reveiller();
        regler().await;
        assert_eq!(a.plages(), [(0.0, 12.0), (40.0, 52.0)]);
    }

    #[tokio::test(start_paused = true)]
    async fn un_segment_n_est_telecharge_qu_une_fois() {
        let (t, j) = servi(10);
        let a = Avance::nouvelle(
            fixe(4.0, 1_000_000_000),
            || 0.0,
            t,
            OptionsAvance::default(),
        );
        let b = a.clone();
        let p = tokio::spawn(async move { b.obtenir("seg0").await });
        regler().await;
        a.definir_fragments(fragments(1, 4.0));
        assert_eq!(p.await.unwrap().unwrap().len(), 10);
        regler().await;
        a.obtenir("seg0").await.unwrap();
        assert_eq!(noms(&j).len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn libere_ce_qui_est_loin_derriere_quand_la_memoire_est_pleine_sinon_s_arrete() {
        let pos = Arc::new(Mutex::new(0.0));
        let p = pos.clone();
        let (t, _) = servi(10);
        let a = Avance::nouvelle(
            fixe(f64::INFINITY, 50),
            move || *p.lock().unwrap(),
            t,
            OptionsAvance::default(),
        );
        a.definir_fragments(fragments(30, 4.0));
        regler().await;
        assert_eq!(a.plages(), [(0.0, 20.0)]);
        *pos.lock().unwrap() = 60.0;
        a.reveiller();
        regler().await;
        assert!(a.octets() <= 50);
        assert_eq!(a.plages(), [(60.0, 80.0)]);
    }

    #[tokio::test(start_paused = true)]
    async fn reessaie_apres_un_echec_puis_abandonne_au_bout_de_trois() {
        let appels = Arc::new(AtomicUsize::new(0));
        let c = appels.clone();
        let t = move |_: &str, _: bool| -> Telechargement {
            c.fetch_add(1, Ordering::SeqCst);
            async { Err(ErreurSegment::Echec("réseau".into())) }.boxed()
        };
        let a = Avance::nouvelle(
            fixe(4.0, 1_000_000_000),
            || 0.0,
            Arc::new(t),
            OptionsAvance::default(),
        );
        a.definir_fragments(fragments(2, 4.0));
        for _ in 0..6 {
            regler().await;
            tokio::time::advance(REESSAI).await;
        }
        regler().await;
        assert_eq!(appels.load(Ordering::SeqCst), 3);
        assert!(a.plages().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn plusieurs_segments_a_la_fois_apres_le_premier_jamais_plus_que_prevu_le_plus_proche_d_abord(
    ) {
        type Attente = Arc<Mutex<Vec<oneshot::Sender<()>>>>;
        let attente: Attente = Arc::default();
        let journal: Journal = Arc::default();
        let en_vol = Arc::new(AtomicUsize::new(0));
        let max = Arc::new(AtomicUsize::new(0));
        let (at, j, ev, mx) = (
            attente.clone(),
            journal.clone(),
            en_vol.clone(),
            max.clone(),
        );
        let t = move |url: &str, avance: bool| -> Telechargement {
            assert!(avance);
            j.lock().unwrap().push(url.to_string());
            let n = ev.fetch_add(1, Ordering::SeqCst) + 1;
            mx.fetch_max(n, Ordering::SeqCst);
            let (tx, rx) = oneshot::channel();
            at.lock().unwrap().push(tx);
            let ev = ev.clone();
            async move {
                let _ = rx.await;
                ev.fetch_sub(1, Ordering::SeqCst);
                Ok(octets(10))
            }
            .boxed()
        };
        let a = Avance::nouvelle(
            fixe(f64::INFINITY, 1_000_000_000),
            || 9.0,
            Arc::new(t),
            OptionsAvance::default(),
        );
        a.definir_fragments(fragments(12, 4.0));
        regler().await;
        assert_eq!(noms(&journal), ["seg2"]);
        let liberer = || {
            let tx = attente.lock().unwrap().remove(0);
            let _ = tx.send(());
        };
        liberer();
        regler().await;
        assert_eq!(noms(&journal), ["seg2", "seg3", "seg4", "seg5"]);
        a.reveiller();
        regler().await;
        assert_eq!(noms(&journal).len(), 4);
        while !attente.lock().unwrap().is_empty() {
            liberer();
            regler().await;
        }
        let attendus: Vec<String> = (2..12).map(|i| format!("seg{i}")).collect();
        assert_eq!(noms(&journal), attendus);
        assert_eq!(max.load(Ordering::SeqCst), PARALLELE);
        assert_eq!(a.plages(), [(8.0, 48.0)]);
    }

    #[tokio::test(start_paused = true)]
    async fn suit_un_reglage_qui_change() {
        let reglage: Arc<Mutex<Option<Budget>>> = Arc::default();
        let r = reglage.clone();
        let (t, j) = servi(10);
        let a = Avance::nouvelle(
            move || *r.lock().unwrap(),
            || 0.0,
            t,
            OptionsAvance::default(),
        );
        a.definir_fragments(fragments(3, 4.0));
        regler().await;
        assert!(noms(&j).is_empty());
        assert!(!a.active());
        *reglage.lock().unwrap() = Some(Budget {
            secondes: 100.0,
            octets: 1_000_000_000,
        });
        a.reveiller();
        regler().await;
        assert_eq!(noms(&j).len(), 3);
        *reglage.lock().unwrap() = None;
        a.definir_fragments(fragments(6, 4.0));
        regler().await;
        assert_eq!(noms(&j).len(), 3);
        assert!(a.contient("seg1"));
    }

    #[tokio::test(start_paused = true)]
    async fn attend_un_segment_que_le_serveur_n_a_pas_encore_sans_compter_d_echec() {
        let pret = Arc::new(Mutex::new(false));
        let pos = Arc::new(Mutex::new(0.0));
        let journal: Journal = Arc::default();
        let (pr, j) = (pret.clone(), journal.clone());
        let t = move |url: &str, _: bool| -> Telechargement {
            j.lock().unwrap().push(url.to_string());
            let r = if url == "seg1" && !*pr.lock().unwrap() {
                Err(ErreurSegment::PasPret)
            } else {
                Ok(octets(10))
            };
            async move { r }.boxed()
        };
        let p = pos.clone();
        let a = Avance::nouvelle(
            fixe(8.0, 1_000_000_000),
            move || *p.lock().unwrap(),
            Arc::new(t),
            OptionsAvance::default(),
        );
        a.definir_fragments(fragments(10, 4.0));
        regler().await;
        assert_eq!(noms(&journal), ["seg0", "seg1", "seg2"]);
        for _ in 0..5 {
            a.reveiller();
            regler().await;
        }
        assert_eq!(noms(&journal).len(), 3);
        *pos.lock().unwrap() = 20.0;
        a.reveiller();
        regler().await;
        assert_eq!(a.plages(), [(0.0, 4.0), (8.0, 12.0), (20.0, 32.0)]);
        *pos.lock().unwrap() = 0.0;
        *pret.lock().unwrap() = true;
        for _ in 0..4 {
            tokio::time::advance(REESSAI).await;
            regler().await;
        }
        assert_eq!(a.plages(), [(0.0, 12.0), (20.0, 32.0)]);
    }

    #[tokio::test(start_paused = true)]
    async fn un_segment_est_reconnu_par_son_nom_quand_le_flux_est_rouvert() {
        assert_eq!(
            nom_de_segment("/stream/jeton.1.x/seg00012.ts?avance=1"),
            "seg00012.ts"
        );
        let depot = partager(Box::new(Memoire::default()));
        let jeton = |j: &str, n: usize| -> Vec<Fragment> {
            fragments(n, 4.0)
                .into_iter()
                .enumerate()
                .map(|(i, f)| Fragment {
                    url: format!("/stream/{j}/seg{i}.ts"),
                    ..f
                })
                .collect()
        };
        let (t1, _) = servi(10);
        let a = Avance::nouvelle(
            fixe(100.0, 1_000_000_000),
            || 0.0,
            t1,
            OptionsAvance {
                depot: Some(depot.clone()),
                parallele: None,
            },
        );
        a.definir_fragments(jeton("a", 3));
        regler().await;
        a.detruire();
        assert_eq!(depot.lock().unwrap().octets(), 30);
        let (t2, j2) = servi(10);
        let b = Avance::nouvelle(
            fixe(100.0, 1_000_000_000),
            || 0.0,
            t2,
            OptionsAvance {
                depot: Some(depot.clone()),
                parallele: None,
            },
        );
        b.definir_fragments(jeton("b", 4));
        regler().await;
        assert_eq!(noms(&j2), ["/stream/b/seg3.ts"]);
        assert_eq!(b.plages(), [(0.0, 16.0)]);
        assert_eq!(b.obtenir("/stream/b/seg1.ts").await.unwrap().len(), 10);
        assert_eq!(noms(&j2).len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn libere_d_abord_les_segments_absents_de_la_liste_du_flux() {
        let depot = partager(Box::new(Memoire::default()));
        depot.lock().unwrap().ecrire("ancien0.ts", octets(20));
        depot.lock().unwrap().ecrire("ancien1.ts", octets(20));
        let (t, _) = servi(10);
        let a = Avance::nouvelle(
            fixe(100.0, 50),
            || 0.0,
            t,
            OptionsAvance {
                depot: Some(depot.clone()),
                parallele: None,
            },
        );
        a.definir_fragments(fragments(4, 4.0));
        regler().await;
        assert!(!depot
            .lock()
            .unwrap()
            .noms()
            .iter()
            .any(|n| n.starts_with("ancien")));
        assert_eq!(a.plages(), [(0.0, 16.0)]);
        assert!(a.octets() <= 50);
    }

    #[tokio::test(start_paused = true)]
    async fn redemande_pour_la_lecture_un_segment_que_le_prechargement_n_a_pas_obtenu() {
        let vus: Arc<Mutex<Vec<bool>>> = Arc::default();
        let v = vus.clone();
        let t = move |_: &str, avance: bool| -> Telechargement {
            v.lock().unwrap().push(avance);
            async move {
                if avance {
                    Err(ErreurSegment::PasPret)
                } else {
                    Ok(octets(7))
                }
            }
            .boxed()
        };
        let a = Avance::nouvelle(
            fixe(4.0, 1_000_000_000),
            || 0.0,
            Arc::new(t),
            OptionsAvance::default(),
        );
        a.definir_fragments(fragments(1, 4.0));
        assert_eq!(a.obtenir("seg0").await.unwrap().len(), 7);
        assert_eq!(*vus.lock().unwrap(), [true, false]);
        a.detruire();
    }

    #[tokio::test(start_paused = true)]
    async fn ne_garde_rien_apres_destruction() {
        let (t, _) = servi(10);
        let a = Avance::nouvelle(
            fixe(100.0, 1_000_000_000),
            || 0.0,
            t,
            OptionsAvance::default(),
        );
        a.definir_fragments(fragments(3, 4.0));
        regler().await;
        a.detruire();
        assert!(a.plages().is_empty());
        assert_eq!(a.octets(), 0);
    }
}
