use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{AppHandle, Manager, State};

use super::galerie;
use super::types::{Clip, Codec, Disponibilite, EtatPublic, Options, Reglages, Stockage};

#[cfg(not(windows))]
const HORS_WINDOWS: &str = "instant replay disponible sous Windows uniquement";

#[derive(Default)]
pub struct Replay {
    reglages: Mutex<Reglages>,
    autorise: Mutex<Option<PathBuf>>,
    pub(super) exports: super::montage::Exports,
    #[cfg(windows)]
    moteur: std::sync::OnceLock<super::win::Moteur>,
}

impl Replay {
    pub(super) fn reglages(&self) -> Reglages {
        self.reglages.lock().map(|r| r.clone()).unwrap_or_default()
    }

    #[cfg(windows)]
    fn moteur(&self, app: &AppHandle) -> &super::win::Moteur {
        self.moteur.get_or_init(|| super::win::Moteur::demarrer(app.clone()))
    }
}

pub(super) fn dossier(app: &AppHandle, etat: &Replay) -> Result<PathBuf, String> {
    let dossier = galerie::dossier_effectif(etat.reglages().dossier.as_deref(), app.path().video_dir().ok())?;
    let mut autorise = etat.autorise.lock().map_err(|e| e.to_string())?;
    if autorise.as_ref() != Some(&dossier) {
        app.asset_protocol_scope().allow_directory(&dossier, true).map_err(|e| e.to_string())?;
        *autorise = Some(dossier.clone());
    }
    Ok(dossier)
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn creer_vignette(app: &AppHandle, dossier: &Path, clip: &Path) -> Result<PathBuf, String> {
    let sortie = galerie::annexe(dossier, clip, "jpg");
    let resultat = crate::video::command(app)?.args(galerie::arguments_vignette(clip, &sortie)).output().map_err(|e| e.to_string())?;
    if resultat.status.success() {
        Ok(sortie)
    } else {
        Err(String::from_utf8_lossy(&resultat.stderr).lines().last().unwrap_or("échec de ffmpeg").to_string())
    }
}

pub(crate) fn convertir(app: &AppHandle, dossier: &Path, source: &Path, taille_max: Option<u64>) -> Result<PathBuf, String> {
    let taille = std::fs::metadata(source).map_err(|e| e.to_string())?.len();
    let annexe = galerie::lire_annexe(dossier, source);
    let pistes = annexe.as_ref().map_or(1, |a| a.pistes_audio);
    let tient = taille_max.map_or(true, |m| taille <= m);
    let copie = tient && annexe.as_ref().is_some_and(|a| a.codec == Codec::H264);
    if copie && pistes <= 1 {
        return Ok(source.to_path_buf());
    }
    let duree = annexe.as_ref().map(|a| a.duree_s).filter(|d| *d > 0.0).unwrap_or_else(|| crate::video::duration(app, source));
    let actuel = crate::video::convert_kbps(taille, duree);
    let kbps = match taille_max {
        Some(m) => galerie::debit_pour_taille(m, duree, 128).ok_or("clip trop long pour tenir dans la limite d'envoi")?.min(actuel),
        None => actuel,
    };
    let hauteur = annexe.as_ref().and_then(|a| galerie::hauteur_pour_debit(kbps, a.hauteur));
    let envois = galerie::creer_annexes(dossier).map_err(|e| e.to_string())?.join(galerie::ENVOIS);
    std::fs::create_dir_all(&envois).map_err(|e| e.to_string())?;
    let sortie = envois.join(format!("{}.mp4", crate::video::temp_tag()));
    let encodeurs: Vec<Option<&str>> = if copie { vec![None] } else { crate::video::encoders(app).iter().map(|e| Some(*e)).collect() };
    let mut dernier = "aucun encodeur matériel H.264 disponible".to_string();
    for encodeur in encodeurs {
        let arguments = galerie::arguments_envoi(source, &sortie, encodeur.map(|e| (e, kbps, hauteur)), pistes);
        match crate::video::command(app)?.args(arguments).output() {
            Ok(o) if o.status.success() => {
                let obtenu = std::fs::metadata(&sortie).map_or(0, |m| m.len());
                if taille_max.is_some_and(|m| obtenu > m) {
                    let _ = std::fs::remove_file(&sortie);
                    return Err("le clip réencodé dépasse encore la limite d'envoi".into());
                }
                return Ok(sortie);
            }
            Ok(o) => dernier = String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("échec de ffmpeg").to_string(),
            Err(e) => dernier = e.to_string(),
        }
    }
    let _ = std::fs::remove_file(&sortie);
    Err(dernier)
}

#[tauri::command(async)]
pub fn replay_disponible(app: AppHandle, etat: State<Replay>) -> Disponibilite {
    #[cfg(windows)]
    return etat.moteur(&app).disponibilite();
    #[cfg(not(windows))]
    {
        let _ = (app, etat);
        Disponibilite { disponible: false, raison: Some(HORS_WINDOWS.into()), encodeur: None }
    }
}

#[tauri::command(async)]
pub fn replay_options(app: AppHandle, etat: State<Replay>, reglages: Reglages) -> Result<Options, String> {
    let reglages = reglages.valider()?;
    #[cfg(windows)]
    return Ok(super::reglages::options(&reglages, &etat.moteur(&app).materiel()?));
    #[cfg(not(windows))]
    {
        let _ = (app, etat, reglages);
        Err(HORS_WINDOWS.into())
    }
}

#[tauri::command]
pub fn replay_configurer(app: AppHandle, etat: State<Replay>, reglages: Reglages) -> Result<(), String> {
    let reglages = reglages.valider()?;
    *etat.reglages.lock().map_err(|e| e.to_string())? = reglages.clone();
    let dossier = dossier(&app, &etat)?;
    galerie::purger_envois(&dossier);
    if etat.exports.en_cours().is_empty() {
        super::montage::purger_partiels(&dossier);
    }
    #[cfg(windows)]
    return etat.moteur(&app).envoyer(super::win::Message::Configurer(Box::new(reglages), dossier));
    #[cfg(not(windows))]
    Ok(())
}

#[tauri::command]
pub fn replay_etat(app: AppHandle, etat: State<Replay>) -> EtatPublic {
    #[cfg(windows)]
    return etat.moteur(&app).etat();
    #[cfg(not(windows))]
    {
        let _ = (app, etat);
        EtatPublic::simple("indisponible", Some(HORS_WINDOWS.into()))
    }
}

#[tauri::command]
pub fn replay_sauvegarder(app: AppHandle, etat: State<Replay>) -> Result<(), String> {
    #[cfg(windows)]
    return etat.moteur(&app).envoyer(super::win::Message::Sauvegarder);
    #[cfg(not(windows))]
    {
        let _ = (app, etat);
        Err(HORS_WINDOWS.into())
    }
}

#[tauri::command]
pub fn replay_partage(app: AppHandle, etat: State<Replay>, actif: bool, suspendre: Option<bool>) -> Result<(), String> {
    #[cfg(windows)]
    return etat.moteur(&app).envoyer(super::win::Message::Partage(actif, suspendre.unwrap_or(false)));
    #[cfg(not(windows))]
    {
        let _ = (app, etat, actif, suspendre);
        Err(HORS_WINDOWS.into())
    }
}

#[tauri::command(async)]
pub fn replay_clips(app: AppHandle, etat: State<Replay>) -> Result<Vec<Clip>, String> {
    Ok(galerie::lister(&dossier(&app, &etat)?))
}

#[tauri::command(async)]
pub fn replay_renommer(app: AppHandle, etat: State<Replay>, chemin: String, nom: String) -> Result<String, String> {
    let dossier = dossier(&app, &etat)?;
    let cible = galerie::renommer(&dossier, &chemin, &nom)?;
    if let (Some(ancien), Some(nouveau)) = (std::path::Path::new(&chemin).file_name(), cible.file_name()) {
        super::montage::suivre_renommage(&dossier, &ancien.to_string_lossy(), &nouveau.to_string_lossy());
    }
    Ok(cible.to_string_lossy().into_owned())
}

#[tauri::command(async)]
pub fn replay_supprimer(app: AppHandle, etat: State<Replay>, chemins: Vec<String>) -> Result<(), String> {
    galerie::supprimer(&dossier(&app, &etat)?, &chemins)
}

#[tauri::command]
pub fn replay_ouvrir_dossier(app: AppHandle, etat: State<Replay>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dossier = dossier(&app, &etat)?;
    std::fs::create_dir_all(&dossier).map_err(|e| format!("dossier des clips inaccessible : {e}"))?;
    app.opener().open_path(dossier.to_string_lossy(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn replay_stockage(app: AppHandle, etat: State<Replay>) -> Result<Stockage, String> {
    Ok(galerie::stockage(&dossier(&app, &etat)?, etat.reglages().quota_octets))
}

#[tauri::command]
pub async fn replay_vers_h264(app: AppHandle, etat: State<'_, Replay>, chemin: String, taille_max_octets: Option<u64>) -> Result<String, String> {
    let dossier = dossier(&app, &etat)?;
    let source = galerie::dans_dossier(&dossier, &chemin)?;
    let tache = tauri::async_runtime::spawn_blocking(move || convertir(&app, &dossier, &source, taille_max_octets));
    tache.await.map_err(|e| e.to_string())?.map(|c| c.to_string_lossy().into_owned())
}

async fn choisir(app: AppHandle, dossiers: bool) -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        let fenetre = app.get_webview_window("main").and_then(|f| f.hwnd().ok()).map_or(0, |h| h.0 as isize);
        tauri::async_runtime::spawn_blocking(move || super::win::dialogue::choisir(dossiers, fenetre)).await.map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = (app, dossiers);
        Err(HORS_WINDOWS.into())
    }
}

#[tauri::command]
pub async fn replay_choisir_executable(app: AppHandle) -> Result<Option<String>, String> {
    choisir(app, false).await
}

#[tauri::command]
pub async fn replay_choisir_dossier(app: AppHandle) -> Result<Option<String>, String> {
    choisir(app, true).await
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    #[test]
    fn dossier_des_clips_par_defaut() {
        assert_eq!(galerie::dossier_effectif(Replay::default().reglages().dossier.as_deref(), Some("/videos".into())), Ok("/videos/Estompe".into()));
    }
}
