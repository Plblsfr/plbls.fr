use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, NaiveDate, Utc};
use futures_util::stream;
use serde_json::{Value, json};
use sqlx::{PgPool, types::Json};

use crate::{
    etat::AppState,
    garde::Personne,
    http::{bd, interne},
    journal,
};

pub const PAGE: i64 = 1000;

pub fn sections() -> &'static [(&'static str, &'static str)] {
    &[
        ("evenements", "/events"),
        ("agregats_jour", "/aggregates"),
        ("battements", "/last_heartbeat"),
        ("sessions_entracte", "/entracte_session"),
        ("deblocages", "/trophies"),
        ("a_evaluer", "/pending_evaluation"),
        ("recaps", "/recaps"),
        ("partages_stats", "/sharing/stats"),
        ("partages_trophees", "/sharing/trophies"),
        ("badges", "/badges"),
        ("personnes", "/name"),
    ]
}

type Session = (NaiveDate, DateTime<Utc>, DateTime<Utc>, i64, i64);

async fn tableau(pool: &PgPool, sub: &str, sql: &'static str) -> Result<Value, sqlx::Error> {
    let Json(valeur): Json<Value> = sqlx::query_scalar(sql).bind(sub).fetch_one(pool).await?;
    Ok(valeur)
}

pub async fn tete(
    pool: &PgPool,
    sub: &str,
    maintenant: DateTime<Utc>,
) -> Result<Value, sqlx::Error> {
    let personne: Option<(String, DateTime<Utc>)> =
        sqlx::query_as("SELECT nom, vu_le FROM personnes WHERE sub = $1")
            .bind(sub)
            .fetch_optional(pool)
            .await?;
    let battement: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT dernier FROM battements WHERE sub = $1")
            .bind(sub)
            .fetch_optional(pool)
            .await?;
    let marque: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT marque_le FROM a_evaluer WHERE sub = $1")
            .bind(sub)
            .fetch_optional(pool)
            .await?;
    let session: Option<Session> = sqlx::query_as(
        "SELECT jour, debut, dernier, secondes, episodes FROM sessions_entracte WHERE sub = $1",
    )
    .bind(sub)
    .fetch_optional(pool)
    .await?;
    Ok(json!({
        "exported_at": maintenant,
        "sub": sub,
        "name": personne.as_ref().map(|p| &p.0),
        "last_seen": personne.as_ref().map(|p| p.1),
        "last_heartbeat": battement,
        "pending_evaluation": marque,
        "entracte_session": session.map(|(jour, debut, dernier, secondes, episodes)| json!({
            "day": jour,
            "started_at": debut,
            "last_at": dernier,
            "seconds": secondes,
            "episodes": episodes,
        })),
        "aggregates": tableau(pool, sub,
            "SELECT COALESCE(jsonb_agg(jsonb_build_object(
                'day', jour, 'stat', stat, 'key', cle, 'value', valeur
            ) ORDER BY jour, stat, cle), '[]'::jsonb)
            FROM agregats_jour WHERE sub = $1",
        ).await?,
        "trophies": tableau(pool, sub,
            "SELECT COALESCE(jsonb_agg(jsonb_build_object(
                'key', t.cle, 'name', t.nom, 'tier', d.palier, 'unlocked_at', d.date_deblocage
            ) ORDER BY d.date_deblocage, t.ordre, t.id, d.palier), '[]'::jsonb)
            FROM deblocages d JOIN trophees t ON t.id = d.trophee WHERE d.sub = $1",
        ).await?,
        "recaps": tableau(pool, sub,
            "SELECT COALESCE(jsonb_agg(contenu || jsonb_build_object('id', id) ORDER BY id), '[]'::jsonb)
            FROM recaps WHERE sub = $1",
        ).await?,
        "sharing": {
            "stats": tableau(pool, sub,
                "SELECT COALESCE(jsonb_agg(stat ORDER BY stat), '[]'::jsonb)
                FROM partages_stats WHERE sub = $1",
            ).await?,
            "trophies": tableau(pool, sub,
                "SELECT COALESCE(jsonb_agg(t.cle ORDER BY t.cle), '[]'::jsonb)
                FROM partages_trophees p JOIN trophees t ON t.id = p.trophee WHERE p.sub = $1",
            ).await?,
        },
        "badges": tableau(pool, sub,
            "SELECT COALESCE(jsonb_agg(jsonb_build_object(
                'kind', genre, 'title_id', titre, 'season', NULLIF(saison, 0), 'obtained_at', obtenu_le, 'shared', partage
            ) ORDER BY obtenu_le, genre, titre, saison), '[]'::jsonb)
            FROM badges WHERE sub = $1",
        ).await?,
        "views": journal::vues(pool, sub, i64::MAX).await?,
    }))
}

type Curseur = Option<(DateTime<Utc>, String)>;
type Brut = (String, String, String, DateTime<Utc>, bool, Json<Value>);

async fn page(
    pool: &PgPool,
    sub: &str,
    apres: &Curseur,
    taille: i64,
) -> Result<Vec<Brut>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, source, type, date, importe, donnees FROM evenements
        WHERE sub = $1 AND ($2::timestamptz IS NULL OR (date, id) > ($2, $3))
        ORDER BY date, id LIMIT $4",
    )
    .bind(sub)
    .bind(apres.as_ref().map(|c| c.0))
    .bind(apres.as_ref().map(|c| c.1.as_str()))
    .bind(taille)
    .fetch_all(pool)
    .await
}

pub fn flux(pool: PgPool, sub: String, tete: &Value, taille: i64) -> Body {
    let mut debut = serde_json::to_vec(tete).unwrap_or_default();
    debut.pop();
    debut.extend_from_slice(b",\"events\":[");
    let depart: Option<(Vec<u8>, Curseur, bool)> = Some((debut, None, true));
    Body::from_stream(stream::unfold(depart, move |etat| {
        let (pool, sub) = (pool.clone(), sub.clone());
        async move {
            let (mut tampon, apres, mut premier) = etat?;
            let lignes = match page(&pool, &sub, &apres, taille).await {
                Ok(lignes) => lignes,
                Err(e) => {
                    tracing::warn!("export interrompu : {e}");
                    return Some((Err(e), None));
                }
            };
            for (id, source, genre, date, importe, Json(donnees)) in &lignes {
                if !premier {
                    tampon.push(b',');
                }
                premier = false;
                let evenement = json!({
                    "id": id,
                    "source": source,
                    "type": genre,
                    "date": date,
                    "imported": importe,
                    "data": donnees,
                });
                tampon.extend(serde_json::to_vec(&evenement).unwrap_or_default());
            }
            let pleine = i64::try_from(lignes.len()).is_ok_and(|n| n >= taille);
            let suite = match lignes.last().filter(|_| pleine) {
                Some(derniere) => {
                    Some((Vec::new(), Some((derniere.3, derniere.0.clone())), premier))
                }
                None => {
                    tampon.extend_from_slice(b"]}");
                    None
                }
            };
            Some((Ok(Bytes::from(tampon)), suite))
        }
    }))
}

pub async fn exporter(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    let pool = state.pool();
    let jour = state.aujourd_hui().await.map_err(bd)?;
    let tete = tete(pool, p.sub(), Utc::now()).await.map_err(bd)?;
    let mut r = flux(pool.clone(), p.sub().to_string(), &tete, PAGE).into_response();
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"sablier-export-{jour}.json\""
        ))
        .map_err(|_| interne())?,
    );
    Ok(r)
}

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, http::StatusCode};
    use commun::evenements::Donnees;

    use super::*;
    use crate::{
        effacement::tables_personnelles,
        testutil::{Env, env},
    };

    fn non_vide(v: &Value) -> bool {
        match v {
            Value::Null => false,
            Value::Array(a) => !a.is_empty(),
            _ => true,
        }
    }

    async fn export_de(e: &Env, qui: &str) -> (String, Value) {
        let r = e.get(qui, "/export").await;
        assert_eq!(r.statut, StatusCode::OK);
        (r.texte(), r.json())
    }

    #[test]
    fn chaque_table_personnelle_a_sa_section_dans_l_export() {
        let mut tables: Vec<&str> = sections().iter().map(|(t, _)| *t).collect();
        tables.sort_unstable();
        let mut attendues = tables_personnelles().to_vec();
        attendues.sort_unstable();
        assert_eq!(tables, attendues);
    }

    #[tokio::test]
    async fn c3_l_export_contient_toutes_les_donnees_de_la_personne_et_aucune_d_une_autre() {
        let Some(e) = env().await else { return };
        e.remplir("alice").await;
        e.remplir("bruno").await;
        e.get("admin", "/admin/people/sub-alice/usage").await;
        e.get("admin", "/admin/people/sub-bruno/usage").await;
        sqlx::query("UPDATE consultations SET admin_nom = 'admin de ' || cible_sub")
            .execute(e.state.pool())
            .await
            .unwrap();
        let lignes = e.lignes_de("sub-alice").await;
        let r = e.get("alice", "/export").await;
        assert_eq!(r.statut, StatusCode::OK);
        assert_eq!(r.entetes["content-type"], "application/json");
        let aujourd_hui = e.state.aujourd_hui().await.unwrap();
        assert_eq!(
            r.entetes["content-disposition"].to_str().unwrap(),
            format!("attachment; filename=\"sablier-export-{aujourd_hui}.json\"")
        );
        let (texte, export) = (r.texte(), r.json());
        assert_eq!(export["sub"], "sub-alice");
        for (table, n) in &lignes {
            assert!(*n > 0, "{table} sans ligne pour alice");
            let pointeur = sections().iter().find(|(t, _)| t == table).unwrap().1;
            let section = export.pointer(pointeur).unwrap_or(&Value::Null);
            assert!(non_vide(section), "{table} absente de l'export : {export}");
            if let Some(liste) = section.as_array() {
                assert_eq!(liste.len(), usize::try_from(*n).unwrap(), "{table}");
            }
        }
        assert_eq!(lignes.len(), tables_personnelles().len());
        for present in [
            "titre-sub-alice",
            "piste-sub-alice",
            "salon-sub-alice",
            "recap-sub-alice",
            "sub-alice-evt-0",
            "admin de sub-alice",
        ] {
            assert!(texte.contains(present), "{present} absent : {texte}");
        }
        for absent in ["bruno", "sub-admin"] {
            assert!(!texte.contains(absent), "{absent} présent : {texte}");
        }
        assert_eq!(
            export["trophies"][0],
            json!({"key": "premier_jour", "name": "Premier jour", "tier": 1, "unlocked_at": "2026-03-10"})
        );
        assert_eq!(export["sharing"]["stats"], json!(["messaging.messages"]));
        assert_eq!(export["sharing"]["trophies"], json!(["premier_jour"]));
        assert_eq!(export["name"], "alice");
        assert_eq!(
            export["events"][0],
            json!({
                "id": "sub-alice-evt-0",
                "source": "estompe",
                "type": "estompe.message",
                "date": "2026-03-10T10:00:00Z",
                "imported": false,
                "data": {"serveur": "serveur-sub-alice", "salon": "salon-sub-alice", "prive": false},
            })
        );
        assert_eq!(
            export["views"].as_array().unwrap().len(),
            1,
            "{}",
            export["views"]
        );
        let (texte, export) = export_de(&e, "bruno").await;
        assert!(!texte.contains("alice"), "{texte}");
        assert_eq!(export["sub"], "sub-bruno");
        let (_, vide) = export_de(&e, "diane").await;
        for (_, pointeur) in sections() {
            assert!(!non_vide(vide.pointer(pointeur).unwrap()), "{pointeur}");
        }
        assert_eq!(vide["views"], json!([]));
        assert_eq!(
            e.get("personne", "/export").await.statut,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn evenements_lus_par_pages_sans_perte_ni_doublon() {
        let Some(e) = env().await else { return };
        let pool = e.state.pool();
        let tete = json!({"sub": "sub-alice"});
        for n in 0..=5_usize {
            if n > 0 {
                let message = Donnees::EstompeMessage {
                    serveur: None,
                    salon: None,
                    prive: true,
                };
                let date = format!("2026-03-10T10:00:0{}Z", n / 2);
                e.recevoir(
                    &format!("evt-{n}"),
                    "sub-alice",
                    &date,
                    false,
                    message.clone(),
                )
                .await;
                e.recevoir(&format!("autre-{n}"), "sub-bruno", &date, false, message)
                    .await;
            }
            for taille in [1, 2, 3, 1000] {
                let corps = flux(pool.clone(), "sub-alice".into(), &tete, taille);
                let octets = to_bytes(corps, usize::MAX).await.unwrap();
                let export: Value = serde_json::from_slice(&octets).unwrap();
                let ids: Vec<&str> = export["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x["id"].as_str().unwrap())
                    .collect();
                let attendus: Vec<String> = (1..=n).map(|i| format!("evt-{i}")).collect();
                assert_eq!(ids, attendus, "{n} évènements par pages de {taille}");
                assert_eq!(export["sub"], "sub-alice");
            }
        }
    }
}
