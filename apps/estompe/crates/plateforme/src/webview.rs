use std::path::PathBuf;

use raw_window_handle::RawWindowHandle;

use crate::erreur::Resultat;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CadreVue {
    pub x: f32,
    pub y: f32,
    pub largeur: f32,
    pub hauteur: f32,
}

pub type RappelNavigation = Box<dyn Fn(&str) -> bool>;
pub type RappelFenetre = Box<dyn Fn(&str)>;
pub type RappelMessage = Box<dyn Fn(&str, &str)>;

pub struct OptionsVueWeb {
    pub html: String,
    pub cadre: CadreVue,
    pub visible: bool,
    pub dossier: PathBuf,
    pub navigation: RappelNavigation,
    pub nouvelle_fenetre: RappelFenetre,
    pub message: RappelMessage,
}

pub trait VueWeb {
    fn executer(&self, script: &str) -> Resultat<()>;
    fn placer(&self, cadre: CadreVue) -> Resultat<()>;
    fn montrer(&self, visible: bool) -> Resultat<()>;
}

pub trait VuesWeb {
    fn disponible(&self) -> bool;
    fn origine(&self) -> &'static str;
    fn ouvrir(&self, fenetre: RawWindowHandle, options: OptionsVueWeb)
        -> Resultat<Box<dyn VueWeb>>;
}

pub struct VuesWebSysteme;

#[cfg(all(windows, feature = "youtube"))]
impl VuesWeb for VuesWebSysteme {
    fn disponible(&self) -> bool {
        true
    }
    fn origine(&self) -> &'static str {
        windows_wry::ORIGINE
    }
    fn ouvrir(
        &self,
        fenetre: RawWindowHandle,
        options: OptionsVueWeb,
    ) -> Resultat<Box<dyn VueWeb>> {
        windows_wry::ouvrir(fenetre, options)
    }
}

#[cfg(not(all(windows, feature = "youtube")))]
impl VuesWeb for VuesWebSysteme {
    fn disponible(&self) -> bool {
        crate::Indisponible.disponible()
    }
    fn origine(&self) -> &'static str {
        crate::Indisponible.origine()
    }
    fn ouvrir(
        &self,
        fenetre: RawWindowHandle,
        options: OptionsVueWeb,
    ) -> Resultat<Box<dyn VueWeb>> {
        crate::Indisponible.ouvrir(fenetre, options)
    }
}

#[cfg(all(windows, feature = "youtube"))]
mod windows_wry {
    use std::borrow::Cow;

    use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};
    use wry::dpi::{LogicalPosition, LogicalSize};
    use wry::http::{header, Response, StatusCode};
    use wry::{
        NewWindowResponse, PermissionKind, PermissionResponse, Rect, WebContext, WebView,
        WebViewBuilder, WebViewBuilderExtWindows,
    };

    use super::{CadreVue, OptionsVueWeb, VueWeb};
    use crate::erreur::{Erreur, Resultat};

    pub const ORIGINE: &str = "https://estompe.localhost";
    const PROTOCOLE: &str = "estompe";
    const CHEMIN: &str = "/lecteur";

    struct Parent(RawWindowHandle);

    impl HasWindowHandle for Parent {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            Ok(unsafe { WindowHandle::borrow_raw(self.0) })
        }
    }

    struct VueWry {
        vue: WebView,
        _contexte: WebContext,
    }

    fn echec(e: impl std::fmt::Display) -> Erreur {
        Erreur::Echec(e.to_string())
    }

    fn rect(c: CadreVue) -> Rect {
        Rect {
            position: LogicalPosition::new(f64::from(c.x), f64::from(c.y)).into(),
            size: LogicalSize::new(f64::from(c.largeur.max(1.0)), f64::from(c.hauteur.max(1.0)))
                .into(),
        }
    }

    fn reponse(statut: StatusCode, corps: Vec<u8>) -> Response<Cow<'static, [u8]>> {
        Response::builder()
            .status(statut)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Cow::<'static, [u8]>::Owned(corps))
            .unwrap_or_else(|_| Response::new(Cow::Owned(Vec::new())))
    }

    pub fn ouvrir(fenetre: RawWindowHandle, o: OptionsVueWeb) -> Resultat<Box<dyn VueWeb>> {
        let mut contexte = WebContext::new(Some(o.dossier));
        let page = o.html.into_bytes();
        let (navigation, nouvelle_fenetre, message) = (o.navigation, o.nouvelle_fenetre, o.message);
        let parent = Parent(fenetre);
        let vue = WebViewBuilder::new_with_web_context(&mut contexte)
            .with_https_scheme(true)
            .with_incognito(true)
            .with_devtools(false)
            .with_clipboard(false)
            .with_hotkeys_zoom(false)
            .with_autoplay(true)
            .with_background_color((0, 0, 0, 255))
            .with_visible(o.visible)
            .with_bounds(rect(o.cadre))
            .with_custom_protocol(PROTOCOLE.to_string(), move |_, requete| {
                if requete.uri().path() == CHEMIN {
                    reponse(StatusCode::OK, page.clone())
                } else {
                    reponse(StatusCode::NOT_FOUND, Vec::new())
                }
            })
            .with_navigation_handler(move |adresse| navigation(&adresse))
            .with_new_window_req_handler(move |adresse, _| {
                nouvelle_fenetre(&adresse);
                NewWindowResponse::Deny
            })
            .with_download_started_handler(|_, _| false)
            .with_permission_handler(|genre| match genre {
                PermissionKind::Autoplay => PermissionResponse::Allow,
                _ => PermissionResponse::Deny,
            })
            .with_ipc_handler(move |requete| message(&requete.uri().to_string(), requete.body()))
            .with_url(format!("{ORIGINE}{CHEMIN}"))
            .build_as_child(&parent)
            .map_err(echec)?;
        Ok(Box::new(VueWry {
            vue,
            _contexte: contexte,
        }))
    }

    impl VueWeb for VueWry {
        fn executer(&self, script: &str) -> Resultat<()> {
            self.vue.evaluate_script(script).map_err(echec)
        }
        fn placer(&self, cadre: CadreVue) -> Resultat<()> {
            self.vue.set_bounds(rect(cadre)).map_err(echec)
        }
        fn montrer(&self, visible: bool) -> Resultat<()> {
            self.vue.set_visible(visible).map_err(echec)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Erreur;

    #[test]
    fn la_vue_web_n_existe_qu_avec_la_feature_sous_windows() {
        assert_eq!(
            VuesWebSysteme.disponible(),
            cfg!(all(windows, feature = "youtube"))
        );
        if !VuesWebSysteme.disponible() {
            let options = OptionsVueWeb {
                html: String::new(),
                cadre: CadreVue::default(),
                visible: true,
                dossier: PathBuf::new(),
                navigation: Box::new(|_| false),
                nouvelle_fenetre: Box::new(|_| {}),
                message: Box::new(|_, _| {}),
            };
            let fenetre = RawWindowHandle::Web(raw_window_handle::WebWindowHandle::new(1));
            assert!(matches!(
                VuesWebSysteme.ouvrir(fenetre, options).map(|_| ()),
                Err(Erreur::Indisponible(_))
            ));
            assert_eq!(VuesWebSysteme.origine(), "");
        }
    }
}
