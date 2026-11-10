//! Bobyn (alpha) : replay instantané de la partie en cours, Windows seulement. Le moteur est dans `bobyn/` ; ici, les commandes de l'onglet.
use bobyn::config::Config;
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
pub struct Bobyn {
    #[cfg(windows)]
    svc: Mutex<Option<bobyn::Service>>,
}

fn config_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_config_dir().map_err(|e| e.to_string())?.join("bobyn.json"))
}

#[cfg(windows)]
fn run(app: &AppHandle, state: &Bobyn, cfg: Config) -> Result<(), String> {
    use tauri_plugin_notification::NotificationExt;
    bobyn::set_logger(|m| crate::debuglog::log("info", "natif/bobyn", m));
    let mut svc = state.svc.lock().map_err(|e| e.to_string())?;
    if !cfg.enabled {
        *svc = None; // arrête la capture et libère les buffers
    } else if let Some(s) = svc.as_ref() {
        s.apply(cfg);
    } else {
        let app = app.clone();
        *svc = Some(bobyn::Service::start(cfg, move |e| match e {
            bobyn::Event::Status(s) => {
                crate::debuglog::log("info", "natif/bobyn", &s.text);
                let _ = app.emit("bobyn-status", s);
            }
            bobyn::Event::Saved(name) => {
                let _ = app.emit("bobyn-saved", name);
            }
            bobyn::Event::Marker(n) => {
                let _ = app.emit("bobyn-marker", n);
            }
            bobyn::Event::Notice { title, text } => {
                let _ = app.notification().builder().title(title).body(text).show();
            }
        })?);
    }
    Ok(())
}

#[cfg(not(windows))]
fn run(_: &AppHandle, _: &Bobyn, _: Config) -> Result<(), String> {
    Err("Bobyn ne fonctionne que sous Windows".into())
}

/// Liste de jeux de l'utilisateur (réglages de Bobyn, même désactivé).
pub fn games(app: &AppHandle) -> Vec<String> {
    config_path(app).map(|p| Config::load(&p).0.games).unwrap_or_default()
}

/// Au lancement de l'app : relance le service si l'utilisateur l'a activé.
pub fn autostart(app: &AppHandle) {
    let Ok(path) = config_path(app) else { return };
    let (cfg, _) = Config::load(&path);
    if cfg.enabled && cfg.check_dir().is_ok() {
        let _ = run(app, &app.state::<Bobyn>(), cfg);
    }
}

/// Réglages, état du service et ce que la machine propose (fenêtres, écrans, applications audio, micros, codecs).
#[tauri::command]
pub async fn bobyn_state(app: AppHandle, state: State<'_, Bobyn>) -> Result<serde_json::Value, String> {
    let (config, error) = Config::load(&config_path(&app)?);
    #[cfg(windows)]
    {
        use serde_json::json;
        let status = state.svc.lock().map_err(|e| e.to_string())?.as_ref().map(|s| s.status());
        let machine = tauri::async_runtime::spawn_blocking(|| {
            json!({
                "windows": bobyn::detect::windows().iter().map(|w| json!({"exe": w.exe, "title": w.title})).collect::<Vec<_>>(),
                "monitors": bobyn::detect::monitors().iter().map(|m| json!({"name": m.name, "width": m.width, "height": m.height, "primary": m.primary})).collect::<Vec<_>>(),
                "audio_apps": bobyn::audio::sessions(),
                "mics": bobyn::audio::mics().iter().map(|(id, name)| json!({"id": id, "name": name})).collect::<Vec<_>>(),
                "codecs": bobyn::codecs(),
            })
        })
        .await
        .map_err(|e| e.to_string())?;
        Ok(json!({"config": config, "error": error, "status": status, "machine": machine}))
    }
    #[cfg(not(windows))]
    {
        let _ = (state, config, error);
        Err("Bobyn ne fonctionne que sous Windows".into())
    }
}

/// Enregistre les réglages et les applique tout de suite (démarre ou arrête le service selon `enabled`).
#[tauri::command]
pub fn bobyn_apply(app: AppHandle, state: State<Bobyn>, config: Config) -> Result<(), String> {
    config.check_dir()?;
    config.store(&config_path(&app)?).map_err(|e| e.to_string())?;
    run(&app, &state, config)
}

/// Sauvegarde le buffer (`last` : seulement les dernières secondes).
#[tauri::command]
pub fn bobyn_save(state: State<Bobyn>, last: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        state.svc.lock().map_err(|e| e.to_string())?.as_ref().ok_or("Bobyn est désactivé")?.save(last);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (state, last);
        Err("Bobyn ne fonctionne que sous Windows".into())
    }
}
