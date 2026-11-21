use crate::{Erreur, Image, Pastille, Resultat};
use raw_window_handle::RawWindowHandle;
use std::cell::{Cell, RefCell};
use windows::core::HSTRING;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject, HGDIOBJ};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, DestroyIcon, HICON, ICONINFO};

#[derive(Default)]
pub struct PastilleWindows {
    fenetre: Cell<Option<isize>>,
    barre: RefCell<Option<ITaskbarList3>>,
}

fn echec(e: windows::core::Error) -> Erreur {
    Erreur::Echec(format!("pastille : {e}"))
}

fn icone(image: &Image) -> Resultat<HICON> {
    let attendu = (image.largeur * image.hauteur * 4) as usize;
    if image.largeur == 0 || image.rgba.len() != attendu {
        return Err(Erreur::Refuse("image de pastille mal formée".into()));
    }
    let bgra: Vec<u8> = image
        .rgba
        .chunks_exact(4)
        .flat_map(|p| [p[2], p[1], p[0], p[3]])
        .collect();
    let masque = vec![0u8; (image.largeur.div_ceil(16) * 2 * image.hauteur) as usize];
    unsafe {
        let couleur = CreateBitmap(
            image.largeur as i32,
            image.hauteur as i32,
            1,
            32,
            Some(bgra.as_ptr().cast()),
        );
        let masque = CreateBitmap(
            image.largeur as i32,
            image.hauteur as i32,
            1,
            1,
            Some(masque.as_ptr().cast()),
        );
        let info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: masque,
            hbmColor: couleur,
        };
        let resultat = CreateIconIndirect(&info);
        let _ = DeleteObject(HGDIOBJ(couleur.0));
        let _ = DeleteObject(HGDIOBJ(masque.0));
        resultat.map_err(echec)
    }
}

impl PastilleWindows {
    fn barre(&self) -> Resultat<ITaskbarList3> {
        if let Some(barre) = self.barre.borrow().as_ref() {
            return Ok(barre.clone());
        }
        let barre: ITaskbarList3 = unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let barre: ITaskbarList3 =
                CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER).map_err(echec)?;
            barre.HrInit().map_err(echec)?;
            barre
        };
        *self.barre.borrow_mut() = Some(barre.clone());
        Ok(barre)
    }
}

impl Pastille for PastilleWindows {
    fn disponible(&self) -> bool {
        true
    }

    fn attacher(&self, fenetre: RawWindowHandle) -> Resultat<()> {
        match fenetre {
            RawWindowHandle::Win32(poignee) => {
                self.fenetre.set(Some(poignee.hwnd.get()));
                Ok(())
            }
            _ => Err(Erreur::Refuse(
                "fenêtre qui n'est pas une fenêtre Win32".into(),
            )),
        }
    }

    fn definir(&self, image: Option<&Image>, description: &str) -> Resultat<()> {
        let Some(fenetre) = self.fenetre.get() else {
            return Err(Erreur::Refuse("pastille sans fenêtre attachée".into()));
        };
        let barre = self.barre()?;
        let hwnd = HWND(fenetre as *mut core::ffi::c_void);
        let description = HSTRING::from(description);
        unsafe {
            match image {
                Some(image) => {
                    let hicon = icone(image)?;
                    let resultat = barre.SetOverlayIcon(hwnd, hicon, &description);
                    let _ = DestroyIcon(hicon);
                    resultat.map_err(echec)
                }
                None => barre
                    .SetOverlayIcon(hwnd, HICON::default(), &description)
                    .map_err(echec),
            }
        }
    }
}
