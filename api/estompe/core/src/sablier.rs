use std::fmt;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, Utc};
pub use commun::{
    emission::Emetteur,
    evenements::{Donnees, Evenement, Sens},
};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};
use utoipa_axum::router::OpenApiRouter;

use crate::{
    http::{internal, write_err, write_json},
    messages::SYSTEME,
    servers::uuid_valide,
    state::AppState,
    users,
};

pub const COMMANDE: &str = "importer-sablier";
const LOT: i64 = 1000;

fn evenement(id: String, sub: &str, donnees: Donnees) -> Evenement {
    let mut e = Evenement::nouveau(sub, donnees);
    e.id = id;
    e
}

pub fn envoi(
    message: i64,
    sub: &str,
    serveur: Option<String>,
    salon: Option<String>,
    pieces: usize,
) -> Vec<Evenement> {
    let prive = salon.is_none();
    let serveur = serveur.filter(|_| !prive);
    let mut sortie = vec![evenement(
        format!("import:estompe:message:{message}"),
        sub,
        Donnees::EstompeMessage {
            serveur: serveur.clone(),
            salon: salon.clone(),
            prive,
        },
    )];
    if pieces > 0 {
        sortie.push(evenement(
            format!("import:estompe:fichier:{message}"),
            sub,
            Donnees::EstompeFichier {
                serveur,
                salon,
                prive,
                nombre: u32::try_from(pieces).unwrap_or(u32::MAX),
            },
        ));
    }
    sortie
}

pub fn reactions(
    message: i64,
    emoji: &str,
    reacteur: &str,
    auteur: Option<&str>,
) -> Vec<Evenement> {
    let empreinte: String = Sha256::digest(format!("{reacteur}\n{emoji}"))
        .iter()
        .take(16)
        .map(|o| format!("{o:02x}"))
        .collect();
    let un = |sens: Sens, nom: &str, sub: &str| {
        evenement(
            format!("import:estompe:reaction:{message}:{empreinte}:{nom}"),
            sub,
            Donnees::EstompeReaction {
                sens,
                emoji: emoji.to_string(),
            },
        )
    };
    let mut sortie = vec![un(Sens::Donnee, "donnee", reacteur)];
    if let Some(a) = auteur.filter(|a| *a != reacteur) {
        sortie.push(un(Sens::Recue, "recue", a));
    }
    sortie
}

pub fn emettre(state: &AppState, evenements: Vec<Evenement>) {
    let emetteur = state.emetteur();
    for e in evenements {
        emetteur.emettre(e);
    }
}

pub async fn reaction_ajoutee(
    state: &AppState,
    pool: &PgPool,
    message: i64,
    emoji: &str,
    reacteur: &str,
    auteur: &str,
) {
    if !state.emetteur().actif() {
        return;
    }
    let cible: Option<String> =
        sqlx::query_scalar("SELECT keycloak_sub FROM users WHERE id=$1::uuid AND id <> $2::uuid")
            .bind(auteur)
            .bind(SYSTEME)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    emettre(state, reactions(message, emoji, reacteur, cible.as_deref()));
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Bilan {
    pub messages: usize,
    pub fichiers: usize,
    pub reactions_donnees: usize,
    pub reactions_recues: usize,
}

impl fmt::Display for Bilan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Import d'Estompe vers Sablier :")?;
        writeln!(f, "- messages : {}", self.messages)?;
        writeln!(f, "- messages avec fichiers : {}", self.fichiers)?;
        writeln!(f, "- réactions données : {}", self.reactions_donnees)?;
        writeln!(f, "- réactions reçues : {}", self.reactions_recues)?;
        writeln!(
            f,
            "Évènements publiés : {} (Sablier ignore ceux qu'il a déjà reçus).",
            self.messages + self.fichiers + self.reactions_donnees + self.reactions_recues
        )
    }
}

fn date(micros: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_micros(micros).unwrap_or_default()
}

fn historique(evenements: Vec<Evenement>, micros: i64) -> Vec<Evenement> {
    evenements
        .into_iter()
        .map(|mut e| {
            e.date = date(micros);
            e.importe = true;
            e
        })
        .collect()
}

type LigneMessage = (i64, String, i64, Option<String>, Option<String>, i64);
type LigneReaction = (i64, String, String, i64, Option<String>);

pub async fn importer(
    pool: &PgPool,
    mut publier: impl AsyncFnMut(&Evenement) -> Result<(), String>,
) -> Result<Bilan, String> {
    let bd = |e: sqlx::Error| format!("base : {e}");
    let dernier: i64 = sqlx::query_scalar("SELECT COALESCE(max(id), 0) FROM messages")
        .fetch_one(pool)
        .await
        .map_err(bd)?;
    let mut bilan = Bilan::default();
    let mut apres = 0;
    while apres < dernier {
        let jusque = apres + LOT;
        let messages: Vec<LigneMessage> = sqlx::query_as(
            "SELECT m.id, u.keycloak_sub, (extract(epoch FROM m.created_at) * 1000000)::bigint, m.channel_id::text, c.server_id::text,
                    (SELECT count(*) FROM files f WHERE f.message_id = m.id)
             FROM messages m JOIN users u ON u.id = m.author_id LEFT JOIN channels c ON c.id = m.channel_id
             WHERE m.id > $1 AND m.id <= $2 AND m.author_id <> $3::uuid AND NOT m.system ORDER BY m.id",
        )
        .bind(apres)
        .bind(jusque)
        .bind(SYSTEME)
        .fetch_all(pool)
        .await
        .map_err(bd)?;
        for (id, sub, micros, salon, serveur, pieces) in messages {
            let pieces = usize::try_from(pieces).unwrap_or_default();
            for e in historique(envoi(id, &sub, serveur, salon, pieces), micros) {
                publier(&e).await?;
            }
            bilan.messages += 1;
            bilan.fichiers += usize::from(pieces > 0);
        }
        let lignes: Vec<LigneReaction> = sqlx::query_as(
            "SELECT r.message_id, ru.keycloak_sub, r.emoji, (extract(epoch FROM r.created_at) * 1000000)::bigint,
                    CASE WHEN m.author_id = $3::uuid THEN NULL ELSE au.keycloak_sub END
             FROM reactions r JOIN users ru ON ru.id = r.user_id JOIN messages m ON m.id = r.message_id JOIN users au ON au.id = m.author_id
             WHERE r.message_id > $1 AND r.message_id <= $2 AND r.user_id <> $3::uuid ORDER BY r.message_id, r.user_id, r.emoji",
        )
        .bind(apres)
        .bind(jusque)
        .bind(SYSTEME)
        .fetch_all(pool)
        .await
        .map_err(bd)?;
        for (id, reacteur, emoji, micros, auteur) in lignes {
            let evenements =
                historique(reactions(id, &emoji, &reacteur, auteur.as_deref()), micros);
            bilan.reactions_donnees += 1;
            bilan.reactions_recues += evenements.len() - 1;
            for e in evenements {
                publier(&e).await?;
            }
        }
        apres = jusque;
    }
    Ok(bilan)
}

pub async fn lancer(get: impl Fn(&str) -> Option<String>) -> Result<Bilan, String> {
    let emetteur = Emetteur::depuis_env(&get);
    if !emetteur.actif() {
        return Err(format!(
            "{} non renseignée ou invalide : rien à importer",
            commun::emission::VARIABLE_URL
        ));
    }
    let url = commun::env::url_base(&get, "estompe")?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .map_err(|e| format!("base : {e}"))?;
    importer(&pool, async |e| emetteur.publier(e).await).await
}

const CHEMIN_PROFIL: &str = "/internal/profile-visible";

pub fn router() -> OpenApiRouter<AppState> {
    let routeur = OpenApiRouter::new().route(CHEMIN_PROFIL, get(profil_visible));
    let op = crate::openapi::operation(
        "interne",
        "Dit à Sablier si `viewer` (`sub`) peut ouvrir le profil de la cible, désignée par `target_user` (identifiant Estompe) ou `target` (`sub`) : `{visible, sub}`, `sub` de la cible seulement si visible. Jeton `SABLIER_ESTOMPE_TOKEN`, réseau interne ; 404 sans ce jeton",
        &[
            ("200", "ok"),
            (
                "400",
                "`viewer` manquant, `target` et `target_user` absents ou donnés ensemble, `target_user` qui n'est pas un uuid",
            ),
            ("401", "jeton de Sablier attendu"),
            ("404", "jeton non configuré"),
        ],
        CHEMIN_PROFIL,
    );
    crate::openapi::documenter(routeur, CHEMIN_PROFIL, utoipa::openapi::HttpMethod::Get, op)
}

#[derive(Deserialize)]
struct Regard {
    viewer: Option<String>,
    target: Option<String>,
    target_user: Option<String>,
}

pub async fn profil_ouvert(pool: &PgPool, viewer: &str, target: &str) -> Result<bool, sqlx::Error> {
    if viewer == target {
        return Ok(true);
    }
    let (Some(v), Some(t)) = (
        users::user_by_sub(pool, viewer).await?,
        users::user_by_sub(pool, target).await?,
    ) else {
        return Ok(false);
    };
    Ok(users::co_members(pool, &v.id).await?.contains(&t.id))
}

enum Cible {
    Sub(String),
    Utilisateur(String),
}

async fn sub_visible(
    pool: &PgPool,
    viewer: &str,
    cible: Cible,
) -> Result<Option<String>, sqlx::Error> {
    let target = match cible {
        Cible::Sub(s) => Some(s),
        Cible::Utilisateur(id) => {
            sqlx::query_scalar("SELECT keycloak_sub FROM users WHERE id=$1::uuid")
                .bind(id)
                .fetch_optional(pool)
                .await?
        }
    };
    let Some(target) = target else {
        return Ok(None);
    };
    Ok(profil_ouvert(pool, viewer, &target)
        .await?
        .then_some(target))
}

async fn profil_visible(
    State(state): State<AppState>,
    Query(r): Query<Regard>,
    h: HeaderMap,
) -> Response {
    let attendu = &state.config().sablier_token;
    if attendu.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let donne = h
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !crate::api::jetons_egaux(attendu, donne) {
        return write_err(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "jeton de Sablier attendu",
        );
    }
    let sub = |v: Option<String>| v.filter(|s| !s.trim().is_empty());
    let (Some(viewer), Some(cible)) = (
        sub(r.viewer),
        match (sub(r.target), sub(r.target_user)) {
            (Some(s), None) => Some(Cible::Sub(s)),
            (None, Some(id)) if uuid_valide(&id) => Some(Cible::Utilisateur(id)),
            _ => None,
        },
    ) else {
        return write_err(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "viewer et un seul de target ou target_user (uuid) sont obligatoires",
        );
    };
    let pool = match state.pool() {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    match sub_visible(pool, &viewer, cible).await {
        Ok(Some(sub)) => write_json(StatusCode::OK, &json!({ "visible": true, "sub": sub })),
        Ok(None) => write_json(StatusCode::OK, &json!({ "visible": false })),
        Err(_) => internal(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use chrono::Datelike;
    use serde_json::{Value, json};

    use super::*;
    use crate::messages::tests::cadre;

    const PORT_FERME: &str = "redis://127.0.0.1:1";

    fn sans(e: &Evenement, interdits: &[&str]) {
        let texte = e.json();
        for i in interdits {
            assert!(!texte.contains(i), "« {i} » dans {texte}");
        }
        let v: Value = serde_json::from_str(&texte).unwrap();
        let cles: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(
            cles,
            ["date", "donnees", "id", "importe", "source", "sub", "type"]
        );
        for cle in v["donnees"].as_object().unwrap().keys() {
            assert!(
                ["serveur", "salon", "prive", "nombre", "sens", "emoji"].contains(&cle.as_str()),
                "{texte}"
            );
        }
    }

    #[test]
    fn evenements_sans_contenu_ni_interlocuteur() {
        let serveur = envoi(7, "sub-alice", Some("srv".into()), Some("sal".into()), 0);
        assert_eq!(serveur.len(), 1);
        assert_eq!(serveur[0].id, "import:estompe:message:7");
        assert!(!serveur[0].importe);
        assert_eq!(
            serveur[0].donnees.contenu(),
            json!({ "serveur": "srv", "salon": "sal", "prive": false })
        );
        let prive = envoi(8, "sub-alice", Some("fuite".into()), None, 3);
        assert_eq!(
            prive.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["import:estompe:message:8", "import:estompe:fichier:8"]
        );
        assert_eq!(
            prive[0].donnees.contenu(),
            json!({ "serveur": null, "salon": null, "prive": true })
        );
        assert_eq!(
            prive[1].donnees.contenu(),
            json!({ "serveur": null, "salon": null, "prive": true, "nombre": 3 })
        );
        let r = reactions(8, "👍", "sub-bob", Some("sub-alice"));
        assert_eq!(r.len(), 2);
        assert_eq!(
            (r[0].sub.as_str(), r[1].sub.as_str()),
            ("sub-bob", "sub-alice")
        );
        assert_eq!(
            r[0].donnees.contenu(),
            json!({ "sens": "donnee", "emoji": "👍" })
        );
        assert_eq!(
            r[1].donnees.contenu(),
            json!({ "sens": "recue", "emoji": "👍" })
        );
        assert!(r[0].id.ends_with(":donnee") && r[1].id.ends_with(":recue"));
        assert_eq!(r[0].id, reactions(8, "👍", "sub-bob", None)[0].id);
        assert_ne!(r[0].id, reactions(8, "🎉", "sub-bob", None)[0].id);
        assert_ne!(r[0].id, reactions(8, "👍", "sub-carol", None)[0].id);
        assert_eq!(reactions(8, "👍", "sub-bob", Some("sub-bob")).len(), 1);
        sans(&r[1], &["sub-bob", "fuite"]);
        for e in serveur.iter().chain(&prive).chain(&r) {
            assert!(e.verifier().is_ok());
            assert!(e.id.chars().count() <= commun::evenements::ID_MAX);
        }
        for e in &prive {
            sans(e, &["sub-bob", "fuite"]);
        }
    }

    async fn collecte(pool: &PgPool) -> (Bilan, Vec<Evenement>) {
        let mut recus = Vec::new();
        let bilan = importer(pool, async |e| {
            recus.push(e.clone());
            Ok(())
        })
        .await
        .unwrap();
        (bilan, recus)
    }

    #[tokio::test]
    async fn import_rejouable_et_sans_donnee_privee() {
        let Some(c) = cadre().await else { return };
        let fichier = c.fichier("alice", "file").await;
        let texte = "contenu secret du salon";
        let m1 = c.poster("alice", texte).await;
        let (s, m2) = c
            .api(
                "alice",
                "POST",
                &c.chemin(),
                json!({ "content": "", "file_ids": [fichier] }),
            )
            .await;
        assert_eq!(s, 201, "{m2}");
        let supprime = c.poster("bob", "message supprimé avant l'import").await;
        let dm = c.ouvrir_dm("alice", "bob").await;
        let (s, m3) = c
            .api(
                "bob",
                "POST",
                &format!("/dms/{dm}/messages"),
                json!({ "content": "confidence privée" }),
            )
            .await;
        assert_eq!(s, 201, "{m3}");
        let (s, sondage) = c
            .api(
                "alice",
                "POST",
                &c.chemin(),
                json!({ "content": "question ?", "poll": { "options": ["a", "b"] } }),
            )
            .await;
        assert_eq!(s, 201, "{sondage}");
        let id = |m: &Value| m["id"].as_str().unwrap().to_string();
        let pouce = "%F0%9F%91%8D";
        for (qui, m) in [
            ("bob", &m1),
            ("alice", &m1),
            ("alice", &m3),
            ("bob", &sondage),
            ("alice", &supprime),
        ] {
            let chemin = format!("/messages/{}/reactions/{pouce}", id(m));
            assert_eq!(c.api(qui, "PUT", &chemin, Value::Null).await.0, 204);
        }
        let chemin = format!("/messages/{}", id(&supprime));
        assert_eq!(c.api("bob", "DELETE", &chemin, Value::Null).await.0, 204);
        c.sql("UPDATE messages SET created_at = '2024-02-29T23:30:00Z'")
            .await;
        c.sql("UPDATE reactions SET created_at = '2023-07-01T10:00:00Z'")
            .await;

        let (bilan, premier) = collecte(c.pool()).await;
        assert_eq!(
            bilan,
            Bilan {
                messages: 3,
                fichiers: 1,
                reactions_donnees: 4,
                reactions_recues: 2,
            }
        );
        assert_eq!(premier.len(), 10);
        let (second_bilan, second) = collecte(c.pool()).await;
        assert_eq!(second_bilan, bilan);
        assert_eq!(second, premier);
        let mut ids: Vec<&str> = premier.iter().map(|e| e.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 10);
        let trouve = |id: String| premier.iter().find(|e| e.id == id).cloned();
        assert!(trouve(format!("import:estompe:message:{}", id(&supprime))).is_none());
        assert!(trouve(format!("import:estompe:message:{}", id(&sondage))).is_none());
        let salon = trouve(format!("import:estompe:message:{}", id(&m1))).unwrap();
        assert_eq!(salon.sub, "sub-alice");
        assert_eq!(
            salon.donnees.contenu(),
            json!({ "serveur": c.sid, "salon": c.salon, "prive": false })
        );
        let joint = trouve(format!("import:estompe:fichier:{}", id(&m2))).unwrap();
        assert_eq!(
            joint.donnees.contenu(),
            json!({ "serveur": c.sid, "salon": c.salon, "prive": false, "nombre": 1 })
        );
        let prive = trouve(format!("import:estompe:message:{}", id(&m3))).unwrap();
        assert_eq!(prive.sub, "sub-bob");
        assert_eq!(
            prive.donnees.contenu(),
            json!({ "serveur": null, "salon": null, "prive": true })
        );
        for e in &premier {
            assert!(e.importe && e.verifier().is_ok(), "{}", e.json());
            let attendue = if matches!(e.donnees, Donnees::EstompeReaction { .. }) {
                2023
            } else {
                2024
            };
            assert_eq!(e.date.year(), attendue, "{}", e.json());
            sans(e, &[texte, "confidence", "question", &dm, &c.alice, &c.bob]);
            let autre = if e.sub == "sub-alice" {
                "sub-bob"
            } else {
                "sub-alice"
            };
            sans(e, &[autre]);
        }
        let reel = envoi(
            id(&m1).parse().unwrap(),
            "sub-alice",
            Some(c.sid.clone()),
            Some(c.salon.clone()),
            0,
        );
        assert_eq!(
            (reel[0].id.as_str(), reel[0].importe),
            (salon.id.as_str(), false)
        );
        let recues: Vec<&Evenement> = premier
            .iter()
            .filter(|e| e.id.ends_with(":recue"))
            .collect();
        assert_eq!(
            recues.iter().map(|e| e.sub.as_str()).collect::<Vec<_>>(),
            ["sub-alice", "sub-bob"]
        );
    }

    #[tokio::test]
    async fn envoi_identique_sans_redis() {
        let mut durees = Vec::new();
        for emetteur in [
            Emetteur::inactif(),
            Emetteur::depuis(Some(PORT_FERME), None),
        ] {
            let Some(c) = cadre().await else { return };
            let actif = emetteur.actif();
            c.e.state.definir_emetteur(emetteur);
            assert_eq!(c.e.state.emetteur().actif(), actif);
            let debut = Instant::now();
            let m = c.poster("alice", "bonjour").await;
            let chemin = format!(
                "/messages/{}/reactions/%F0%9F%91%8D",
                m["id"].as_str().unwrap()
            );
            assert_eq!(c.api("bob", "PUT", &chemin, Value::Null).await.0, 204);
            let dm = c.ouvrir_dm("alice", "bob").await;
            let (s, v) = c
                .api(
                    "alice",
                    "POST",
                    &format!("/dms/{dm}/messages"),
                    json!({ "content": "x" }),
                )
                .await;
            assert_eq!(s, 201, "{v}");
            durees.push(debut.elapsed());
            assert_eq!(m["content"], "bonjour");
            let (_, liste) = c.api("bob", "GET", &c.chemin(), Value::Null).await;
            assert_eq!(liste["messages"][0]["reactions"][0]["count"], 1);
        }
        for d in &durees {
            assert!(*d < Duration::from_secs(2), "{durees:?}");
        }
    }

    #[tokio::test]
    async fn profil_visible_pour_sablier() {
        use crate::servers::tests::{appel, env_avec_blocs, invitation_de, membre, serveur};

        let Some(c) = cadre().await else { return };
        let chemin = "/internal/profile-visible?viewer=sub-alice&target=sub-bob";
        let (s, _) = appel(&c.e, "jeton-sablier", "GET", chemin, None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);

        let droits = |r: &crate::testutil::Requete| crate::testutil::reponse_droits(r, |_, _| true);
        let variables = [("SABLIER_ESTOMPE_TOKEN", "jeton-sablier")];
        let Some(e) = env_avec_blocs(droits, vec![], &variables).await else {
            return;
        };
        let mut ids = std::collections::HashMap::new();
        for nom in ["alice", "bob", "carol", "dave"] {
            ids.insert(nom, membre(&e, nom).await);
        }
        let sid = serveur(&e, "Chat", "alice").await;
        let code = invitation_de(&e, "alice", &sid, "").await;
        let rejoindre = format!("/server-invites/{code}/join");
        assert_eq!(
            appel(&e, "bob", "POST", &rejoindre, None).await.0,
            StatusCode::OK
        );
        let corps = json!({ "pseudo": "alice" }).to_string();
        assert_eq!(
            appel(&e, "dave", "POST", "/dms", Some(&corps)).await.0,
            StatusCode::OK
        );
        for jeton in ["", "faux", "jeton-sablier-de-trop", "alice"] {
            let (s, v) = appel(&e, jeton, "GET", chemin, None).await;
            assert_eq!(
                (s, v["error"].as_str()),
                (StatusCode::UNAUTHORIZED, Some("unauthorized"))
            );
        }
        let sans_entete = axum::http::Request::get(chemin)
            .body(axum::body::Body::empty())
            .unwrap();
        let rep = tower::ServiceExt::oneshot(e.app.clone(), sans_entete)
            .await
            .unwrap();
        assert_eq!(rep.status(), StatusCode::UNAUTHORIZED);
        for requete in [
            "",
            "?viewer=sub-alice",
            "?target=sub-bob",
            "?viewer=&target=sub-bob",
            "?viewer=sub-alice&target_user=",
            "?viewer=sub-alice&target_user=sub-bob",
            "?viewer=sub-alice&target_user=pas-un-uuid",
            &format!(
                "?viewer=sub-alice&target=sub-bob&target_user={}",
                ids["bob"]
            ),
            &format!("?target_user={}", ids["bob"]),
        ] {
            let chemin = format!("/internal/profile-visible{requete}");
            let (s, _) = appel(&e, "jeton-sablier", "GET", &chemin, None).await;
            assert_eq!(s, StatusCode::BAD_REQUEST, "{requete}");
        }
        for (viewer, target, attendu) in [
            ("alice", "alice", true),
            ("alice", "bob", true),
            ("bob", "alice", true),
            ("dave", "alice", true),
            ("carol", "alice", false),
            ("alice", "carol", false),
            ("bob", "dave", false),
            ("alice", "inconnu", false),
            ("inconnu", "alice", false),
        ] {
            let chemin =
                format!("/internal/profile-visible?viewer=sub-{viewer}&target=sub-{target}");
            let reponse = if attendu {
                json!({ "visible": true, "sub": format!("sub-{target}") })
            } else {
                json!({ "visible": false })
            };
            let (s, v) = appel(&e, "jeton-sablier", "GET", &chemin, None).await;
            assert_eq!((s, &v), (StatusCode::OK, &reponse), "{viewer} → {target}");
            let inconnu = "00000000-0000-4000-8000-000000000000".to_string();
            let chemin = format!(
                "/internal/profile-visible?viewer=sub-{viewer}&target_user={}",
                ids.get(target).unwrap_or(&inconnu)
            );
            let (s, v) = appel(&e, "jeton-sablier", "GET", &chemin, None).await;
            assert_eq!(
                (s, &v),
                (StatusCode::OK, &reponse),
                "{viewer} → identifiant de {target}"
            );
        }
        let quitter = format!("/servers/{sid}/leave");
        assert_eq!(
            appel(&e, "bob", "POST", &quitter, None).await.0,
            StatusCode::NO_CONTENT
        );
        let par_identifiant = format!(
            "/internal/profile-visible?viewer=sub-alice&target_user={}",
            ids["bob"]
        );
        for chemin in [chemin, par_identifiant.as_str()] {
            let (_, v) = appel(&e, "jeton-sablier", "GET", chemin, None).await;
            assert_eq!(v, json!({ "visible": false }));
        }
    }
}
