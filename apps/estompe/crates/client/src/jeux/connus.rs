use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Entree {
    name: String,
    #[serde(default)]
    exe: Vec<String>,
    #[serde(default)]
    path: Vec<String>,
}

fn entrees() -> &'static [Entree] {
    static LISTE: OnceLock<Vec<Entree>> = OnceLock::new();
    LISTE.get_or_init(|| {
        serde_json::from_str(include_str!("connus.json")).expect("connus.json invalide")
    })
}

const BIBLIOTHEQUES: &[&str] = &[
    "/steamapps/common/",
    "/epic games/",
    "/gog galaxy/games/",
    "/gog games/",
    "/xboxgames/",
];
const DOSSIERS_HORS_JEU: &[&str] = &[
    "launcher",
    "epiconlineservices",
    "steamworks shared",
    "directxredist",
    "wallpaper_engine",
    "steam controller configs",
];

pub fn nom_connu(exe: &str, chemin: &str) -> Option<String> {
    let (exe, minuscules) = (
        exe.to_ascii_lowercase(),
        chemin.replace('\\', "/").to_ascii_lowercase(),
    );
    if let Some(e) = entrees()
        .iter()
        .find(|e| e.exe.contains(&exe) || e.path.iter().any(|p| minuscules.contains(p.as_str())))
    {
        return Some(e.name.clone());
    }
    let original = chemin.replace('\\', "/");
    BIBLIOTHEQUES.iter().find_map(|bibliotheque| {
        let debut = minuscules.find(bibliotheque)? + bibliotheque.len();
        let dossier = original
            .get(debut..)?
            .split('/')
            .next()
            .filter(|d| !d.is_empty() && d.len() < original.len() - debut)?;
        (!DOSSIERS_HORS_JEU.contains(&dossier.to_ascii_lowercase().as_str()))
            .then(|| dossier.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::nom_connu;

    #[test]
    fn liste_integree() {
        assert_eq!(
            nom_connu("cs2.exe", r"C:\Games\x\cs2.exe").as_deref(),
            Some("Counter-Strike 2")
        );
        assert_eq!(
            nom_connu(
                "javaw.exe",
                r"C:\Users\a\AppData\Roaming\.minecraft\runtime\bin\javaw.exe"
            )
            .as_deref(),
            Some("Minecraft")
        );
    }

    #[test]
    fn dossiers_de_lanceur() {
        assert_eq!(
            nom_connu(
                "lc.exe",
                r"D:\SteamLibrary\steamapps\common\Lethal Company\lc.exe"
            )
            .as_deref(),
            Some("Lethal Company")
        );
        assert_eq!(
            nom_connu("a.exe", r"C:\Program Files\Epic Games\Hades\bin\a.exe").as_deref(),
            Some("Hades")
        );
        assert_eq!(
            nom_connu("w.exe", r"C:\Steam\steamapps\common\wallpaper_engine\w.exe"),
            None
        );
        assert_eq!(
            nom_connu(
                "EpicGamesLauncher.exe",
                r"C:\Program Files\Epic Games\Launcher\Portal\x.exe"
            ),
            None
        );
        assert_eq!(nom_connu("notepad.exe", r"C:\Windows\notepad.exe"), None);
    }

    #[test]
    fn un_fichier_pose_a_la_racine_d_une_bibliotheque_n_est_pas_un_jeu() {
        assert_eq!(nom_connu("x.exe", r"C:\Steam\steamapps\common\x.exe"), None);
    }

    #[test]
    fn la_liste_integree_se_lit() {
        assert!(super::entrees().len() >= 30);
        assert!(super::entrees()
            .iter()
            .all(|e| !e.name.is_empty() && !(e.exe.is_empty() && e.path.is_empty())));
    }
}
