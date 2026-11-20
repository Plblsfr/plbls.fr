use crate::{Erreur, EvenementZone, Image, RecepteurZone, Resultat, ZoneNotification};
use std::cell::RefCell;
use std::sync::Arc;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub const LIBELLE_OUVRIR: &str = "Ouvrir Estompe natif";
pub const LIBELLE_QUITTER: &str = "Quitter";

#[derive(Default)]
pub struct ZoneSysteme {
    icone: RefCell<Option<TrayIcon>>,
}

fn echec(e: impl std::fmt::Display) -> Erreur {
    Erreur::Echec(format!("zone de notification : {e}"))
}

fn icone(image: &Image) -> Resultat<Icon> {
    Icon::from_rgba(image.rgba.clone(), image.largeur, image.hauteur).map_err(echec)
}

impl ZoneNotification for ZoneSysteme {
    fn disponible(&self) -> bool {
        true
    }

    fn presente(&self) -> bool {
        zone_presente()
    }

    fn installer(&self, image: &Image, info: &str, evenements: RecepteurZone) -> Resultat<()> {
        let menu = Menu::new();
        let ouvrir = MenuItem::new(LIBELLE_OUVRIR, true, None);
        let quitter = MenuItem::new(LIBELLE_QUITTER, true, None);
        menu.append_items(&[&ouvrir, &quitter]).map_err(echec)?;
        let evenements = Arc::new(evenements);
        let (id_ouvrir, id_quitter) = (ouvrir.id().clone(), quitter.id().clone());
        let recepteur = evenements.clone();
        MenuEvent::set_event_handler(Some(move |evenement: MenuEvent| {
            if evenement.id == id_ouvrir {
                recepteur(EvenementZone::Ouvrir);
            } else if evenement.id == id_quitter {
                recepteur(EvenementZone::Quitter);
            }
        }));
        TrayIconEvent::set_event_handler(Some(move |evenement: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = evenement
            {
                evenements(EvenementZone::Clic);
            }
        }));
        let zone = TrayIconBuilder::new()
            .with_id(crate::IDENTIFIANT)
            .with_icon(icone(image)?)
            .with_tooltip(info)
            .with_title(info)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .map_err(echec)?;
        *self.icone.borrow_mut() = Some(zone);
        Ok(())
    }

    fn definir_icone(&self, image: &Image) -> Resultat<()> {
        match self.icone.borrow().as_ref() {
            Some(zone) => zone.set_icon(Some(icone(image)?)).map_err(echec),
            None => Err(Erreur::Refuse("icône de la zone non installée".into())),
        }
    }

    fn retirer(&self) {
        self.icone.borrow_mut().take();
    }
}

#[cfg(windows)]
pub fn zone_presente() -> bool {
    true
}

#[cfg(target_os = "linux")]
pub const NOM_OBSERVATEUR: &str = "org.kde.StatusNotifierWatcher";

#[cfg(target_os = "linux")]
pub fn zone_presente() -> bool {
    let Ok(connexion) = zbus::blocking::Connection::session() else {
        return false;
    };
    let Ok(bus) = zbus::blocking::fdo::DBusProxy::new(&connexion) else {
        return false;
    };
    let Ok(nom) = zbus::names::BusName::try_from(NOM_OBSERVATEUR) else {
        return false;
    };
    bus.name_has_owner(nom).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    #[test]
    fn la_zone_est_presente_seulement_si_un_observateur_tient_le_nom() {
        let Ok(connexion) = zbus::blocking::Connection::session() else {
            assert!(
                std::env::var_os("ESTOMPE_DBUS_EXIGE").is_none(),
                "ESTOMPE_DBUS_EXIGE est défini mais le bus de session ne répond pas"
            );
            eprintln!("ignoré : aucun bus de session");
            assert!(!super::zone_presente());
            return;
        };
        drop(connexion);
        if super::zone_presente() {
            eprintln!("ignoré : une vraie zone de notification tient déjà le nom");
            return;
        }
        let observateur = zbus::blocking::connection::Builder::session()
            .unwrap()
            .name(super::NOM_OBSERVATEUR)
            .unwrap()
            .build()
            .unwrap();
        assert!(super::zone_presente(), "observateur présent");
        drop(observateur);
        let limite = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while super::zone_presente() {
            assert!(std::time::Instant::now() < limite, "nom toujours tenu");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}
