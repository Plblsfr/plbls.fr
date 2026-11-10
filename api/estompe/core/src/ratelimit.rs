use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Etat {
    coups: HashMap<String, Vec<Instant>>,
    balayage: Option<Instant>,
}

#[derive(Default)]
pub struct RateLimiter(Mutex<Etat>);

impl RateLimiter {
    pub fn allow(&self, cle: &str, max: usize, fenetre: Duration) -> bool {
        self.allow_at(cle, max, fenetre, Instant::now())
    }

    pub fn allow_at(&self, cle: &str, max: usize, fenetre: Duration, maintenant: Instant) -> bool {
        let mut etat = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let depuis = |t: Instant| maintenant.saturating_duration_since(t);
        if etat.balayage.is_none_or(|b| depuis(b) >= fenetre) {
            etat.coups
                .retain(|_, hs| hs.last().is_some_and(|t| depuis(*t) < fenetre));
            etat.balayage = Some(maintenant);
        }
        let coups = etat.coups.entry(cle.to_string()).or_default();
        coups.retain(|t| depuis(*t) < fenetre);
        if coups.len() >= max {
            return false;
        }
        coups.push(maintenant);
        true
    }

    #[cfg(test)]
    fn cles(&self) -> usize {
        self.0.lock().unwrap().coups.len()
    }
}

#[derive(Default)]
pub struct Limiteurs {
    pub apercu: RateLimiter,
    pub envoi: RateLimiter,
    pub recherche: RateLimiter,
    pub saisie: RateLimiter,
    pub gif: RateLimiter,
    pub transcription: RateLimiter,
    pub signalement: RateLimiter,
    pub regarder_ensemble: RateLimiter,
    pub films: RateLimiter,
    pub ws_ticket: RateLimiter,
    pub serveurs: RateLimiter,
    pub dms: RateLimiter,
    pub reactions: RateLimiter,
}

#[cfg(test)]
mod tests {
    use super::{Limiteurs, RateLimiter};
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, Instant},
    };

    const F: Duration = Duration::from_secs(60);

    #[test]
    fn le_n_plus_un_est_refuse_puis_accepte_apres_la_fenetre() {
        let l = RateLimiter::default();
        let t0 = Instant::now();
        for i in 0..3 {
            assert!(l.allow_at("a", 3, F, t0 + Duration::from_secs(i)), "{i}");
        }
        assert!(!l.allow_at("a", 3, F, t0 + Duration::from_secs(10)));
        assert!(!l.allow_at("a", 3, F, t0 + Duration::from_secs(59)));
        assert!(l.allow_at("a", 3, F, t0 + Duration::from_secs(60)));
        assert!(!l.allow_at("a", 3, F, t0 + Duration::from_secs(60)));
        assert!(l.allow_at("a", 3, F, t0 + Duration::from_secs(61)));
    }

    #[test]
    fn les_cles_sont_independantes() {
        let l = RateLimiter::default();
        let t0 = Instant::now();
        assert!(l.allow_at("a", 1, F, t0));
        assert!(!l.allow_at("a", 1, F, t0));
        assert!(l.allow_at("b", 1, F, t0));
    }

    #[test]
    fn les_cles_inactives_sont_oubliees() {
        let l = RateLimiter::default();
        let t0 = Instant::now();
        for k in ["a", "b", "c"] {
            assert!(l.allow_at(k, 5, F, t0));
        }
        assert_eq!(l.cles(), 3);
        assert!(l.allow_at("d", 5, F, t0 + F));
        assert_eq!(l.cles(), 1);
    }

    #[test]
    fn exactement_max_acceptes_entre_threads() {
        let l = RateLimiter::default();
        let acceptes = AtomicUsize::new(0);
        let t0 = Instant::now();
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    for _ in 0..50 {
                        if l.allow_at("a", 20, F, t0) {
                            acceptes.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                });
            }
        });
        assert_eq!(acceptes.load(Ordering::SeqCst), 20);
    }

    #[test]
    fn limiteurs_distincts() {
        let l = Limiteurs::default();
        let tous = [
            &l.apercu,
            &l.envoi,
            &l.recherche,
            &l.saisie,
            &l.gif,
            &l.transcription,
            &l.signalement,
            &l.regarder_ensemble,
            &l.films,
            &l.ws_ticket,
            &l.serveurs,
            &l.dms,
            &l.reactions,
        ];
        assert!(tous[0].allow("a", 1, F));
        assert!(!tous[0].allow("a", 1, F));
        for autre in &tous[1..] {
            assert!(autre.allow("a", 1, F));
        }
    }
}
