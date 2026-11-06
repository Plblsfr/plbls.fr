use std::collections::HashMap;
use std::fmt;

use utoipa_axum::router::OpenApiRouter;

pub struct Block<S> {
    pub name: &'static str,
    pub needs: &'static [&'static str],
    pub mount: fn(OpenApiRouter<S>) -> OpenApiRouter<S>,
}

#[derive(Clone, Debug, Default)]
pub struct Set(Option<HashMap<String, bool>>);

impl Set {
    pub fn all() -> Self {
        Set(None)
    }

    pub fn has(&self, name: &str) -> bool {
        match &self.0 {
            None => true,
            Some(m) => m.get(name).copied().unwrap_or(false),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub fn var(name: &str) -> String {
    format!("BLOCK_{}", name.to_uppercase().replace('-', "_"))
}

pub fn select<S>(all: &[Block<S>], getenv: impl Fn(&str) -> Option<String>) -> Result<Set, Error> {
    let mut set = HashMap::new();
    for b in all {
        let v = getenv(&var(b.name)).unwrap_or_default();
        match v.as_str() {
            "" | "true" => set.insert(b.name.to_string(), true),
            "false" => set.insert(b.name.to_string(), false),
            _ => {
                return Err(Error(format!(
                    "{}={:?} : attendu true ou false",
                    var(b.name),
                    v
                )));
            }
        };
    }
    let mut errs = Vec::new();
    for b in all {
        for need in b.needs {
            let actif = set[b.name];
            match set.get(*need) {
                _ if !actif => {}
                Some(true) => {}
                None => errs.push(format!(
                    "le bloc {} dépend du bloc inconnu {}",
                    b.name, need
                )),
                Some(false) => errs.push(format!(
                    "le bloc {} dépend de {} : activer {}=true ou désactiver {}=false",
                    b.name,
                    need,
                    var(need),
                    var(b.name)
                )),
            }
        }
    }
    if errs.is_empty() {
        Ok(Set(Some(set)))
    } else {
        Err(Error(format!("blocs incohérents : {}", errs.join(" ; "))))
    }
}

pub fn actifs<S>(all: &[Block<S>], set: &Set) -> Vec<String> {
    all.iter()
        .filter(|b| set.has(b.name))
        .map(|b| b.name.to_string())
        .collect()
}

pub fn mount<S>(all: &[Block<S>], set: &Set, mut router: OpenApiRouter<S>) -> OpenApiRouter<S> {
    for b in all {
        if set.has(b.name) {
            router = (b.mount)(router);
        }
    }
    router
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::get,
    };
    use tower::ServiceExt;

    fn salons(r: OpenApiRouter<()>) -> OpenApiRouter<()> {
        r.route("/salons", get(|| async { "salons" }))
    }
    fn mp(r: OpenApiRouter<()>) -> OpenApiRouter<()> {
        r.route("/mp", get(|| async { "mp" }))
    }
    fn messages(r: OpenApiRouter<()>) -> OpenApiRouter<()> {
        r.route("/messages", get(|| async { "messages" }))
    }

    fn blocs() -> Vec<Block<()>> {
        vec![
            Block {
                name: "salons",
                needs: &[],
                mount: salons,
            },
            Block {
                name: "messages-prives",
                needs: &[],
                mount: mp,
            },
            Block {
                name: "messages",
                needs: &["salons"],
                mount: messages,
            },
        ]
    }

    fn env(paires: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| {
            paires
                .iter()
                .find(|(c, _)| *c == k)
                .map(|(_, v)| v.to_string())
        }
    }

    async fn statut(r: Router<()>, chemin: &str) -> StatusCode {
        r.oneshot(Request::get(chemin).body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[test]
    fn nom_de_variable() {
        assert_eq!(var("messages-prives"), "BLOCK_MESSAGES_PRIVES");
        assert_eq!(var("gifs"), "BLOCK_GIFS");
    }

    #[test]
    fn tout_actif_par_defaut() {
        let set = select(&blocs(), env(&[])).unwrap();
        assert!(blocs().iter().all(|b| set.has(b.name)));
        let set = select(&blocs(), env(&[("BLOCK_SALONS", "true")])).unwrap();
        assert!(set.has("salons"));
        assert!(!set.has("inconnu"));
    }

    #[test]
    fn set_nul_active_tout() {
        assert!(Set::all().has("n'importe quoi"));
    }

    #[tokio::test]
    async fn bloc_desactive_non_monte() {
        let set = select(&blocs(), env(&[("BLOCK_MESSAGES_PRIVES", "false")])).unwrap();
        assert!(!set.has("messages-prives"));
        let r = Router::from(mount(&blocs(), &set, OpenApiRouter::new()));
        assert_eq!(statut(r.clone(), "/salons").await, StatusCode::OK);
        assert_eq!(statut(r, "/mp").await, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn coeur_seul() {
        let set = select(
            &blocs(),
            env(&[
                ("BLOCK_SALONS", "false"),
                ("BLOCK_MESSAGES_PRIVES", "false"),
                ("BLOCK_MESSAGES", "false"),
            ]),
        )
        .unwrap();
        let r = Router::from(mount(&blocs(), &set, OpenApiRouter::new()));
        for chemin in ["/salons", "/mp", "/messages"] {
            assert_eq!(statut(r.clone(), chemin).await, StatusCode::NOT_FOUND);
        }
    }

    #[test]
    fn ordre_des_montages() {
        static ORDRE: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
        fn a(r: OpenApiRouter<()>) -> OpenApiRouter<()> {
            ORDRE.lock().unwrap().push("salons");
            r
        }
        fn b(r: OpenApiRouter<()>) -> OpenApiRouter<()> {
            ORDRE.lock().unwrap().push("mp");
            r
        }
        let bs = vec![
            Block {
                name: "salons",
                needs: &[],
                mount: a,
            },
            Block {
                name: "mp",
                needs: &[],
                mount: b,
            },
        ];
        let _ = mount(&bs, &Set::all(), OpenApiRouter::new());
        assert_eq!(*ORDRE.lock().unwrap(), ["salons", "mp"]);
    }

    #[test]
    fn noms_des_blocs_actifs() {
        let set = select(&blocs(), env(&[("BLOCK_MESSAGES_PRIVES", "false")])).unwrap();
        assert_eq!(actifs(&blocs(), &set), ["salons", "messages"]);
        assert_eq!(actifs(&blocs(), &Set::all()).len(), 3);
    }

    #[test]
    fn dependance_desactivee() {
        let e = select(&blocs(), env(&[("BLOCK_SALONS", "false")])).unwrap_err();
        assert_eq!(
            e.to_string(),
            "blocs incohérents : le bloc messages dépend de salons : activer BLOCK_SALONS=true ou désactiver BLOCK_MESSAGES=false"
        );
        select(
            &blocs(),
            env(&[("BLOCK_SALONS", "false"), ("BLOCK_MESSAGES", "false")]),
        )
        .unwrap();
    }

    #[test]
    fn dependance_inconnue() {
        let bs = vec![Block::<()> {
            name: "a",
            needs: &["inconnu"],
            mount: |r| r,
        }];
        let e = select(&bs, env(&[])).unwrap_err();
        assert_eq!(
            e.to_string(),
            "blocs incohérents : le bloc a dépend du bloc inconnu inconnu"
        );
        select(&bs, env(&[("BLOCK_A", "false")])).unwrap();
    }

    #[test]
    fn valeur_invalide() {
        let e = select(&blocs(), env(&[("BLOCK_SALONS", "0")])).unwrap_err();
        assert_eq!(e.to_string(), "BLOCK_SALONS=\"0\" : attendu true ou false");
    }
}
