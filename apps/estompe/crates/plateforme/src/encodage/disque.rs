use std::io;
use std::path::Path;

#[cfg(windows)]
pub const ERREUR_DISQUE_PLEIN: i32 = 112;
#[cfg(not(windows))]
pub const ERREUR_DISQUE_PLEIN: i32 = 28;

#[cfg(windows)]
const DISQUE_PLEIN: &[i32] = &[39, 112, 1295];
#[cfg(not(windows))]
const DISQUE_PLEIN: &[i32] = &[28, 122];

pub fn disque_plein(e: &io::Error) -> bool {
    e.raw_os_error().is_some_and(|c| DISQUE_PLEIN.contains(&c))
}

#[cfg(windows)]
pub fn masquer(chemin: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN};
    let large: Vec<u16> = chemin.as_os_str().encode_wide().chain([0]).collect();
    unsafe {
        SetFileAttributesW(large.as_ptr(), FILE_ATTRIBUTE_HIDDEN);
    }
}

#[cfg(not(windows))]
pub fn masquer(_: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconnait_le_disque_plein() {
        assert!(disque_plein(&io::Error::from_raw_os_error(
            ERREUR_DISQUE_PLEIN
        )));
        assert!(!disque_plein(&io::Error::from_raw_os_error(2)));
        assert!(!disque_plein(&io::Error::other("x")));
        let dossier = tempfile::tempdir().unwrap();
        masquer(dossier.path());
        assert!(dossier.path().is_dir());
    }
}
