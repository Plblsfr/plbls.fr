mod bobyn;
mod foreground;
mod replays;
mod capture;
mod screen_native;
mod toast;
mod webview_data;

pub(crate) use estompe_core::{commands::show_main, debuglog};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::WindowEvent;

pub fn run() {
    if let Some(dossier) = webview_data::dossier_donnees() {
        webview_data::purger_si_nouvelle_version(&dossier, env!("CARGO_PKG_VERSION"));
    }
    let builder = tauri::Builder::default();
    // Doit être le premier plugin : la seconde instance transmet ses arguments à la première, puis quitte
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
        debuglog::log("info", "instance", &format!("second lancement refusé, arguments transmis : {args:?}"));
        show_main(app);
    }));
    let builder = builder
        // Micro : sans réponse explicite, WebView2 refuse silencieusement getUserMedia. La page est notre propre bundle (CSP restrictive).
        .on_permission_request(|_, kind| match kind {
            tauri::webview::PermissionKind::Microphone => tauri::webview::PermissionResponse::Allow,
            _ => tauri::webview::PermissionResponse::Default,
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(estompe_core::commands::OAuth::default())
        .manage(screen_native::ScreenNative::default())
        .manage(bobyn::Bobyn::default())
        .manage(replays::Replays::default())
        .setup(|#[allow(unused_variables)] app| {
            {
                use tauri::Manager as _;
                if let Ok(dir) = app.path().app_log_dir() {
                    debuglog::init(dir);
                }
            }
            // Les mises à jour ne sont actives que si la configuration de build contient `plugins.updater` (clé publique et adresse).
            // Raccourci global du push-to-talk : enregistré par le front uniquement en salon vocal et fenêtre hors focus.
                        app.handle().plugin(tauri_plugin_global_shortcut::Builder::new().build())?;
                        if app.config().plugins.0.contains_key("updater") {
                app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
            }
                        {
                // Icône dans la zone de notification : la fenêtre se masque au lieu de se fermer (vocal et notifications restent
                // actifs) et WebView2 cesse de dessiner tant qu'elle est cachée, donc aucun coût dans un jeu.
                let ouvrir = MenuItem::with_id(app, "ouvrir", "Ouvrir Estompe", true, None::<&str>)?;
                let quitter = MenuItem::with_id(app, "quitter", "Quitter", true, None::<&str>)?;
                TrayIconBuilder::new()
                    .icon(app.default_window_icon().cloned().ok_or("icône absente")?)
                    .tooltip("Estompe")
                    .menu(&Menu::with_items(app, &[&ouvrir, &quitter])?)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, e| match e.id.as_ref() {
                        "ouvrir" => show_main(app),
                        _ => app.exit(0),
                    })
                    .on_tray_icon_event(|tray, e| {
                        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                            show_main(tray.app_handle());
                        }
                    })
                    .build(app)?;
            }
            bobyn::autostart(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![bobyn::bobyn_state, bobyn::bobyn_apply, bobyn::bobyn_save, replays::replays_list, replays::replays_thumb, replays::replays_tracks, replays::replays_rename, replays::replays_markers_set, replays::replays_delete, replays::replays_reveal,
            replays::replays_copy, replays::replays_export, replays::replays_cancel, replays::replays_plan, replays::replays_keyframes, replays::replays_encoders, replays::video_to_h264, estompe_core::commands::oauth_start, estompe_core::commands::oauth_wait, estompe_core::commands::native_fetch, estompe_core::commands::secret_set, estompe_core::commands::secret_get, estompe_core::commands::secret_delete,
            foreground::foreground_info, capture::capture_sources, capture::capture_thumb, capture::capture_self,
            toast::toast_show, screen_native::screen_native_start, screen_native::screen_native_stop,
            estompe_core::debuglog::debug_set, estompe_core::debuglog::debug_write, estompe_core::debuglog::debug_read, estompe_core::debuglog::debug_clear]);
    // La fenêtre se masque au lieu de se fermer (bureau uniquement : sur mobile, c'est le système qui gère l'app).
        let builder = builder.on_window_event(|w, e| {
        if let WindowEvent::CloseRequested { api, .. } = e {
            api.prevent_close();
            let _ = w.hide();
        }
    });
    builder
        .run(tauri::generate_context!())
        .expect("erreur au lancement d'Estompe");
}
