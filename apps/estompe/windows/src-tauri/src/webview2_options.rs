#![cfg_attr(not(windows), allow(dead_code))]

use std::fs;
use std::path::{Path, PathBuf};

pub const VARIABLE: &str = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";
const FICHIER: &str = "options-webview2";
const DESACTIVE: &str = "defaut";
pub const FONCTIONS_TAURI: &[&str] = &["msWebOOUI", "msPdfOOUI", "msSmartScreenProtection"];
pub const FONCTIONS_SOBRES: &[&str] = &["SpareRendererForSitePerProcess", "BackForwardCache"];
pub const OPTIONS_SOBRES: &[&str] = &[
    "--renderer-process-limit=1",
    "--disable-site-isolation-trials",
    "--js-flags=--optimize-for-size",
];

pub fn arguments_sobres() -> String {
    let fonctions = FONCTIONS_TAURI
        .iter()
        .chain(FONCTIONS_SOBRES)
        .copied()
        .collect::<Vec<_>>()
        .join(",");
    std::iter::once(format!("--disable-features={fonctions}"))
        .chain(OPTIONS_SOBRES.iter().map(|o| (*o).to_owned()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn fichier(dossier: &Path) -> PathBuf {
    dossier.join(FICHIER)
}

pub fn active(dossier: &Path) -> bool {
    fs::read_to_string(fichier(dossier)).map_or(true, |v| v.trim() != DESACTIVE)
}

pub fn enregistrer(dossier: &Path, oui: bool) -> std::io::Result<()> {
    if oui {
        match fs::remove_file(fichier(dossier)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        fs::create_dir_all(dossier)?;
        fs::write(fichier(dossier), DESACTIVE)
    }
}

pub fn appliquer(dossier: Option<&Path>) -> Option<String> {
    if std::env::var_os(VARIABLE).is_some() || dossier.is_some_and(|d| !active(d)) {
        return None;
    }
    let args = arguments_sobres();
    std::env::set_var(VARIABLE, &args);
    Some(args)
}

#[tauri::command]
pub fn webview2_options_get() -> bool {
    !crate::webview_data::dossier_donnees().is_some_and(|d| !active(&d))
}

#[tauri::command]
pub fn webview2_options_set(actives: bool) -> Result<(), String> {
    let dossier = crate::webview_data::dossier_donnees().ok_or("dossier de données introuvable")?;
    enregistrer(&dossier, actives).map_err(|e| e.to_string())
}

#[cfg(windows)]
pub fn niveau_memoire(fenetre: &tauri::WebviewWindow, bas: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    };
    use windows_core::Interface;
    let niveau = if bas {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
    } else {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
    };
    let _ = fenetre.with_webview(move |wv| unsafe {
        if let Ok(w) = wv
            .controller()
            .CoreWebView2()
            .and_then(|w| w.cast::<ICoreWebView2_19>())
        {
            let _ = w.SetMemoryUsageTargetLevel(niveau);
        }
    });
}

#[cfg(not(windows))]
pub fn niveau_memoire(_fenetre: &tauri::WebviewWindow, _bas: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier_temporaire(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("estompe-options-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn une_seule_liste_de_fonctions_desactivees_qui_garde_celles_de_tauri() {
        let args = arguments_sobres();
        assert_eq!(args.matches("--disable-features=").count(), 1);
        assert!(args.starts_with(
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,SpareRendererForSitePerProcess,BackForwardCache "
        ));
        assert!(args.contains("--renderer-process-limit=1"));
        assert!(args.contains("--disable-site-isolation-trials"));
        assert!(args.contains("--js-flags=--optimize-for-size"));
        assert!(!args.contains("CalculateNativeWinOcclusion"));
        assert!(!args.contains("--disable-extensions"));
        assert!(!args.contains("--disable-component-update"));
    }

    #[test]
    fn options_actives_par_defaut_et_desactivables() {
        let d = dossier_temporaire("choix");
        assert!(active(&d));
        enregistrer(&d, false).unwrap();
        assert!(!active(&d));
        enregistrer(&d, true).unwrap();
        assert!(active(&d));
        enregistrer(&d, true).unwrap();
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn configuration_tauri_sans_arguments_propres() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert!(conf["app"]["windows"][0]
            .get("additionalBrowserArgs")
            .is_none());
    }
}
