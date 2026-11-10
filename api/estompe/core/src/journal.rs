use std::{
    fmt,
    time::{Duration, Instant},
};

use axum::{
    body::{Body, to_bytes},
    extract::Request,
    http::{HeaderMap, StatusCode, Version, header},
    middleware::Next,
    response::Response,
};
use reqwest::{RequestBuilder, Url};
use serde::{Serialize, Serializer};
use serde_json::{Map, Value, json};

pub const MASQUE: &str = "**MASKED**";
pub const CIBLE: &str = "journal";
const LIMITE_CORPS: usize = 16 * 1024;
const NON_JOURNALISE: &str = "<non journalisé>";

pub struct Sensible<T>(pub T);

impl<T> fmt::Debug for Sensible<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(MASQUE)
    }
}

impl<T> fmt::Display for Sensible<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(MASQUE)
    }
}

impl<T> Serialize for Sensible<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(MASQUE)
    }
}

pub fn masquer<T, S: Serializer>(_: &T, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(MASQUE)
}

pub fn cle_sensible(nom: &str) -> bool {
    const MOTIFS: [&str; 14] = [
        "authorization",
        "cookie",
        "token",
        "secret",
        "password",
        "passwd",
        "mot_de_passe",
        "motdepasse",
        "credential",
        "jwt",
        "signature",
        "apikey",
        "api_key",
        "api-key",
    ];
    let n = nom.to_ascii_lowercase();
    n == "key"
        || n.ends_with("-key")
        || n.ends_with("_key")
        || n.starts_with("key_")
        || n.starts_with("key-")
        || MOTIFS.iter().any(|m| n.contains(m))
}

pub fn entetes(entetes: &HeaderMap) -> Value {
    let mut sortie = Map::new();
    for (nom, valeur) in entetes {
        let texte = if cle_sensible(nom.as_str()) {
            MASQUE.to_string()
        } else {
            String::from_utf8_lossy(valeur.as_bytes()).into_owned()
        };
        match sortie.get_mut(nom.as_str()) {
            Some(Value::String(existant)) => {
                existant.push_str(", ");
                existant.push_str(&texte);
            }
            _ => {
                sortie.insert(nom.as_str().to_string(), Value::String(texte));
            }
        }
    }
    Value::Object(sortie)
}

pub fn json_masque(valeur: Value) -> Value {
    match valeur {
        Value::Object(m) => Value::Object(
            m.into_iter()
                .map(|(k, v)| {
                    let v = if cle_sensible(&k) {
                        Value::String(MASQUE.into())
                    } else {
                        json_masque(v)
                    };
                    (k, v)
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.into_iter().map(json_masque).collect()),
        autre => autre,
    }
}

fn segment_secret(segment: &str) -> bool {
    let uuid = segment.len() == 36 && segment.bytes().filter(|b| *b == b'-').count() == 4;
    segment.len() >= 32 && !uuid
}

pub fn chemin(chemin: &str) -> String {
    chemin
        .split('/')
        .map(|s| if segment_secret(s) { MASQUE } else { s })
        .collect::<Vec<_>>()
        .join("/")
}

fn paires_masquees<'a>(paires: impl Iterator<Item = (&'a str, &'a str)>) -> Value {
    let mut sortie = Map::new();
    for (k, v) in paires {
        let valeur = if cle_sensible(k) || segment_secret(v) {
            MASQUE.to_string()
        } else {
            v.to_string()
        };
        sortie.insert(k.to_string(), Value::String(valeur));
    }
    Value::Object(sortie)
}

fn requete_de(url: &Url) -> Value {
    let paires: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    paires_masquees(paires.iter().map(|(k, v)| (k.as_str(), v.as_str())))
}

fn type_contenu(entetes: &HeaderMap) -> String {
    entetes
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn est_texte(type_contenu: &str) -> bool {
    type_contenu.contains("json")
        || type_contenu.starts_with("text/")
        || type_contenu.contains("xml")
        || type_contenu.contains("mpegurl")
        || type_contenu.contains("x-www-form-urlencoded")
}

pub fn corps(octets: &[u8], type_contenu: &str) -> Value {
    if octets.is_empty() {
        return Value::Null;
    }
    if !type_contenu.is_empty() && !est_texte(type_contenu) {
        return Value::String(format!("<{} octets>", octets.len()));
    }
    if type_contenu.contains("x-www-form-urlencoded") {
        let texte = String::from_utf8_lossy(octets);
        let paires: Vec<(String, String)> = texte
            .split('&')
            .filter_map(|p| p.split_once('=').or(Some((p, ""))))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        return paires_masquees(paires.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    }
    if let Ok(v) = serde_json::from_slice::<Value>(octets) {
        return json_masque(v);
    }
    let texte = String::from_utf8_lossy(octets);
    let mut coupe: String = texte.chars().take(2048).collect();
    if coupe.len() < texte.len() {
        coupe.push('…');
    }
    Value::String(coupe)
}

fn niveau(statut: Option<u16>, erreur: bool) -> tracing::Level {
    match statut {
        None if erreur => tracing::Level::ERROR,
        Some(s) if s >= 500 => tracing::Level::ERROR,
        Some(s) if s >= 400 => tracing::Level::WARN,
        _ => tracing::Level::INFO,
    }
}

fn emettre(entree: Value, statut: Option<u16>) {
    let erreur = entree.get("error").is_some_and(|e| !e.is_null());
    let message = format!(
        "{} {}",
        entree["request_type"].as_str().unwrap_or_default(),
        entree["method"].as_str().unwrap_or_default()
    );
    let donnees = entree.to_string();
    match niveau(statut, erreur) {
        tracing::Level::ERROR => {
            tracing::error!(target: CIBLE, donnees = %donnees, "{message}")
        }
        tracing::Level::WARN => tracing::warn!(target: CIBLE, donnees = %donnees, "{message}"),
        _ => tracing::info!(target: CIBLE, donnees = %donnees, "{message}"),
    }
}

fn millisecondes(d: Duration) -> f64 {
    (d.as_secs_f64() * 1000.0 * 100.0).round() / 100.0
}

pub struct Echange<'a> {
    pub request_type: &'a str,
    pub service: Option<&'a str>,
    pub methode: &'a str,
    pub chemin: String,
    pub requete: Value,
    pub request_data: Value,
    pub header_data: Value,
    pub statut: Option<u16>,
    pub response_header_data: Value,
    pub response_data: Value,
    pub duree: Duration,
    pub erreur: Option<String>,
}

impl Echange<'_> {
    pub fn en_json(&self) -> Value {
        json!({
            "request_type": self.request_type,
            "service": self.service,
            "method": self.methode,
            "path": self.chemin,
            "query": self.requete,
            "request_data": self.request_data,
            "header_data": { "request": self.header_data, "response": self.response_header_data },
            "response_status": self.statut,
            "response_data": self.response_data,
            "duration_ms": millisecondes(self.duree),
            "error": self.erreur,
        })
    }

    pub fn emettre(&self) {
        emettre(self.en_json(), self.statut);
    }
}

fn message_erreur(e: &reqwest::Error) -> String {
    let texte = e.to_string();
    match e.url() {
        Some(u) => texte.replace(u.as_str(), "<url>"),
        None => texte,
    }
}

async fn lire_borne(mut r: reqwest::Response, max: usize) -> (Vec<u8>, Option<reqwest::Error>) {
    let mut sortie = Vec::new();
    loop {
        match r.chunk().await {
            Ok(Some(m)) => {
                sortie.extend_from_slice(&m);
                if sortie.len() > max {
                    return (sortie, None);
                }
            }
            Ok(None) => return (sortie, None),
            Err(e) => return (sortie, Some(e)),
        }
    }
}

fn reconstruire(
    statut: StatusCode,
    version: Version,
    entetes: HeaderMap,
    octets: Vec<u8>,
) -> reqwest::Response {
    let mut r = axum::http::Response::new(octets);
    *r.status_mut() = statut;
    *r.version_mut() = version;
    *r.headers_mut() = entetes;
    reqwest::Response::from(r)
}

pub async fn sortant(
    service: &str,
    requete: RequestBuilder,
) -> Result<reqwest::Response, reqwest::Error> {
    let (client, construite) = requete.build_split();
    let req = match construite {
        Ok(r) => r,
        Err(e) => {
            Echange {
                request_type: "output",
                service: Some(service),
                methode: "",
                chemin: String::new(),
                requete: Value::Null,
                request_data: Value::Null,
                header_data: Value::Null,
                statut: None,
                response_header_data: Value::Null,
                response_data: Value::Null,
                duree: Duration::ZERO,
                erreur: Some(message_erreur(&e)),
            }
            .emettre();
            return Err(e);
        }
    };
    let methode = req.method().to_string();
    let url = req.url().clone();
    let en_tetes = entetes(req.headers());
    let donnees = req
        .body()
        .and_then(|b| b.as_bytes())
        .map(|b| corps(b, &type_contenu(req.headers())))
        .unwrap_or(Value::Null);
    let debut = Instant::now();
    let resultat = client.execute(req).await;
    let duree = debut.elapsed();
    let mut echange = Echange {
        request_type: "output",
        service: Some(service),
        methode: &methode,
        chemin: format!(
            "{}://{}{}",
            url.scheme(),
            url.host_str().unwrap_or_default(),
            chemin(url.path())
        ),
        requete: requete_de(&url),
        request_data: donnees,
        header_data: en_tetes,
        statut: None,
        response_header_data: Value::Null,
        response_data: Value::Null,
        duree,
        erreur: None,
    };
    let reponse = match resultat {
        Ok(r) => r,
        Err(e) => {
            echange.erreur = Some(message_erreur(&e));
            echange.emettre();
            return Err(e);
        }
    };
    let statut = reponse.status();
    echange.statut = Some(statut.as_u16());
    echange.response_header_data = entetes(reponse.headers());
    let contenu = type_contenu(reponse.headers());
    let longueur = reponse.content_length();
    let erreur_http = statut.is_client_error() || statut.is_server_error();
    let tamponner = if erreur_http {
        longueur.is_none_or(|l| l <= LIMITE_CORPS as u64)
    } else {
        est_texte(&contenu) && longueur.is_some_and(|l| l <= LIMITE_CORPS as u64)
    };
    if !tamponner {
        echange.response_data = Value::String(NON_JOURNALISE.into());
        if erreur_http {
            echange.erreur = Some(format!("HTTP {}", statut.as_u16()));
        }
        echange.emettre();
        return Ok(reponse);
    }
    let version = reponse.version();
    let entetes_reponse = reponse.headers().clone();
    let (octets, echec) = lire_borne(reponse, LIMITE_CORPS).await;
    echange.response_data = corps(&octets, &contenu);
    echange.duree = debut.elapsed();
    if let Some(e) = echec {
        echange.erreur = Some(message_erreur(&e));
        echange.emettre();
        return Err(e);
    }
    if erreur_http {
        echange.erreur = Some(format!("HTTP {}", statut.as_u16()));
    }
    echange.emettre();
    Ok(reconstruire(statut, version, entetes_reponse, octets))
}

pub async fn entrant(req: Request, next: Next) -> Response {
    let chemin_brut = req.uri().path();
    if chemin_brut.starts_with("/ws") || chemin_brut == "/metrics" || chemin_brut == "/health" {
        return next.run(req).await;
    }
    let debut = Instant::now();
    let (parties, corps_requete) = req.into_parts();
    let methode = parties.method.to_string();
    let chemin_log = chemin(&percent_decoder(parties.uri.path()));
    let requete = parties
        .uri
        .query()
        .map(|q| {
            let paires: Vec<(String, String)> = q
                .split('&')
                .filter_map(|p| p.split_once('=').or(Some((p, ""))))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            paires_masquees(paires.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        })
        .unwrap_or(Value::Null);
    let en_tetes = entetes(&parties.headers);
    let contenu_requete = type_contenu(&parties.headers);
    let longueur: Option<usize> = parties
        .headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let (corps_requete, request_data) = match longueur {
        Some(0) => (corps_requete, Value::Null),
        None if corps_requete_vide(&parties.headers) => (corps_requete, Value::Null),
        Some(l) if l <= LIMITE_CORPS && est_texte(&contenu_requete) => {
            match to_bytes(corps_requete, LIMITE_CORPS).await {
                Ok(o) => {
                    let v = corps(&o, &contenu_requete);
                    (Body::from(o), v)
                }
                Err(_) => (Body::empty(), Value::String("<corps illisible>".into())),
            }
        }
        _ => (corps_requete, Value::String(NON_JOURNALISE.into())),
    };
    let reponse = next.run(Request::from_parts(parties, corps_requete)).await;
    let statut = reponse.status();
    let contenu_reponse = type_contenu(reponse.headers());
    let response_headers = entetes(reponse.headers());
    let (reponse, response_data) = {
        let (p, b) = reponse.into_parts();
        let petite = b
            .size_hint_exact()
            .is_some_and(|n| n <= LIMITE_CORPS as u64);
        if petite && est_texte(&contenu_reponse) {
            match to_bytes(b, LIMITE_CORPS).await {
                Ok(o) => {
                    let v = corps(&o, &contenu_reponse);
                    (Response::from_parts(p, Body::from(o)), v)
                }
                Err(_) => (
                    Response::from_parts(p, Body::empty()),
                    Value::String("<corps illisible>".into()),
                ),
            }
        } else {
            (
                Response::from_parts(p, b),
                Value::String(NON_JOURNALISE.into()),
            )
        }
    };
    let erreur = (statut.is_client_error() || statut.is_server_error())
        .then(|| format!("HTTP {}", statut.as_u16()));
    Echange {
        request_type: "input",
        service: None,
        methode: &methode,
        chemin: chemin_log,
        requete,
        request_data,
        header_data: en_tetes,
        statut: Some(statut.as_u16()),
        response_header_data: response_headers,
        response_data,
        duree: debut.elapsed(),
        erreur,
    }
    .emettre();
    reponse
}

fn corps_requete_vide(entetes: &HeaderMap) -> bool {
    !entetes.contains_key(header::CONTENT_LENGTH)
        && !entetes.contains_key(header::TRANSFER_ENCODING)
}

fn percent_decoder(chemin: &str) -> String {
    percent_encoding::percent_decode_str(chemin)
        .decode_utf8_lossy()
        .into_owned()
}

trait TailleExacte {
    fn size_hint_exact(&self) -> Option<u64>;
}

impl TailleExacte for Body {
    fn size_hint_exact(&self) -> Option<u64> {
        axum::body::HttpBody::size_hint(self).exact()
    }
}

#[cfg(test)]
mod tests {
    use axum::{Router, routing::post};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn sensible_ne_montre_jamais_la_valeur() {
        let s = Sensible("hunter2".to_string());
        assert_eq!(format!("{s}"), MASQUE);
        assert_eq!(format!("{s:?}"), MASQUE);
        assert_eq!(serde_json::to_string(&s).unwrap(), format!("\"{MASQUE}\""));
    }

    #[test]
    fn annotation_serde_masque_le_champ() {
        #[derive(Serialize)]
        struct Connexion {
            nom: String,
            #[serde(serialize_with = "masquer")]
            mot_de_passe: String,
        }
        let v = serde_json::to_value(Connexion {
            nom: "alice".into(),
            mot_de_passe: "hunter2".into(),
        })
        .unwrap();
        assert_eq!(v, json!({"nom": "alice", "mot_de_passe": MASQUE}));
    }

    #[test]
    fn entetes_sensibles_masques() {
        let mut h = HeaderMap::new();
        h.insert("authorization", "Bearer abc".parse().unwrap());
        h.insert("x-service-token", "cle".parse().unwrap());
        h.insert("cookie", "a=b".parse().unwrap());
        h.insert("accept", "application/json".parse().unwrap());
        let v = entetes(&h);
        assert_eq!(v["authorization"], MASQUE);
        assert_eq!(v["x-service-token"], MASQUE);
        assert_eq!(v["cookie"], MASQUE);
        assert_eq!(v["accept"], "application/json");
    }

    #[test]
    fn corps_json_masque_en_profondeur() {
        let v = corps(
            br#"{"user":"a","password":"p","nested":[{"api_key":"k","ok":1}]}"#,
            "application/json",
        );
        assert_eq!(v["user"], "a");
        assert_eq!(v["password"], MASQUE);
        assert_eq!(v["nested"][0]["api_key"], MASQUE);
        assert_eq!(v["nested"][0]["ok"], 1);
    }

    #[test]
    fn formulaire_et_chemin_masques() {
        let v = corps(b"login=a&password=p", "application/x-www-form-urlencoded");
        assert_eq!(v["login"], "a");
        assert_eq!(v["password"], MASQUE);
        let jeton = "a".repeat(20) + "." + &"b".repeat(30);
        assert_eq!(
            chemin(&format!("/flux/{jeton}/hls")),
            format!("/flux/{MASQUE}/hls")
        );
        assert_eq!(
            chemin("/servers/6f1d3c52-8a4b-4e0f-9c11-2f7a5d1b9e00"),
            "/servers/6f1d3c52-8a4b-4e0f-9c11-2f7a5d1b9e00"
        );
    }

    #[test]
    fn echange_contient_les_champs_demandes() {
        let e = Echange {
            request_type: "output",
            service: Some("entracte"),
            methode: "GET",
            chemin: "http://e/internal/titles".into(),
            requete: Value::Null,
            request_data: Value::Null,
            header_data: json!({"x-service-token": MASQUE}),
            statut: Some(401),
            response_header_data: Value::Null,
            response_data: Value::String("Unauthorized".into()),
            duree: Duration::from_millis(12),
            erreur: Some("HTTP 401".into()),
        }
        .en_json();
        for champ in [
            "request_type",
            "request_data",
            "header_data",
            "response_data",
            "duration_ms",
            "error",
        ] {
            assert!(e.get(champ).is_some(), "{champ}");
        }
        assert_eq!(e["duration_ms"], 12.0);
    }

    async fn faux_serveur(reponse: &'static str) -> String {
        let ecoute = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let adresse = ecoute.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = ecoute.accept().await.unwrap();
            let mut tampon = [0u8; 2048];
            let _ = s.read(&mut tampon).await;
            s.write_all(reponse.as_bytes()).await.unwrap();
        });
        format!("http://{adresse}/Items")
    }

    #[tokio::test]
    async fn sortant_rend_la_reponse_intacte_meme_en_401() {
        let url = faux_serveur(
            "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: 18\r\nconnection: close\r\n\r\n{\"erreur\":\"jeton\"}",
        )
        .await;
        let r = sortant(
            "test",
            reqwest::Client::new()
                .get(url)
                .header("X-Service-Token", "s"),
        )
        .await
        .unwrap();
        assert_eq!(r.status(), 401);
        assert_eq!(r.text().await.unwrap(), "{\"erreur\":\"jeton\"}");
    }

    #[tokio::test]
    async fn sortant_propage_l_erreur_de_connexion() {
        let r = sortant("test", reqwest::Client::new().get("http://127.0.0.1:1/x")).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn entrant_laisse_passer_corps_et_reponse() {
        let app = Router::new()
            .route("/echo", post(|corps: String| async move { corps }))
            .layer(axum::middleware::from_fn(entrant));
        let r = app
            .oneshot(
                Request::post("/echo")
                    .header("content-type", "application/json")
                    .header("content-length", "17")
                    .body(Body::from(r#"{"password":"p1"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let o = to_bytes(r.into_body(), 1024).await.unwrap();
        assert_eq!(&o[..], br#"{"password":"p1"}"#);
    }
}
