//! Jeu au premier plan (overlay des voix, pastille Bobyn) et jeu en cours (activité) : la détection est celle de Bobyn
//! (`bobyn::detect`), avec la liste de jeux de l'utilisateur ; ce qu'Estompe affiche est donc ce que Bobyn va capturer.

#[derive(serde::Serialize, Default)]
pub struct Foreground {
    /// Un jeu est au premier plan.
    game: bool,
    /// Jeu en cours, au premier plan ou non.
    name: Option<String>,
    /// Écran de la fenêtre au premier plan, en pixels physiques (x, y, largeur, hauteur).
    monitor: Option<[i32; 4]>,
}

#[cfg(windows)]
fn read(games: &[String]) -> Foreground {
    let f = bobyn::detect::foreground(games);
    Foreground { game: f.game.is_some(), name: f.running, monitor: f.monitor }
}

#[cfg(not(windows))]
fn read(_: &[String]) -> Foreground {
    Foreground::default()
}

#[tauri::command]
pub fn foreground_info(app: tauri::AppHandle) -> Foreground {
    read(&crate::bobyn::games(&app))
}
