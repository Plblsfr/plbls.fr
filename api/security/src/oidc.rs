use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::Deserialize;
use tokio::sync::{OnceCell, RwLock};

use crate::identity::Identity;
use crate::settings::Oidc as OidcSettings;

#[derive(Debug, Clone, Deserialize)]
pub struct Metadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    pub end_session_endpoint: Option<String>,
    pub revocation_endpoint: Option<String>,
}

/// Keycloak a refusé la demande (400, ex. `invalid_grant`) : à distinguer d'une panne (réseau, 5xx).
#[derive(Debug)]
pub struct Rejected(pub String);

impl std::fmt::Display for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Rejected {}

#[derive(Debug)]
pub struct Indisponible(pub String);

impl std::fmt::Display for Indisponible {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Indisponible {}

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub expires_in: u64,
    pub refresh_expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct KcUser {
    pub id: String,
    pub email: Option<String>,
    #[serde(default, rename = "firstName")]
    pub first_name: Option<String>,
    #[serde(default, rename = "lastName")]
    pub last_name: Option<String>,
}

impl KcUser {
    pub fn nom(&self) -> Option<String> {
        let nom = [self.first_name.as_deref(), self.last_name.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        (!nom.is_empty()).then_some(nom)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KcClient {
    client_id: String,
    #[serde(default)]
    default_client_scopes: Vec<String>,
    #[serde(default)]
    optional_client_scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub preferred_username: Option<String>,
    pub email: Option<String>,
    pub azp: Option<String>,
    pub nonce: Option<String>,
    pub exp: u64,
    pub realm_access: Option<Roles>,
    pub resource_access: Option<HashMap<String, Roles>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Roles {
    #[serde(default)]
    pub roles: Vec<String>,
}

impl Claims {
    pub fn identity(&self) -> Identity {
        let mut roles = self
            .realm_access
            .as_ref()
            .map(|r| r.roles.clone())
            .unwrap_or_default();
        if let Some(ra) = &self.resource_access {
            let mut clients: Vec<_> = ra.iter().collect();
            clients.sort_by(|a, b| a.0.cmp(b.0));
            for (client, r) in clients {
                roles.extend(r.roles.iter().map(|role| format!("{client}:{role}")));
            }
        }
        Identity {
            id: self.sub.clone(),
            name: self.preferred_username.clone().unwrap_or_default(),
            email: self.email.clone().unwrap_or_default(),
            roles,
            client: self.azp.clone().unwrap_or_default(),
        }
    }
}

pub struct Oidc {
    http: reqwest::Client,
    cfg: OidcSettings,
    meta: OnceCell<Metadata>,
    jwks: RwLock<Option<JwkSet>>,
    /// Dernier rechargement du JWKS déclenché par un `kid` inconnu.
    last_reload: std::sync::Mutex<Option<Instant>>,
    clients: RwLock<Clients>,
    recharge_clients: tokio::sync::Mutex<()>,
}

#[derive(Default)]
struct Clients {
    liste: Option<(Instant, HashSet<String>)>,
    echec: Option<Instant>,
}

#[derive(Debug, PartialEq)]
enum Decision {
    Autorise,
    Refuse,
    Recharger,
    Indisponible,
}

impl Clients {
    fn decision(&self, azp: &str) -> Decision {
        let echec_recent = self
            .echec
            .is_some_and(|t| t.elapsed() < CLIENTS_MISS_RELOAD_EVERY);
        let connu = match &self.liste {
            Some((at, set)) if at.elapsed() < CLIENTS_TTL => {
                if set.contains(azp) {
                    return Decision::Autorise;
                }
                if at.elapsed() < CLIENTS_MISS_RELOAD_EVERY || echec_recent {
                    return Decision::Refuse;
                }
                return Decision::Recharger;
            }
            Some((_, set)) => set.contains(azp),
            None if echec_recent => return Decision::Indisponible,
            None => return Decision::Recharger,
        };
        match (echec_recent, connu) {
            (false, _) => Decision::Recharger,
            (true, true) => Decision::Autorise,
            (true, false) => Decision::Refuse,
        }
    }
}

const CLIENTS_PAGE: usize = 500;

async fn toutes_les_pages<T, F, Fut>(max: usize, mut page: F) -> anyhow::Result<Vec<T>>
where
    F: FnMut(usize) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<Vec<T>>>,
{
    let mut tout = Vec::new();
    loop {
        let lus = page(tout.len()).await?;
        let fin = lus.len() < max;
        tout.extend(lus);
        if fin {
            return Ok(tout);
        }
    }
}

/// Durée de validité de la liste des clients : un scope retiré à un client coupe son accès au bout de ce délai.
const CLIENTS_TTL: Duration = Duration::from_secs(60);
/// Un `azp` inconnu ne relit la liste qu'une fois par intervalle (même raison que `JWKS_RELOAD_EVERY`).
const CLIENTS_MISS_RELOAD_EVERY: Duration = Duration::from_secs(10);

/// Un `kid` inconnu ne recharge le JWKS qu'une fois par intervalle : sinon un faux JWT par requête ferait un appel à Keycloak.
const JWKS_RELOAD_EVERY: Duration = Duration::from_secs(30);

impl Oidc {
    pub fn new(cfg: OidcSettings) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        Ok(Self {
            http,
            cfg,
            meta: OnceCell::new(),
            jwks: RwLock::new(None),
            last_reload: Default::default(),
            clients: Default::default(),
            recharge_clients: Default::default(),
        })
    }

    /// Réécrit une URL de Keycloak vers l'URL interne (appels serveur à serveur uniquement).
    fn to_internal(&self, url: String) -> String {
        let (public, internal) = (
            self.cfg.issuer.trim_end_matches('/'),
            self.cfg.internal_issuer.trim_end_matches('/'),
        );
        match url.strip_prefix(public) {
            Some(rest) if !internal.is_empty() => format!("{internal}{rest}"),
            _ => url,
        }
    }

    /// Discovery paresseuse : Keycloak peut démarrer après le gateway.
    pub async fn metadata(&self) -> anyhow::Result<&Metadata> {
        self.meta
            .get_or_try_init(|| async {
                let base = if self.cfg.internal_issuer.is_empty() {
                    &self.cfg.issuer
                } else {
                    &self.cfg.internal_issuer
                };
                let url = format!(
                    "{}/.well-known/openid-configuration",
                    base.trim_end_matches('/')
                );
                let mut m: Metadata = self
                    .http
                    .get(&url)
                    .send()
                    .await?
                    .error_for_status()?
                    .json()
                    .await
                    .with_context(|| format!("discovery {url}"))?;
                if m.issuer != self.cfg.issuer {
                    return Err(anyhow!(
                        "issuer inattendu: {} (attendu {}) : régler KC_HOSTNAME côté Keycloak",
                        m.issuer,
                        self.cfg.issuer
                    ));
                }
                // le navigateur garde les URLs publiques (authorize, logout), le gateway passe par le réseau interne
                m.token_endpoint = self.to_internal(m.token_endpoint);
                m.jwks_uri = self.to_internal(m.jwks_uri);
                m.revocation_endpoint = m.revocation_endpoint.map(|u| self.to_internal(u));
                Ok(m)
            })
            .await
    }

    async fn token_request(&self, mut form: Vec<(&str, String)>) -> anyhow::Result<TokenResponse> {
        let meta = self.metadata().await?;
        form.push(("client_id", self.cfg.client_id.clone()));
        form.push(("client_secret", self.cfg.client_secret.clone()));
        let resp = self
            .http
            .post(&meta.token_endpoint)
            .form(&form)
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let msg = format!("token endpoint {status}: {body}");
            return Err(if status == reqwest::StatusCode::BAD_REQUEST {
                Rejected(msg).into()
            } else {
                anyhow!(msg)
            });
        }
        Ok(resp.json().await?)
    }

    pub async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> anyhow::Result<TokenResponse> {
        self.token_request(vec![
            ("grant_type", "authorization_code".into()),
            ("code", code.into()),
            ("code_verifier", verifier.into()),
            ("redirect_uri", redirect_uri.into()),
        ])
        .await
    }

    pub async fn refresh(&self, refresh_token: &str) -> anyhow::Result<TokenResponse> {
        self.token_request(vec![
            ("grant_type", "refresh_token".into()),
            ("refresh_token", refresh_token.into()),
        ])
        .await
    }

    fn admin_url(&self, path: &str) -> String {
        let base = if self.cfg.internal_issuer.is_empty() {
            &self.cfg.issuer
        } else {
            &self.cfg.internal_issuer
        };
        format!(
            "{}/{path}",
            base.trim_end_matches('/')
                .replacen("/realms/", "/admin/realms/", 1)
        )
    }

    /// Clients du realm qui ont le scope `audience` (par défaut ou optionnel). API admin Keycloak : le compte de
    /// service du gateway a besoin de `view-clients`.
    async fn fetch_clients(&self) -> anyhow::Result<HashSet<String>> {
        let token = self
            .token_request(vec![("grant_type", "client_credentials".into())])
            .await?;
        let url = self.admin_url("clients");
        let list: Vec<KcClient> = toutes_les_pages(CLIENTS_PAGE, |first| {
            let req = self
                .http
                .get(&url)
                .bearer_auth(&token.access_token)
                .query(&[("first", first), ("max", CLIENTS_PAGE)]);
            async move { Ok(req.send().await?.error_for_status()?.json().await?) }
        })
        .await?;
        let scope = &self.cfg.audience;
        Ok(list
            .into_iter()
            .filter(|c| {
                c.default_client_scopes.contains(scope) || c.optional_client_scopes.contains(scope)
            })
            .map(|c| c.client_id)
            .collect())
    }

    /// `azp` d'un client du realm portant le scope `audience`. Liste en cache, relue à l'expiration ou sur `azp` inconnu.
    async fn client_allowed(&self, azp: &str) -> anyhow::Result<bool> {
        let mut verrou = None;
        loop {
            let decision = self.clients.read().await.decision(azp);
            match decision {
                Decision::Autorise => return Ok(true),
                Decision::Refuse => return Ok(false),
                Decision::Indisponible => {
                    return Err(Indisponible("liste des clients indisponible".into()).into());
                }
                Decision::Recharger if verrou.is_some() => break,
                Decision::Recharger => verrou = Some(self.recharge_clients.lock().await),
            }
        }
        let lu = self.fetch_clients().await;
        let mut clients = self.clients.write().await;
        match lu {
            Ok(set) => {
                let ok = set.contains(azp);
                *clients = Clients {
                    liste: Some((Instant::now(), set)),
                    echec: None,
                };
                Ok(ok)
            }
            Err(e) => {
                clients.echec = Some(Instant::now());
                match &clients.liste {
                    Some((_, set)) => {
                        tracing::warn!(
                            "liste des clients non relue, ancienne liste conservée: {e:#}"
                        );
                        Ok(set.contains(azp))
                    }
                    None => Err(Indisponible(format!("liste des clients: {e:#}")).into()),
                }
            }
        }
    }

    /// Comptes dont le nom d'utilisateur, l'e-mail ou le nom commence par `search` (API admin Keycloak, compte de
    /// service du gateway avec `view-users`). Un token par appel : route interne, faible volume.
    pub async fn search_users(
        &self,
        search: &str,
        first: u32,
        max: u32,
    ) -> anyhow::Result<Vec<KcUser>> {
        let token = self
            .token_request(vec![("grant_type", "client_credentials".into())])
            .await?;
        let resp = self
            .http
            .get(self.admin_url("users"))
            .bearer_auth(token.access_token)
            .query(&[
                ("search", search.to_string()),
                ("first", first.to_string()),
                ("max", max.to_string()),
            ])
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    /// Révoque le refresh token (ignoré si Keycloak n'expose pas d'endpoint de révocation).
    pub async fn revoke(&self, refresh_token: &str) {
        let Ok(meta) = self.metadata().await else {
            return;
        };
        let Some(url) = &meta.revocation_endpoint else {
            return;
        };
        let form = [
            ("token", refresh_token),
            ("token_type_hint", "refresh_token"),
            ("client_id", self.cfg.client_id.as_str()),
            ("client_secret", self.cfg.client_secret.as_str()),
        ];
        if let Err(e) = self.http.post(url).form(&form).send().await {
            tracing::warn!("révocation du refresh token échouée: {e}");
        }
    }

    /// `register` : page d'inscription Keycloak (`.../registrations`, mêmes paramètres que `.../auth`).
    pub async fn authorization_url(
        &self,
        redirect_uri: &str,
        state: &str,
        nonce: &str,
        challenge: &str,
        register: bool,
    ) -> anyhow::Result<String> {
        let meta = self.metadata().await?;
        let mut url = url::Url::parse(&meta.authorization_endpoint)?;
        if register {
            url.set_path(&registration_path(url.path()));
        }
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.cfg.client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("scope", "openid")
            .append_pair("state", state)
            .append_pair("nonce", nonce)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(url.into())
    }

    pub async fn logout_url(
        &self,
        id_token_hint: Option<&str>,
        post_logout_redirect: &str,
    ) -> Option<String> {
        let meta = self.metadata().await.ok()?;
        let mut url = url::Url::parse(meta.end_session_endpoint.as_ref()?).ok()?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("client_id", &self.cfg.client_id)
                .append_pair("post_logout_redirect_uri", post_logout_redirect);
            if let Some(h) = id_token_hint {
                q.append_pair("id_token_hint", h);
            }
        }
        Some(url.into())
    }

    async fn refresh_jwks(&self) -> anyhow::Result<()> {
        let meta = self.metadata().await?;
        let set: JwkSet = self
            .http
            .get(&meta.jwks_uri)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        *self.jwks.write().await = Some(set);
        Ok(())
    }

    /// Cache vide : toujours (démarrage). Sinon au plus une fois par `JWKS_RELOAD_EVERY`.
    fn reload_allowed(&self, has_cache: bool) -> bool {
        let mut last = self.last_reload.lock().unwrap_or_else(|e| e.into_inner());
        if has_cache && last.is_some_and(|t| t.elapsed() < JWKS_RELOAD_EVERY) {
            return false;
        }
        *last = Some(Instant::now());
        true
    }

    async fn key_for(&self, kid: &str) -> anyhow::Result<DecodingKey> {
        for attempt in 0..2 {
            let has_cache = {
                let cache = self.jwks.read().await;
                if let Some(jwk) = cache.as_ref().and_then(|set| set.find(kid)) {
                    return Ok(DecodingKey::from_jwk(jwk)?);
                }
                cache.is_some()
            };
            // cache vide ou kid inconnu : un seul rechargement, limité dans le temps
            if attempt == 0 && self.reload_allowed(has_cache) {
                self.refresh_jwks().await?;
            } else {
                break;
            }
        }
        Err(anyhow!("kid inconnu: {kid}"))
    }

    /// Vérifie signature RS256, `iss`, `exp` et audience.
    async fn verify(&self, token: &str, audience: &str) -> anyhow::Result<Claims> {
        let header = decode_header(token)?;
        if header.alg != Algorithm::RS256 {
            return Err(anyhow!("algorithme refusé: {:?}", header.alg));
        }
        let kid = header.kid.ok_or_else(|| anyhow!("kid absent"))?;
        let key = self.key_for(&kid).await?;
        let mut v = Validation::new(Algorithm::RS256);
        v.set_issuer(&[&self.cfg.issuer]);
        v.set_audience(&[audience]);
        v.leeway = 5;
        v.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        Ok(decode::<Claims>(token, &key, &v)?.claims)
    }

    /// Access token d'un client : audience `plbls-api` et `azp` = client du realm portant le scope `plbls-api`.
    pub async fn verify_access_token(&self, token: &str) -> anyhow::Result<Claims> {
        let claims = self.verify(token, &self.cfg.audience).await?;
        match &claims.azp {
            Some(azp) if self.client_allowed(azp).await? => Ok(claims),
            other => Err(anyhow!("azp non autorisé: {other:?}")),
        }
    }

    /// ID token reçu par le flow BFF : audience = client du gateway, nonce vérifié.
    pub async fn verify_id_token(&self, token: &str, nonce: &str) -> anyhow::Result<Claims> {
        let claims = self.verify(token, &self.cfg.client_id).await?;
        if claims.nonce.as_deref() != Some(nonce) {
            return Err(anyhow!("nonce invalide"));
        }
        Ok(claims)
    }
}

fn registration_path(auth_path: &str) -> String {
    format!(
        "{}/registrations",
        auth_path.trim_end_matches('/').trim_end_matches("/auth")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_decode_has_crypto_backend() {
        use jsonwebtoken::{EncodingKey, Header, encode};
        use std::hash::BuildHasher;
        let cle = std::collections::hash_map::RandomState::new()
            .hash_one(std::process::id())
            .to_string();
        let claims = serde_json::json!({ "sub": "u1", "exp": u64::MAX / 2 });
        let jeton = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(cle.as_bytes()),
        )
        .unwrap();
        let mut v = Validation::new(Algorithm::HS256);
        v.validate_aud = false;
        let lu = decode::<serde_json::Value>(&jeton, &DecodingKey::from_secret(cle.as_bytes()), &v)
            .unwrap();
        assert_eq!(lu.claims["sub"], "u1");
    }

    fn oidc(internal: &str) -> Oidc {
        Oidc::new(OidcSettings {
            issuer: "https://auth.plbls.fr/realms/plbls".into(),
            internal_issuer: internal.into(),
            client_id: "c".into(),
            client_secret: "s".into(),
            audience: "a".into(),
        })
        .unwrap()
    }

    #[test]
    fn jwks_reload_is_throttled() {
        let o = oidc("");
        assert!(
            o.reload_allowed(false) && o.reload_allowed(false),
            "cache vide : toujours"
        );
        assert!(
            !o.reload_allowed(true),
            "kid inconnu juste après un rechargement"
        );
        *o.last_reload.lock().unwrap() = Some(Instant::now() - JWKS_RELOAD_EVERY);
        assert!(o.reload_allowed(true));
    }

    fn clients(age: Option<Duration>, echec: Option<Duration>) -> Clients {
        Clients {
            liste: age.map(|a| {
                (
                    Instant::now() - a,
                    HashSet::from(["plbls-android".to_string()]),
                )
            }),
            echec: echec.map(|a| Instant::now() - a),
        }
    }

    #[test]
    fn clients_decision() {
        let s = Duration::from_secs;
        let cas = [
            (
                clients(Some(s(0)), None),
                "plbls-android",
                Decision::Autorise,
            ),
            (clients(Some(s(0)), None), "inconnu", Decision::Refuse),
            (clients(Some(s(20)), None), "inconnu", Decision::Recharger),
            (
                clients(Some(s(20)), Some(s(1))),
                "inconnu",
                Decision::Refuse,
            ),
            (
                clients(Some(s(90)), None),
                "plbls-android",
                Decision::Recharger,
            ),
            (
                clients(Some(s(90)), Some(s(1))),
                "plbls-android",
                Decision::Autorise,
            ),
            (
                clients(Some(s(90)), Some(s(1))),
                "inconnu",
                Decision::Refuse,
            ),
            (
                clients(Some(s(90)), Some(s(20))),
                "plbls-android",
                Decision::Recharger,
            ),
            (clients(None, None), "plbls-android", Decision::Recharger),
            (
                clients(None, Some(s(1))),
                "plbls-android",
                Decision::Indisponible,
            ),
            (
                clients(None, Some(s(20))),
                "plbls-android",
                Decision::Recharger,
            ),
        ];
        for (i, (c, azp, attendu)) in cas.into_iter().enumerate() {
            assert_eq!(c.decision(azp), attendu, "cas {i}");
        }
    }

    fn oidc_injoignable() -> Oidc {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let mut o = oidc("");
        o.cfg.issuer = format!("http://127.0.0.1:{port}/realms/plbls");
        o
    }

    #[tokio::test]
    async fn keycloak_down_keeps_previous_clients() {
        let o = oidc_injoignable();
        *o.clients.write().await = clients(Some(CLIENTS_TTL * 2), None);
        assert!(
            o.client_allowed("plbls-android").await.unwrap(),
            "ancienne liste conservée"
        );
        assert!(!o.client_allowed("inconnu").await.unwrap());
        assert!(
            o.clients.read().await.echec.is_some(),
            "échec noté : pas de rappel immédiat"
        );
    }

    #[tokio::test]
    async fn keycloak_down_without_clients_is_unavailable() {
        let o = oidc_injoignable();
        for _ in 0..2 {
            let e = o.client_allowed("plbls-android").await.unwrap_err();
            assert!(e.is::<Indisponible>(), "{e:#}");
        }
    }

    #[tokio::test]
    async fn concurrent_reloads_wait_for_the_first() {
        let o = oidc_injoignable();
        let verrou = o.recharge_clients.lock().await;
        let attente = o.client_allowed("plbls-android");
        tokio::pin!(attente);
        assert!(
            futures_util::FutureExt::now_or_never(attente.as_mut()).is_none(),
            "rechargement en cours : on attend"
        );
        *o.clients.write().await = clients(Some(Duration::ZERO), None);
        drop(verrou);
        assert!(attente.await.unwrap(), "liste relue par l'autre requête");
        assert!(
            o.clients.read().await.echec.is_none(),
            "pas de second appel à Keycloak"
        );
    }

    #[tokio::test]
    async fn clients_pages_until_short_page() {
        let mut firsts = Vec::new();
        let tout = toutes_les_pages(2, |first| {
            firsts.push(first);
            async move { Ok((first..(first + 2).min(5)).collect::<Vec<_>>()) }
        })
        .await
        .unwrap();
        assert_eq!(tout, vec![0, 1, 2, 3, 4]);
        assert_eq!(firsts, vec![0, 2, 4]);
        let pleines = toutes_les_pages(2, |first| async move {
            Ok((first..(first + 2).min(4)).collect::<Vec<_>>())
        })
        .await
        .unwrap();
        assert_eq!(pleines.len(), 4, "dernière page vide");
    }

    #[tokio::test]
    async fn client_allowed_uses_cached_scope_list() {
        let o = oidc("");
        *o.clients.write().await = clients(Some(Duration::ZERO), None);
        assert!(
            o.client_allowed("plbls-android").await.unwrap(),
            "client connu"
        );
        assert!(
            !o.client_allowed("inconnu").await.unwrap(),
            "azp inconnu, liste lue à l'instant : pas de rappel Keycloak"
        );
    }

    #[test]
    fn rewrites_to_internal_only_when_configured() {
        let url = "https://auth.plbls.fr/realms/plbls/protocol/openid-connect/token".to_string();
        assert_eq!(
            oidc("http://keycloak:8080/realms/plbls/").to_internal(url.clone()),
            "http://keycloak:8080/realms/plbls/protocol/openid-connect/token"
        );
        assert_eq!(oidc("").to_internal(url.clone()), url);
        let other = "https://autre.example/x".to_string();
        assert_eq!(
            oidc("http://keycloak:8080/realms/plbls").to_internal(other.clone()),
            other
        );
    }

    #[test]
    fn registration_endpoint_replaces_auth() {
        assert_eq!(
            registration_path("/realms/plbls/protocol/openid-connect/auth"),
            "/realms/plbls/protocol/openid-connect/registrations"
        );
    }
}
