//! Jeux reconnus sans réglage, dans l'esprit de la liste de jeux détectables de Discord (en bien plus petit) :
//! une liste intégrée (`games.json`), puis les dossiers de bibliothèque des lanceurs (Steam, Epic, GOG, Xbox), dont le nom du dossier fait le nom du jeu.
use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Entry {
    name: String,
    /// Nom du fichier exécutable (minuscules).
    #[serde(default)]
    exe: Vec<String>,
    /// Morceau du chemin complet (minuscules, séparateur « / »).
    #[serde(default)]
    path: Vec<String>,
}

fn entries() -> &'static [Entry] {
    static LIST: OnceLock<Vec<Entry>> = OnceLock::new();
    LIST.get_or_init(|| serde_json::from_str(include_str!("games.json")).expect("games.json invalide"))
}

/// Dossiers de bibliothèque : le jeu porte le nom du dossier qui suit.
const LIBRARIES: &[&str] = &["/steamapps/common/", "/epic games/", "/gog galaxy/games/", "/gog games/", "/xboxgames/"];
/// Dossiers de ces bibliothèques qui ne sont pas des jeux.
const NOT_GAMES: &[&str] = &["launcher", "epiconlineservices", "steamworks shared", "directxredist", "wallpaper_engine", "steam controller configs"];

/// Nom du jeu pour un exécutable (nom de fichier, chemin complet), None s'il n'est pas reconnu.
pub fn name_of(exe: &str, path: &str) -> Option<String> {
    let (exe, lower) = (exe.to_ascii_lowercase(), path.replace('\\', "/").to_ascii_lowercase());
    if let Some(e) = entries().iter().find(|e| e.exe.contains(&exe) || e.path.iter().any(|p| lower.contains(p.as_str()))) {
        return Some(e.name.clone());
    }
    let original = path.replace('\\', "/");
    LIBRARIES.iter().find_map(|lib| {
        let at = lower.find(lib)? + lib.len();
        let folder = original[at..].split('/').next().filter(|f| !f.is_empty() && f.len() < original.len() - at)?; // un dossier, pas un fichier posé à la racine
        (!NOT_GAMES.contains(&folder.to_ascii_lowercase().as_str())).then(|| folder.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::name_of;

    #[test]
    fn liste_integree() {
        assert_eq!(name_of("cs2.exe", r"C:\Games\x\cs2.exe").as_deref(), Some("Counter-Strike 2"));
        assert_eq!(name_of("javaw.exe", r"C:\Users\a\AppData\Roaming\.minecraft\runtime\bin\javaw.exe").as_deref(), Some("Minecraft"));
    }

    #[test]
    fn dossiers_de_lanceur() {
        assert_eq!(name_of("lc.exe", r"D:\SteamLibrary\steamapps\common\Lethal Company\lc.exe").as_deref(), Some("Lethal Company"));
        assert_eq!(name_of("a.exe", r"C:\Program Files\Epic Games\Hades\bin\a.exe").as_deref(), Some("Hades"));
        assert_eq!(name_of("w.exe", r"C:\Steam\steamapps\common\wallpaper_engine\w.exe"), None);
        assert_eq!(name_of("EpicGamesLauncher.exe", r"C:\Program Files\Epic Games\Launcher\Portal\x.exe"), None);
        assert_eq!(name_of("notepad.exe", r"C:\Windows\notepad.exe"), None);
    }
}
