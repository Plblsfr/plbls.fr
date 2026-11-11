#![allow(clippy::result_large_err)]

use axum::{
    body::{Body, to_bytes},
    extract::{Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use estompe_core::{
    api::ActiveUser,
    http::{champ_texte, decode, internal, write_err, write_json},
    openapi,
    servers::{self, Params, acces, bd, evenement_serveur, mauvais, param, pool, sans_contenu},
    sniff,
    state::AppState,
};
use serde_json::{Value, json};
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

const OCTETS_MAX: usize = 128 << 10;
const COTE_MAX: u32 = 256;
const PAR_SERVEUR: i64 = 50;

type Doc<'a> = (HttpMethod, &'a str, &'a str, &'a [(&'a str, &'a str)]);
type Requete = Query<Vec<(String, String)>>;

pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    let mut r = OpenApiRouter::new()
        .route("/emojis", get(lister))
        .route("/servers/{id}/emojis", post(creer))
        .route(
            "/servers/{id}/emojis/{eid}",
            patch(renommer).delete(supprimer),
        )
        .route("/emojis/{eid}/image", get(image));
    let doc: [Doc; 5] = [
        (
            HttpMethod::Get,
            "/emojis",
            "Emojis personnalisés de tous les serveurs de l'utilisateur",
            &[("200", "ok")],
        ),
        (
            HttpMethod::Post,
            "/servers/{id}/emojis",
            "Ajouter un emoji (corps = image PNG, JPEG ou GIF de 256 px et 128 Ko au plus ; ?name=2 à 32 caractères a-z 0-9 _)",
            &[
                ("201", "ok (évènement `emoji.updated`)"),
                ("400", "invalid_name ou invalid_image"),
                ("404", "serveur introuvable"),
                ("409", "emoji_taken ou too_many_emojis (50 par serveur)"),
                ("413", "too_large"),
            ],
        ),
        (
            HttpMethod::Patch,
            "/servers/{id}/emojis/{eid}",
            "Renommer un emoji (créateur du serveur ou auteur de l'emoji)",
            &[
                ("204", "ok (évènement `emoji.updated`)"),
                ("400", "invalid_name"),
                ("404", "introuvable ou non modifiable"),
                ("409", "emoji_taken"),
            ],
        ),
        (
            HttpMethod::Delete,
            "/servers/{id}/emojis/{eid}",
            "Supprimer un emoji (créateur du serveur ou auteur de l'emoji)",
            &[
                ("204", "ok (évènement `emoji.updated`)"),
                ("404", "introuvable ou non modifiable"),
            ],
        ),
        (
            HttpMethod::Get,
            "/emojis/{eid}/image",
            "Image d'un emoji (membres du serveur)",
            &[("200", "image"), ("404", "introuvable")],
        ),
    ];
    for (methode, chemin, resume, reponses) in doc {
        r = openapi::documenter(
            r,
            chemin,
            methode,
            openapi::operation("serveurs", resume, reponses, chemin),
        );
    }
    router.merge(r)
}

fn nom_valide(nom: &str) -> bool {
    (2..=32).contains(&nom.len())
        && nom
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn nom_invalide() -> Response {
    write_err(
        StatusCode::BAD_REQUEST,
        "invalid_name",
        "nom : 2 à 32 caractères parmi a-z, 0-9 et _",
    )
}

fn pris() -> Response {
    write_err(
        StatusCode::CONFLICT,
        "emoji_taken",
        "ce nom d'emoji existe déjà dans ce serveur",
    )
}

fn introuvable() -> Response {
    write_err(StatusCode::NOT_FOUND, "not_found", "introuvable")
}

fn non_modifiable() -> Response {
    write_err(
        StatusCode::NOT_FOUND,
        "not_found",
        "emoji introuvable ou non modifiable par vous",
    )
}

fn u16_be(o: &[u8], i: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes([*o.get(i)?, *o.get(i + 1)?])))
}

fn dimensions(octets: &[u8], mime: &str) -> Option<(u32, u32)> {
    let (l, h) = match mime {
        "image/png" => {
            if octets.get(12..16)? != b"IHDR" {
                return None;
            }
            let u32_be = |i: usize| -> Option<u32> {
                Some(u32::from_be_bytes(octets.get(i..i + 4)?.try_into().ok()?))
            };
            if estompe_core::sniff::crc32(octets.get(12..29)?) != u32_be(29)? {
                return None;
            }
            (u32_be(16)?, u32_be(20)?)
        }
        "image/gif" => {
            let u16_le = |i: usize| -> Option<u32> {
                Some(u32::from(u16::from_le_bytes([
                    *octets.get(i)?,
                    *octets.get(i + 1)?,
                ])))
            };
            (u16_le(6)?, u16_le(8)?)
        }
        "image/jpeg" => {
            let mut i = 2;
            loop {
                while *octets.get(i)? != 0xFF {
                    i += 1;
                }
                while *octets.get(i)? == 0xFF {
                    i += 1;
                }
                let marqueur = *octets.get(i)?;
                i += 1;
                match marqueur {
                    0x00 | 0x01 | 0xD0..=0xD8 => {}
                    0xC0..=0xC2 => break (u16_be(octets, i + 5)?, u16_be(octets, i + 3)?),
                    0xD9 | 0xDA => return None,
                    _ => i += usize::try_from(u16_be(octets, i)?).ok()?,
                }
            }
        }
        _ => return None,
    };
    (l > 0 && h > 0).then_some((l, h))
}

async fn lister(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
) -> Result<Response, Response> {
    let lignes: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT e.id::text, e.server_id::text, e.name, e.created_by::text FROM custom_emojis e
         JOIN server_members m ON m.server_id = e.server_id WHERE m.user_id=$1::uuid ORDER BY lower(e.name)",
    )
    .bind(&u.id)
    .fetch_all(pool(&state)?)
    .await
    .map_err(bd)?;
    let liste: Vec<Value> = lignes
        .into_iter()
        .map(|(id, server_id, name, created_by)| {
            json!({ "id": id, "server_id": server_id, "name": name, "created_by": created_by })
        })
        .collect();
    Ok(write_json(StatusCode::OK, &Value::Array(liste)))
}

async fn creer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    Query(q): Requete,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), 0).await?;
    let nom = q.iter().find(|(k, _)| k == "name").map_or("", |(_, v)| v);
    if !nom_valide(nom) {
        return Ok(nom_invalide());
    }
    let Ok(octets) = to_bytes(body, OCTETS_MAX).await else {
        return Ok(write_err(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too_large",
            "image trop lourde (128 Ko maximum)",
        ));
    };
    let mime = sniff::detecter(&octets);
    let valide = matches!(mime, "image/png" | "image/jpeg" | "image/gif")
        && dimensions(&octets, mime).is_some_and(|(l, h)| l <= COTE_MAX && h <= COTE_MAX);
    if !valide {
        return Ok(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_image",
            "image PNG, JPEG ou GIF de 256 px au plus attendue",
        ));
    }
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM custom_emojis WHERE server_id=$1::uuid")
        .bind(&sv.id)
        .fetch_one(pool)
        .await
        .map_err(bd)?;
    if n >= PAR_SERVEUR {
        return Ok(write_err(
            StatusCode::CONFLICT,
            "too_many_emojis",
            "50 emojis au maximum par serveur",
        ));
    }
    let id = match sqlx::query_scalar::<_, String>(
        "INSERT INTO custom_emojis (server_id, name, mime, data, created_by) VALUES ($1::uuid,$2,$3,$4,$5::uuid) RETURNING id::text",
    )
    .bind(&sv.id)
    .bind(nom)
    .bind(mime)
    .bind(&octets[..])
    .bind(&u.id)
    .fetch_one(pool)
    .await
    {
        Ok(id) => id,
        Err(e) if servers::unique(&e) => return Ok(pris()),
        Err(e) => return Err(bd(e)),
    };
    evenement_serveur(
        &state,
        pool,
        &sv.id,
        "emoji.updated",
        &json!({ "server_id": sv.id }),
    )
    .await;
    Ok(write_json(
        StatusCode::CREATED,
        &json!({ "id": id, "server_id": sv.id, "name": nom, "created_by": u.id }),
    ))
}

async fn renommer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    body: Body,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), 0).await?;
    let eid = param(&p, "eid");
    if !servers::uuid_valide(eid) {
        return Ok(introuvable());
    }
    let entree = decode(body).await?;
    let nom = champ_texte(&entree, "name").ok_or_else(mauvais)?;
    if !nom_valide(&nom) {
        return Ok(nom_invalide());
    }
    let r = sqlx::query(
        "UPDATE custom_emojis e SET name=$4 FROM servers sv WHERE e.id=$1::uuid AND e.server_id=$2::uuid
         AND sv.id = e.server_id AND (sv.owner_id=$3::uuid OR e.created_by=$3::uuid)",
    )
    .bind(eid)
    .bind(&sv.id)
    .bind(&u.id)
    .bind(&nom)
    .execute(pool)
    .await;
    match r {
        Err(e) if servers::unique(&e) => return Ok(pris()),
        Err(e) => return Err(bd(e)),
        Ok(r) if r.rows_affected() == 0 => return Ok(non_modifiable()),
        Ok(_) => {}
    }
    evenement_serveur(
        &state,
        pool,
        &sv.id,
        "emoji.updated",
        &json!({ "server_id": sv.id }),
    )
    .await;
    Ok(sans_contenu())
}

async fn supprimer(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let pool = pool(&state)?;
    let (sv, _) = acces(&state, &u, param(&p, "id"), 0).await?;
    let eid = param(&p, "eid");
    if !servers::uuid_valide(eid) {
        return Ok(introuvable());
    }
    let r = sqlx::query(
        "DELETE FROM custom_emojis e USING servers sv WHERE e.id=$1::uuid AND e.server_id=$2::uuid
         AND sv.id = e.server_id AND (sv.owner_id=$3::uuid OR e.created_by=$3::uuid)",
    )
    .bind(eid)
    .bind(&sv.id)
    .bind(&u.id)
    .execute(pool)
    .await
    .map_err(bd)?;
    if r.rows_affected() == 0 {
        return Ok(non_modifiable());
    }
    evenement_serveur(
        &state,
        pool,
        &sv.id,
        "emoji.updated",
        &json!({ "server_id": sv.id }),
    )
    .await;
    Ok(sans_contenu())
}

async fn image(
    State(state): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Result<Response, Response> {
    let eid = param(&p, "eid");
    if !servers::uuid_valide(eid) {
        return Ok(introuvable());
    }
    let Some((donnees, mime)) = sqlx::query_as::<_, (Vec<u8>, String)>(
        "SELECT e.data, e.mime FROM custom_emojis e JOIN server_members m ON m.server_id = e.server_id
         WHERE e.id=$1::uuid AND m.user_id=$2::uuid",
    )
    .bind(eid)
    .bind(&u.id)
    .fetch_optional(pool(&state)?)
    .await
    .map_err(bd)?
    else {
        return Ok(introuvable());
    };
    let Ok(type_contenu) = HeaderValue::from_str(&mime) else {
        return Ok(internal());
    };
    let longueur = HeaderValue::from(donnees.len());
    let mut r = donnees.into_response();
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, type_contenu);
    h.insert(header::CONTENT_LENGTH, longueur);
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400, immutable"),
    );
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::{PAR_SERVEUR, dimensions, mount};
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{HeaderMap, Request, StatusCode, header},
    };
    use estompe_core::{
        db::migrate_embarquees,
        identity::Cred,
        state::AppState,
        testutil::{self, BasePostgres, Faux, faux, faux_aegis},
    };
    use serde_json::{Value, json};
    use tower::ServiceExt;
    use utoipa_axum::router::OpenApiRouter;

    struct Env {
        app: Router,
        state: AppState,
        pool: sqlx::PgPool,
        serveur: String,
        _faux: (Faux, Faux),
        _pg: BasePostgres,
    }

    async fn env() -> Option<Env> {
        let gateway = faux(|r| {
            let nom = r.entete("authorization").replace("Bearer ", "");
            (
                200,
                json!({ "id": format!("sub-{nom}"), "name": nom })
                    .to_string()
                    .into_bytes(),
            )
        })
        .await;
        let permission = faux_aegis(|_, _| true).await;
        let (pg, url) = testutil::postgres().await?;
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        migrate_embarquees(&pool).await.unwrap();
        for nom in ["alice", "bob", "carol"] {
            sqlx::query("INSERT INTO users (keycloak_sub, pseudo) VALUES ($1, $2)")
                .bind(format!("sub-{nom}"))
                .bind(nom)
                .execute(&pool)
                .await
                .unwrap();
        }
        let serveur: String = sqlx::query_scalar(
            "INSERT INTO servers (name, owner_id) SELECT 'S', id FROM users WHERE pseudo='alice' RETURNING id::text",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO server_members (server_id, user_id) SELECT $1::uuid, id FROM users WHERE pseudo IN ('alice','bob')",
        )
        .bind(&serveur)
        .execute(&pool)
        .await
        .unwrap();
        let state = AppState::new(
            testutil::config_avec(&[
                ("GATEWAY_URL", gateway.url.as_str()),
                ("AEGIS_URL", permission.url.as_str()),
            ]),
            vec![],
        );
        state.set_pool(pool.clone());
        let app = Router::from(mount(OpenApiRouter::new())).with_state(state.clone());
        Some(Env {
            app,
            state,
            pool,
            serveur,
            _faux: (gateway, permission),
            _pg: pg,
        })
    }

    async fn appel(
        e: &Env,
        nom: &str,
        methode: &str,
        chemin: &str,
        corps: Vec<u8>,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let rep = e
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(methode)
                    .uri(chemin)
                    .header(header::AUTHORIZATION, format!("Bearer {nom}"))
                    .body(Body::from(corps))
                    .unwrap(),
            )
            .await
            .unwrap();
        let (s, h) = (rep.status(), rep.headers().clone());
        let c = to_bytes(rep.into_body(), 1 << 20).await.unwrap();
        (s, h, c.to_vec())
    }

    async fn json_de(
        e: &Env,
        nom: &str,
        methode: &str,
        chemin: &str,
        corps: Vec<u8>,
    ) -> (u16, Value) {
        let (s, _, c) = appel(e, nom, methode, chemin, corps).await;
        (
            s.as_u16(),
            serde_json::from_slice(&c).unwrap_or(Value::Null),
        )
    }

    fn png(l: u32, h: u32) -> Vec<u8> {
        let mut o = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        o.extend(l.to_be_bytes());
        o.extend(h.to_be_bytes());
        o.extend([8, 6, 0, 0, 0]);
        o.extend(estompe_core::sniff::crc32(&o[12..]).to_be_bytes());
        o
    }

    fn gif(l: u16, h: u16) -> Vec<u8> {
        let mut o = b"GIF89a".to_vec();
        o.extend(l.to_le_bytes());
        o.extend(h.to_le_bytes());
        o.extend([0, 0, 0, b';']);
        o
    }

    fn jpeg(l: u16, h: u16) -> Vec<u8> {
        let mut o = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0, 0xFF, 0xC0, 0, 17, 8];
        o.extend(h.to_be_bytes());
        o.extend(l.to_be_bytes());
        o.extend([0; 12]);
        o
    }

    #[test]
    fn dimensions_des_trois_formats() {
        assert_eq!(dimensions(&png(32, 16), "image/png"), Some((32, 16)));
        assert_eq!(dimensions(&gif(7, 9), "image/gif"), Some((7, 9)));
        assert_eq!(dimensions(&jpeg(300, 20), "image/jpeg"), Some((300, 20)));
        assert_eq!(dimensions(&png(0, 16), "image/png"), None);
        assert_eq!(dimensions(&png(1, 1)[..18], "image/png"), None);
        let mut mauvais_crc = png(8, 8);
        mauvais_crc[29] ^= 1;
        assert_eq!(dimensions(&mauvais_crc, "image/png"), None);
        assert_eq!(dimensions(&jpeg(5, 5)[..12], "image/jpeg"), None);
        assert_eq!(dimensions(b"\xFF\xD8\xFF\xDA\0\x02", "image/jpeg"), None);
        assert_eq!(dimensions(b"x", "text/plain"), None);
    }

    #[tokio::test]
    async fn parcours_complet() {
        let Some(e) = env().await else { return };
        let alice: String = sqlx::query_scalar("SELECT id::text FROM users WHERE pseudo='alice'")
            .fetch_one(&e.pool)
            .await
            .unwrap();
        let (_c, mut rx, _) = e.state.hub().register(
            &alice,
            Cred {
                value: "a".into(),
                cookie: false,
            },
        );
        let base = format!("/servers/{}/emojis", e.serveur);
        let img = png(32, 32);

        let (s, v) = json_de(
            &e,
            "bob",
            "POST",
            &format!("{base}?name=chat_ok"),
            img.clone(),
        )
        .await;
        assert_eq!(
            (s, v["name"].as_str(), v["server_id"].as_str()),
            (201, Some("chat_ok"), Some(e.serveur.as_str()))
        );
        let eid = v["id"].as_str().unwrap().to_string();
        let msg: Value = serde_json::from_str(&rx.recv().await.unwrap()).unwrap();
        assert_eq!(msg["type"], "emoji.updated");
        assert_eq!(msg["data"]["server_id"], e.serveur.as_str());

        let un = format!("{base}/{eid}");
        let nom = |n: &str| json!({ "name": n }).to_string().into_bytes();
        assert_eq!(
            json_de(&e, "bob", "PATCH", &un, nom("chat_ko")).await.0,
            204
        );
        assert!(rx.recv().await.unwrap().contains("emoji.updated"));
        assert_eq!(json_de(&e, "alice", "PATCH", &un, nom("Chat")).await.0, 400);
        assert_eq!(
            json_de(&e, "alice", "PATCH", &un, b"{".to_vec()).await.0,
            400
        );
        assert_eq!(
            json_de(&e, "carol", "PATCH", &un, nom("intrus")).await.0,
            404
        );
        assert_eq!(
            json_de(&e, "alice", "PATCH", &format!("{base}/x"), nom("ab"))
                .await
                .0,
            404
        );
        assert_eq!(
            json_de(&e, "bob", "PATCH", &un, nom("chat_ok")).await.0,
            204
        );
        rx.recv().await.unwrap();

        assert_eq!(
            json_de(
                &e,
                "alice",
                "POST",
                &format!("{base}?name=CHAT_OK"),
                img.clone()
            )
            .await
            .0,
            400
        );
        let (s, v) = json_de(
            &e,
            "alice",
            "POST",
            &format!("{base}?name=chat_ok"),
            img.clone(),
        )
        .await;
        assert_eq!((s, v["error"].as_str()), (409, Some("emoji_taken")));
        let (s, v) = json_de(
            &e,
            "alice",
            "POST",
            &format!("{base}?name=Chat_OK"),
            img.clone(),
        )
        .await;
        assert_eq!((s, v["error"].as_str()), (400, Some("invalid_name")));
        let (_, _, c) = appel(&e, "alice", "POST", &format!("{base}?name=ab"), gif(3, 3)).await;
        assert!(String::from_utf8_lossy(&c).contains("\"id\""));
        assert_eq!(
            json_de(
                &e,
                "carol",
                "POST",
                &format!("{base}?name=intrus"),
                img.clone()
            )
            .await
            .0,
            404
        );

        let (s, v) = json_de(&e, "alice", "GET", "/emojis", vec![]).await;
        let noms: Vec<_> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["name"].as_str().unwrap())
            .collect();
        assert_eq!((s, noms), (200, vec!["ab", "chat_ok"]));
        assert_eq!(
            json_de(&e, "carol", "GET", "/emojis", vec![]).await.1,
            json!([])
        );

        let (s, h, c) = appel(&e, "alice", "GET", &format!("/emojis/{eid}/image"), vec![]).await;
        assert_eq!((s, c), (StatusCode::OK, img));
        assert_eq!(h[header::CONTENT_TYPE], "image/png");
        assert_eq!(h[header::CONTENT_LENGTH], "33");
        assert_eq!(
            h[header::CACHE_CONTROL],
            "private, max-age=86400, immutable"
        );
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(
            appel(&e, "carol", "GET", &format!("/emojis/{eid}/image"), vec![])
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            appel(&e, "alice", "GET", "/emojis/zzz/image", vec![])
                .await
                .0,
            StatusCode::NOT_FOUND
        );

        assert_eq!(json_de(&e, "carol", "DELETE", &un, vec![]).await.0, 404);
        assert_eq!(json_de(&e, "alice", "DELETE", &un, vec![]).await.0, 204);
        assert!(rx.recv().await.unwrap().contains("emoji.updated"));
        assert_eq!(json_de(&e, "alice", "DELETE", &un, vec![]).await.0, 404);
    }

    #[tokio::test]
    async fn validation_de_l_image() {
        let Some(e) = env().await else { return };
        let chemin = format!("/servers/{}/emojis?name=img", e.serveur);
        let code = |corps: Vec<u8>| {
            let (e, chemin) = (&e, &chemin);
            async move { json_de(e, "bob", "POST", chemin, corps).await }
        };
        assert_eq!(code(png(300, 300)).await.0, 400);
        assert_eq!(code(png(256, 257)).await.0, 400);
        let mut mauvais_crc = png(8, 8);
        mauvais_crc[29] ^= 1;
        assert_eq!(code(mauvais_crc).await.0, 400);
        assert_eq!(code(gif(300, 1)).await.0, 400);
        assert_eq!(code(jpeg(1, 300)).await.0, 400);
        assert_eq!(code(b"<svg/>".to_vec()).await.0, 400);
        assert_eq!(code(vec![]).await.0, 400);
        let mut gros = png(8, 8);
        gros.resize(128 << 10, 0);
        assert_eq!(code(gros.clone()).await.0, 201);
        gros.push(0);
        let (s, v) = json_de(&e, "bob", "POST", &chemin.replace("img", "gros"), gros).await;
        assert_eq!((s, v["error"].as_str()), (413, Some("too_large")));
        assert_eq!(
            json_de(
                &e,
                "bob",
                "POST",
                &chemin.replace("img", "jp"),
                jpeg(256, 256)
            )
            .await
            .0,
            201
        );
        let (s, h, _) = appel(&e, "bob", "GET", "/emojis", vec![]).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(h[header::CONTENT_TYPE], "application/json");
    }

    #[tokio::test]
    async fn plafond_par_serveur() {
        let Some(e) = env().await else { return };
        for i in 0..PAR_SERVEUR {
            sqlx::query(
                "INSERT INTO custom_emojis (server_id, name, mime, data, created_by)
                 SELECT $1::uuid, $2, 'image/png', ''::bytea, id FROM users WHERE pseudo='bob'",
            )
            .bind(&e.serveur)
            .bind(format!("e{i}"))
            .execute(&e.pool)
            .await
            .unwrap();
        }
        let chemin = format!("/servers/{}/emojis?name=de_trop", e.serveur);
        let (s, v) = json_de(&e, "bob", "POST", &chemin, png(8, 8)).await;
        assert_eq!((s, v["error"].as_str()), (409, Some("too_many_emojis")));
    }
}
