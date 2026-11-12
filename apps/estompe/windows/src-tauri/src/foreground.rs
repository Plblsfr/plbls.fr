#[derive(serde::Serialize, Default)]
pub struct Foreground {
    game: bool,
    name: Option<String>,
    monitor: Option<[i32; 4]>,
}

#[cfg(windows)]
fn read() -> Foreground {
    let f = crate::jeux::foreground();
    Foreground { game: f.game, name: f.running, monitor: f.monitor }
}

#[cfg(not(windows))]
fn read() -> Foreground {
    Foreground::default()
}

#[tauri::command]
pub fn foreground_info() -> Foreground {
    read()
}
