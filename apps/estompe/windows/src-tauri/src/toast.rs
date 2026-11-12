//! Notifications Windows natives avec boutons (« Répondre », « Marquer comme lu ») : le plugin de notification de Tauri
//! n'en gère ni les actions ni le clic sur le bureau. Le choix de l'utilisateur est renvoyé au front par l'évènement `toast-action`.

#[cfg(windows)]
#[tauri::command]
pub fn toast_show(app: tauri::AppHandle, title: String, body: String, href: String, conv: String) -> Result<(), String> {
    use tauri::Emitter;
    use tauri_winrt_notification::Toast;
    let id = app.config().identifier.clone();
    let actions = !conv.is_empty();
    let mut toast = Toast::new(&id).title(&title).text1(&body).on_activated(move |action| {
        crate::show_main(&app);
        // clic sur le corps : pas d'action, on ouvre la conversation
        let action = action.unwrap_or_else(|| "open".into());
        let _ = app.emit("toast-action", serde_json::json!({ "action": action, "href": href, "conv": conv }));
        Ok(())
    });
    if actions {
        toast = toast.add_button("Répondre", "open").add_button("Marquer comme lu", "read");
    }
    toast.show().map_err(|e| e.to_string())
}

#[cfg(not(windows))]
#[tauri::command]
pub fn toast_show(_title: String, _body: String, _href: String, _conv: String) -> Result<(), String> {
    Err("notifications natives : Windows uniquement".into())
}
