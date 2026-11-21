use std::ffi::OsStr;
use std::process::Command;

pub fn commande_d_arriere_plan(programme: impl AsRef<OsStr>) -> Command {
    let mut commande = Command::new(programme);
    arriere_plan(&mut commande);
    commande
}

#[cfg(windows)]
fn arriere_plan(commande: &mut Command) {
    use std::os::windows::process::CommandExt;
    commande.creation_flags(0x0800_0000 | 0x0000_4000);
}

#[cfg(not(windows))]
fn arriere_plan(_: &mut Command) {}
