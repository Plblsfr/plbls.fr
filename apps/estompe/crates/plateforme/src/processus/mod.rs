use crate::traits::{CompteursProcessus, InfosMachine, MesureProcessus};
use crate::{Erreur, Resultat};

mod commande;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as os;
#[cfg(windows)]
use windows as os;

pub use commande::commande_d_arriere_plan;

#[derive(Debug, Clone, Copy, Default)]
pub struct MesureSysteme;

#[cfg(any(windows, target_os = "linux"))]
impl MesureProcessus for MesureSysteme {
    fn disponible(&self) -> bool {
        true
    }
    fn compteurs(&self) -> Resultat<CompteursProcessus> {
        os::compteurs().ok_or(Erreur::Indisponible("compteurs du processus"))
    }
    fn machine(&self) -> InfosMachine {
        os::machine()
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
impl MesureProcessus for MesureSysteme {
    fn disponible(&self) -> bool {
        false
    }
    fn compteurs(&self) -> Resultat<CompteursProcessus> {
        Err(Erreur::Indisponible("compteurs du processus"))
    }
    fn machine(&self) -> InfosMachine {
        InfosMachine::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn mesure_le_processus_courant() {
        let mesure = MesureSysteme;
        assert!(mesure.disponible());
        let avant = mesure.compteurs().unwrap();
        let mut x = 0u64;
        for i in 0..20_000_000u64 {
            x = x.wrapping_add(i * i);
        }
        std::hint::black_box(x);
        let apres = mesure.compteurs().unwrap();
        assert!(avant.ws > 0 && avant.prive > 0);
        assert!(apres.pic_ws >= apres.ws);
        assert!(apres.cpu_ms >= avant.cpu_ms);
        assert!(mesure.machine().coeurs > 0);
    }
}
