use std::collections::HashMap;

use axum::{body::Body, extract::State, http::StatusCode, response::Response};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    etat::AppState,
    garde::Personne,
    http::{bd, horodatage, invalide, json, ok},
    ordre::{self, Contexte, Element, Place, Suggestions},
    playlists::{
        PISTES_MAX, booleen, champ_uuid, contenu, entier, objet, pistes_de_source, pistes_json,
        plafond, texte,
    },
};

pub const FILE_MAX: usize = PISTES_MAX;
pub const POSITION_MAX: f64 = 86_400.0;
pub const SOURCES: [&str; 5] = ["album", "playlist", "artist", "likes", "search"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Elem {
    pub id: Uuid,
    pub piste: Uuid,
    pub rang_source: i32,
    pub suggeree: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub genre: String,
    pub id: Option<Uuid>,
    pub nom: String,
}

#[derive(Clone, Debug)]
pub struct File {
    pub revision: i64,
    pub source: Option<Source>,
    pub index: usize,
    pub position: f64,
    pub aleatoire: bool,
    pub repetition: String,
    pub graine: i64,
    pub suggestions: Suggestions,
    pub elements: Vec<Elem>,
    pub modifie_le: Option<DateTime<Utc>>,
}

impl Default for File {
    fn default() -> Self {
        Self {
            revision: 0,
            source: None,
            index: 0,
            position: 0.0,
            aleatoire: false,
            repetition: "off".into(),
            graine: 0,
            suggestions: Suggestions::Off,
            elements: Vec::new(),
            modifie_le: None,
        }
    }
}

impl File {
    fn courant(&self) -> Option<Uuid> {
        self.elements.get(self.index).map(|e| e.id)
    }

    fn suivre(&mut self, courant: Option<Uuid>) {
        self.index = courant
            .and_then(|c| self.elements.iter().position(|e| e.id == c))
            .unwrap_or(self.index);
    }

    fn appliquer(&mut self, places: Vec<Place>) {
        let mut anciens: HashMap<Uuid, Elem> = std::mem::take(&mut self.elements)
            .into_iter()
            .map(|e| (e.id, e))
            .collect();
        let mut rang = anciens.values().map(|e| e.rang_source).max().unwrap_or(-1);
        self.elements = places
            .into_iter()
            .map(|p| match p.element.and_then(|id| anciens.remove(&id)) {
                Some(e) => e,
                None => {
                    rang += 1;
                    Elem {
                        id: Uuid::new_v4(),
                        piste: p.piste,
                        rang_source: rang,
                        suggeree: p.suggeree,
                    }
                }
            })
            .collect();
    }

    fn sans_suggestions(&mut self, passees: bool) {
        let courant = self.courant();
        let index = self.index;
        let mut i = 0;
        self.elements.retain(|e| {
            i += 1;
            !e.suggeree || Some(e.id) == courant || (passees && i <= index)
        });
        self.suivre(courant);
    }

    async fn melanger(&mut self, state: &AppState, sub: &str) {
        let courant = self.courant();
        self.elements
            .retain(|e| !e.suggeree || Some(e.id) == courant);
        self.graine = ordre::graine();
        let elements: Vec<Element> = self
            .elements
            .iter()
            .map(|e| Element {
                id: e.id,
                piste: e.piste,
            })
            .collect();
        let places = ordre::melanger(
            state,
            &Contexte {
                sub,
                graine: self.graine,
                premier: courant,
                elements: &elements,
                suggestions: self.suggestions,
            },
        )
        .await;
        self.appliquer(places);
        self.index = 0;
    }

    async fn prolonger(&mut self, state: &AppState, sub: &str) -> bool {
        let Some(rythme) = self.suggestions.rythme().filter(|_| self.aleatoire) else {
            return false;
        };
        let mut places: Vec<Place> = self
            .elements
            .iter()
            .map(|e| Place {
                element: Some(e.id),
                piste: e.piste,
                suggeree: e.suggeree,
            })
            .collect();
        if !ordre::a_prolonger(&places, self.index, rythme) {
            return false;
        }
        let contexte = Contexte {
            sub,
            graine: self.graine,
            premier: None,
            elements: &[],
            suggestions: self.suggestions,
        };
        if ordre::prolonger(state, &contexte, &mut places, self.index).await == 0 {
            return false;
        }
        self.appliquer(places);
        true
    }

    fn source(&self) -> Vec<Uuid> {
        self.elements
            .iter()
            .filter(|e| !e.suggeree)
            .map(|e| e.piste)
            .collect()
    }

    fn remettre_en_ordre(&mut self) {
        self.sans_suggestions(false);
        let courant = self.courant();
        self.elements.sort_by_key(|e| e.rang_source);
        self.suivre(courant);
    }

    fn normaliser(&mut self) {
        if !self.aleatoire {
            for (i, e) in self.elements.iter_mut().enumerate() {
                e.rang_source = i32::try_from(i).unwrap_or(i32::MAX);
            }
        }
        if self.index >= self.elements.len() {
            self.index = self.elements.len().saturating_sub(1);
        }
    }
}

pub fn avancer(presentes: &[bool], index: usize, pas: i64, boucle: bool) -> Option<usize> {
    let n = i64::try_from(presentes.len()).ok()?;
    let mut i = i64::try_from(index).ok()?;
    for _ in 0..n {
        i += pas;
        if !(0..n).contains(&i) {
            if !boucle {
                return None;
            }
            i = i.rem_euclid(n);
        }
        let u = usize::try_from(i).ok()?;
        if presentes[u] {
            return Some(u);
        }
    }
    None
}

async fn charger(c: &mut PgConnection, sub: &str, verrou: bool) -> Result<File, sqlx::Error> {
    let sql = if verrou {
        "SELECT revision, source_type, source_id, source_nom, index_courant, position, aleatoire, repetition, graine,
                suggestions, modifie_le FROM files WHERE sub = $1 FOR UPDATE"
    } else {
        "SELECT revision, source_type, source_id, source_nom, index_courant, position, aleatoire, repetition, graine,
                suggestions, modifie_le FROM files WHERE sub = $1"
    };
    let Some(l) = sqlx::query(sql).bind(sub).fetch_optional(&mut *c).await? else {
        return Ok(File::default());
    };
    let elements = sqlx::query(
        "SELECT id, piste_id, rang_source, suggeree FROM files_elements WHERE sub = $1 ORDER BY rang",
    )
    .bind(sub)
    .fetch_all(&mut *c)
    .await?
    .iter()
    .map(|e| Elem {
        id: e.get(0),
        piste: e.get(1),
        rang_source: e.get(2),
        suggeree: e.get(3),
    })
    .collect();
    Ok(File {
        revision: l.get(0),
        source: l.get::<Option<String>, _>(1).map(|genre| Source {
            genre,
            id: l.get(2),
            nom: l.get::<Option<String>, _>(3).unwrap_or_default(),
        }),
        index: usize::try_from(l.get::<i32, _>(4)).unwrap_or(0),
        position: l.get(5),
        aleatoire: l.get(6),
        repetition: l.get(7),
        graine: l.get(8),
        suggestions: Suggestions::lire(&l.get::<String, _>(9)).unwrap_or(Suggestions::Off),
        elements,
        modifie_le: Some(l.get(10)),
    })
}

pub async fn etat_public(pool: &PgPool, sub: &str, f: &File) -> Result<Value, sqlx::Error> {
    let ids: Vec<Uuid> = f.elements.iter().map(|e| e.piste).collect();
    let pistes = pistes_json(pool, &ids).await?;
    let decouvertes: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT p.id FROM ajouts a JOIN albums al ON al.mbid = ANY(a.albums) JOIN pistes p ON p.album_id = al.id
         WHERE a.sub = $2 AND a.origine = 'decouverte' AND a.etat <> 'supprime' AND p.id = ANY($1)",
    )
    .bind(&ids)
    .bind(sub)
    .fetch_all(pool)
    .await?;
    Ok(json!({
        "revision": f.revision,
        "source": f.source.as_ref().map(|s| json!({"type": s.genre, "id": s.id, "name": s.nom})),
        "index": f.index,
        "position": f.position,
        "shuffle": f.aleatoire,
        "repeat": f.repetition,
        "suggestions": f.suggestions.nom(),
        "seed": f.graine,
        "updated_at": f.modifie_le.as_ref().map(horodatage),
        "items": f.elements.iter().zip(pistes).map(|(e, piste)| json!({
            "id": e.id,
            "suggested": e.suggeree,
            "discovery": decouvertes.contains(&e.piste),
            "track": piste,
        })).collect::<Vec<_>>(),
    }))
}

pub async fn lire(State(state): State<AppState>, p: Personne) -> Result<Response, Response> {
    let mut c = state.pool().acquire().await.map_err(bd)?;
    let f = charger(&mut c, p.sub(), false).await.map_err(bd)?;
    drop(c);
    Ok(ok(&etat_public(state.pool(), p.sub(), &f)
        .await
        .map_err(bd)?))
}

struct Ecriture<'a> {
    tx: Transaction<'a, Postgres>,
    file: File,
}

async fn ouvrir<'a>(
    state: &'a AppState,
    sub: &str,
    corps: &Map<String, Value>,
) -> Result<Ecriture<'a>, Response> {
    let attendue = corps
        .get("revision")
        .and_then(Value::as_i64)
        .ok_or_else(|| invalide("revision requise"))?;
    let mut tx = state.pool().begin().await.map_err(bd)?;
    sqlx::query("INSERT INTO files (sub) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(sub)
        .execute(&mut *tx)
        .await
        .map_err(bd)?;
    let file = charger(&mut tx, sub, true).await.map_err(bd)?;
    if file.revision != attendue {
        drop(tx);
        let courant = etat_public(state.pool(), sub, &file).await.map_err(bd)?;
        return Err(json(
            StatusCode::CONFLICT,
            &json!({
                "error": "conflict",
                "message": "la file a été modifiée sur un autre appareil",
                "queue": courant,
            }),
        ));
    }
    Ok(Ecriture { tx, file })
}

async fn fermer(state: &AppState, sub: &str, e: Ecriture<'_>) -> Result<Response, Response> {
    let Ecriture {
        mut tx,
        file: mut f,
    } = e;
    f.normaliser();
    f.revision += 1;
    let modifie_le: DateTime<Utc> = sqlx::query_scalar(
        "UPDATE files SET revision = $2, source_type = $3, source_id = $4, source_nom = $5, index_courant = $6,
           position = $7, aleatoire = $8, repetition = $9, graine = $10, suggestions = $11, modifie_le = now()
         WHERE sub = $1 RETURNING modifie_le",
    )
    .bind(sub)
    .bind(f.revision)
    .bind(f.source.as_ref().map(|s| s.genre.clone()))
    .bind(f.source.as_ref().and_then(|s| s.id))
    .bind(f.source.as_ref().map(|s| s.nom.clone()))
    .bind(i32::try_from(f.index).unwrap_or(0))
    .bind(f.position)
    .bind(f.aleatoire)
    .bind(&f.repetition)
    .bind(f.graine)
    .bind(f.suggestions.nom())
    .fetch_one(&mut *tx)
    .await
    .map_err(bd)?;
    f.modifie_le = Some(modifie_le);
    sqlx::query("DELETE FROM files_elements WHERE sub = $1")
        .bind(sub)
        .execute(&mut *tx)
        .await
        .map_err(bd)?;
    let ids: Vec<Uuid> = f.elements.iter().map(|e| e.id).collect();
    let pistes: Vec<Uuid> = f.elements.iter().map(|e| e.piste).collect();
    let rangs: Vec<i32> = f.elements.iter().map(|e| e.rang_source).collect();
    let suggerees: Vec<bool> = f.elements.iter().map(|e| e.suggeree).collect();
    sqlx::query(
        "INSERT INTO files_elements (sub, rang, id, piste_id, rang_source, suggeree)
         SELECT $1, r - 1, id, piste, src, sug FROM unnest($2::uuid[], $3::uuid[], $4::int[], $5::bool[])
           WITH ORDINALITY AS a(id, piste, src, sug, r)",
    )
    .bind(sub)
    .bind(&ids)
    .bind(&pistes)
    .bind(&rangs)
    .bind(&suggerees)
    .execute(&mut *tx)
    .await
    .map_err(bd)?;
    tx.commit().await.map_err(bd)?;
    Ok(ok(&etat_public(state.pool(), sub, &f).await.map_err(bd)?))
}

fn decouvrir(state: &AppState, f: &File) -> Option<Vec<Uuid>> {
    (f.aleatoire && f.suggestions != Suggestions::Off && state.config().listenbrainz.is_some())
        .then(|| f.source())
}

fn position(corps: &Map<String, Value>) -> Result<Option<f64>, Response> {
    match corps.get("position") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|p| p.is_finite() && (0.0..=POSITION_MAX).contains(p))
            .map(Some)
            .ok_or_else(|| invalide("position invalide")),
    }
}

fn modes(f: &mut File, corps: &Map<String, Value>) -> Result<Option<bool>, Response> {
    if let Some(r) = corps.get("repeat").filter(|v| !v.is_null()) {
        f.repetition = r
            .as_str()
            .filter(|r| ["off", "all", "one"].contains(r))
            .ok_or_else(|| invalide("repeat : off, all ou one"))?
            .to_string();
    }
    if let Some(s) = corps.get("suggestions").filter(|v| !v.is_null()) {
        f.suggestions = s
            .as_str()
            .and_then(Suggestions::lire)
            .ok_or_else(|| invalide("suggestions : off, light ou frequent"))?;
    }
    booleen(corps, "shuffle")
}

pub async fn remplacer(
    State(state): State<AppState>,
    p: Personne,
    corps: Body,
) -> Result<Response, Response> {
    let corps = objet(corps).await?;
    let source = match corps.get("source").filter(|v| !v.is_null()) {
        None => None,
        Some(Value::Object(s)) => {
            let genre = s
                .get("type")
                .and_then(Value::as_str)
                .filter(|g| SOURCES.contains(g))
                .ok_or_else(|| invalide("source : type inconnu"))?
                .to_string();
            Some((genre, champ_uuid(s, "id")?, texte(s, "query", 100, false)?))
        }
        Some(_) => return Err(invalide("source : objet attendu")),
    };
    let (pistes, source) = match (corps.get("tracks").is_some(), source) {
        (true, s) => {
            let pistes = contenu(state.pool(), p.sub(), &corps).await?;
            let s = match s {
                None => None,
                Some((genre, _, query)) if genre == "search" => Some(Source {
                    genre,
                    id: None,
                    nom: query.ok_or_else(|| invalide("source : query requise"))?,
                }),
                Some((genre, id, _)) => Some(Source {
                    nom: pistes_de_source(state.pool(), p.sub(), &genre, id).await?.1,
                    genre,
                    id,
                }),
            };
            (pistes, s)
        }
        (false, Some((genre, id, _))) if genre != "search" => {
            let (pistes, nom) = pistes_de_source(state.pool(), p.sub(), &genre, id).await?;
            (pistes, Some(Source { genre, id, nom }))
        }
        _ => return Err(invalide("tracks ou source requis")),
    };
    if pistes.len() > FILE_MAX {
        return Err(plafond(&format!("{FILE_MAX} pistes dans la file au plus")));
    }
    let index = entier(&corps, "index")?.unwrap_or(0);
    if index >= pistes.len().max(1) {
        return Err(invalide("index hors de la file"));
    }
    let position = position(&corps)?.unwrap_or(0.0);
    let mut e = ouvrir(&state, p.sub(), &corps).await?;
    let f = &mut e.file;
    let aleatoire = modes(f, &corps)?;
    f.source = source;
    f.elements = pistes
        .into_iter()
        .enumerate()
        .map(|(i, piste)| Elem {
            id: Uuid::new_v4(),
            piste,
            rang_source: i32::try_from(i).unwrap_or(i32::MAX),
            suggeree: false,
        })
        .collect();
    f.index = index;
    f.position = position;
    f.aleatoire = aleatoire.unwrap_or(f.aleatoire);
    if f.aleatoire {
        f.melanger(&state, p.sub()).await;
    }
    let source = decouvrir(&state, f);
    let r = fermer(&state, p.sub(), e).await?;
    if let Some(source) = source {
        crate::decouverte::declencher(&state, &p, source);
    }
    Ok(r)
}

pub async fn modifier(
    State(state): State<AppState>,
    p: Personne,
    corps: Body,
) -> Result<Response, Response> {
    let corps = objet(corps).await?;
    let index = entier(&corps, "index")?;
    let position = position(&corps)?;
    let pas = match corps.get("step").filter(|v| !v.is_null()) {
        None => None,
        Some(v) => Some(
            v.as_i64()
                .filter(|s| *s == 1 || *s == -1)
                .ok_or_else(|| invalide("step : 1 ou -1"))?,
        ),
    };
    let mut e = ouvrir(&state, p.sub(), &corps).await?;
    let f = &mut e.file;
    let avant = f.suggestions;
    let aleatoire = modes(f, &corps)?;
    let mut planifie = false;
    if let Some(i) = index {
        if i >= f.elements.len() {
            return Err(invalide("index hors de la file"));
        }
        f.index = i;
        f.position = 0.0;
    }
    if let Some(pos) = position {
        f.position = pos;
    }
    match aleatoire {
        Some(true) if !f.aleatoire => {
            f.aleatoire = true;
            f.melanger(&state, p.sub()).await;
            planifie = true;
        }
        Some(false) if f.aleatoire => {
            f.aleatoire = false;
            f.remettre_en_ordre();
        }
        _ => {}
    }
    if let Some(pas) = pas {
        let ids: Vec<Uuid> = f.elements.iter().map(|e| e.piste).collect();
        let presentes: Vec<Uuid> =
            sqlx::query_scalar("SELECT id FROM pistes WHERE id = ANY($1) AND present")
                .bind(&ids)
                .fetch_all(&mut *e.tx)
                .await
                .map_err(bd)?;
        let f = &mut e.file;
        let drapeaux: Vec<bool> = ids.iter().map(|i| presentes.contains(i)).collect();
        f.index = avancer(&drapeaux, f.index, pas, f.repetition != "off").ok_or_else(|| {
            crate::http::erreur(
                StatusCode::UNPROCESSABLE_ENTITY,
                "end_of_queue",
                "aucune piste disponible dans ce sens",
            )
        })?;
        f.position = 0.0;
    }
    let f = &mut e.file;
    if f.aleatoire && f.suggestions == Suggestions::Off && avant != Suggestions::Off {
        f.sans_suggestions(true);
    }
    if !planifie && (index.is_some() || pas.is_some() || f.suggestions != avant) {
        planifie = f.prolonger(&state, p.sub()).await;
    }
    let source = decouvrir(&state, f).filter(|_| planifie);
    let r = fermer(&state, p.sub(), e).await?;
    if let Some(source) = source {
        crate::decouverte::declencher(&state, &p, source);
    }
    Ok(r)
}

pub async fn ajouter(
    State(state): State<AppState>,
    p: Personne,
    corps: Body,
) -> Result<Response, Response> {
    let mut corps = objet(corps).await?;
    let ensuite = match corps.remove("mode").as_ref().and_then(Value::as_str) {
        Some("next") => true,
        Some("end") | None => false,
        Some(_) => return Err(invalide("mode : next ou end")),
    };
    let pistes = contenu(state.pool(), p.sub(), &corps).await?;
    let mut e = ouvrir(&state, p.sub(), &corps).await?;
    let f = &mut e.file;
    if f.elements.len() + pistes.len() > FILE_MAX {
        return Err(plafond(&format!("{FILE_MAX} pistes dans la file au plus")));
    }
    let k = i32::try_from(pistes.len()).unwrap_or(i32::MAX);
    let (rang, apres) = match (ensuite, f.elements.get(f.index)) {
        (true, Some(c)) => (c.rang_source, f.index + 1),
        _ => (
            f.elements.iter().map(|e| e.rang_source).max().unwrap_or(-1),
            f.elements.len(),
        ),
    };
    for e in f.elements.iter_mut().filter(|e| e.rang_source > rang) {
        e.rang_source += k;
    }
    let nouveaux: Vec<Elem> = pistes
        .into_iter()
        .zip(1..)
        .map(|(piste, i)| Elem {
            id: Uuid::new_v4(),
            piste,
            rang_source: rang + i,
            suggeree: false,
        })
        .collect();
    f.elements.splice(apres..apres, nouveaux);
    fermer(&state, p.sub(), e).await
}

pub async fn retirer(
    State(state): State<AppState>,
    p: Personne,
    corps: Body,
) -> Result<Response, Response> {
    let corps = objet(corps).await?;
    let items = corps
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| invalide("items : liste attendue"))?
        .iter()
        .map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
        .collect::<Option<Vec<Uuid>>>()
        .ok_or_else(|| invalide("items : identifiant invalide"))?;
    let mut e = ouvrir(&state, p.sub(), &corps).await?;
    let f = &mut e.file;
    let courant = f.courant();
    let avant = f.elements[..f.index.min(f.elements.len())]
        .iter()
        .filter(|e| items.contains(&e.id))
        .count();
    f.elements.retain(|e| !items.contains(&e.id));
    if courant.is_some_and(|c| items.contains(&c)) {
        f.index -= avant;
        f.position = 0.0;
    } else {
        f.suivre(courant);
    }
    fermer(&state, p.sub(), e).await
}

pub async fn deplacer(
    State(state): State<AppState>,
    p: Personne,
    corps: Body,
) -> Result<Response, Response> {
    let corps = objet(corps).await?;
    let depuis = entier(&corps, "from")?.ok_or_else(|| invalide("from requis"))?;
    let nombre = entier(&corps, "count")?.unwrap_or(1);
    let vers = entier(&corps, "to")?.ok_or_else(|| invalide("to requis"))?;
    let mut e = ouvrir(&state, p.sub(), &corps).await?;
    let f = &mut e.file;
    let courant = f.courant();
    if !ordre::deplacer(&mut f.elements, depuis, nombre, vers) {
        return Err(invalide("plage hors de la file"));
    }
    f.suivre(courant);
    fermer(&state, p.sub(), e).await
}

pub async fn element(pool: &PgPool, sub: &str, id: Uuid) -> Result<Option<bool>, sqlx::Error> {
    sqlx::query_scalar("SELECT suggeree FROM files_elements WHERE sub = $1 AND id = $2")
        .bind(sub)
        .bind(id)
        .fetch_optional(pool)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avancer_passe_les_absentes_et_boucle_selon_la_repetition() {
        let p = [true, false, true, false];
        assert_eq!(avancer(&p, 0, 1, false), Some(2));
        assert_eq!(avancer(&p, 2, 1, false), None);
        assert_eq!(avancer(&p, 2, 1, true), Some(0));
        assert_eq!(avancer(&p, 0, -1, false), None);
        assert_eq!(avancer(&p, 0, -1, true), Some(2));
        assert_eq!(avancer(&[false, false], 0, 1, true), None);
        assert_eq!(avancer(&[], 0, 1, true), None);
    }
}

#[cfg(test)]
mod tests_api {
    use axum::http::StatusCode;
    use serde_json::{Value, json};

    use crate::{
        playlists::tests::{catalogue, pistes_de},
        testutil::{appel_sur, env},
    };

    #[tokio::test]
    async fn file_enregistree_sur_un_appareil_rendue_a_l_identique_sur_un_autre() {
        let Some(e) = env().await else { return };
        let p = catalogue(e.state.pool(), 9).await;
        let vide = e.get("alice", "/queue").await.json();
        assert_eq!(
            (vide["revision"].clone(), vide["items"].clone()),
            (json!(0), json!([]))
        );
        let piste = e.get("alice", &format!("/tracks/{}", p[0])).await.json();
        let album = piste["album"]["id"].as_str().unwrap().to_string();
        let r = e
            .appel(
                Some("alice"),
                "PUT",
                "/queue",
                Some(json!({"revision": 0, "source": {"type": "album", "id": album}, "index": 1, "shuffle": true, "repeat": "all", "suggestions": "light"})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::OK, "{}", r.texte());
        let f = r.json();
        assert_eq!(f["revision"], 1);
        assert_eq!(f["source"]["name"], "Second");
        assert_eq!(f["items"].as_array().unwrap().len(), 3);
        assert_eq!(
            f["items"][0]["track"]["id"],
            p[3].as_str(),
            "piste de départ en tête"
        );
        assert_eq!(f["index"], 0);
        assert!(f["seed"].as_i64().is_some());
        assert!(
            f["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["suggested"] == false)
        );
        let r = e
            .appel(
                Some("alice"),
                "PATCH",
                "/queue",
                Some(json!({"revision": 1, "index": 2, "position": 42.5})),
            )
            .await;
        let ecrite = r.json();
        assert_eq!(
            (
                ecrite["index"].clone(),
                ecrite["position"].clone(),
                ecrite["revision"].clone()
            ),
            (json!(2), json!(42.5), json!(2))
        );
        let ailleurs = e
            .appel_avec(
                None,
                "GET",
                "/queue",
                None,
                &[("cookie", "__Secure-plbls_sid=alice")],
            )
            .await
            .json();
        assert_eq!(ailleurs, ecrite);
        assert_eq!(
            (
                ailleurs["shuffle"].clone(),
                ailleurs["repeat"].clone(),
                ailleurs["suggestions"].clone()
            ),
            (json!(true), json!("all"), json!("light"))
        );
        assert_eq!(e.get("bruno", "/queue").await.json()["revision"], 0);
        for corps in [
            json!({"index": 0}),
            json!({"revision": 2, "index": 3}),
            json!({"revision": 2, "position": -1}),
            json!({"revision": 2, "repeat": "jamais"}),
            json!({"revision": 2, "suggestions": "beaucoup"}),
            json!({"revision": 2, "step": 2}),
        ] {
            let r = e
                .appel(Some("alice"), "PATCH", "/queue", Some(corps.clone()))
                .await;
            assert_eq!(r.statut, StatusCode::BAD_REQUEST, "{corps}");
        }
        assert_eq!(
            e.get("alice", "/queue").await.json(),
            ecrite,
            "refus sans effet"
        );
    }

    #[tokio::test]
    async fn deux_appareils_simultanes_le_second_recoit_409_et_l_etat_courant() {
        let Some(e) = env().await else { return };
        let p = catalogue(e.state.pool(), 5).await;
        let mut f = e
            .appel(
                Some("alice"),
                "PUT",
                "/queue",
                Some(json!({"revision": 0, "tracks": p})),
            )
            .await
            .json();
        for tour in 0..5 {
            let revision = f["revision"].as_i64().unwrap();
            let modifs = [
                json!({"repeat": if tour % 2 == 0 { "one" } else { "all" }}),
                json!({"position": 10.0 + f64::from(tour)}),
            ];
            let taches: Vec<_> = modifs
                .iter()
                .map(|m| {
                    let (app, mut corps) = (e.app.clone(), m.clone());
                    corps["revision"] = json!(revision);
                    tokio::spawn(async move {
                        appel_sur(&app, Some("alice"), "PATCH", "/queue", Some(corps), &[]).await
                    })
                })
                .collect();
            let mut reps = Vec::new();
            for t in taches {
                reps.push(t.await.unwrap());
            }
            let statuts: Vec<StatusCode> = reps.iter().map(|r| r.statut).collect();
            assert!(
                statuts.contains(&StatusCode::OK) && statuts.contains(&StatusCode::CONFLICT),
                "{statuts:?}"
            );
            let (i_ok, i_refus) = if reps[0].statut == StatusCode::OK {
                (0, 1)
            } else {
                (1, 0)
            };
            let refus = reps[i_refus].json();
            assert_eq!(refus["error"], "conflict");
            assert_eq!(
                refus["queue"],
                reps[i_ok].json(),
                "état courant rendu avec le 409"
            );
            assert_eq!(refus["queue"]["revision"], revision + 1);
            let mut corps = modifs[i_refus].clone();
            corps["revision"] = refus["queue"]["revision"].clone();
            let r = e.appel(Some("alice"), "PATCH", "/queue", Some(corps)).await;
            assert_eq!(r.statut, StatusCode::OK);
            f = r.json();
            assert_eq!(f["repeat"], if tour % 2 == 0 { "one" } else { "all" });
            assert_eq!(f["position"], 10.0 + f64::from(tour));
            assert_eq!(f["revision"], revision + 2);
        }
        let r = e
            .appel(
                Some("alice"),
                "POST",
                "/queue/tracks",
                Some(json!({"revision": 1, "tracks": [p[0]]})),
            )
            .await;
        assert_eq!(r.statut, StatusCode::CONFLICT);
        assert_eq!(pistes_de(&r.json()["queue"]), p);
    }

    fn ids(f: &Value) -> Vec<String> {
        f["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap().to_string())
            .collect()
    }

    #[tokio::test]
    async fn lire_ensuite_ajouter_retirer_deplacer_aleatoire_et_pistes_absentes() {
        let Some(e) = env().await else { return };
        let p = catalogue(e.state.pool(), 8).await;
        let ecrire = |m: &'static str, chemin: &'static str, f: &Value, mut corps: Value| {
            corps["revision"] = f["revision"].clone();
            let e = &e;
            async move {
                let r = e.appel(Some("alice"), m, chemin, Some(corps)).await;
                assert_eq!(r.statut, StatusCode::OK, "{m} {chemin} : {}", r.texte());
                r.json()
            }
        };
        let f = ecrire(
            "PUT",
            "/queue",
            &json!({"revision": 0}),
            json!({"tracks": p[..5], "index": 1}),
        )
        .await;
        let f = ecrire(
            "POST",
            "/queue/tracks",
            &f,
            json!({"mode": "next", "tracks": [p[6]]}),
        )
        .await;
        assert_eq!(
            pistes_de(&f),
            [&p[0], &p[1], &p[6], &p[2], &p[3], &p[4]].map(String::clone)
        );
        let f = ecrire("POST", "/queue/tracks", &f, json!({"tracks": [p[7]]})).await;
        assert_eq!(pistes_de(&f).last(), Some(&p[7]));
        assert_eq!(f["index"], 1);
        let courant = ids(&f)[1].clone();
        let f = ecrire(
            "POST",
            "/queue/move",
            &f,
            json!({"from": 0, "count": 2, "to": 3}),
        )
        .await;
        assert_eq!(
            pistes_de(&f),
            [&p[6], &p[2], &p[3], &p[0], &p[1], &p[4], &p[7]].map(String::clone)
        );
        assert_eq!(
            (f["index"].clone(), ids(&f)[4].clone()),
            (json!(4), courant.clone()),
            "la piste courante suit"
        );
        let melange = ecrire("PATCH", "/queue", &f, json!({"shuffle": true})).await;
        assert_eq!(
            (melange["index"].clone(), ids(&melange)[0].clone()),
            (json!(0), courant.clone())
        );
        let mut a = ids(&melange);
        let mut b = ids(&f);
        a.sort();
        b.sort();
        assert_eq!(a, b);
        assert_ne!(melange["seed"], f["seed"]);
        let f2 = ecrire(
            "POST",
            "/queue/tracks",
            &melange,
            json!({"mode": "next", "tracks": [p[5]]}),
        )
        .await;
        assert_eq!(pistes_de(&f2)[1], p[5]);
        let ordre = ecrire("PATCH", "/queue", &f2, json!({"shuffle": false})).await;
        assert_eq!(
            pistes_de(&ordre),
            [&p[6], &p[2], &p[3], &p[0], &p[1], &p[5], &p[4], &p[7]].map(String::clone)
        );
        assert_eq!(
            (ordre["index"].clone(), ids(&ordre)[4].clone()),
            (json!(4), courant.clone())
        );
        let a_retirer = vec![ids(&ordre)[0].clone(), courant.clone()];
        let f = ecrire("POST", "/queue/remove", &ordre, json!({"items": a_retirer})).await;
        assert_eq!(
            pistes_de(&f),
            [&p[2], &p[3], &p[0], &p[5], &p[4], &p[7]].map(String::clone)
        );
        assert_eq!(f["index"], 3, "la suivante devient courante");
        sqlx::query("UPDATE pistes SET present = false WHERE id = ANY($1::uuid[])")
            .bind(vec![
                uuid::Uuid::parse_str(&p[4]).unwrap(),
                uuid::Uuid::parse_str(&p[7]).unwrap(),
            ])
            .execute(e.state.pool())
            .await
            .unwrap();
        let mut corps = json!({"step": 1, "revision": f["revision"]});
        let r = e
            .appel(Some("alice"), "PATCH", "/queue", Some(corps.clone()))
            .await;
        assert_eq!(
            (r.statut, r.json()["error"].clone()),
            (StatusCode::UNPROCESSABLE_ENTITY, json!("end_of_queue"))
        );
        corps["repeat"] = json!("all");
        let r = e.appel(Some("alice"), "PATCH", "/queue", Some(corps)).await;
        assert_eq!(
            r.json()["index"],
            0,
            "absentes passées, retour au début : {}",
            r.texte()
        );
        assert_eq!(r.json()["items"][4]["track"]["available"], false);
        let f = ecrire("PATCH", "/queue", &r.json(), json!({"step": -1})).await;
        assert_eq!(f["index"], 3);
    }
}
