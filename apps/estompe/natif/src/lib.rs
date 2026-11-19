slint::include_modules!();

pub mod ecrans;
pub mod entree;
pub mod session;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const OPTION_INVITATION: &str = "--invitation";

pub fn creer_fenetre() -> Result<Principale, slint::PlatformError> {
    let fenetre = Principale::new()?;
    fenetre.set_version(VERSION.into());
    fenetre.set_debogage(cfg!(debug_assertions));
    Ok(fenetre)
}

pub fn invitation_des_arguments(arguments: impl IntoIterator<Item = String>) -> Option<String> {
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        if argument == OPTION_INVITATION {
            return arguments.next().filter(|valeur| !valeur.trim().is_empty());
        }
        if let Some(valeur) = argument.strip_prefix("--invitation=") {
            return Some(valeur.to_string()).filter(|valeur| !valeur.trim().is_empty());
        }
    }
    None
}

pub struct Application {
    pub entree: entree::Entree,
    pub client: estompe_client::http::Client,
    pub auth: std::sync::Arc<estompe_client::auth::Authentification>,
}

pub fn demarrer(
    session: &session::Session,
    fenetre: &Principale,
    invitation: Option<String>,
) -> Result<Application, String> {
    let canal = entree::canal();
    let reels = session.services(&canal.0)?;
    ecrans::brancher(fenetre, &canal.0);
    let entree = entree::Entree::demarrer(
        entree::Machine::nouvelle(invitation),
        reels.services,
        canal,
        ecrans::publieur(fenetre),
    )
    .map_err(|e| format!("fil de l'entrée : {e}"))?;
    Ok(Application {
        entree,
        client: reels.client,
        auth: reels.auth,
    })
}
