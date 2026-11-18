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
}

pub async fn modifier(
    State(state): State<AppState>,
    p: Personne,
    octets: Bytes,
) -> Result<Response, Response> {
    let (pool, sub) = (state.pool(), p.sub());
    let m: Modification = corps(
        &octets,
        "corps : stats {clé: booléen}, trophies {identifiant: booléen}",
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
    let mut tx = pool.begin().await.map_err(bd)?;
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
    })
}

pub async fn profil_visible(state: &AppState, lecteur: &str, cible: &str) -> bool {
    let Some((url, jeton)) = &state.config().estompe else {
        return false;
    };
    let reponse = state
        .http()
        .get(format!("{url}/internal/profile-visible"))
        .query(&[("viewer", lecteur), ("target", cible)])
        .bearer_auth(jeton)
        .timeout(DELAI_ESTOMPE)
        .send()
        .await;
    match reponse {
        Ok(r) if r.status().is_success() => r
            .json::<Value>()
            .await
            .is_ok_and(|v| v["visible"] == json!(true)),
        _ => false,
    }
}

pub async fn voir_profil(
    State(state): State<AppState>,
    p: Personne,
    Path(cible): Path<String>,
) -> Result<Response, Response> {
    if cible != p.sub() && !profil_visible(&state, p.sub(), &cible).await {
        return Ok(ok(&Profil::default()));
    }
    let aujourd_hui = state.aujourd_hui().await.map_err(bd)?;
    Ok(ok(&profil(state.pool(), &cible, &p.droits, aujourd_hui)
        .await
        .map_err(bd)?))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use chrono::Days;

    use super::*;
    use crate::{
        domaines::{entracte, messagerie},
        testutil::{Env, Faux, env, env_avec, faux},
    };

    const DIANE: &str = "sub-diane";
    const PROFIL: &str = "/profiles/sub-diane";

    async fn faux_estompe() -> Faux {
        faux(|r| {
            let lecteur = |sub: &str| r.query.contains(&format!("viewer={sub}&"));
            let autorise = r.chemin == "/internal/profile-visible"
                && r.entete("authorization") == "Bearer jeton-estompe";
            if !autorise || lecteur("sub-admin") {
                return (500, Vec::new());
            }
            if lecteur("sub-personne") {
                return (200, b"oui".to_vec());
            }
            let visible = if lecteur("sub-bruno") {
                json!(false)
            } else if lecteur("sub-chloe") {
                json!("true")
            } else {
                json!(true)
            };
            (200, json!({ "visible": visible }).to_string().into_bytes())
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
        json!({"month": [], "total": [], "trophies": []})
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
        assert_eq!(e.get("diane", PROFIL).await.json(), avec_entracte);
        assert_eq!(estompe.appels(), appels);
        let requetes = estompe.requetes();
        assert!(requetes.iter().all(|r| {
            r.methode == "GET"
                && r.chemin == "/internal/profile-visible"
                && r.entete("authorization") == "Bearer jeton-estompe"
                && r.query.ends_with("&target=sub-diane")
        }));
        assert_eq!(requetes[0].query, "viewer=sub-alice&target=sub-diane");
        partager(
            &e,
            json!({"stats": {"messaging.messages": false}, "trophies": {pipelette.to_string(): false}}),
        )
        .await;
        assert_eq!(
            e.get("alice", PROFIL).await.json(),
            json!({"month": [], "total": [], "first_day": avant, "trophies": []})
        );
        assert_eq!(
            e.get("emma", PROFIL).await.json()["month"],
            json!([regarde(7200)])
        );
        assert_eq!(e.get("emma", "/profiles/sub-inconnu").await.json(), vide());
    }

    #[tokio::test]
    async fn sans_estompe_configure_ou_joignable_le_profil_d_un_autre_est_vide() {
        for regler in [
            |_: &mut crate::Config| {},
            |c: &mut crate::Config| c.estompe = Some(("http://127.0.0.1:9".into(), "jeton".into())),
        ] {
            let Some(e) = env_avec(regler).await else {
                return;
            };
            activite_de_diane(&e).await;
            partager(&e, json!({"stats": {"messaging.messages": true}})).await;
            assert_eq!(e.get("alice", PROFIL).await.json(), vide());
            assert_eq!(
                e.get("diane", PROFIL).await.json()["total"][0]["value"],
                640
            );
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
}
