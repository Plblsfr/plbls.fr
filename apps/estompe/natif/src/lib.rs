slint::include_modules!();

pub mod session;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn creer_fenetre() -> Result<Principale, slint::PlatformError> {
    let fenetre = Principale::new()?;
    fenetre.set_version(VERSION.into());
    fenetre.set_debogage(cfg!(debug_assertions));
    Ok(fenetre)
}
