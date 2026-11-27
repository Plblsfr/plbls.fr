use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use axum::{
    body::Bytes,
    extract::{Path, State},
    response::Response,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::{
    agregats,
    badges::{self, Badge},
    catalogue::{self, Domaine, PREMIER_JOUR, Stat, Unite},
    droits::Droits,
    etat::AppState,
    garde::Personne,
    http::{bd, corps, invalide, ok},
    periode::{Periode, debut_des_temps, fin_des_temps},
    trophees::{self, Public},
};

const DELAI_ESTOMPE: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StatPartagee {
    pub key: &'static str,
    pub label: &'static str,
    pub domain: Domaine,
    pub shared: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TropheePartage {
    pub id: i64,
    pub name: String,
    pub icon: String,
    pub shared: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Reglages {
    pub stats: Vec<StatPartagee>,
    pub trophies: Vec<TropheePartage>,
    pub badges: Vec<Badge>,
}

async fn stats_partagees(pool: &PgPool, sub: &str) -> Result<HashSet<String>, sqlx::Error> {
    Ok(
        sqlx::query_scalar("SELECT stat FROM partages_stats WHERE sub = $1")
            .bind(sub)
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect(),
    )
}

pub async fn lire(pool: &PgPool, sub: &str, droits: &Droits) -> Result<Reglages, sqlx::Error> {
    let partagees = stats_partagees(pool, sub).await?;
    Ok(Reglages {
        stats: catalogue::STATS
            .iter()
            .filter(|s| s.partageable && s.domaine.accessible(droits))
            .map(|s| StatPartagee {
                key: s.cle,
                label: s.libelle,
                domain: s.domaine,
                shared: partagees.contains(s.cle),
            })
            .collect(),
        trophies: trophees::etats(pool, sub, droits)
            .await?
            .into_iter()
            .filter(|e| e.palier() > 0)
            .map(|e| TropheePartage {
                id: e.trophee.id,
                name: e.trophee.nom,
                icon: e.trophee.icone,
                shared: e.partage,
            })
            .collect(),
        badges: badges::reglages(pool, sub, droits).await?,
    })
}

pub async fn voir(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    Ok(ok(&lire(state.pool(), p.sub(), &p.droits)
        .await
        .map_err(bd)?))
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Modification {
    #[serde(default)]
    stats: HashMap<String, bool>,
    #[serde(default)]
    trophies: HashMap<String, bool>,
    #[serde(default)]
    badges: HashMap<String, bool>,
}

pub async fn modifier(
    State(state): State<AppState>,
    p: Personne,
    octets: Bytes,
) -> Result<Response, Response> {
    let (pool, sub) = (state.pool(), p.sub());
    let m: Modification = corps(
        &octets,
        "corps : stats {clé: booléen}, trophies {identifiant: booléen}, badges {clé: booléen}",
    )?;
    if let Some(inconnue) = m
        .stats
        .keys()
        .find(|cle| !catalogue::trouver(cle).is_some_and(|s| s.partageable))
    {
        return Err(invalide(&format!(
            "stats : {inconnue} n'est pas partageable"
        )));
    }
    let debloques: HashSet<i64> = trophees::etats(pool, sub, &p.droits)
        .await
        .map_err(bd)?
        .iter()
        .filter(|e| e.palier() > 0)
        .map(|e| e.trophee.id)
        .collect();
    let mut trophees = Vec::new();
    for (id, partage) in &m.trophies {
        match id.parse::<i64>() {
            Ok(id) if !*partage || debloques.contains(&id) => trophees.push((id, *partage)),
            _ => return Err(invalide(&format!("trophies : {id} n'est pas débloqué"))),
        }
    }
    if !m.badges.is_empty() {
        let obtenus: HashSet<String> = if catalogue::Domaine::Entracte.accessible(&p.droits) {
            badges::existants(pool, sub)
                .await
                .map_err(bd)?
                .into_iter()
                .collect()
        } else {
            HashSet::new()
        };
        if let Some(cle) = m.badges.keys().find(|c| !obtenus.contains(*c)) {
            return Err(invalide(&format!("badges : {cle} n'est pas obtenu")));
        }
    }
    let mut tx = pool.begin().await.map_err(bd)?;
    for (cle, partage) in &m.badges {
        badges::partager(&mut tx, sub, cle, *partage)
            .await
            .map_err(bd)?;
    }
    for (stat, partage) in &m.stats {
        let requete = if *partage {
            "INSERT INTO partages_stats (sub, stat) VALUES ($1, $2) ON CONFLICT DO NOTHING"
        } else {
            "DELETE FROM partages_stats WHERE sub = $1 AND stat = $2"
        };
        sqlx::query(requete)
            .bind(sub)
            .bind(stat)
            .execute(&mut *tx)
            .await
            .map_err(bd)?;
    }
    for (trophee, partage) in trophees {
        let requete = if partage {
            "INSERT INTO partages_trophees (sub, trophee) VALUES ($1, $2) ON CONFLICT DO NOTHING"
        } else {
            "DELETE FROM partages_trophees WHERE sub = $1 AND trophee = $2"
        };
        sqlx::query(requete)
            .bind(sub)
            .bind(trophee)
            .execute(&mut *tx)
            .await
            .map_err(bd)?;
    }
    tx.commit().await.map_err(bd)?;
    Ok(ok(&lire(pool, sub, &p.droits).await.map_err(bd)?))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Valeur {
    pub key: &'static str,
    pub label: &'static str,
    pub domain: Domaine,
    pub unit: Unite,
    pub value: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Profil {
    pub month: Vec<Valeur>,
    pub total: Vec<Valeur>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_day: Option<NaiveDate>,
    pub trophies: Vec<Public>,
    pub badges: Vec<Badge>,
}

async fn valeurs(
    pool: &PgPool,
    sub: &str,
    stats: &[&'static Stat],
    du: NaiveDate,
    au: NaiveDate,
) -> Result<Vec<Valeur>, sqlx::Error> {
    let mut liste = Vec::new();
    for s in stats {
        let value = catalogue::valeur(pool, sub, s, du, au).await?;
        if value != 0 {
            liste.push(Valeur {
                key: s.cle,
                label: s.libelle,
                domain: s.domaine,
                unit: s.unite,
                value,
            });
        }
    }
    Ok(liste)
}

pub async fn profil(
    pool: &PgPool,
    cible: &str,
    lecteur: &Droits,
    aujourd_hui: NaiveDate,
) -> Result<Profil, sqlx::Error> {
    let partagees = stats_partagees(pool, cible).await?;
    let stats: Vec<&'static Stat> = catalogue::STATS
        .iter()
        .filter(|s| s.partageable && s.domaine.accessible(lecteur) && partagees.contains(s.cle))
        .collect();
    let (debut_du_mois, _) = Periode::Mois.bornes(aujourd_hui);
    Ok(Profil {
        month: valeurs(pool, cible, &stats, debut_du_mois, aujourd_hui).await?,
        total: valeurs(pool, cible, &stats, debut_des_temps(), aujourd_hui).await?,
        first_day: if partagees.contains(PREMIER_JOUR) {
            agregats::premier_jour(pool, cible).await?
        } else {
            None
        },
        trophies: trophees::etats(pool, cible, lecteur)
            .await?
            .iter()
            .filter(|e| e.partage)
            .filter_map(|e| e.public(debut_des_temps(), fin_des_temps()))
            .collect(),
        badges: badges::publics(pool, cible, lecteur).await?,
    })
}

pub async fn profil_visible(state: &AppState, lecteur: &str, cible: &str) -> Option<String> {
    let (url, jeton) = state.config().estompe.as_ref()?;
    let reponse = state
        .http()
        .get(format!("{url}/internal/profile-visible"))
        .query(&[("viewer", lecteur), ("target_user", cible)])
        .bearer_auth(jeton)
        .timeout(DELAI_ESTOMPE)
        .send()
        .await
        .ok()?;
    if !reponse.status().is_success() {
        return None;
    }
    let v = reponse.json::<Value>().await.ok()?;
    if v["visible"] != json!(true) {
        return None;
    }
    v["sub"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn uuid_valide(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

pub async fn voir_profil(
    State(state): State<AppState>,
    p: Personne,
    Path(utilisateur): Path<String>,
) -> Result<Response, Response> {
    if !uuid_valide(&utilisateur) {
        return Err(invalide("user : identifiant Estompe (uuid) attendu"));
    }
    let Some(cible) = profil_visible(&state, p.sub(), &utilisateur).await else {
        return Ok(ok(&Profil::default()));
    };
    let aujourd_hui = state.aujourd_hui().await.map_err(bd)?;
    Ok(ok(&profil(state.pool(), &cible, &p.droits, aujourd_hui)
        .await
        .map_err(bd)?))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use chrono::Days;
    use commun::evenements::GenreBadge;

    use super::*;
    use crate::{
        domaines::{entracte, messagerie},
        testutil::{Env, Faux, env, env_avec, faux},
    };

    const DIANE: &str = "sub-diane";
    const ID_DIANE: &str = "0d1a2e00-0000-4000-8000-00000000d1a2";
    const ID_FRED: &str = "0f2ed000-0000-4000-8000-0000000f2ed0";
    const ID_INCONNU: &str = "00000000-0000-4000-8000-000000000000";
    const PROFIL: &str = "/profiles/0d1a2e00-0000-4000-8000-00000000d1a2";

    async fn faux_estompe() -> Faux {
        faux(|r| {
            let lecteur = |sub: &str| r.query.contains(&format!("viewer={sub}&"));
            let cible = |id: &str| r.query.ends_with(&format!("&target_user={id}"));
            let autorise = r.chemin == "/internal/profile-visible"
                && r.entete("authorization") == "Bearer jeton-estompe";
            if !autorise || lecteur("sub-admin") {
                return (500, Vec::new());
            }
            if lecteur("sub-personne") {
                return (200, b"oui".to_vec());
            }
            let reponse = if cible(ID_FRED) {
                json!({ "visible": true })
            } else if !cible(ID_DIANE) || lecteur("sub-bruno") {
                json!({ "visible": false, "sub": DIANE })
            } else if lecteur("sub-chloe") {
                json!({ "visible": "true", "sub": DIANE })
            } else {
                json!({ "visible": true, "sub": DIANE })
            };
            (200, reponse.to_string().into_bytes())
        })
        .await
    }

    async fn env_avec_estompe(estompe: &Faux) -> Option<Env> {
        let url = estompe.url.clone();
        let e = env_avec(move |c| c.estompe = Some((url, "jeton-estompe".into()))).await?;
        let mut droits = e.droits.lock().unwrap();
        droits.insert("sub-emma".into(), vec!["access", "entracte:access"]);
        droits.insert("sub-chloe".into(), vec!["access", "entracte:access"]);
        droits.insert("sub-personne".into(), vec!["access"]);
        drop(droits);
        Some(e)
    }

    async fn activite_de_diane(e: &Env) -> (NaiveDate, NaiveDate, i64) {
        let pool = e.state.pool();
        let aujourd_hui = e.state.aujourd_hui().await.unwrap();
        let avant = aujourd_hui - Days::new(400);
        for (j, stat, cle, n) in [
            (aujourd_hui, entracte::SECONDES, "", 7200),
            (aujourd_hui, messagerie::MESSAGES, "srv/a", 600),
            (aujourd_hui, messagerie::FICHIERS, "", 12),
            (avant, entracte::SECONDES, "", 100),
            (avant, messagerie::MESSAGES, "prive", 40),
        ] {
            agregats::ajouter(pool, DIANE, j, stat, cle, n)
                .await
                .unwrap();
        }
        trophees::evaluer(pool, DIANE, aujourd_hui).await.unwrap();
        let pipelette: i64 = sqlx::query_scalar("SELECT id FROM trophees WHERE cle = 'pipelette'")
            .fetch_one(pool)
            .await
            .unwrap();
        (aujourd_hui, avant, pipelette)
    }

    async fn partager(e: &Env, corps: Value) {
        let r = e.appel(Some("diane"), "PUT", "/sharing", Some(corps)).await;
        assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
    }

    fn vide() -> Value {
        json!({"month": [], "total": [], "trophies": [], "badges": []})
    }

    #[tokio::test]
    async fn c8_rien_de_prive_n_est_servi_et_le_partage_exige_le_profil_visible_et_l_acces_au_service()
     {
        let estompe = faux_estompe().await;
        let Some(e) = env_avec_estompe(&estompe).await else {
            return;
        };
        let (aujourd_hui, avant, pipelette) = activite_de_diane(&e).await;
        for qui in ["alice", "emma", "bruno", "diane"] {
            let r = e.get(qui, PROFIL).await;
            assert_eq!((r.statut, r.json()), (StatusCode::OK, vide()), "{qui}");
        }
        partager(
            &e,
            json!({
                "stats": {"entracte.seconds": true, "messaging.messages": true, "usage.first_day": true},
                "trophies": {pipelette.to_string(): true},
            }),
        )
        .await;
        let messages = |n: i64| json!({"key": "messaging.messages", "label": "Messages envoyés", "domain": "messaging", "unit": "count", "value": n});
        let regarde = |n: i64| json!({"key": "entracte.seconds", "label": "Temps regardé", "domain": "entracte", "unit": "seconds", "value": n});
        let trophee = json!([{
            "name": "Pipelette",
            "description": "Des messages envoyés.",
            "icon": "bulle",
            "tier": 1,
            "unlocked_at": aujourd_hui,
        }]);
        let r = e.get("alice", PROFIL).await;
        assert_eq!(
            r.json(),
            json!({
                "month": [messages(600)],
                "total": [messages(640)],
                "first_day": avant,
                "trophies": trophee,
                "badges": [],
            })
        );
        for prive in [
            "entracte",
            "files",
            "Premier jour",
            "Moulin",
            "7200",
            "7300",
        ] {
            assert!(!r.texte().contains(prive), "{prive} : {}", r.texte());
        }
        let avec_entracte = json!({
            "month": [regarde(7200), messages(600)],
            "total": [regarde(7300), messages(640)],
            "first_day": avant,
            "trophies": trophee,
            "badges": [],
        });
        assert_eq!(e.get("emma", PROFIL).await.json(), avec_entracte);
        for (qui, motif) in [
            ("bruno", "profil non visible"),
            ("chloe", "visible qui n'est pas le booléen vrai"),
            ("personne", "réponse illisible"),
            ("admin", "Estompe en erreur"),
        ] {
            assert_eq!(e.get(qui, PROFIL).await.json(), vide(), "{motif}");
        }
        let appels = estompe.appels();
        let r = e.get("diane", PROFIL).await;
        assert_eq!(r.json(), avec_entracte);
        assert!(!r.texte().contains(DIANE), "{}", r.texte());
        assert_eq!(estompe.appels(), appels + 1);
        let requetes = estompe.requetes();
        assert!(requetes.iter().all(|r| {
            r.methode == "GET"
                && r.chemin == "/internal/profile-visible"
                && r.entete("authorization") == "Bearer jeton-estompe"
                && r.query.ends_with(&format!("&target_user={ID_DIANE}"))
        }));
        assert_eq!(
            requetes[0].query,
            format!("viewer=sub-alice&target_user={ID_DIANE}")
        );
        assert_eq!(
            requetes[appels].query,
            format!("viewer=sub-diane&target_user={ID_DIANE}")
        );
        let appels = estompe.appels();
        for (chemin, motif) in [
            (format!("/profiles/{ID_FRED}"), "visible sans sub"),
            (format!("/profiles/{ID_INCONNU}"), "identifiant inconnu"),
        ] {
            assert_eq!(e.get("emma", &chemin).await.json(), vide(), "{motif}");
        }
        assert_eq!(estompe.appels(), appels + 2);
        for chemin in ["/profiles/sub-diane", "/profiles/0d1a2e00"] {
            let r = e.get("emma", chemin).await;
            assert_eq!(
                (r.statut, r.json()["error"].as_str()),
                (StatusCode::BAD_REQUEST, Some("invalid")),
                "{chemin}"
            );
        }
        assert_eq!(estompe.appels(), appels + 2);
        partager(
            &e,
            json!({"stats": {"messaging.messages": false}, "trophies": {pipelette.to_string(): false}}),
        )
        .await;
        assert_eq!(
            e.get("alice", PROFIL).await.json(),
            json!({"month": [], "total": [], "first_day": avant, "trophies": [], "badges": []})
        );
        assert_eq!(
            e.get("emma", PROFIL).await.json()["month"],
            json!([regarde(7200)])
        );
    }

    #[tokio::test]
    async fn sans_estompe_configure_ou_joignable_le_profil_est_vide() {
        for regler in [
            |c: &mut crate::Config| c.estompe = None,
            |c: &mut crate::Config| c.estompe = Some(("http://127.0.0.1:9".into(), "jeton".into())),
        ] {
            let Some(e) = env_avec(regler).await else {
                return;
            };
            activite_de_diane(&e).await;
            partager(&e, json!({"stats": {"messaging.messages": true}})).await;
            for qui in ["alice", "diane"] {
                assert_eq!(e.get(qui, PROFIL).await.json(), vide(), "{qui}");
            }
        }
    }

    #[tokio::test]
    async fn reglages_de_partage_prives_par_defaut_lus_et_modifies() {
        let Some(e) = env().await else { return };
        let (_, _, pipelette) = activite_de_diane(&e).await;
        let lus = |r: &Value, liste: &str, champ: &str| -> Vec<(String, bool)> {
            r[liste]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| (x[champ].to_string(), x["shared"].as_bool().unwrap()))
                .collect()
        };
        let r = e.get("diane", "/sharing").await;
        assert_eq!(r.statut, StatusCode::OK);
        let r = r.json();
        assert_eq!(r["stats"].as_array().unwrap().len(), 19);
        assert_eq!(
            e.get("alice", "/sharing").await.json()["stats"]
                .as_array()
                .unwrap()
                .len(),
            11
        );
        assert_eq!(
            r["stats"][0],
            json!({"key": "usage.active_seconds", "label": "Temps actif", "domain": "usage", "shared": false})
        );
        assert!(lus(&r, "stats", "key").iter().all(|(_, partage)| !partage));
        for privee in [
            "longest",
            "biggest",
            "busiest",
            "abandoned",
            "intros",
            "sillon",
        ] {
            assert!(!r["stats"].to_string().contains(privee), "{privee}");
        }
        let noms: Vec<&str> = r["trophies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(noms, ["Pipelette", "Premier jour", "Moulin à paroles"]);
        assert!(
            lus(&r, "trophies", "id")
                .iter()
                .all(|(_, partage)| !partage)
        );
        let cinephile: i64 = sqlx::query_scalar("SELECT id FROM trophees WHERE cle = 'cinephile'")
            .fetch_one(e.state.pool())
            .await
            .unwrap();
        for faux in [
            json!({"stats": {"messaging.messages": true, "messaging.inconnue": true}}),
            json!({"stats": {"messaging.messages": true, "messaging.longest_call": true}}),
            json!({"stats": {"messaging.messages": true}, "trophies": {cinephile.to_string(): true}}),
            json!({"stats": {"messaging.messages": true}, "trophies": {"abc": true}}),
            json!({"stats": {"messaging.messages": true}, "autre": {}}),
            json!({"stats": {"messaging.messages": "oui"}}),
        ] {
            let r = e
                .appel(Some("diane"), "PUT", "/sharing", Some(faux.clone()))
                .await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{faux}");
            assert_eq!(r.json()["error"], "invalid");
        }
        let r = e.get("diane", "/sharing").await.json();
        assert!(lus(&r, "stats", "key").iter().all(|(_, partage)| !partage));
        let r = e
            .appel(
                Some("diane"),
                "PUT",
                "/sharing",
                Some(json!({
                    "stats": {"messaging.messages": true, "streak.entracte": true},
                    "trophies": {pipelette.to_string(): true, cinephile.to_string(): false},
                })),
            )
            .await;
        assert_eq!(r.statut, StatusCode::OK);
        let partagees = |r: &Value, liste: &str, champ: &str| -> Vec<String> {
            lus(r, liste, champ)
                .into_iter()
                .filter(|(_, partage)| *partage)
                .map(|(cle, _)| cle)
                .collect()
        };
        assert_eq!(
            partagees(&r.json(), "stats", "key"),
            ["\"messaging.messages\"", "\"streak.entracte\""]
        );
        assert_eq!(
            partagees(&r.json(), "trophies", "id"),
            [pipelette.to_string()]
        );
        partager(&e, json!({"stats": {"streak.entracte": false}})).await;
        partager(&e, json!({})).await;
        let r = e.get("diane", "/sharing").await.json();
        assert_eq!(partagees(&r, "stats", "key"), ["\"messaging.messages\""]);
        assert_eq!(partagees(&r, "trophies", "id"), [pipelette.to_string()]);
        let r = e.get("alice", "/sharing").await.json();
        assert!(partagees(&r, "stats", "key").is_empty());
        assert_eq!(r["trophies"], json!([]));
    }

    #[tokio::test]
    async fn badges_partages_un_par_un_et_servis_a_qui_a_entracte() {
        let estompe = faux_estompe().await;
        let Some(e) = env_avec_estompe(&estompe).await else {
            return;
        };
        let pool = e.state.pool();
        let date = |j: u32| {
            chrono::NaiveDate::from_ymd_opt(2026, 3, j)
                .unwrap()
                .and_hms_opt(20, 0, 0)
                .unwrap()
                .and_utc()
        };
        let mut tx = pool.begin().await.unwrap();
        for (genre, titre, saison, j) in [
            (GenreBadge::Film, "f1", None, 1),
            (GenreBadge::Saison, "s1", Some(2), 2),
            (GenreBadge::Serie, "s1", None, 3),
            (GenreBadge::Saison, "s1", Some(0), 4),
            (GenreBadge::Film, "f1", None, 5),
        ] {
            badges::enregistrer(&mut tx, DIANE, genre, titre, saison, date(j))
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();
        let r = e.get("diane", "/sharing").await.json();
        assert_eq!(
            r["badges"],
            json!([
                {"key": "series:s1", "kind": "series", "title_id": "s1", "season": null, "obtained_at": date(3), "shared": false},
                {"key": "season:s1:2", "kind": "season", "title_id": "s1", "season": 2, "obtained_at": date(2), "shared": false},
                {"key": "movie:f1", "kind": "movie", "title_id": "f1", "season": null, "obtained_at": date(1), "shared": false},
            ])
        );
        assert_eq!(e.get("alice", "/sharing").await.json()["badges"], json!([]));
        for faux in [
            json!({"badges": {"movie:f2": true}}),
            json!({"badges": {"season:s1:3": true}}),
            json!({"badges": {"movie:f1": true, "inconnu": true}}),
            json!({"badges": {"movie:f1": "oui"}}),
        ] {
            let r = e
                .appel(Some("diane"), "PUT", "/sharing", Some(faux.clone()))
                .await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{faux}");
        }
        assert_eq!(e.get("emma", PROFIL).await.json()["badges"], json!([]));
        partager(
            &e,
            json!({"badges": {"movie:f1": true, "season:s1:2": true}}),
        )
        .await;
        partager(&e, json!({"badges": {"season:s1:2": false}})).await;
        let attendu = json!([{"key": "movie:f1", "kind": "movie", "title_id": "f1", "season": null, "obtained_at": date(1)}]);
        assert_eq!(e.get("emma", PROFIL).await.json()["badges"], attendu);
        assert_eq!(e.get("diane", PROFIL).await.json()["badges"], attendu);
        assert_eq!(
            e.get("alice", PROFIL).await.json()["badges"],
            json!([]),
            "sans entracte:access"
        );
        assert_eq!(
            e.get("bruno", PROFIL).await.json()["badges"],
            json!([]),
            "profil non visible"
        );
    }
}
