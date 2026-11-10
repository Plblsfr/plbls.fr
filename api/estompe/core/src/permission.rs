use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::{Value, json};

use crate::{
    catalogue,
    http::{champ, lire_corps, premier_objet},
};

const CACHE_TTL: Duration = Duration::from_secs(30);
const CACHE_MAX: usize = 1000;
const JOKER: &str = "*";

pub const ADMIN_PERMISSION: &str = "permission:admin";
pub const COMPTES: &str = "permission:users.read";
const ANCIEN_ADMIN: &str = "estompe:admin";

#[derive(Debug, PartialEq, Eq)]
pub struct Indisponible;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Acces {
    pub allow: BTreeSet<String>,
    pub deny: BTreeSet<String>,
}

impl Acces {
    fn joker() -> Self {
        Acces {
            allow: BTreeSet::from([JOKER.to_string()]),
            deny: BTreeSet::new(),
        }
    }

    pub fn autorise(&self, ressource: &str) -> bool {
        if self.deny.contains(JOKER) || self.deny.contains(ressource) {
            return false;
        }
        self.allow.contains(JOKER) || self.allow.contains(ressource)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Droits(Arc<BTreeMap<String, Acces>>);

impl Droits {
    pub fn de<I: IntoIterator<Item = S>, S: Into<String>>(noms: I) -> Self {
        Droits(Arc::new(
            noms.into_iter()
                .map(|n| (n.into(), Acces::joker()))
                .collect(),
        ))
    }

    pub fn a(&self, nom: &str) -> bool {
        self.a_sur(nom, JOKER)
    }

    pub fn quelque_part(&self, nom: &str) -> bool {
        self.0
            .get(nom)
            .is_some_and(|a| !a.deny.contains(JOKER) && !a.allow.is_empty())
    }

    pub fn a_sur(&self, nom: &str, ressource: &str) -> bool {
        self.0.get(nom).is_some_and(|a| a.autorise(ressource))
    }

    pub fn noms(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }

    pub fn json(&self, prefixe: &str) -> Value {
        Value::Object(
            self.0
                .iter()
                .filter(|(k, _)| k.starts_with(prefixe))
                .map(|(k, a)| (k.clone(), json!({"allow": a.allow, "deny": a.deny})))
                .collect(),
        )
    }

    pub fn depuis_permission(noms: &[String], acces: &str) -> Self {
        let mut m: BTreeMap<String, Acces> =
            noms.iter().map(|n| (n.clone(), Acces::joker())).collect();
        for e in catalogue::ouvertes() {
            m.insert(e.nom(), Acces::joker());
        }
        let a = |n: &str| noms.iter().any(|x| x == n);
        if a(acces) {
            m.insert(catalogue::ACCES.into(), Acces::joker());
        }
        if a(ANCIEN_ADMIN) {
            for e in catalogue::CATALOGUE.iter().filter(|e| e.admin()) {
                m.insert(e.nom(), Acces::joker());
            }
        }
        Droits(Arc::new(m))
    }

    fn depuis_aegis(permissions: &serde_json::Map<String, Value>) -> Result<Self, Indisponible> {
        let ensemble = |v: Option<&Value>| -> Result<BTreeSet<String>, Indisponible> {
            match v {
                None | Some(Value::Null) => Ok(BTreeSet::new()),
                Some(Value::Array(a)) => a
                    .iter()
                    .map(|x| x.as_str().map(str::to_string).ok_or(Indisponible))
                    .collect(),
                Some(_) => Err(Indisponible),
            }
        };
        let mut m = BTreeMap::new();
        for (nom, v) in permissions {
            let o = v.as_object().ok_or(Indisponible)?;
            m.insert(
                nom.clone(),
                Acces {
                    allow: ensemble(o.get("allow"))?,
                    deny: ensemble(o.get("deny"))?,
                },
            );
        }
        Ok(Droits(Arc::new(m)))
    }
}

type Horloge = Arc<dyn Fn() -> Instant + Send + Sync>;

#[derive(Clone)]
enum Source {
    Permission {
        url: String,
        token: String,
        acces: String,
    },
    Aegis {
        url: String,
        token: String,
    },
}

struct Inner {
    source: Source,
    client: reqwest::Client,
    now: Horloge,
    cache: Mutex<HashMap<String, (Droits, Instant)>>,
    ttl: Mutex<Duration>,
}

#[derive(Clone)]
pub struct Client(Arc<Inner>);

pub struct Profil<'a> {
    pub email: &'a str,
    pub nom: &'a str,
}

impl Client {
    pub fn new(base_url: &str, token: &str) -> Self {
        Self::with_now(base_url, token, Arc::new(Instant::now))
    }

    pub fn with_now(base_url: &str, token: &str, now: Horloge) -> Self {
        Self::construire(
            Source::Permission {
                url: base_url.trim_end_matches('/').to_string(),
                token: token.to_string(),
                acces: catalogue::ACCES.to_string(),
            },
            now,
        )
    }

    pub fn permission(base_url: &str, token: &str, acces: &str, now: Horloge) -> Self {
        Self::construire(
            Source::Permission {
                url: base_url.trim_end_matches('/').to_string(),
                token: token.to_string(),
                acces: acces.to_string(),
            },
            now,
        )
    }

    pub fn aegis(base_url: &str, token: &str, now: Horloge) -> Self {
        Self::construire(
            Source::Aegis {
                url: base_url.trim_end_matches('/').to_string(),
                token: token.to_string(),
            },
            now,
        )
    }

    fn construire(source: Source, now: Horloge) -> Self {
        Client(Arc::new(Inner {
            source,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("client HTTP"),
            now,
            cache: Mutex::new(HashMap::new()),
            ttl: Mutex::new(CACHE_TTL),
        }))
    }

    pub fn aegis_actif(&self) -> bool {
        matches!(self.0.source, Source::Aegis { .. })
    }

    pub fn invalider(&self, sub: &str) {
        self.0.cache.lock().unwrap().remove(sub);
    }

    pub fn vider(&self) {
        self.0.cache.lock().unwrap().clear();
    }

    pub fn ttl(&self, ttl: Duration) {
        *self.0.ttl.lock().unwrap() = ttl;
    }

    pub async fn droits(&self, sub: &str) -> Result<Droits, Indisponible> {
        self.droits_de(sub, None).await
    }

    pub async fn droits_de(
        &self,
        sub: &str,
        profil: Option<Profil<'_>>,
    ) -> Result<Droits, Indisponible> {
        let inner = &self.0;
        if let Some((d, exp)) = inner.cache.lock().unwrap().get(sub)
            && (inner.now)() < *exp
        {
            return Ok(d.clone());
        }
        let (droits, validite) = match &inner.source {
            Source::Permission { url, token, acces } => {
                (self.lire_permission(url, token, acces, sub).await?, None)
            }
            Source::Aegis { url, token } => self.lire_aegis(url, token, sub, profil).await?,
        };
        let ttl = *inner.ttl.lock().unwrap();
        let ttl = validite.map_or(ttl, |v: Duration| v.min(ttl));
        let mut c = inner.cache.lock().unwrap();
        if c.len() >= CACHE_MAX {
            c.clear();
        }
        c.insert(sub.to_string(), (droits.clone(), (inner.now)() + ttl));
        Ok(droits)
    }

    async fn lire_permission(
        &self,
        url: &str,
        token: &str,
        acces: &str,
        sub: &str,
    ) -> Result<Droits, Indisponible> {
        let reponse = self
            .0
            .client
            .get(format!(
                "{url}/v1/users/{}/permissions",
                utf8_percent_encode(sub, NON_ALPHANUMERIC)
            ))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| Indisponible)?;
        if reponse.status() != reqwest::StatusCode::OK {
            return Err(Indisponible);
        }
        let corps = lire_corps(reponse).await.ok_or(Indisponible)?;
        let objet = premier_objet(&corps).ok_or(Indisponible)?;
        let noms = match champ(&objet, "permissions") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(noms)) => noms
                .iter()
                .map(|n| n.as_str().map(str::to_string).ok_or(Indisponible))
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => return Err(Indisponible),
        };
        Ok(Droits::depuis_permission(&noms, acces))
    }

    async fn lire_aegis(
        &self,
        url: &str,
        token: &str,
        sub: &str,
        profil: Option<Profil<'_>>,
    ) -> Result<(Droits, Option<Duration>), Indisponible> {
        let mut corps = json!({"sub": sub});
        if let Some(p) = profil {
            if !p.email.is_empty() {
                corps["email"] = json!(p.email);
            }
            if !p.nom.is_empty() {
                corps["display_name"] = json!(p.nom);
            }
        }
        let reponse = self
            .0
            .client
            .post(format!("{url}/v1/effective"))
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(corps.to_string())
            .send()
            .await
            .map_err(|_| Indisponible)?;
        if reponse.status() != reqwest::StatusCode::OK {
            return Err(Indisponible);
        }
        let octets = lire_corps(reponse).await.ok_or(Indisponible)?;
        let objet = premier_objet(&octets).ok_or(Indisponible)?;
        let droits = match champ(&objet, "permissions") {
            None | Some(Value::Null) => Droits::default(),
            Some(Value::Object(m)) => Droits::depuis_aegis(m)?,
            Some(_) => return Err(Indisponible),
        };
        let validite = match champ(&objet, "valid_until") {
            Some(Value::String(s)) => chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| {
                (d.with_timezone(&chrono::Utc) - chrono::Utc::now())
                    .to_std()
                    .unwrap_or_default()
            }),
            _ => None,
        };
        Ok((droits, validite))
    }

    pub async fn envoyer_manifeste(&self, manifeste: &Value) -> Result<Value, Indisponible> {
        let Source::Aegis { url, token } = &self.0.source else {
            return Err(Indisponible);
        };
        let reponse = self
            .0
            .client
            .put(format!("{url}/v1/manifest"))
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(manifeste.to_string())
            .send()
            .await
            .map_err(|_| Indisponible)?;
        if !reponse.status().is_success() {
            tracing::warn!(statut = %reponse.status(), "manifeste refusé par aegis");
            return Err(Indisponible);
        }
        let octets = lire_corps(reponse).await.ok_or(Indisponible)?;
        premier_objet(&octets)
            .map(Value::Object)
            .ok_or(Indisponible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::CORPS_MAX;
    use crate::testutil::{faux, faux_permission};

    #[tokio::test]
    async fn lit_les_droits_du_compte_et_met_en_cache() {
        let f = faux_permission(|sub, p| sub == "alice" && p != ANCIEN_ADMIN).await;
        let t = Arc::new(Mutex::new(Instant::now()));
        let t2 = t.clone();
        let c = Client::with_now(&f.url, "jeton", Arc::new(move || *t2.lock().unwrap()));
        let d = c.droits("alice").await.unwrap();
        assert!(d.a("estompe:access") && !d.a("estompe:admin.view"));
        assert!(
            d.a("estompe:gifs.use"),
            "ouverte par défaut en mode permission"
        );
        c.droits("alice").await.unwrap();
        assert_eq!(f.appels(), 1);
        let r = &f.requetes()[0];
        assert_eq!(
            (r.methode.as_str(), r.chemin.as_str()),
            ("GET", "/v1/users/alice/permissions")
        );
        assert_eq!(r.entete("authorization"), "Bearer jeton");
        assert!(!c.droits("bob").await.unwrap().a("estompe:access"));
        *t.lock().unwrap() += Duration::from_secs(60);
        c.droits("alice").await.unwrap();
        assert_eq!(f.appels(), 3);
        c.invalider("alice");
        c.droits("alice").await.unwrap();
        assert_eq!(f.appels(), 4);
    }

    #[tokio::test]
    async fn ancien_admin_donne_toute_l_administration() {
        let f = faux(|_| (200, br#"{"permissions":["estompe:admin"]}"#.to_vec())).await;
        let d = Client::new(&f.url, "x").droits("a").await.unwrap();
        assert!(
            d.a("estompe:admin.users") && d.a(catalogue::ADMIN_FICHIERS) && !d.a("estompe:access")
        );
    }

    #[tokio::test]
    async fn sub_encode_dans_le_chemin() {
        let f = faux(|_| (200, br#"{"permissions":[]}"#.to_vec())).await;
        Client::new(&f.url, "x").droits("a/b c").await.unwrap();
        assert_eq!(f.requetes()[0].chemin, "/v1/users/a%2Fb%20c/permissions");
    }

    #[tokio::test]
    async fn plafond_du_cache() {
        let f = faux(|_| (200, br#"{"permissions":["a:b"]}"#.to_vec())).await;
        let c = Client::new(&f.url, "tok");
        for i in 0..CACHE_MAX {
            c.droits(&i.to_string()).await.unwrap();
        }
        assert_eq!(c.0.cache.lock().unwrap().len(), CACHE_MAX);
        c.droits("un de plus").await.unwrap();
        assert_eq!(c.0.cache.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn echoue_ferme() {
        for (statut, corps) in [
            (401, ""),
            (500, r#"{"permissions":["estompe:access"]}"#),
            (200, "pas du json"),
            (200, r#"{"permissions":"estompe:access"}"#),
            (200, r#"{"permissions":["estompe:access",1]}"#),
        ] {
            let f = faux(move |_| (statut, corps.as_bytes().to_vec())).await;
            assert_eq!(
                Client::new(&f.url, "x").droits("a").await,
                Err(Indisponible),
                "{statut} {corps}"
            );
        }
        let gros = format!(r#"{{"permissions":["{}"]}}"#, "a".repeat(CORPS_MAX));
        let f = faux(move |_| (200, gros.clone().into_bytes())).await;
        assert_eq!(
            Client::new(&f.url, "x").droits("a").await,
            Err(Indisponible)
        );
        let f = faux(|_| (200, br#"{"Permissions":["x:y"]}"#.to_vec())).await;
        assert!(Client::new(&f.url, "x").droits("a").await.unwrap().a("x:y"));
        let f = faux(|_| (200, vec![])).await;
        let url = f.url.clone();
        drop(f);
        assert_eq!(Client::new(&url, "x").droits("a").await, Err(Indisponible));
    }

    #[tokio::test]
    async fn aegis_ressources_refus_et_validite() {
        let f = faux(|r| {
            assert_eq!(
                (r.methode.as_str(), r.chemin.as_str()),
                ("POST", "/v1/effective")
            );
            let corps: Value = serde_json::from_slice(&r.corps).unwrap();
            assert_eq!(corps["sub"], "alice");
            let fin = (chrono::Utc::now() + chrono::Duration::seconds(5)).to_rfc3339();
            (
                200,
                json!({"sub": "alice", "generation": 1, "valid_until": fin, "permissions": {
                    "estompe:access": {"allow": ["*"], "deny": []},
                    "estompe:vocal.screen": {"allow": ["*"], "deny": ["srv-1"]},
                    "aegis:admin": {"allow": ["*"], "deny": []}
                }})
                .to_string()
                .into_bytes(),
            )
        })
        .await;
        let t = Arc::new(Mutex::new(Instant::now()));
        let t2 = t.clone();
        let c = Client::aegis(&f.url, "jeton", Arc::new(move || *t2.lock().unwrap()));
        let d = c
            .droits_de(
                "alice",
                Some(Profil {
                    email: "alice@plbls.fr",
                    nom: "",
                }),
            )
            .await
            .unwrap();
        assert!(d.a("estompe:access") && d.a("aegis:admin"));
        assert!(
            d.a_sur("estompe:vocal.screen", "srv-2") && !d.a_sur("estompe:vocal.screen", "srv-1")
        );
        assert!(
            !d.a("estompe:gifs.use"),
            "pas d'ouverture implicite en mode aegis"
        );
        assert_eq!(f.requetes()[0].entete("authorization"), "Bearer jeton");
        let corps: Value = serde_json::from_slice(&f.requetes()[0].corps).unwrap();
        assert_eq!(corps["email"], "alice@plbls.fr");
        *t.lock().unwrap() += Duration::from_secs(6);
        c.droits("alice").await.unwrap();
        assert_eq!(f.appels(), 2, "valid_until borne le cache");
        let f = faux(|_| (200, br#"{"permissions":{"a:b":{"allow":"*"}}}"#.to_vec())).await;
        assert_eq!(
            Client::aegis(&f.url, "x", Arc::new(Instant::now))
                .droits("a")
                .await,
            Err(Indisponible)
        );
    }
}
