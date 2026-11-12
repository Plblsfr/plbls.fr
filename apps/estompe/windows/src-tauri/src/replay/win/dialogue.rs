use windows::core::w;
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{FileOpenDialog, IFileOpenDialog, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH};

use super::materiel::texte;

fn ouvrir(dossiers: bool, fenetre: isize) -> windows::core::Result<Option<String>> {
    unsafe {
        let dialogue: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        let options = dialogue.GetOptions()?;
        dialogue.SetOptions(options | FOS_FORCEFILESYSTEM | if dossiers { FOS_PICKFOLDERS } else { FOS_FILEMUSTEXIST })?;
        if !dossiers {
            dialogue.SetFileTypes(&[COMDLG_FILTERSPEC { pszName: w!("Exécutables"), pszSpec: w!("*.exe") }])?;
        }
        match dialogue.Show(Some(HWND(fenetre as *mut _))) {
            Err(e) if e.code() == ERROR_CANCELLED.to_hresult() => return Ok(None),
            autre => autre?,
        }
        let chemin = dialogue.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
        let lu = chemin.to_string().ok();
        CoTaskMemFree(Some(chemin.0 as *const _));
        Ok(lu)
    }
}

pub fn choisir(dossiers: bool, fenetre: isize) -> Result<Option<String>, String> {
    let fil = std::thread::spawn(move || unsafe {
        let initialise = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let resultat = ouvrir(dossiers, fenetre).map_err(texte);
        if initialise {
            CoUninitialize();
        }
        resultat
    });
    fil.join().map_err(|_| "sélecteur de fichiers interrompu".to_string())?
}
