use super::types::{Clip, Disponibilite, EtatPublic, Options, Reglages, Stockage};

const ABSENT: &str = "instant replay absent de ce build";

#[derive(Default)]
pub struct Replay {}

fn absent<T>() -> Result<T, String> {
    Err(ABSENT.into())
}

#[tauri::command]
pub fn replay_disponible() -> Disponibilite {
    Disponibilite { disponible: false, raison: Some(ABSENT.into()), encodeur: None }
}

#[tauri::command]
pub fn replay_options(reglages: Reglages) -> Result<Options, String> {
    let _ = reglages;
    absent()
}

#[tauri::command]
pub fn replay_configurer(reglages: Reglages) -> Result<(), String> {
    let _ = reglages;
    absent()
}

#[tauri::command]
pub fn replay_etat() -> EtatPublic {
    EtatPublic::simple("indisponible", Some(ABSENT.into()))
}

#[tauri::command]
pub fn replay_sauvegarder() -> Result<(), String> {
    absent()
}

#[tauri::command]
pub fn replay_partage(actif: bool, suspendre: Option<bool>) -> Result<(), String> {
    let _ = (actif, suspendre);
    absent()
}

#[tauri::command]
pub fn replay_clips() -> Result<Vec<Clip>, String> {
    absent()
}

#[tauri::command]
pub fn replay_renommer(chemin: String, nom: String) -> Result<String, String> {
    let _ = (chemin, nom);
    absent()
}

#[tauri::command]
pub fn replay_supprimer(chemins: Vec<String>) -> Result<(), String> {
    let _ = chemins;
    absent()
}

#[tauri::command]
pub fn replay_ouvrir_dossier() -> Result<(), String> {
    absent()
}

#[tauri::command]
pub fn replay_stockage() -> Result<Stockage, String> {
    absent()
}

#[tauri::command]
pub async fn replay_vers_h264(chemin: String, taille_max_octets: Option<u64>) -> Result<String, String> {
    let _ = (chemin, taille_max_octets);
    absent()
}

#[tauri::command]
pub async fn replay_choisir_executable() -> Result<Option<String>, String> {
    absent()
}

#[tauri::command]
pub async fn replay_choisir_dossier() -> Result<Option<String>, String> {
    absent()
}

#[tauri::command]
pub async fn montage_ouvrir(chemin: Option<String>, projet: Option<String>) -> Result<serde_json::Value, String> {
    let _ = (chemin, projet);
    absent()
}

#[tauri::command]
pub async fn montage_clip(chemin: String) -> Result<serde_json::Value, String> {
    let _ = chemin;
    absent()
}

#[tauri::command]
pub fn montage_projets() -> Vec<serde_json::Value> {
    Vec::new()
}

#[tauri::command]
pub fn montage_renommer(projet: String, nom: String) -> Result<(), String> {
    let _ = (projet, nom);
    absent()
}

#[tauri::command]
pub fn montage_supprimer(projet: String) -> Result<(), String> {
    let _ = projet;
    absent()
}

#[tauri::command]
pub fn montage_enregistrer(projet: String, montage: serde_json::Value) -> Result<(), String> {
    let _ = (projet, montage);
    absent()
}

#[tauri::command]
pub fn montage_exporter(projet: String, montage: serde_json::Value, images: Vec<Vec<u8>>) -> Result<serde_json::Value, String> {
    let _ = (projet, montage, images);
    absent()
}

#[tauri::command]
pub fn montage_annuler(id: u64) -> Result<(), String> {
    let _ = id;
    absent()
}

#[tauri::command]
pub fn montage_exports() -> Vec<serde_json::Value> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indisponible_sans_la_feature() {
        assert!(!replay_disponible().disponible);
        assert_eq!(replay_etat().etat, "indisponible");
        assert!(replay_sauvegarder().is_err() && replay_clips().is_err());
        assert!(montage_annuler(1).is_err() && montage_exports().is_empty() && montage_projets().is_empty());
    }
}
