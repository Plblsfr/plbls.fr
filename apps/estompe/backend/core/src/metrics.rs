use std::{
    fmt::Write,
    sync::{
        Mutex,
        atomic::{AtomicI64, Ordering},
    },
    time::Duration,
};

#[derive(Default)]
struct Compteurs {
    requetes: [i64; 4],
    duree_somme: f64,
    duree_nombre: i64,
    recherches: i64,
    recherche_somme: f64,
}

#[derive(Default)]
pub struct Metrics {
    compteurs: Mutex<Compteurs>,
    inflight: AtomicI64,
}

pub struct Inflight<'a>(&'a Metrics);

impl Drop for Inflight<'_> {
    fn drop(&mut self) {
        self.0.inflight.fetch_sub(1, Ordering::SeqCst);
    }
}

pub struct Totaux {
    pub utilisateurs: i64,
    pub messages: i64,
    pub fichiers: i64,
}

pub struct Stockage {
    pub utilise: i64,
    pub plafond: i64,
}

pub struct Instantane {
    pub uptime: u64,
    pub base_up: bool,
    pub totaux: Option<Totaux>,
    pub stockage: Option<Stockage>,
    pub ws: (usize, usize),
}

pub fn go_g(x: f64) -> String {
    let expo = format!("{x:e}");
    let Some((mantisse, exposant)) = expo.split_once('e') else {
        return expo;
    };
    let e: i32 = exposant.parse().unwrap_or(0);
    if (-4..6).contains(&e) {
        return x.to_string();
    }
    let signe = if e < 0 { '-' } else { '+' };
    format!("{mantisse}e{signe}{:02}", e.abs())
}

fn gauge(sortie: &mut String, nom: &str, aide: &str, valeur: impl std::fmt::Display) {
    let _ = write!(
        sortie,
        "# HELP {nom} {aide}\n# TYPE {nom} gauge\n{nom} {valeur}\n"
    );
}

impl Metrics {
    pub fn commencer(&self) -> Inflight<'_> {
        self.inflight.fetch_add(1, Ordering::SeqCst);
        Inflight(self)
    }

    pub fn observer(&self, code: u16, duree: Duration) {
        let mut c = self.compteurs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = usize::from(code / 100)
            .checked_sub(2)
            .and_then(|i| c.requetes.get_mut(i))
        {
            *n += 1;
        }
        c.duree_somme += duree.as_secs_f64();
        c.duree_nombre += 1;
    }

    pub fn searched(&self, duree: Duration) {
        let mut c = self.compteurs.lock().unwrap_or_else(|e| e.into_inner());
        c.recherches += 1;
        c.recherche_somme += duree.as_secs_f64();
    }

    pub fn rendu(&self, i: &Instantane) -> String {
        let mut s = String::new();
        gauge(&mut s, "estompe_up", "1 si l'API répond", 1);
        gauge(
            &mut s,
            "estompe_uptime_seconds",
            "Durée depuis le démarrage",
            i.uptime,
        );
        gauge(
            &mut s,
            "estompe_database_up",
            "1 si PostgreSQL répond",
            u8::from(i.base_up),
        );
        if let Some(t) = &i.totaux {
            gauge(&mut s, "estompe_users", "Comptes Estompe", t.utilisateurs);
            gauge(
                &mut s,
                "estompe_messages",
                "Messages enregistrés",
                t.messages,
            );
            gauge(&mut s, "estompe_files", "Fichiers conservés", t.fichiers);
        }
        if let Some(st) = &i.stockage {
            gauge(
                &mut s,
                "estompe_storage_bytes",
                "Octets de fichiers stockés",
                st.utilise,
            );
            gauge(
                &mut s,
                "estompe_storage_max_bytes",
                "Plafond de stockage (0 = illimité)",
                st.plafond,
            );
        }
        gauge(
            &mut s,
            "estompe_ws_users",
            "Utilisateurs connectés en WebSocket",
            i.ws.0,
        );
        gauge(
            &mut s,
            "estompe_ws_connections",
            "Connexions WebSocket ouvertes",
            i.ws.1,
        );
        gauge(
            &mut s,
            "estompe_http_inflight",
            "Requêtes HTTP en cours",
            self.inflight.load(Ordering::SeqCst),
        );
        let c = self.compteurs.lock().unwrap_or_else(|e| e.into_inner());
        s.push_str("# HELP estompe_http_requests_total Requêtes HTTP traitées, par classe de code\n# TYPE estompe_http_requests_total counter\n");
        for (classe, n) in ["2xx", "3xx", "4xx", "5xx"].iter().zip(c.requetes) {
            let _ = writeln!(s, "estompe_http_requests_total{{code=\"{classe}\"}} {n}");
        }
        let _ = write!(
            s,
            "# HELP estompe_http_request_duration_seconds Durée des requêtes HTTP\n# TYPE estompe_http_request_duration_seconds summary\nestompe_http_request_duration_seconds_sum {}\nestompe_http_request_duration_seconds_count {}\n",
            go_g(c.duree_somme),
            c.duree_nombre
        );
        let _ = write!(
            s,
            "# HELP estompe_search_duration_seconds Durée des recherches de messages\n# TYPE estompe_search_duration_seconds summary\nestompe_search_duration_seconds_sum {}\nestompe_search_duration_seconds_count {}\n",
            go_g(c.recherche_somme),
            c.recherches
        );
        s
    }
}

#[cfg(test)]
mod tests {
    use super::{Instantane, Metrics, Stockage, Totaux, go_g};
    use std::time::Duration;

    #[test]
    fn go_g_comme_le_go() {
        for (x, attendu) in [
            (0.0, "0"),
            (1e-5, "1e-05"),
            (1.234e-5, "1.234e-05"),
            (0.0001, "0.0001"),
            (0.00012345, "0.00012345"),
            (0.5, "0.5"),
            (1.0, "1"),
            (100000.0, "100000"),
            (123456.0, "123456"),
            (999999.5, "999999.5"),
            (1e6, "1e+06"),
            (1234567.0, "1.234567e+06"),
            (1.5e10, "1.5e+10"),
            (0.000123456789, "0.000123456789"),
            (12.5, "12.5"),
            (3e-7, "3e-07"),
        ] {
            assert_eq!(go_g(x), attendu, "{x}");
        }
    }

    fn vide() -> Instantane {
        Instantane {
            uptime: 7,
            base_up: true,
            totaux: None,
            stockage: None,
            ws: (0, 0),
        }
    }

    #[test]
    fn rendu_compteurs_a_zero() {
        let attendu = "\
# HELP estompe_up 1 si l'API répond
# TYPE estompe_up gauge
estompe_up 1
# HELP estompe_uptime_seconds Durée depuis le démarrage
# TYPE estompe_uptime_seconds gauge
estompe_uptime_seconds 7
# HELP estompe_database_up 1 si PostgreSQL répond
# TYPE estompe_database_up gauge
estompe_database_up 1
# HELP estompe_ws_users Utilisateurs connectés en WebSocket
# TYPE estompe_ws_users gauge
estompe_ws_users 0
# HELP estompe_ws_connections Connexions WebSocket ouvertes
# TYPE estompe_ws_connections gauge
estompe_ws_connections 0
# HELP estompe_http_inflight Requêtes HTTP en cours
# TYPE estompe_http_inflight gauge
estompe_http_inflight 0
# HELP estompe_http_requests_total Requêtes HTTP traitées, par classe de code
# TYPE estompe_http_requests_total counter
estompe_http_requests_total{code=\"2xx\"} 0
estompe_http_requests_total{code=\"3xx\"} 0
estompe_http_requests_total{code=\"4xx\"} 0
estompe_http_requests_total{code=\"5xx\"} 0
# HELP estompe_http_request_duration_seconds Durée des requêtes HTTP
# TYPE estompe_http_request_duration_seconds summary
estompe_http_request_duration_seconds_sum 0
estompe_http_request_duration_seconds_count 0
# HELP estompe_search_duration_seconds Durée des recherches de messages
# TYPE estompe_search_duration_seconds summary
estompe_search_duration_seconds_sum 0
estompe_search_duration_seconds_count 0
";
        assert_eq!(Metrics::default().rendu(&vide()), attendu);
    }

    #[test]
    fn rendu_avec_base_et_stockage() {
        let r = Metrics::default().rendu(&Instantane {
            base_up: false,
            totaux: Some(Totaux {
                utilisateurs: 3,
                messages: 40,
                fichiers: 5,
            }),
            stockage: Some(Stockage {
                utilise: 1234,
                plafond: 0,
            }),
            ..vide()
        });
        for ligne in [
            "estompe_database_up 0\n",
            "estompe_users 3\n",
            "estompe_messages 40\n",
            "estompe_files 5\n",
            "estompe_storage_bytes 1234\n",
            "estompe_storage_max_bytes 0\n",
        ] {
            assert!(r.contains(ligne), "{ligne}");
        }
    }

    #[test]
    fn rendu_des_jauges_websocket() {
        let r = Metrics::default().rendu(&Instantane {
            ws: (2, 5),
            ..vide()
        });
        assert!(r.contains("estompe_ws_users 2\n"), "{r}");
        assert!(r.contains("estompe_ws_connections 5\n"), "{r}");
    }

    #[test]
    fn observer_classe_par_code_et_cumule_la_duree() {
        let m = Metrics::default();
        m.observer(200, Duration::from_millis(500));
        m.observer(204, Duration::from_millis(500));
        m.observer(404, Duration::from_micros(10));
        m.observer(503, Duration::ZERO);
        m.observer(302, Duration::ZERO);
        let r = m.rendu(&vide());
        for ligne in [
            "estompe_http_requests_total{code=\"2xx\"} 2\n",
            "estompe_http_requests_total{code=\"3xx\"} 1\n",
            "estompe_http_requests_total{code=\"4xx\"} 1\n",
            "estompe_http_requests_total{code=\"5xx\"} 1\n",
            "estompe_http_request_duration_seconds_sum 1.00001\n",
            "estompe_http_request_duration_seconds_count 5\n",
        ] {
            assert!(r.contains(ligne), "{ligne}\n{r}");
        }
    }

    #[test]
    fn searched_incremente() {
        let m = Metrics::default();
        m.searched(Duration::from_millis(250));
        m.searched(Duration::from_millis(250));
        let r = m.rendu(&vide());
        assert!(r.contains("estompe_search_duration_seconds_sum 0.5\n"));
        assert!(r.contains("estompe_search_duration_seconds_count 2\n"));
    }

    #[test]
    fn la_garde_inflight_decremente_au_drop() {
        let m = Metrics::default();
        let a = m.commencer();
        let b = m.commencer();
        assert!(m.rendu(&vide()).contains("estompe_http_inflight 2\n"));
        drop(a);
        assert!(m.rendu(&vide()).contains("estompe_http_inflight 1\n"));
        drop(b);
        assert!(m.rendu(&vide()).contains("estompe_http_inflight 0\n"));
    }
}
