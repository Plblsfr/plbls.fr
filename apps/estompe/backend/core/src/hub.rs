use std::{
    collections::HashMap,
    fmt::Write,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use rand::Rng;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{mpsc, watch};

use crate::{identity::Cred, state::Horloge};

pub const FILE_ENVOI: usize = 64;
pub const TICKET_TTL: Duration = Duration::from_secs(30);

#[derive(Serialize)]
pub struct Evenement<'a> {
    pub r#type: &'a str,
    pub data: &'a Value,
}

pub fn enveloppe(typ: &str, data: &Value) -> String {
    serde_json::to_string(&Evenement { r#type: typ, data }).unwrap_or_default()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Client {
    pub user_id: String,
    pub platform: String,
    pub version: String,
}

pub struct Conn {
    pub user_id: String,
    pub cred: Cred,
    client: Mutex<(String, String)>,
    tx: mpsc::Sender<String>,
    ferme: watch::Sender<bool>,
}

impl Conn {
    pub fn enqueue(&self, message: String) {
        if self.tx.try_send(message).is_err() {
            self.close();
        }
    }

    pub fn close(&self) {
        self.ferme.send_replace(true);
    }

    pub fn is_closed(&self) -> bool {
        *self.ferme.borrow()
    }

    pub async fn closed(&self) {
        let _ = self.ferme.subscribe().wait_for(|f| *f).await;
    }

    pub fn tag(&self, platform: &str, version: &str) {
        *verrou(&self.client) = (platform.to_string(), version.to_string());
    }
}

struct Ticket {
    user_id: String,
    cred: Cred,
    exp: Instant,
}

#[derive(Default)]
struct Etat {
    conns: HashMap<String, Vec<Arc<Conn>>>,
    acts: HashMap<String, String>,
}

pub struct Hub {
    etat: Mutex<Etat>,
    tickets: Mutex<HashMap<String, Ticket>>,
    now: Horloge,
}

fn verrou<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Default for Hub {
    fn default() -> Self {
        Self::with_now(Arc::new(Instant::now))
    }
}

impl Hub {
    pub fn with_now(now: Horloge) -> Self {
        Hub {
            etat: Mutex::default(),
            tickets: Mutex::default(),
            now,
        }
    }

    pub fn issue_ticket(&self, user_id: &str, cred: Cred) -> String {
        let mut octets = [0u8; 24];
        rand::rng().fill_bytes(&mut octets);
        let ticket = octets.iter().fold(String::with_capacity(48), |mut s, o| {
            let _ = write!(s, "{o:02x}");
            s
        });
        let maintenant = (self.now)();
        let mut tickets = verrou(&self.tickets);
        tickets.retain(|_, t| maintenant < t.exp);
        tickets.insert(
            ticket.clone(),
            Ticket {
                user_id: user_id.to_string(),
                cred,
                exp: maintenant + TICKET_TTL,
            },
        );
        ticket
    }

    pub fn redeem_ticket(&self, ticket: &str) -> Option<(String, Cred)> {
        let t = verrou(&self.tickets).remove(ticket)?;
        ((self.now)() < t.exp).then_some((t.user_id, t.cred))
    }

    pub fn register(&self, user_id: &str, cred: Cred) -> (Arc<Conn>, mpsc::Receiver<String>, bool) {
        let (tx, rx) = mpsc::channel(FILE_ENVOI);
        let conn = Arc::new(Conn {
            user_id: user_id.to_string(),
            cred,
            client: Mutex::default(),
            tx,
            ferme: watch::channel(false).0,
        });
        let mut etat = verrou(&self.etat);
        let liste = etat.conns.entry(user_id.to_string()).or_default();
        let premiere = liste.is_empty();
        liste.push(conn.clone());
        (conn, rx, premiere)
    }

    pub fn unregister(&self, conn: &Arc<Conn>) -> bool {
        conn.close();
        let mut etat = verrou(&self.etat);
        if let Some(liste) = etat.conns.get_mut(&conn.user_id) {
            liste.retain(|c| !Arc::ptr_eq(c, conn));
            if liste.is_empty() {
                etat.conns.remove(&conn.user_id);
                etat.acts.remove(&conn.user_id);
                return true;
            }
        }
        false
    }

    pub fn online(&self, user_id: &str) -> bool {
        verrou(&self.etat).conns.contains_key(user_id)
    }

    pub fn online_among(&self, ids: &[String]) -> Vec<String> {
        let etat = verrou(&self.etat);
        ids.iter()
            .filter(|id| etat.conns.contains_key(*id))
            .cloned()
            .collect()
    }

    pub fn set_activity(&self, user_id: &str, name: &str) -> bool {
        let mut etat = verrou(&self.etat);
        if !etat.conns.contains_key(user_id) {
            return false;
        }
        if name.is_empty() {
            etat.acts.remove(user_id);
        } else {
            etat.acts.insert(user_id.to_string(), name.to_string());
        }
        true
    }

    pub fn activities_among(&self, ids: &[String]) -> HashMap<String, String> {
        let etat = verrou(&self.etat);
        ids.iter()
            .filter_map(|id| etat.acts.get(id).map(|n| (id.clone(), n.clone())))
            .collect()
    }

    pub fn publish(&self, to: &[String], typ: &str, data: &Value) {
        let message = enveloppe(typ, data);
        let cibles: Vec<Arc<Conn>> = {
            let etat = verrou(&self.etat);
            to.iter()
                .filter_map(|id| etat.conns.get(id))
                .flatten()
                .cloned()
                .collect()
        };
        for c in cibles {
            c.enqueue(message.clone());
        }
    }

    pub fn close_all(&self) {
        let toutes: Vec<Arc<Conn>> = verrou(&self.etat)
            .conns
            .values()
            .flatten()
            .cloned()
            .collect();
        toutes.iter().for_each(|c| c.close());
    }

    pub fn kick(&self, user_id: &str) {
        let siennes = verrou(&self.etat).conns.get(user_id).cloned();
        siennes.iter().flatten().for_each(|c| c.close());
    }

    pub fn counts(&self) -> (usize, usize) {
        let etat = verrou(&self.etat);
        (etat.conns.len(), etat.conns.values().map(Vec::len).sum())
    }

    pub fn clients(&self) -> Vec<Client> {
        let etat = verrou(&self.etat);
        etat.conns
            .values()
            .flatten()
            .map(|c| {
                let (platform, version) = verrou(&c.client).clone();
                Client {
                    user_id: c.user_id.clone(),
                    platform,
                    version,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{FILE_ENVOI, Hub, TICKET_TTL};
    use crate::identity::Cred;
    use serde_json::{Value, json};
    use std::{
        sync::{Arc, Mutex},
        time::Instant,
    };

    fn cred() -> Cred {
        Cred {
            value: String::new(),
            cookie: false,
        }
    }

    #[test]
    fn publish_n_atteint_que_les_cibles_sur_toutes_leurs_connexions() {
        let h = Hub::default();
        let (_a, mut ra, premiere) = h.register("a", cred());
        assert!(premiere);
        let (_b, mut rb, _) = h.register("b", cred());
        let (_a2, mut ra2, premiere) = h.register("a", cred());
        assert!(!premiere);
        h.publish(&["a".into()], "x", &json!({ "n": 1 }));
        for r in [&mut ra, &mut ra2] {
            let m: Value = serde_json::from_str(&r.try_recv().unwrap()).unwrap();
            assert_eq!(m, json!({ "type": "x", "data": { "n": 1 } }));
        }
        assert!(rb.try_recv().is_err());
    }

    #[test]
    fn l_enveloppe_garde_l_ordre_type_puis_data() {
        assert_eq!(
            super::enveloppe("pong", &Value::Null),
            r#"{"type":"pong","data":null}"#
        );
    }

    #[test]
    fn un_client_lent_est_deconnecte_sans_bloquer() {
        let h = Hub::default();
        let (c, _rx, _) = h.register("a", cred());
        let (autre, _rx2, _) = h.register("b", cred());
        for i in 0..=FILE_ENVOI {
            h.publish(&["a".into()], "x", &json!(i));
        }
        assert!(c.is_closed());
        assert!(!autre.is_closed());
        h.publish(&["a".into()], "x", &json!(0));
    }

    #[tokio::test]
    async fn closed_se_resout_apres_close() {
        let h = Hub::default();
        let (c, _rx, _) = h.register("a", cred());
        let attente = tokio::spawn({
            let c = c.clone();
            async move { c.closed().await }
        });
        c.close();
        c.close();
        attente.await.unwrap();
        c.closed().await;
    }

    #[test]
    fn presence_premiere_et_derniere_seulement() {
        let h = Hub::default();
        let (c1, _r1, premiere) = h.register("a", cred());
        assert!(premiere);
        let (c2, _r2, premiere) = h.register("a", cred());
        assert!(!premiere);
        assert!(h.online("a"));
        assert!(!h.unregister(&c1));
        assert!(h.online("a"));
        assert!(h.unregister(&c2));
        assert!(!h.online("a"));
        assert!(h.online_among(&["a".into(), "b".into()]).is_empty());
        assert!(!h.unregister(&c2));
    }

    #[test]
    fn tickets_usage_unique_expiration_et_purge() {
        let maintenant = Arc::new(Mutex::new(Instant::now()));
        let h = Hub::with_now({
            let m = maintenant.clone();
            Arc::new(move || *m.lock().unwrap())
        });
        let t = h.issue_ticket(
            "u1",
            Cred {
                value: "v".into(),
                cookie: true,
            },
        );
        assert_eq!(t.len(), 48);
        assert!(t.bytes().all(|b| b.is_ascii_hexdigit()));
        let (id, c) = h.redeem_ticket(&t).unwrap();
        assert_eq!((id.as_str(), c.value.as_str(), c.cookie), ("u1", "v", true));
        assert!(h.redeem_ticket(&t).is_none());

        let expire = h.issue_ticket("u1", cred());
        let purge = h.issue_ticket("u2", cred());
        *maintenant.lock().unwrap() += TICKET_TTL;
        assert!(h.redeem_ticket(&expire).is_none());
        let _ = h.issue_ticket("u3", cred());
        assert!(!h.tickets.lock().unwrap().contains_key(&purge));
        assert!(h.redeem_ticket("inconnu").is_none());
    }

    #[test]
    fn activite_effacee_a_la_derniere_deconnexion() {
        let h = Hub::default();
        assert!(!h.set_activity("a", "Jeu"));
        let (c, _rx, _) = h.register("a", cred());
        assert!(h.set_activity("a", "Jeu"));
        let ids = ["a".to_string(), "b".to_string()];
        assert_eq!(
            h.activities_among(&ids),
            [("a".to_string(), "Jeu".to_string())].into()
        );
        assert!(h.set_activity("a", ""));
        assert!(h.activities_among(&ids).is_empty());
        h.set_activity("a", "Jeu");
        h.unregister(&c);
        assert!(h.activities_among(&ids).is_empty());
    }

    #[test]
    fn counts_clients_kick_et_close_all() {
        let h = Hub::default();
        let (a1, _r1, _) = h.register("a", cred());
        let (a2, _r2, _) = h.register("a", cred());
        let (b, _r3, _) = h.register("b", cred());
        a1.tag("web", "1.2.3");
        assert_eq!(h.counts(), (2, 3));
        let mut clients = h.clients();
        clients.sort_by(|x, y| (&x.user_id, &x.platform).cmp(&(&y.user_id, &y.platform)));
        assert_eq!(clients.len(), 3);
        assert_eq!(
            (clients[1].platform.as_str(), clients[1].version.as_str()),
            ("web", "1.2.3")
        );
        h.kick("a");
        assert!(a1.is_closed() && a2.is_closed() && !b.is_closed());
        h.close_all();
        assert!(b.is_closed());
    }
}
