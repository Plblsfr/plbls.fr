use crate::{Erreur, Resultat};
use windows::core::HSTRING;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};

fn echec(quoi: &str, code: u32) -> Erreur {
    Erreur::Echec(format!("registre, {quoi} : erreur {code}"))
}

pub fn lire(cle: &str, nom: &str) -> Resultat<Option<String>> {
    let (cle_w, nom_w) = (HSTRING::from(cle), HSTRING::from(nom));
    let mut taille = 0u32;
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &cle_w,
            &nom_w,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut taille),
        )
    };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if code != ERROR_SUCCESS {
        return Err(echec("lecture", code.0));
    }
    let mut tampon = vec![0u16; (taille as usize).div_ceil(2)];
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &cle_w,
            &nom_w,
            RRF_RT_REG_SZ,
            None,
            Some(tampon.as_mut_ptr().cast()),
            Some(&mut taille),
        )
    };
    if code == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if code != ERROR_SUCCESS {
        return Err(echec("lecture", code.0));
    }
    let fin = tampon.iter().position(|c| *c == 0).unwrap_or(tampon.len());
    Ok(Some(String::from_utf16_lossy(&tampon[..fin])))
}

pub fn ecrire(cle: &str, nom: &str, valeur: &str) -> Resultat<()> {
    let donnees: Vec<u16> = valeur.encode_utf16().chain(std::iter::once(0)).collect();
    let code = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(cle),
            &HSTRING::from(nom),
            REG_SZ.0,
            Some(donnees.as_ptr().cast()),
            (donnees.len() * 2) as u32,
        )
    };
    if code != ERROR_SUCCESS {
        return Err(echec("écriture", code.0));
    }
    Ok(())
}

pub fn effacer(cle: &str, nom: &str) -> Resultat<()> {
    let code =
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(cle), &HSTRING::from(nom)) };
    if code != ERROR_SUCCESS && code != ERROR_FILE_NOT_FOUND {
        return Err(echec("suppression", code.0));
    }
    Ok(())
}
