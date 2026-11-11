#![allow(clippy::result_large_err)]

mod image;

use std::collections::HashMap;

use axum::{
    body::{Body, to_bytes},
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
    routing::{get, put},
};
use estompe_core::{
    api::ActiveUser,
    fichiers::est_admin,
    http::{write_err, write_json},
    openapi,
    roles::MANAGE_SERVER,
    servers::{acces, bd, evenement_serveur, non_trouve, param, pool, sans_contenu, uuid_valide},
    state::AppState,
    users::co_members,
};
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use utoipa::openapi::HttpMethod;
use utoipa_axum::router::OpenApiRouter;

const AVATAR_OCTETS: usize = 256 << 10;
const AVATAR_COTE: u32 = 512;
const FOND_OCTETS: usize = 1 << 20;
const FOND_COTE: u32 = 1920;

type Params = Path<HashMap<String, String>>;
type Resultat = Result<Response, Response>;

fn image_reponse(donnees: Vec<u8>, type_mime: &str) -> Response {
    let mut r = Response::new(Body::from(donnees));
    let h = r.headers_mut();
    if let Ok(v) = HeaderValue::from_str(type_mime) {
        h.insert(header::CONTENT_TYPE, v);
    }
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=86400"),
    );
    r
}

fn introuvable_image() -> Response {
    non_trouve("introuvable")
}

async fn lire_image(
    corps: Body,
    max_octets: usize,
    max_cote: u32,
    poids: &str,
    cote: &str,
) -> Result<(Vec<u8>, &'static str), Response> {
    let Ok(octets) = to_bytes(corps, max_octets).await else {
        return Err(write_err(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too_large",
            &format!("image trop lourde ({poids} maximum)"),
        ));
    };
    match image::analyser(&octets) {
        Some(a) if a.largeur <= max_cote && a.hauteur <= max_cote => {
            Ok((octets.to_vec(), a.type_mime))
        }
        _ => Err(write_err(
            StatusCode::BAD_REQUEST,
            "invalid_image",
            &format!("image PNG ou JPEG de {cote} au plus attendue"),
        )),
    }
}

async fn lire_avatar(corps: Body) -> Result<(Vec<u8>, &'static str), Response> {
    lire_image(corps, AVATAR_OCTETS, AVATAR_COTE, "256 Ko", "512 px").await
}

fn versions(lignes: Vec<(String, i64)>) -> Response {
    let objet: Map<String, Value> = lignes.into_iter().map(|(k, v)| (k, json!(v))).collect();
    write_json(StatusCode::OK, &Value::Object(objet))
}

async fn voisins(pool: &PgPool, utilisateur: &str) -> Result<Vec<String>, sqlx::Error> {
    let mut ids = co_members(pool, utilisateur).await?;
    ids.push(utilisateur.to_string());
    Ok(ids)
}

type LigneImage = Result<Option<(Vec<u8>, Option<String>)>, sqlx::Error>;

async fn image_ou_404(ligne: LigneImage) -> Resultat {
    match ligne.map_err(bd)? {
        Some((donnees, Some(type_mime))) => Ok(image_reponse(donnees, &type_mime)),
        _ => Ok(introuvable_image()),
    }
}

async fn lister(State(etat): State<AppState>, ActiveUser(u): ActiveUser) -> Resultat {
    let pool = pool(&etat)?;
    let ids = voisins(pool, &u.id).await.map_err(bd)?;
    let lignes = sqlx::query_as(
        "SELECT id::text, avatar_version FROM users WHERE id = ANY($1::uuid[]) AND avatar IS NOT NULL",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    Ok(versions(lignes))
}

async fn definir_avatar(
    etat: &AppState,
    u: &estompe_core::users::User,
    donnees: Option<(Vec<u8>, &str)>,
) -> Resultat {
    let pool = pool(etat)?;
    let supprime = donnees.is_none();
    let (octets, type_mime) = donnees.unzip();
    let version: i64 = sqlx::query_scalar(
        "UPDATE users SET avatar=$2, avatar_type=$3, avatar_version = avatar_version + 1 WHERE id=$1::uuid RETURNING avatar_version",
    )
    .bind(&u.id)
    .bind(octets)
    .bind(type_mime)
    .fetch_one(pool)
    .await
    .map_err(bd)?;
    if let Ok(to) = voisins(pool, &u.id).await {
        let version = if supprime { 0 } else { version };
        etat.publier(
            &to,
            "avatar.updated",
            &json!({ "user_id": u.id, "version": version }),
        );
    }
    Ok(sans_contenu())
}

async fn deposer_avatar(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    corps: Body,
) -> Resultat {
    let (octets, type_mime) = lire_avatar(corps).await?;
    definir_avatar(&etat, &u, Some((octets, type_mime))).await
}

async fn retirer_avatar(State(etat): State<AppState>, ActiveUser(u): ActiveUser) -> Resultat {
    definir_avatar(&etat, &u, None).await
}

async fn voir_avatar(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Resultat {
    let pool = pool(&etat)?;
    let id = param(&p, "id");
    if !uuid_valide(id) {
        return Ok(introuvable_image());
    }
    let ligne = sqlx::query_as(
        "SELECT avatar, avatar_type FROM users WHERE id=$1::uuid AND avatar IS NOT NULL AND (
            id=$2::uuid
            OR EXISTS (SELECT 1 FROM server_members a JOIN server_members b ON a.server_id=b.server_id WHERE a.user_id=$1::uuid AND b.user_id=$2::uuid)
            OR EXISTS (SELECT 1 FROM dm_conversations d WHERE (d.user_a=$1::uuid AND d.user_b=$2::uuid) OR (d.user_a=$2::uuid AND d.user_b=$1::uuid)))",
    )
    .bind(id)
    .bind(&u.id)
    .fetch_optional(pool)
    .await;
    image_ou_404(ligne).await
}

async fn deposer_fond(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    corps: Body,
) -> Resultat {
    let pool = pool(&etat)?;
    let (octets, type_mime) = lire_image(corps, FOND_OCTETS, FOND_COTE, "1 Mo", "1920 px").await?;
    ecrire_fond(pool, &u.id, Some(octets), Some(type_mime)).await
}

async fn retirer_fond(State(etat): State<AppState>, ActiveUser(u): ActiveUser) -> Resultat {
    ecrire_fond(pool(&etat)?, &u.id, None, None).await
}

async fn ecrire_fond(
    pool: &PgPool,
    user: &str,
    octets: Option<Vec<u8>>,
    type_mime: Option<&str>,
) -> Resultat {
    sqlx::query("UPDATE users SET camera_bg=$2, camera_bg_type=$3 WHERE id=$1::uuid")
        .bind(user)
        .bind(octets)
        .bind(type_mime)
        .execute(pool)
        .await
        .map_err(bd)?;
    Ok(sans_contenu())
}

async fn voir_fond(State(etat): State<AppState>, ActiveUser(u): ActiveUser) -> Resultat {
    let ligne = sqlx::query_as(
        "SELECT camera_bg, camera_bg_type FROM users WHERE id=$1::uuid AND camera_bg IS NOT NULL",
    )
    .bind(&u.id)
    .fetch_optional(pool(&etat)?)
    .await;
    image_ou_404(ligne).await
}

async fn lister_serveur(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Resultat {
    let pool = pool(&etat)?;
    let (sv, _) = acces(&etat, &u, param(&p, "id"), 0).await?;
    let lignes = sqlx::query_as(
        "SELECT user_id::text, version FROM server_avatars WHERE server_id=$1::uuid",
    )
    .bind(&sv.id)
    .fetch_all(pool)
    .await
    .map_err(bd)?;
    Ok(versions(lignes))
}

async fn definir_avatar_serveur(
    etat: &AppState,
    u: &estompe_core::users::User,
    p: &HashMap<String, String>,
    donnees: Option<(Vec<u8>, &str)>,
) -> Resultat {
    let pool = pool(etat)?;
    let (sv, _) = acces(etat, u, param(p, "id"), 0).await?;
    let version: i64 = match donnees {
        None => {
            sqlx::query("DELETE FROM server_avatars WHERE server_id=$1::uuid AND user_id=$2::uuid")
                .bind(&sv.id)
                .bind(&u.id)
                .execute(pool)
                .await
                .map_err(bd)?;
            0
        }
        Some((octets, type_mime)) => sqlx::query_scalar(
            "INSERT INTO server_avatars (server_id, user_id, avatar, avatar_type) VALUES ($1::uuid,$2::uuid,$3,$4)
             ON CONFLICT (server_id, user_id) DO UPDATE SET avatar=$3, avatar_type=$4, version = server_avatars.version + 1
             RETURNING version",
        )
        .bind(&sv.id)
        .bind(&u.id)
        .bind(octets)
        .bind(type_mime)
        .fetch_one(pool)
        .await
        .map_err(bd)?,
    };
    evenement_serveur(
        etat,
        pool,
        &sv.id,
        "server_avatar.updated",
        &json!({ "server_id": sv.id, "user_id": u.id, "version": version }),
    )
    .await;
    Ok(sans_contenu())
}

async fn deposer_avatar_serveur(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    corps: Body,
) -> Resultat {
    let (octets, type_mime) = lire_avatar(corps).await?;
    definir_avatar_serveur(&etat, &u, &p, Some((octets, type_mime))).await
}

async fn retirer_avatar_serveur(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Resultat {
    definir_avatar_serveur(&etat, &u, &p, None).await
}

async fn voir_avatar_serveur(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Resultat {
    let pool = pool(&etat)?;
    let (sv, _) = acces(&etat, &u, param(&p, "id"), 0).await?;
    let uid = param(&p, "uid");
    if !uuid_valide(uid) {
        return Ok(introuvable_image());
    }
    let ligne = sqlx::query_as::<_, (Vec<u8>, String)>(
        "SELECT avatar, avatar_type FROM server_avatars WHERE server_id=$1::uuid AND user_id=$2::uuid",
    )
    .bind(&sv.id)
    .bind(uid)
    .fetch_optional(pool)
    .await
    .map(|l| l.map(|(d, t)| (d, Some(t))));
    image_ou_404(ligne).await
}

async fn gerant_icone(
    etat: &AppState,
    u: &estompe_core::users::User,
    p: &HashMap<String, String>,
) -> Result<String, Response> {
    let (sv, acc) = acces(etat, u, param(p, "id"), 0).await?;
    if !acc.can(MANAGE_SERVER) && !est_admin(etat, &u.sub).await {
        return Err(write_err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "permission manquante",
        ));
    }
    Ok(sv.id)
}

async fn definir_icone(
    etat: &AppState,
    serveur: &str,
    donnees: Option<(Vec<u8>, &str)>,
) -> Resultat {
    let pool = pool(etat)?;
    let (octets, type_mime) = donnees.unzip();
    let version: i64 = sqlx::query_scalar(
        "UPDATE servers SET icon=$2, icon_type=$3,
         icon_version = CASE WHEN $2::bytea IS NULL THEN 0 ELSE icon_version + 1 END
         WHERE id=$1::uuid RETURNING icon_version",
    )
    .bind(serveur)
    .bind(octets)
    .bind(type_mime)
    .fetch_one(pool)
    .await
    .map_err(bd)?;
    evenement_serveur(
        etat,
        pool,
        serveur,
        "server_icon.updated",
        &json!({ "id": serveur, "version": version }),
    )
    .await;
    Ok(sans_contenu())
}

async fn deposer_icone(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
    corps: Body,
) -> Resultat {
    let serveur = gerant_icone(&etat, &u, &p).await?;
    let (octets, type_mime) = lire_avatar(corps).await?;
    definir_icone(&etat, &serveur, Some((octets, type_mime))).await
}

async fn retirer_icone(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Resultat {
    let serveur = gerant_icone(&etat, &u, &p).await?;
    definir_icone(&etat, &serveur, None).await
}

async fn voir_icone(
    State(etat): State<AppState>,
    ActiveUser(u): ActiveUser,
    Path(p): Params,
) -> Resultat {
    let pool = pool(&etat)?;
    let (sv, _) = acces(&etat, &u, param(&p, "id"), 0).await?;
    let ligne = sqlx::query_as(
        "SELECT icon, icon_type FROM servers WHERE id=$1::uuid AND icon IS NOT NULL",
    )
    .bind(&sv.id)
    .fetch_optional(pool)
    .await;
    image_ou_404(ligne).await
}

type Doc = (
    HttpMethod,
    &'static str,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const IMAGE_ECRITE: &[(&str, &str)] = &[
    ("204", "ok"),
    ("400", "invalid_image"),
    ("413", "too_large"),
];
const IMAGE_SERVEUR: &[(&str, &str)] = &[
    ("204", "ok"),
    ("400", "invalid_image"),
    ("404", "serveur introuvable ou non membre"),
    ("413", "too_large"),
];
const IMAGE_LUE: &[(&str, &str)] = &[("200", "image PNG ou JPEG"), ("404", "introuvable")];
const VERSIONS: &[(&str, &str)] = &[("200", "ok")];
const SERVEUR_404: &[(&str, &str)] = &[("204", "ok"), ("404", "serveur introuvable ou non membre")];
const ICONE_ECRITE: &[(&str, &str)] = &[
    ("204", "ok"),
    ("400", "invalid_image"),
    ("403", "permission manquante"),
    ("404", "serveur introuvable ou non membre"),
    ("413", "too_large"),
];
const ICONE_RETIREE: &[(&str, &str)] = &[
    ("204", "ok"),
    ("403", "permission manquante"),
    ("404", "serveur introuvable ou non membre"),
];

const DOC: &[Doc] = &[
    (
        HttpMethod::Get,
        "/avatars",
        "comptes",
        "Versions d'avatar des personnes côtoyées, soi-même compris",
        VERSIONS,
    ),
    (
        HttpMethod::Put,
        "/me/avatar",
        "comptes",
        "Définir son avatar (PNG ou JPEG, 256 Ko et 512 px au plus)",
        IMAGE_ECRITE,
    ),
    (
        HttpMethod::Delete,
        "/me/avatar",
        "comptes",
        "Supprimer son avatar",
        &[("204", "ok")],
    ),
    (
        HttpMethod::Put,
        "/me/camera-background",
        "comptes",
        "Définir son fond de caméra personnalisé (PNG ou JPEG, 1 Mo et 1920 px au plus)",
        IMAGE_ECRITE,
    ),
    (
        HttpMethod::Get,
        "/me/camera-background",
        "comptes",
        "Son fond de caméra personnalisé (visible de soi seul)",
        IMAGE_LUE,
    ),
    (
        HttpMethod::Delete,
        "/me/camera-background",
        "comptes",
        "Supprimer son fond de caméra personnalisé",
        &[("204", "ok")],
    ),
    (
        HttpMethod::Get,
        "/users/{id}/avatar",
        "comptes",
        "Image d'avatar d'une personne côtoyée",
        IMAGE_LUE,
    ),
    (
        HttpMethod::Get,
        "/servers/{id}/avatars",
        "comptes",
        "Versions de la photo propre à ce serveur, pour les membres qui en ont une",
        &[("200", "ok"), ("404", "serveur introuvable ou non membre")],
    ),
    (
        HttpMethod::Put,
        "/servers/{id}/me/avatar",
        "comptes",
        "Définir sa photo de profil pour ce serveur",
        IMAGE_SERVEUR,
    ),
    (
        HttpMethod::Delete,
        "/servers/{id}/me/avatar",
        "comptes",
        "Retirer sa photo de ce serveur",
        SERVEUR_404,
    ),
    (
        HttpMethod::Get,
        "/servers/{id}/users/{uid}/avatar",
        "comptes",
        "Image de la photo propre à ce serveur d'un membre",
        IMAGE_LUE,
    ),
    (
        HttpMethod::Get,
        "/servers/{id}/icon",
        "serveurs",
        "Icône du serveur (réservée aux membres)",
        IMAGE_LUE,
    ),
    (
        HttpMethod::Put,
        "/servers/{id}/icon",
        "serveurs",
        "Définir l'icône du serveur (PNG ou JPEG, 256 Ko et 512 px au plus)",
        ICONE_ECRITE,
    ),
    (
        HttpMethod::Delete,
        "/servers/{id}/icon",
        "serveurs",
        "Retirer l'icône du serveur",
        ICONE_RETIREE,
    ),
];

pub fn mount(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    let mut r = OpenApiRouter::new()
        .route("/avatars", get(lister))
        .route("/me/avatar", put(deposer_avatar).delete(retirer_avatar))
        .route(
            "/me/camera-background",
            put(deposer_fond).get(voir_fond).delete(retirer_fond),
        )
        .route("/users/{id}/avatar", get(voir_avatar))
        .route("/servers/{id}/avatars", get(lister_serveur))
        .route(
            "/servers/{id}/me/avatar",
            put(deposer_avatar_serveur).delete(retirer_avatar_serveur),
        )
        .route("/servers/{id}/users/{uid}/avatar", get(voir_avatar_serveur))
        .route(
            "/servers/{id}/icon",
            put(deposer_icone).get(voir_icone).delete(retirer_icone),
        );
    for (methode, chemin, tag, resume, reponses) in DOC {
        let op = openapi::operation(tag, resume, reponses, chemin);
        r = openapi::documenter(r, chemin, methode.clone(), op);
    }
    router.merge(r)
}

#[cfg(test)]
mod tests {
    use super::{AVATAR_OCTETS, FOND_OCTETS, image, mount};
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{HeaderMap, Request, StatusCode, header},
    };
    use estompe_core::{
        db::migrate_embarquees,
        identity::Cred,
        state::AppState,
        testutil::{self, BasePostgres, Faux, faux},
    };
    use serde_json::{Value, json};
    use tower::ServiceExt;
    use utoipa_axum::router::OpenApiRouter;

    struct Env {
        app: Router,
        state: AppState,
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
        let permission =
            testutil::faux_aegis(|sub, p| !testutil::est_admin(p) || sub == "sub-root").await;
        let (pg, url) = testutil::postgres().await?;
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        migrate_embarquees(&pool).await.unwrap();
        let state = AppState::new(
            testutil::config_avec(&[
                ("GATEWAY_URL", gateway.url.as_str()),
                ("AEGIS_URL", permission.url.as_str()),
            ]),
            vec![],
        );
        state.set_pool(pool);
        let routeur = estompe_core::api::router().merge(mount(OpenApiRouter::new()));
        let app = Router::from(routeur).with_state(state.clone());
        Some(Env {
            app,
            state,
            _faux: (gateway, permission),
            _pg: pg,
        })
    }

    async fn membre(e: &Env, nom: &str) -> String {
        sqlx::query_scalar(
            "INSERT INTO users (keycloak_sub, pseudo) VALUES ($1, $2) RETURNING id::text",
        )
        .bind(format!("sub-{nom}"))
        .bind(nom)
        .fetch_one(e.state.pool().ok().unwrap())
        .await
        .unwrap()
    }

    async fn requete(
        e: &Env,
        qui: &str,
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
                    .header(header::AUTHORIZATION, format!("Bearer {qui}"))
                    .body(Body::from(corps))
                    .unwrap(),
            )
            .await
            .unwrap();
        let (s, h) = (rep.status(), rep.headers().clone());
        let c = to_bytes(rep.into_body(), 4 << 20).await.unwrap();
        (s, h, c.to_vec())
    }

    async fn code(e: &Env, qui: &str, methode: &str, chemin: &str, corps: &[u8]) -> StatusCode {
        requete(e, qui, methode, chemin, corps.to_vec()).await.0
    }

    async fn json_de(e: &Env, qui: &str, chemin: &str) -> Value {
        let (s, _, c) = requete(e, qui, "GET", chemin, vec![]).await;
        assert_eq!(s, StatusCode::OK);
        serde_json::from_slice(&c).unwrap()
    }

    fn crc32(o: &[u8]) -> u32 {
        let mut c = !0u32;
        for &b in o {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 == 1 {
                    (c >> 1) ^ 0xEDB8_8320
                } else {
                    c >> 1
                };
            }
        }
        !c
    }

    fn png(l: u32, h: u32) -> Vec<u8> {
        let mut ihdr = b"IHDR".to_vec();
        ihdr.extend(l.to_be_bytes());
        ihdr.extend(h.to_be_bytes());
        ihdr.extend([8, 6, 0, 0, 0]);
        let mut o = b"\x89PNG\r\n\x1a\n\0\0\0\x0d".to_vec();
        o.extend(&ihdr);
        o.extend(crc32(&ihdr).to_be_bytes());
        o
    }

    fn jpeg(l: u16, h: u16) -> Vec<u8> {
        let mut o = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16];
        o.extend(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        o.extend([0xFF, 0xC0, 0, 11, 8]);
        o.extend(h.to_be_bytes());
        o.extend(l.to_be_bytes());
        o.extend([1, 1, 0x11, 0]);
        o
    }

    #[test]
    fn analyse_des_images() {
        let a = image::analyser(&png(64, 32)).unwrap();
        assert_eq!((a.type_mime, a.largeur, a.hauteur), ("image/png", 64, 32));
        let a = image::analyser(&jpeg(10, 20)).unwrap();
        assert_eq!((a.type_mime, a.largeur, a.hauteur), ("image/jpeg", 10, 20));
        let mut mauvais_crc = png(8, 8);
        *mauvais_crc.last_mut().unwrap() ^= 1;
        assert!(image::analyser(&mauvais_crc).is_none());
        assert!(image::analyser(&png(0, 8)).is_none());
        assert!(image::analyser(&png(8, 8)[..20]).is_none());
        assert!(image::analyser(&jpeg(10, 20)[..30]).is_none());
        let sans_jfif = [
            &[
                0xFF, 0xD8, 0xFF, 0xFE, 0, 2, 0xFF, 0xC0, 0, 11, 8, 0, 20, 0, 10,
            ][..],
            &[1, 1, 0x11, 0],
        ]
        .concat();
        assert!(image::analyser(&sans_jfif).is_none());
        let avec_sos = [&sans_jfif[..], &[0xFF, 0xDA, 0, 2]].concat();
        assert_eq!(
            image::analyser(&avec_sos).map(|a| (a.largeur, a.hauteur)),
            Some((10, 20))
        );
        assert!(image::analyser(b"<svg onload=x>").is_none());
        assert!(image::analyser(b"GIF89a\x01\x00\x01\x00").is_none());
    }

    #[tokio::test]
    async fn avatars_globaux() {
        let Some(e) = env().await else { return };
        let alice = membre(&e, "alice").await;
        let bob = membre(&e, "bob").await;
        membre(&e, "carl").await;
        let (_c, mut rx, _) = e.state.hub().register(
            &bob,
            Cred {
                value: "b".into(),
                cookie: false,
            },
        );
        let sid = {
            let (s, _, c) =
                requete(&e, "alice", "POST", "/servers", br#"{"name":"S"}"#.to_vec()).await;
            assert_eq!(s, StatusCode::CREATED);
            serde_json::from_slice::<Value>(&c).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        };
        sqlx::query("INSERT INTO server_members (server_id, user_id) VALUES ($1::uuid, $2::uuid)")
            .bind(&sid)
            .bind(&bob)
            .execute(e.state.pool().ok().unwrap())
            .await
            .unwrap();

        let img = png(64, 64);
        assert_eq!(
            code(&e, "alice", "PUT", "/me/avatar", &img).await,
            StatusCode::NO_CONTENT
        );
        let ev: Value = serde_json::from_str(&rx.recv().await.unwrap()).unwrap();
        assert_eq!(ev["type"], "avatar.updated");
        assert_eq!(ev["data"], json!({ "user_id": alice, "version": 1 }));

        let (s, h, c) = requete(&e, "bob", "GET", &format!("/users/{alice}/avatar"), vec![]).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(c, img);
        assert_eq!(h[header::CONTENT_TYPE], "image/png");
        assert_eq!(h[header::CACHE_CONTROL], "private, max-age=86400");
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        let (s, _, _) = requete(&e, "carl", "GET", &format!("/users/{alice}/avatar"), vec![]).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        let (s, _, _) = requete(&e, "bob", "GET", "/users/pas-un-uuid/avatar", vec![]).await;
        assert_eq!(s, StatusCode::NOT_FOUND);

        assert_eq!(
            json_de(&e, "bob", "/avatars").await,
            json!({ alice.clone(): 1 })
        );
        assert_eq!(json_de(&e, "carl", "/avatars").await, json!({}));

        assert_eq!(
            code(&e, "alice", "PUT", "/me/avatar", &png(600, 600)).await,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            code(&e, "alice", "PUT", "/me/avatar", b"<svg onload=x>").await,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "PUT",
                "/me/avatar",
                &vec![0; AVATAR_OCTETS + 1]
            )
            .await,
            StatusCode::PAYLOAD_TOO_LARGE
        );
        let jpg = jpeg(100, 100);
        assert_eq!(
            code(&e, "alice", "PUT", "/me/avatar", &jpg).await,
            StatusCode::NO_CONTENT
        );
        let (_, h, c) = requete(
            &e,
            "alice",
            "GET",
            &format!("/users/{alice}/avatar"),
            vec![],
        )
        .await;
        assert_eq!(
            (c, h[header::CONTENT_TYPE].to_str().unwrap()),
            (jpg, "image/jpeg")
        );
        assert_eq!(
            json_de(&e, "bob", "/avatars").await,
            json!({ alice.clone(): 2 })
        );

        assert_eq!(
            code(&e, "alice", "DELETE", "/me/avatar", b"").await,
            StatusCode::NO_CONTENT
        );
        let mut dernier = Value::Null;
        while let Ok(m) = rx.try_recv() {
            dernier = serde_json::from_str(&m).unwrap();
        }
        assert_eq!(dernier["data"]["version"], 0);
        let (s, _, _) = requete(&e, "bob", "GET", &format!("/users/{alice}/avatar"), vec![]).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn fond_de_camera() {
        let Some(e) = env().await else { return };
        membre(&e, "alice").await;
        membre(&e, "bob").await;
        assert_eq!(
            code(&e, "alice", "GET", "/me/camera-background", b"").await,
            StatusCode::NOT_FOUND
        );
        let img = png(1280, 720);
        assert_eq!(
            code(&e, "alice", "PUT", "/me/camera-background", &img).await,
            StatusCode::NO_CONTENT
        );
        let (s, h, c) = requete(&e, "alice", "GET", "/me/camera-background", vec![]).await;
        assert_eq!(
            (s, c, h[header::CONTENT_TYPE].to_str().unwrap()),
            (StatusCode::OK, img, "image/png")
        );
        assert_eq!(
            code(&e, "bob", "GET", "/me/camera-background", b"").await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            code(&e, "alice", "PUT", "/me/camera-background", &png(2000, 100)).await,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "PUT",
                "/me/camera-background",
                b"<svg onload=x>"
            )
            .await,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "PUT",
                "/me/camera-background",
                &vec![0; FOND_OCTETS + 1]
            )
            .await,
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(
            code(&e, "alice", "DELETE", "/me/camera-background", b"").await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            code(&e, "alice", "GET", "/me/camera-background", b"").await,
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn avatars_de_serveur_et_icone() {
        let Some(e) = env().await else { return };
        let alice = membre(&e, "alice").await;
        let bob = membre(&e, "bob").await;
        membre(&e, "carl").await;
        let (_c, mut rx, _) = e.state.hub().register(
            &bob,
            Cred {
                value: "b".into(),
                cookie: false,
            },
        );
        let (_, _, c) = requete(&e, "alice", "POST", "/servers", br#"{"name":"S"}"#.to_vec()).await;
        let sid = serde_json::from_slice::<Value>(&c).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        sqlx::query("INSERT INTO server_members (server_id, user_id) VALUES ($1::uuid, $2::uuid)")
            .bind(&sid)
            .bind(&bob)
            .execute(e.state.pool().ok().unwrap())
            .await
            .unwrap();
        let base = format!("/servers/{sid}");

        let img = png(32, 32);
        assert_eq!(
            code(&e, "alice", "PUT", &format!("{base}/me/avatar"), &img).await,
            StatusCode::NO_CONTENT
        );
        let ev: Value = serde_json::from_str(&rx.recv().await.unwrap()).unwrap();
        assert_eq!(ev["type"], "server_avatar.updated");
        assert_eq!(
            ev["data"],
            json!({ "server_id": sid, "user_id": alice, "version": 1 })
        );
        let (s, _, c) = requete(
            &e,
            "bob",
            "GET",
            &format!("{base}/users/{alice}/avatar"),
            vec![],
        )
        .await;
        assert_eq!((s, c), (StatusCode::OK, img.clone()));
        assert_eq!(
            code(
                &e,
                "carl",
                "GET",
                &format!("{base}/users/{alice}/avatar"),
                b""
            )
            .await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            code(&e, "carl", "PUT", &format!("{base}/me/avatar"), &img).await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "PUT",
                &format!("{base}/me/avatar"),
                &png(600, 600)
            )
            .await,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            code(
                &e,
                "bob",
                "GET",
                &format!("{base}/users/{alice}/avatar/x"),
                b""
            )
            .await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            json_de(&e, "bob", &format!("{base}/avatars")).await,
            json!({ alice.clone(): 1 })
        );
        assert_eq!(
            code(&e, "alice", "PUT", &format!("{base}/me/avatar"), &img).await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            json_de(&e, "bob", &format!("{base}/avatars")).await,
            json!({ alice.clone(): 2 })
        );
        assert_eq!(
            code(&e, "bob", "GET", &format!("/users/{alice}/avatar"), b"").await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            code(&e, "alice", "DELETE", &format!("{base}/me/avatar"), b"").await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            code(
                &e,
                "bob",
                "GET",
                &format!("{base}/users/{alice}/avatar"),
                b""
            )
            .await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            json_de(&e, "bob", &format!("{base}/avatars")).await,
            json!({})
        );

        assert_eq!(
            code(&e, "alice", "GET", &format!("{base}/icon"), b"").await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            code(&e, "alice", "PUT", &format!("{base}/icon"), &img).await,
            StatusCode::NO_CONTENT
        );
        let mut dernier = Value::Null;
        while let Ok(m) = rx.try_recv() {
            dernier = serde_json::from_str(&m).unwrap();
        }
        assert_eq!(dernier["type"], "server_icon.updated");
        assert_eq!(dernier["data"], json!({ "id": sid, "version": 1 }));
        let (s, h, c) = requete(&e, "bob", "GET", &format!("{base}/icon"), vec![]).await;
        assert_eq!(
            (s, c, h[header::CONTENT_TYPE].to_str().unwrap()),
            (StatusCode::OK, img.clone(), "image/png")
        );
        assert_eq!(json_de(&e, "bob", "/servers").await[0]["icon_version"], 1);
        assert_eq!(
            code(&e, "bob", "PUT", &format!("{base}/icon"), &img).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            code(&e, "bob", "DELETE", &format!("{base}/icon"), b"").await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            code(&e, "carl", "GET", &format!("{base}/icon"), b"").await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            code(&e, "alice", "PUT", &format!("{base}/icon"), &png(600, 600)).await,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            code(
                &e,
                "alice",
                "PUT",
                &format!("{base}/icon"),
                &vec![0; AVATAR_OCTETS + 1]
            )
            .await,
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(
            code(&e, "alice", "DELETE", &format!("{base}/icon"), b"").await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            code(&e, "bob", "GET", &format!("{base}/icon"), b"").await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(json_de(&e, "bob", "/servers").await[0]["icon_version"], 0);

        membre(&e, "root").await;
        sqlx::query("INSERT INTO server_members (server_id, user_id) SELECT $1::uuid, id FROM users WHERE pseudo='root'")
            .bind(&sid)
            .execute(e.state.pool().ok().unwrap())
            .await
            .unwrap();
        assert_eq!(
            code(&e, "root", "PUT", &format!("{base}/icon"), &img).await,
            StatusCode::NO_CONTENT
        );
    }
}
