use std::collections::HashMap;
use std::time::{Duration, Instant};

use axum::{
    extract::{RawQuery, State},
    http::StatusCode,
    response::Response,
    routing::get,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

use crate::{
    api::ActiveUser,
    gestion_roles::premier_parametre,
    http::{write_err, write_json},
    messages::{
        Conv, Ligne, avec_reessai, construire, conv_du_dm, conv_du_salon, entier_requete, hydrater,
        selection,
    },
    openapi,
    roles::{self, VIEW_CHANNEL},
    servers::{Params, bd, param, pool},
    state::AppState,
    users::User,
};

pub const MAX_RECHERCHE: usize = 200;
const FENETRE_CLASSEMENT: i64 = 2000;
const RECHERCHES: usize = 30;
const FENETRE_RECHERCHE: Duration = Duration::from_secs(60);
const JOUR: i64 = 86_400;
const TYPES_A: &str = " fichier image video lien sondage ";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Saisie {
    pub(crate) texte: String,
    pub(crate) de: String,
    pub(crate) dans: String,
    pub(crate) serveur: String,
    pub(crate) mentionne: String,
    pub(crate) avant: Option<i64>,
    pub(crate) apres: Option<i64>,
    pub(crate) a: Vec<String>,
    pub(crate) epingle: bool,
}

fn minuscules_go(s: &str) -> String {
    s.chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect()
}

fn fold(s: &str) -> String {
    minuscules_go(s)
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'à' | 'â' => 'a',
            'î' | 'ï' => 'i',
            'ô' => 'o',
            'û' | 'ù' | 'ü' => 'u',
            'ç' => 'c',
            autre => autre,
        })
        .collect()
}

fn decouper(q: &str) -> Vec<String> {
    let mut sortie = Vec::new();
    let mut courant = String::new();
    let mut guillemets = false;
    for c in q.chars() {
        if c == '"' {
            guillemets = !guillemets;
            courant.push(c);
        } else if c.is_whitespace() && !guillemets {
            if !courant.is_empty() {
                sortie.push(std::mem::take(&mut courant));
            }
        } else {
            courant.push(c);
        }
    }
    if !courant.is_empty() {
        sortie.push(courant);
    }
    sortie
}

fn invalide(detail: &str) -> String {
    format!("recherche invalide : {detail}")
}

fn jours_depuis_epoque(annee: i64, mois: i64, jour: i64) -> i64 {
    let a = if mois <= 2 { annee - 1 } else { annee };
    let ere = a.div_euclid(400);
    let annee_ere = a - ere * 400;
    let m = if mois > 2 { mois - 3 } else { mois + 9 };
    let jour_annee = (153 * m + 2) / 5 + jour - 1;
    let jour_ere = annee_ere * 365 + annee_ere / 4 - annee_ere / 100 + jour_annee;
    ere * 146_097 + jour_ere - 719_468
}

fn jour(s: &str) -> Result<i64, String> {
    let erreur = || invalide(&format!("date « {s} » (AAAA-MM-JJ attendu)"));
    let b = s.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return Err(erreur());
    }
    let nombre = |a: usize, z: usize| s[a..z].parse::<i64>().unwrap_or(0);
    let (annee, mois, j) = (nombre(0, 4), nombre(5, 7), nombre(8, 10));
    let bissextile = annee % 4 == 0 && (annee % 100 != 0 || annee % 400 == 0);
    let dans_le_mois = match mois {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if bissextile => 29,
        2 => 28,
        _ => return Err(erreur()),
    };
    if j < 1 || j > dans_le_mois {
        return Err(erreur());
    }
    Ok(jours_depuis_epoque(annee, mois, j) * JOUR)
}

enum Champ {
    De,
    Dans,
    Serveur,
    Mentionne,
    Avant,
    Apres,
    Pendant,
    A,
    Epingle,
}

fn cle(nom: &str) -> Option<Champ> {
    Some(match nom {
        "de" => Champ::De,
        "dans" => Champ::Dans,
        "serveur" => Champ::Serveur,
        "mentionne" => Champ::Mentionne,
        "avant" => Champ::Avant,
        "apres" => Champ::Apres,
        "pendant" => Champ::Pendant,
        "a" => Champ::A,
        "epingle" => Champ::Epingle,
        _ => return None,
    })
}

pub(crate) fn analyser(q: &str) -> Result<Saisie, String> {
    let mut s = Saisie::default();
    if q.chars().count() > MAX_RECHERCHE {
        return Err(invalide(&format!("{MAX_RECHERCHE} caractères au plus")));
    }
    let mut texte: Vec<String> = Vec::new();
    for t in decouper(q) {
        let (nom, valeur, present) = match t.split_once(':') {
            Some((n, v)) => (n, v, true),
            None => (t.as_str(), "", false),
        };
        let champ = cle(&fold(nom));
        let valeur = valeur.trim_matches('"');
        let (true, Some(champ), false) = (present, champ, valeur.is_empty()) else {
            texte.push(t);
            continue;
        };
        match champ {
            Champ::De => s.de = minuscules_go(valeur.strip_prefix('@').unwrap_or(valeur)),
            Champ::Mentionne => {
                s.mentionne = minuscules_go(valeur.strip_prefix('@').unwrap_or(valeur))
            }
            Champ::Dans => s.dans = minuscules_go(valeur.strip_prefix('#').unwrap_or(valeur)),
            Champ::Serveur => s.serveur = valeur.to_string(),
            Champ::A => {
                let h = fold(valeur);
                if !TYPES_A.contains(&format!(" {h} ")) {
                    return Err(invalide(&format!(
                        "a:{valeur} (fichier, image, vidéo, lien ou sondage)"
                    )));
                }
                s.a.push(h);
            }
            Champ::Epingle => s.epingle = fold(valeur) == "oui",
            Champ::Avant => s.avant = Some(jour(valeur)?),
            Champ::Apres => s.apres = Some(jour(valeur)? + JOUR),
            Champ::Pendant => {
                let d = jour(valeur)?;
                s.apres = Some(d);
                s.avant = Some(d + JOUR);
            }
        }
    }
    s.texte = texte.join(" ");
    let n = s.texte.chars().count();
    if n > 0 && n < 2 {
        return Err(invalide("au moins 2 caractères"));
    }
    if s.texte.is_empty()
        && s.de.is_empty()
        && s.dans.is_empty()
        && s.serveur.is_empty()
        && s.mentionne.is_empty()
        && s.avant.is_none()
        && s.apres.is_none()
        && s.a.is_empty()
        && !s.epingle
    {
        return Err(invalide("saisie vide"));
    }
    Ok(s)
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Portee {
    pub(crate) viewer: String,
    pub(crate) saisie: Saisie,
    pub(crate) server_id: String,
    pub(crate) channel_id: String,
    pub(crate) dm_id: String,
    pub(crate) par_date: bool,
    pub(crate) avant: i64,
    pub(crate) decalage: i64,
    pub(crate) limite: i64,
}

enum Arg {
    Texte(String),
    Entier(i64),
    Liste(Vec<String>),
}

struct Parametres {
    args: Vec<Arg>,
}

impl Parametres {
    fn arg(&mut self, a: Arg) -> String {
        self.args.push(a);
        format!("${}", self.args.len())
    }

    fn texte(&mut self, t: &str) -> String {
        self.arg(Arg::Texte(t.to_string()))
    }

    fn mp(&mut self, viewer: &str) -> String {
        let v = format!("{}::uuid", self.texte(viewer));
        format!("m.dm_id IN (SELECT id FROM dm_conversations WHERE user_a={v} OR user_b={v})")
    }
}

pub(crate) async fn salons_visibles(
    pool: &PgPool,
    user: &str,
    server_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let lignes = sqlx::query_as::<_, (String, String, String)>(
        "SELECT c.server_id::text, c.id::text, c.kind FROM channels c
         JOIN server_members m ON m.server_id = c.server_id
         WHERE m.user_id=$1::uuid AND ($2 = '' OR c.server_id::text = $2)",
    )
    .bind(user)
    .bind(server_id)
    .fetch_all(pool)
    .await?;
    let mut acces: HashMap<String, Option<roles::Access>> = HashMap::new();
    let mut sortie = Vec::new();
    for (serveur, id, kind) in lignes {
        if !["text", "videos", "music"].contains(&kind.as_str()) {
            continue;
        }
        if !acces.contains_key(&serveur) {
            let a = roles::access_for(pool, &serveur, user).await?;
            acces.insert(serveur.clone(), a);
        }
        if let Some(Some(a)) = acces.get(&serveur)
            && roles::contient(a.channel(&id), VIEW_CHANNEL)
        {
            sortie.push(id);
        }
    }
    Ok(sortie)
}

pub(crate) async fn chercher(pool: &PgPool, p: &Portee) -> Result<(Vec<Value>, bool), sqlx::Error> {
    let mut r = Parametres { args: Vec::new() };
    let mut cond: Vec<String> = Vec::new();
    if !p.dm_id.is_empty() {
        let mp = r.mp(&p.viewer);
        let dm = r.texte(&p.dm_id);
        cond.push(format!("{mp} AND m.dm_id::text={dm}"));
    } else {
        let mut salons = salons_visibles(pool, &p.viewer, &p.server_id).await?;
        if !p.channel_id.is_empty() {
            salons.retain(|s| *s == p.channel_id);
        }
        let liste = r.arg(Arg::Liste(salons));
        let mut vis = format!("m.channel_id = ANY({liste}::uuid[])");
        if p.server_id.is_empty() && p.channel_id.is_empty() {
            let mp = r.mp(&p.viewer);
            vis = format!("({vis} OR {mp})");
        }
        cond.push(vis);
    }
    let q = &p.saisie;
    let mut rang = "0".to_string();
    if !q.texte.is_empty() {
        let tq = format!("websearch_to_tsquery('fr', {})", r.texte(&q.texte));
        cond.push(format!(
            "(m.tsv @@ {tq} OR EXISTS (SELECT 1 FROM files tf WHERE tf.message_id = m.id AND tf.transcript <> '' AND tf.transcript_tsv @@ {tq}))"
        ));
        rang = format!("ts_rank_cd(m.tsv, {tq})");
    }
    if !q.de.is_empty() {
        let a = r.texte(&q.de);
        cond.push(format!(
            "m.author_id IN (SELECT id FROM users WHERE lower(pseudo)={a})"
        ));
    }
    if !q.mentionne.is_empty() {
        let a = r.texte(&q.mentionne);
        cond.push(format!(
            "EXISTS (SELECT 1 FROM message_mentions mm JOIN users mu ON mu.id = mm.user_id WHERE mm.message_id = m.id AND lower(mu.pseudo)={a})"
        ));
    }
    if !q.dans.is_empty() {
        let a = r.texte(&q.dans);
        cond.push(format!(
            "m.channel_id IN (SELECT id FROM channels WHERE unaccent(lower(name))=unaccent({a}))"
        ));
    }
    if !q.serveur.is_empty() {
        let a = r.texte(&q.serveur);
        cond.push(format!(
            "m.channel_id IN (SELECT c.id FROM channels c JOIN servers sv ON sv.id = c.server_id WHERE position(unaccent(lower({a})) IN unaccent(lower(sv.name))) > 0)"
        ));
    }
    if let Some(d) = q.apres {
        let a = r.arg(Arg::Entier(d));
        cond.push(format!("m.created_at >= to_timestamp({a})"));
    }
    if let Some(d) = q.avant {
        let a = r.arg(Arg::Entier(d));
        cond.push(format!("m.created_at < to_timestamp({a})"));
    }
    for h in &q.a {
        match h.as_str() {
            "fichier" => {
                cond.push("EXISTS (SELECT 1 FROM files f WHERE f.message_id = m.id)".to_string())
            }
            "image" | "video" => {
                let a = r.texte(&format!("{h}/%"));
                cond.push(format!(
                    "EXISTS (SELECT 1 FROM files f WHERE f.message_id = m.id AND f.mime LIKE {a})"
                ));
            }
            "lien" => cond.push("m.content ~* 'https?://'".to_string()),
            "sondage" => {
                cond.push("EXISTS (SELECT 1 FROM polls po WHERE po.message_id = m.id)".to_string());
            }
            _ => {}
        }
    }
    if q.epingle {
        cond.push("m.pinned_at IS NOT NULL".to_string());
    }
    if p.avant > 0 {
        let a = r.arg(Arg::Entier(p.avant));
        cond.push(format!("m.id < {a}"));
    }
    let mut cond = cond.join(" AND ");
    let mut ordre = "m.id DESC".to_string();
    if !q.texte.is_empty() && !p.par_date {
        cond = format!(
            "m.id IN (SELECT m.id FROM messages m WHERE {cond} ORDER BY m.id DESC LIMIT {FENETRE_CLASSEMENT})"
        );
        ordre = format!("{rang} DESC, m.id DESC");
    }
    let limite = r.arg(Arg::Entier(p.limite + 1));
    let decalage = r.arg(Arg::Entier(p.decalage));
    let sql = format!(
        "{} WHERE {cond} ORDER BY {ordre} LIMIT {limite} OFFSET {decalage}",
        selection()
    );
    let mut requete = sqlx::query_as::<_, Ligne>(sqlx::AssertSqlSafe(sql));
    for a in r.args {
        requete = match a {
            Arg::Texte(t) => requete.bind(t),
            Arg::Entier(n) => requete.bind(n),
            Arg::Liste(l) => requete.bind(l),
        };
    }
    let lignes = requete.fetch_all(pool).await?;
    let mut messages = construire(lignes);
    let plus = messages.len() as i64 > p.limite;
    if plus {
        messages.truncate(p.limite as usize);
    }
    hydrater(pool, &mut messages, &p.viewer).await?;
    Ok((messages.into_iter().map(Value::Object).collect(), plus))
}

type Doc = (
    HttpMethod,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const DOC: &[Doc] = &[
    (
        HttpMethod::Get,
        "/search",
        "Rechercher dans les messages visibles (salons vus, messages privés)",
        &[
            ("200", "ok — {messages, has_more}"),
            ("400", "`invalid_query` ou paramètre invalide"),
            ("429", "`too_many_requests` : 30 recherches / minute"),
        ],
    ),
    (
        HttpMethod::Get,
        "/channels/{cid}/search",
        "Rechercher dans un salon",
        &[
            ("200", "ok — {messages, has_more}"),
            (
                "400",
                "`not_text_channel`, `invalid_query` ou paramètre invalide",
            ),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            ("429", "`too_many_requests` : 30 recherches / minute"),
        ],
    ),
    (
        HttpMethod::Get,
        "/dms/{id}/search",
        "Rechercher dans une conversation privée",
        &[
            ("200", "ok — {messages, has_more}"),
            ("400", "`invalid_query` ou paramètre invalide"),
            ("404", "Introuvable, ou non visible par l'utilisateur"),
            ("429", "`too_many_requests` : 30 recherches / minute"),
        ],
    ),
];

pub fn router() -> OpenApiRouter<AppState> {
    let mut routeur = OpenApiRouter::new()
        .route("/search", get(recherche_globale))
        .route("/channels/{cid}/search", get(recherche_salon))
        .route("/dms/{id}/search", get(recherche_dm));
    for (methode, chemin, resume, reponses) in DOC {
        let op = openapi::operation("messages", resume, reponses, chemin);
        routeur = openapi::documenter(routeur, chemin, methode.clone(), op);
    }
    routeur
}

async fn rechercher(
    state: &AppState,
    u: &User,
    mut portee: Portee,
    requete: Option<String>,
) -> Result<Response, Response> {
    let requete = requete.unwrap_or_default();
    portee.saisie = analyser(premier_parametre(&requete, "q").trim())
        .map_err(|e| write_err(StatusCode::BAD_REQUEST, "invalid_query", &e))?;
    portee.viewer = u.id.clone();
    portee.par_date = premier_parametre(&requete, "sort") != "relevance";
    portee.avant = entier_requete(&requete, "before", "before invalide", |n| n >= 0)?.unwrap_or(0);
    portee.decalage = entier_requete(&requete, "offset", "offset : 0 à 1000", |n| {
        (0..=1000).contains(&n)
    })?
    .unwrap_or(0);
    portee.limite = entier_requete(&requete, "limit", "limit : 1 à 50", |n| {
        (1..=50).contains(&n)
    })?
    .unwrap_or(25);
    if !state
        .limiteurs()
        .recherche
        .allow(&u.id, RECHERCHES, FENETRE_RECHERCHE)
    {
        return Err(avec_reessai(
            write_err(
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_requests",
                "trop de recherches",
            ),
            60,
        ));
    }
    let pool = pool(state)?;
    let debut = Instant::now();
    let resultat = chercher(pool, &portee).await;
    state.metrics().searched(debut.elapsed());
    let (messages, plus) = resultat.map_err(bd)?;
    Ok(write_json(
        StatusCode::OK,
        &json!({ "messages": messages, "has_more": plus }),
    ))
}

async fn recherche_globale(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    RawQuery(requete): RawQuery,
) -> Result<Response, Response> {
    let brut = requete.clone().unwrap_or_default();
    let portee = Portee {
        server_id: premier_parametre(&brut, "server_id"),
        channel_id: premier_parametre(&brut, "channel_id"),
        dm_id: premier_parametre(&brut, "dm_id"),
        ..Portee::default()
    };
    rechercher(&state, &u, portee, requete).await
}

async fn recherche_salon(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    RawQuery(requete): RawQuery,
) -> Result<Response, Response> {
    let conv: Conv = conv_du_salon(pool(&state)?, param(&p.0, "cid"), &u.id).await?;
    let portee = Portee {
        channel_id: conv.channel_id.unwrap_or_default(),
        ..Portee::default()
    };
    rechercher(&state, &u, portee, requete).await
}

async fn recherche_dm(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    p: Params,
    RawQuery(requete): RawQuery,
) -> Result<Response, Response> {
    let conv = conv_du_dm(pool(&state)?, param(&p.0, "id"), &u.id).await?;
    let portee = Portee {
        dm_id: conv.dm_id.unwrap_or_default(),
        ..Portee::default()
    };
    rechercher(&state, &u, portee, requete).await
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
    use tower::ServiceExt;

    use super::*;
    use crate::messages::tests::{Cadre, cadre};

    fn chemin(base: &str, q: &str, reste: &str) -> String {
        format!(
            "{base}?q={}{reste}",
            utf8_percent_encode(q, NON_ALPHANUMERIC)
        )
    }

    async fn ids(c: &Cadre, qui: &str, base: &str, q: &str, reste: &str) -> Vec<String> {
        let (s, v) = c
            .api(qui, "GET", &chemin(base, q, reste), Value::Null)
            .await;
        assert_eq!(s, 200, "{q} : {v}");
        v["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["id"].as_str().unwrap().to_string())
            .collect()
    }

    fn id(m: &Value) -> String {
        m["id"].as_str().unwrap().to_string()
    }

    #[test]
    fn analyse_de_la_saisie() {
        let s = analyser(
            r#"bon "jour à tous" -nuit DE:@Alice dans:#général a:vidéo a:Image épinglé:oui après:2026-03-01 truc:machin"#,
        )
        .unwrap();
        assert_eq!(s.texte, r#"bon "jour à tous" -nuit truc:machin"#);
        assert_eq!((s.de.as_str(), s.dans.as_str()), ("alice", "général"));
        assert!(s.epingle);
        assert_eq!(s.a, ["video", "image"]);
        assert_eq!(s.apres, Some(jour("2026-03-02").unwrap()));
        assert_eq!(s.avant, None);
        assert_eq!(jour("2026-03-02").unwrap(), 1_772_409_600);

        let s = analyser(r#"serveur:"Mon serveur" pendant:2026-03-01"#).unwrap();
        assert_eq!(s.texte, "");
        assert_eq!(s.serveur, "Mon serveur");
        assert_eq!(s.apres, Some(jour("2026-03-01").unwrap()));
        assert_eq!(s.avant, Some(jour("2026-03-02").unwrap()));

        let s = analyser("Épinglé:OUI APRÈS:2026-03-01").unwrap();
        assert!(s.epingle && s.apres.is_some());

        let s = analyser("truc:machin de: ab").unwrap();
        assert_eq!(s.texte, "truc:machin de: ab");
        assert!(s.de.is_empty());

        assert_eq!(analyser(r#"a:"image video""#).unwrap().a, ["image video"]);
        assert_eq!(analyser("de:İİ").unwrap().de, "ii");
        assert_eq!(jour("0000-01-01").unwrap(), -62_167_219_200);
        assert_eq!(jour("1970-01-01").unwrap(), 0);
        assert_eq!(jour("2024-02-29").unwrap(), 1_709_164_800);
        assert_eq!(
            analyser("avant:9999-12-31").unwrap().avant,
            Some(253_402_214_400)
        );
        assert_eq!(
            analyser("après:9999-12-31").unwrap().apres,
            Some(253_402_300_800)
        );

        let erreurs = [
            (
                "avant:demain",
                "recherche invalide : date « demain » (AAAA-MM-JJ attendu)",
            ),
            (
                "avant:2026-02-30",
                "recherche invalide : date « 2026-02-30 » (AAAA-MM-JJ attendu)",
            ),
            (
                "avant:2026-1-05",
                "recherche invalide : date « 2026-1-05 » (AAAA-MM-JJ attendu)",
            ),
            (
                "avant:2026-01-05x",
                "recherche invalide : date « 2026-01-05x » (AAAA-MM-JJ attendu)",
            ),
            (
                "avant:2025-02-29",
                "recherche invalide : date « 2025-02-29 » (AAAA-MM-JJ attendu)",
            ),
            (
                "a:truc",
                "recherche invalide : a:truc (fichier, image, vidéo, lien ou sondage)",
            ),
            ("a", "recherche invalide : au moins 2 caractères"),
            ("", "recherche invalide : saisie vide"),
            ("   ", "recherche invalide : saisie vide"),
        ];
        for (saisie, message) in erreurs {
            assert_eq!(analyser(saisie).unwrap_err(), message, "{saisie}");
        }
        let long = "é".repeat(201);
        assert_eq!(
            analyser(&long).unwrap_err(),
            "recherche invalide : 200 caractères au plus"
        );
        assert!(analyser(&"é".repeat(200)).is_ok());
    }

    #[tokio::test]
    async fn recherche_globale_filtres() {
        let Some(c) = cadre().await else { return };
        let delai = c
            .poster("alice", "La réunion est décalée, voir https://exemple.fr")
            .await;
        let autre = c.poster("bob", "Pas de réunion demain").await;
        let photo = c.poster("alice", "voici la photo").await;
        let fichier = c.fichier("alice", "file").await;
        c.sql(&format!(
            "UPDATE files SET message_id={}, mime='image/png', transcript='enregistrement vocal important' WHERE id='{fichier}'",
            id(&photo)
        ))
        .await;
        let sondage = c
            .api(
                "alice",
                "POST",
                &format!("/channels/{}/messages", c.salon),
                json!({ "content": "vote du jour" }),
            )
            .await
            .1;
        c.sql(&format!(
            "INSERT INTO polls (message_id, created_by) VALUES ({}, '{}')",
            id(&sondage),
            c.alice
        ))
        .await;
        let dm = c.ouvrir_dm("alice", "bob").await;
        let (s, secret) = c
            .api(
                "alice",
                "POST",
                &format!("/dms/{dm}/messages"),
                json!({ "content": "réunion secrète" }),
            )
            .await;
        assert_eq!(s, 201);
        assert_eq!(
            c.api(
                "alice",
                "PUT",
                &format!("/messages/{}/pin", id(&autre)),
                Value::Null
            )
            .await
            .0,
            204
        );

        let tous = |q: &'static str| {
            let c = &c;
            async move { ids(c, "bob", "/search", q, "").await }
        };
        assert_eq!(
            tous("REUNIONS").await,
            [id(&secret), id(&autre), id(&delai)]
        );
        assert_eq!(tous(r#""réunion demain""#).await, [id(&autre)]);
        assert_eq!(tous("réunion -demain -secrète").await, [id(&delai)]);
        assert_eq!(tous("réunion de:bob").await, [id(&autre)]);
        assert_eq!(tous("réunion de:@BOB").await, [id(&autre)]);
        assert_eq!(tous("épinglé:oui").await, [id(&autre)]);
        assert_eq!(tous("a:lien").await, [id(&delai)]);
        assert_eq!(tous("a:image").await, [id(&photo)]);
        assert_eq!(tous("a:fichier a:image").await, [id(&photo)]);
        assert_eq!(tous("a:sondage").await, [id(&sondage)]);
        assert!(tous("a:vidéo").await.is_empty());
        assert_eq!(tous("vocal").await, [id(&photo)]);
        assert!(tous("réunion dans:#inconnu").await.is_empty());
        assert_eq!(tous("photo serveur:chat").await, [id(&photo)]);
        assert!(tous("photo serveur:zzz").await.is_empty());
        assert!(tous("photo avant:2001-01-01").await.is_empty());
        assert_eq!(tous("photo après:2001-01-01").await, [id(&photo)]);
        assert!(tous("photo après:9999-12-31").await.is_empty());
        c.sql("UPDATE messages SET created_at='2020-05-05 12:00:00+00'")
            .await;
        assert_eq!(tous("photo pendant:2020-05-05").await, [id(&photo)]);
        assert!(tous("photo pendant:2020-05-06").await.is_empty());
        assert_eq!(tous("photo avant:2020-05-06").await, [id(&photo)]);

        let salon = format!("&channel_id={}", c.salon);
        assert_eq!(
            ids(&c, "bob", "/search", "réunion", &salon).await,
            [id(&autre), id(&delai)]
        );
        let serveur = format!("&server_id={}", c.sid);
        assert_eq!(
            ids(&c, "bob", "/search", "réunion", &serveur).await,
            [id(&autre), id(&delai)]
        );
        let mp = format!("&dm_id={dm}");
        assert_eq!(
            ids(&c, "bob", "/search", "réunion", &mp).await,
            [id(&secret)]
        );
        assert!(ids(&c, "admin", "/search", "réunion", &mp).await.is_empty());
        assert!(
            ids(&c, "admin", "/search", "réunion", &salon)
                .await
                .is_empty()
        );
        assert_eq!(
            ids(&c, "alice", "/search", "réunion", "&sort=relevance&limit=1")
                .await
                .len(),
            1
        );
        assert_eq!(
            ids(&c, "bob", "/search", "réunion", "&limit=2").await,
            [id(&secret), id(&autre)]
        );
        assert_eq!(
            ids(&c, "bob", "/search", "réunion", "&limit=2&offset=2").await,
            [id(&delai)]
        );
        let (_, p) = c
            .api(
                "bob",
                "GET",
                &chemin("/search", "réunion", "&limit=2"),
                Value::Null,
            )
            .await;
        assert_eq!(p["has_more"], true);
        let (_, p) = c
            .api(
                "bob",
                "GET",
                &chemin("/search", "réunion", "&limit=3"),
                Value::Null,
            )
            .await;
        assert_eq!(p["has_more"], false);
        let avant = format!("&before={}", id(&autre));
        assert_eq!(
            ids(&c, "bob", "/search", "réunion", &avant).await,
            [id(&delai)]
        );
        let (_, vide) = c
            .api(
                "bob",
                "GET",
                &chemin("/search", "introuvable", ""),
                Value::Null,
            )
            .await;
        assert_eq!(vide, json!({ "messages": [], "has_more": false }));

        let tous = |q: &'static str| {
            let c = &c;
            async move { ids(c, "alice", "/search", q, "").await }
        };
        let mention = c.poster("alice", "salut @bob on se voit").await;
        assert_eq!(tous("mentionne:bob").await, [id(&mention)]);

        let (s, _) = c
            .api(
                "alice",
                "PATCH",
                &format!("/messages/{}", id(&delai)),
                json!({ "content": "Le rendez-vous est annulé" }),
            )
            .await;
        assert_eq!(s, 200);
        assert!(tous("décalée").await.is_empty());
        assert_eq!(tous("annulé").await, [id(&delai)]);
        assert_eq!(
            c.api(
                "alice",
                "DELETE",
                &format!("/messages/{}", id(&delai)),
                Value::Null
            )
            .await
            .0,
            204
        );
        assert!(tous("annulé").await.is_empty());

        let longue = "x".repeat(201);
        for mauvais in [
            "/search?q=".to_string(),
            "/search?q=a".to_string(),
            "/search?q=avant%3Ademain".to_string(),
            "/search?q=a%3Atruc".to_string(),
            format!("/search?q={longue}"),
            "/search?q=ab&limit=0".to_string(),
            "/search?q=ab&limit=51".to_string(),
            "/search?q=ab&offset=-1".to_string(),
            "/search?q=ab&offset=1001".to_string(),
            "/search?q=ab&before=x".to_string(),
        ] {
            let (s, v) = c.api("bob", "GET", &mauvais, Value::Null).await;
            assert_eq!(s, 400, "{mauvais}");
            assert!(
                v["error"] == "invalid_query" || v["error"] == "bad_request",
                "{v}"
            );
        }
        let (_, v) = c.api("bob", "GET", "/search?q=a", Value::Null).await;
        assert_eq!(v["message"], "recherche invalide : au moins 2 caractères");
        let r = Request::builder()
            .uri("/search?q=ab")
            .body(Body::empty())
            .unwrap();
        let rep = c.e.app.clone().oneshot(r).await.unwrap();
        assert_eq!(rep.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn recherche_suit_les_droits() {
        let Some(c) = cadre().await else { return };
        let (s, secret) = c
            .api(
                "alice",
                "POST",
                &format!("/servers/{}/channels", c.sid),
                json!({ "name": "secret", "kind": "text" }),
            )
            .await;
        assert_eq!(s, 201, "{secret}");
        let secret = secret["id"].as_str().unwrap().to_string();
        let (s, m) = c
            .api(
                "alice",
                "POST",
                &format!("/channels/{secret}/messages"),
                json!({ "content": "plan confidentiel" }),
            )
            .await;
        assert_eq!(s, 201);
        let touche = id(&m);
        for kind in ["videos", "music"] {
            let (s, v) = c
                .api(
                    "alice",
                    "POST",
                    &format!("/servers/{}/channels", c.sid),
                    json!({ "name": kind, "kind": kind }),
                )
                .await;
            assert_eq!(s, 201, "{v}");
            let (s, _) = c
                .api(
                    "alice",
                    "POST",
                    &format!("/channels/{}/messages", v["id"].as_str().unwrap()),
                    json!({ "content": format!("contenu {kind}") }),
                )
                .await;
            assert_eq!(s, 201);
        }
        assert_eq!(ids(&c, "bob", "/search", "contenu", "").await.len(), 2);
        assert_eq!(
            ids(&c, "bob", "/search", "confidentiel", "").await,
            std::slice::from_ref(&touche)
        );
        let defaut: String = sqlx::query_scalar(
            "SELECT id::text FROM roles WHERE server_id=$1::uuid AND is_default",
        )
        .bind(&c.sid)
        .fetch_one(c.pool())
        .await
        .unwrap();
        let (s, _) = c
            .api(
                "alice",
                "PUT",
                &format!("/servers/{}/channels/{secret}/overrides/{defaut}", c.sid),
                json!({ "deny": ["view_channel"] }),
            )
            .await;
        assert_eq!(s, 204);
        assert!(
            ids(&c, "bob", "/search", "confidentiel", "")
                .await
                .is_empty()
        );
        let cible = format!("&channel_id={secret}");
        assert!(
            ids(&c, "bob", "/search", "confidentiel", &cible)
                .await
                .is_empty()
        );
        assert_eq!(
            ids(&c, "alice", "/search", "confidentiel", "").await,
            [touche]
        );
        let (s, _) = c
            .api(
                "bob",
                "GET",
                &chemin(&format!("/channels/{secret}/search"), "confidentiel", ""),
                Value::Null,
            )
            .await;
        assert_eq!(s, 404);
        let (s, v) = c
            .api(
                "bob",
                "GET",
                &chemin(&format!("/channels/{}/search", c.vocal), "ab", ""),
                Value::Null,
            )
            .await;
        assert_eq!((s, v["error"].as_str().unwrap()), (400, "not_text_channel"));
        let dm = c.ouvrir_dm("alice", "bob").await;
        let (s, _) = c
            .api(
                "admin",
                "GET",
                &chemin(&format!("/dms/{dm}/search"), "ab", ""),
                Value::Null,
            )
            .await;
        assert_eq!(s, 404);
        let rendez = c.poster("alice", "Le rendez-vous est à 20h").await;
        c.poster("alice", "100% sûr, c'est_un test").await;
        let recherche_salon = format!("/channels/{}/search", c.salon);
        assert_eq!(
            ids(&c, "bob", &recherche_salon, "RENDEZ", "").await,
            [id(&rendez)]
        );
        assert_eq!(ids(&c, "bob", &recherche_salon, "100%", "").await.len(), 1);
        assert_eq!(ids(&c, "bob", &recherche_salon, "TEST", "").await.len(), 1);
        let (s, _) = c
            .api(
                "alice",
                "POST",
                &format!("/dms/{dm}/messages"),
                json!({ "content": "privé" }),
            )
            .await;
        assert_eq!(s, 201);
        assert_eq!(
            ids(&c, "alice", &format!("/dms/{dm}/search"), "privé", "")
                .await
                .len(),
            1
        );
        let (s, _) = c
            .api(
                "bob",
                "POST",
                &format!("/servers/{}/leave", c.sid),
                Value::Null,
            )
            .await;
        assert_eq!(s, 204);
        assert!(ids(&c, "bob", "/search", "contenu", "").await.is_empty());
    }

    #[tokio::test]
    async fn limiteur_et_metrique() {
        let Some(c) = cadre().await else { return };
        c.poster("alice", "bonjour tout le monde").await;
        for _ in 0..30 {
            assert_eq!(ids(&c, "bob", "/search", "bonjour", "").await.len(), 1);
            let (s, _) = c.api("bob", "GET", "/search?q=a", Value::Null).await;
            assert_eq!(s, 400);
        }
        let r = Request::builder()
            .uri(chemin("/search", "bonjour", ""))
            .header("authorization", "Bearer bob")
            .body(Body::empty())
            .unwrap();
        let rep = c.e.app.clone().oneshot(r).await.unwrap();
        assert_eq!(rep.status().as_u16(), 429);
        assert_eq!(rep.headers()["retry-after"], "60");
        let (s, _) = c
            .api(
                "alice",
                "GET",
                &chemin("/search", "bonjour", ""),
                Value::Null,
            )
            .await;
        assert_eq!(s, 200);
        let instantane = crate::metrics::Instantane {
            uptime: 0,
            base_up: true,
            totaux: None,
            stockage: None,
            ws: (0, 0),
        };
        assert!(
            c.e.state
                .metrics()
                .rendu(&instantane)
                .contains("estompe_search_duration_seconds_count 31\n")
        );
    }
}
