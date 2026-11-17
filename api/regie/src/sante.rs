use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    response::Response,
};
use serde_json::{Value, json};

use crate::{
    arr::{Client, Service},
    config::Arr,
    etat::AppState,
    http::{indisponible, introuvable, ok, sans_contenu},
    qbittorrent,
};

fn config_de(state: &AppState, service: Service) -> Option<&Arr> {
    let c = state.config();
    match service {
        Service::Radarr => c.radarr.as_ref(),
        Service::Sonarr => c.sonarr.as_ref(),
        Service::Lidarr => c.lidarr.as_ref(),
        Service::Prowlarr => c.prowlarr.as_ref(),
    }
}

fn client(state: &AppState, service: Service) -> Option<Client<'_>> {
    config_de(state, service).map(|arr| Client::new(service, arr, state.http()))
}

fn non_configure(service: Service) -> Response {
    indisponible(
        "not_configured",
        &format!("{} n'est pas configuré", service.nom()),
    )
}

fn injoignable(service: Service, e: &str) -> Response {
    indisponible("upstream", &format!("{} : {e}", service.nom()))
}

fn fiche(cle: &str, nom: &str) -> Value {
    json!({
        "key": cle,
        "name": nom,
        "configured": false,
        "reachable": false,
        "version": null,
        "error": null,
        "alerts": [],
    })
}

#[derive(Default)]
struct Releve {
    fiche: Value,
    dossiers: Vec<Value>,
    bloques: Vec<Value>,
    indexeurs: Vec<Value>,
}

fn noter(fiche: &mut Value, quoi: &str, e: String) {
    if fiche["error"].is_null() {
        fiche["error"] = json!(format!("{quoi} : {e}"));
    }
}

async fn relever(state: &AppState, service: Service) -> Releve {
    let mut r = Releve {
        fiche: fiche(service.cle(), service.nom()),
        ..Releve::default()
    };
    let Some(c) = client(state, service) else {
        return r;
    };
    r.fiche["configured"] = json!(true);
    match c.version().await {
        Ok(v) => {
            r.fiche["reachable"] = json!(true);
            r.fiche["version"] = json!(v);
        }
        Err(e) => {
            r.fiche["error"] = json!(e);
            return r;
        }
    }
    match c.alertes().await {
        Ok(a) => r.fiche["alerts"] = json!(a),
        Err(e) => noter(&mut r.fiche, "alertes", e),
    }
    if service == Service::Prowlarr {
        match c.indexeurs().await {
            Ok(i) => r.indexeurs = i,
            Err(e) => noter(&mut r.fiche, "indexeurs", e),
        }
        return r;
    }
    match c.dossiers().await {
        Ok(d) => r.dossiers = d,
        Err(e) => noter(&mut r.fiche, "dossiers", e),
    }
    match c.bloques().await {
        Ok(b) => r.bloques = b,
        Err(e) => noter(&mut r.fiche, "file", e),
    }
    r
}

async fn relever_qbittorrent(state: &AppState) -> Value {
    let mut f = fiche("qbittorrent", "qBittorrent");
    let Some(cfg) = state.config().qbittorrent.as_ref() else {
        return f;
    };
    f["configured"] = json!(true);
    let c = qbittorrent::Client::new(cfg, state.http(), state.session_qbittorrent());
    match c.version().await {
        Ok(v) => {
            f["reachable"] = json!(true);
            f["version"] = json!(v);
        }
        Err(e) => {
            f["error"] = json!(e);
            return f;
        }
    }
    match c.transfert().await {
        Ok(t) => f["transfer"] = t,
        Err(e) => noter(&mut f, "débits", e),
    }
    f
}

pub async fn etat(State(state): State<AppState>) -> Response {
    let (radarr, sonarr, lidarr, prowlarr, qbittorrent) = tokio::join!(
        relever(&state, Service::Radarr),
        relever(&state, Service::Sonarr),
        relever(&state, Service::Lidarr),
        relever(&state, Service::Prowlarr),
        relever_qbittorrent(&state),
    );
    let mut services = Vec::new();
    let mut dossiers = Vec::new();
    let mut bloques = Vec::new();
    let mut indexeurs = Vec::new();
    for r in [radarr, sonarr, lidarr, prowlarr] {
        services.push(r.fiche);
        dossiers.extend(r.dossiers);
        bloques.extend(r.bloques);
        indexeurs.extend(r.indexeurs);
    }
    services.push(qbittorrent);
    ok(&json!({
        "services": services,
        "indexers": indexeurs,
        "disks": dossiers,
        "queue": bloques,
    }))
}

pub async fn tester_indexeur(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    let Some(c) = client(&state, Service::Prowlarr) else {
        return non_configure(Service::Prowlarr);
    };
    match c.tester_indexeur(id).await {
        Ok(Some(Ok(()))) => ok(&json!({ "ok": true, "message": null })),
        Ok(Some(Err(m))) => ok(&json!({ "ok": false, "message": m })),
        Ok(None) => introuvable(),
        Err(e) => injoignable(Service::Prowlarr, &e),
    }
}

pub async fn retirer(
    State(state): State<AppState>,
    Path((service, id)): Path<(String, i64)>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(service) = Service::bibliotheque(&service) else {
        return introuvable();
    };
    let Some(c) = client(&state, service) else {
        return non_configure(service);
    };
    let relancer = q.get("retry").is_some_and(|v| v == "true");
    match c.retirer(id, relancer).await {
        Ok(true) => sans_contenu(),
        Ok(false) => introuvable(),
        Err(e) => injoignable(service, &e),
    }
}
