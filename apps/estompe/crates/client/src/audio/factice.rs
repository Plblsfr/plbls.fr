use std::sync::{Arc, Mutex};

use estompe_plateforme::{
    Erreur, ErreurFluxAudio, FluxAudio, FormatAudio, FournisseurAudio, PeripheriqueAudio,
    RecepteurAudio, Resultat, SensAudio, SignalErreurAudio, SystemeAudio,
};

struct PeripheriqueFactice {
    sens: SensAudio,
    id: String,
    nom: String,
    format: FormatAudio,
}

enum Rappel {
    Entree(RecepteurAudio),
    Sortie(FournisseurAudio),
}

struct FluxOuvert {
    numero: u64,
    peripherique: String,
    rappel: Rappel,
    erreur: SignalErreurAudio,
}

#[derive(Default)]
struct Etat {
    peripheriques: Vec<PeripheriqueFactice>,
    defaut_entree: Option<String>,
    defaut_sortie: Option<String>,
    flux: Vec<FluxOuvert>,
    prochain: u64,
    ouvertures: Vec<(SensAudio, String)>,
}

#[derive(Clone, Default)]
pub struct SystemeFactice {
    etat: Arc<Mutex<Etat>>,
}

struct FluxFactice {
    numero: u64,
    peripherique: String,
    format: FormatAudio,
    etat: Arc<Mutex<Etat>>,
}

impl FluxAudio for FluxFactice {
    fn format(&self) -> FormatAudio {
        self.format
    }
    fn peripherique(&self) -> &str {
        &self.peripherique
    }
}

impl Drop for FluxFactice {
    fn drop(&mut self) {
        if let Ok(mut etat) = self.etat.lock() {
            etat.flux.retain(|f| f.numero != self.numero);
        }
    }
}

impl SystemeFactice {
    pub fn nouveau() -> Self {
        Self::default()
    }

    fn etat(&self) -> std::sync::MutexGuard<'_, Etat> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn brancher(&self, sens: SensAudio, id: &str, nom: &str, format: FormatAudio) {
        let mut etat = self.etat();
        etat.peripheriques.retain(|p| p.id != id);
        etat.peripheriques.push(PeripheriqueFactice {
            sens,
            id: id.to_string(),
            nom: nom.to_string(),
            format,
        });
        let defaut = match sens {
            SensAudio::Entree => &mut etat.defaut_entree,
            SensAudio::Sortie => &mut etat.defaut_sortie,
        };
        if defaut.is_none() {
            *defaut = Some(id.to_string());
        }
    }

    pub fn definir_defaut(&self, sens: SensAudio, id: &str) {
        let mut etat = self.etat();
        match sens {
            SensAudio::Entree => etat.defaut_entree = Some(id.to_string()),
            SensAudio::Sortie => etat.defaut_sortie = Some(id.to_string()),
        }
    }

    pub fn debrancher(&self, id: &str) {
        let mut retires = Vec::new();
        {
            let mut etat = self.etat();
            etat.peripheriques.retain(|p| p.id != id);
            if etat.defaut_entree.as_deref() == Some(id) {
                etat.defaut_entree = None;
            }
            if etat.defaut_sortie.as_deref() == Some(id) {
                etat.defaut_sortie = None;
            }
            let (garde, retire): (Vec<_>, Vec<_>) = std::mem::take(&mut etat.flux)
                .into_iter()
                .partition(|f| f.peripherique != id);
            etat.flux = garde;
            retires.extend(retire);
        }
        for mut flux in retires {
            (flux.erreur)(ErreurFluxAudio::Debranche);
        }
    }

    pub fn pousser(&self, id: &str, entrelace: &[f32]) -> bool {
        let mut etat = self.etat();
        let mut pousse = false;
        for flux in etat.flux.iter_mut().filter(|f| f.peripherique == id) {
            if let Rappel::Entree(recepteur) = &mut flux.rappel {
                recepteur(entrelace);
                pousse = true;
            }
        }
        pousse
    }

    pub fn tirer(&self, id: &str, echantillons: usize) -> Option<Vec<f32>> {
        let mut etat = self.etat();
        let flux = etat.flux.iter_mut().find(|f| f.peripherique == id)?;
        let Rappel::Sortie(fournisseur) = &mut flux.rappel else {
            return None;
        };
        let mut sortie = vec![0.0; echantillons];
        fournisseur(&mut sortie);
        Some(sortie)
    }

    pub fn flux_ouverts(&self) -> Vec<String> {
        self.etat()
            .flux
            .iter()
            .map(|f| f.peripherique.clone())
            .collect()
    }

    pub fn ouvertures(&self) -> Vec<(SensAudio, String)> {
        self.etat().ouvertures.clone()
    }

    fn ouvrir(
        &self,
        sens: SensAudio,
        id: Option<&str>,
        rappel: Rappel,
        erreur: SignalErreurAudio,
    ) -> Resultat<Box<dyn FluxAudio>> {
        let mut etat = self.etat();
        let cible = match id {
            Some(id) => Some(id.to_string()),
            None => match sens {
                SensAudio::Entree => etat.defaut_entree.clone(),
                SensAudio::Sortie => etat.defaut_sortie.clone(),
            },
        };
        let Some(peripherique) = cible.and_then(|id| {
            etat.peripheriques
                .iter()
                .find(|p| p.sens == sens && p.id == id)
                .map(|p| (p.id.clone(), p.format))
        }) else {
            return Err(Erreur::Refuse("périphérique audio introuvable".to_string()));
        };
        etat.prochain += 1;
        let numero = etat.prochain;
        etat.ouvertures.push((sens, peripherique.0.clone()));
        etat.flux.push(FluxOuvert {
            numero,
            peripherique: peripherique.0.clone(),
            rappel,
            erreur,
        });
        Ok(Box::new(FluxFactice {
            numero,
            peripherique: peripherique.0,
            format: peripherique.1,
            etat: self.etat.clone(),
        }))
    }
}

impl SystemeAudio for SystemeFactice {
    fn disponible(&self) -> bool {
        true
    }

    fn peripheriques(&self, sens: SensAudio) -> Resultat<Vec<PeripheriqueAudio>> {
        let etat = self.etat();
        let defaut = match sens {
            SensAudio::Entree => etat.defaut_entree.as_deref(),
            SensAudio::Sortie => etat.defaut_sortie.as_deref(),
        };
        Ok(etat
            .peripheriques
            .iter()
            .filter(|p| p.sens == sens)
            .map(|p| PeripheriqueAudio {
                id: p.id.clone(),
                nom: p.nom.clone(),
                defaut: defaut == Some(p.id.as_str()),
            })
            .collect())
    }

    fn ouvrir_entree(
        &self,
        id: Option<&str>,
        echantillons: RecepteurAudio,
        erreur: SignalErreurAudio,
    ) -> Resultat<Box<dyn FluxAudio>> {
        self.ouvrir(SensAudio::Entree, id, Rappel::Entree(echantillons), erreur)
    }

    fn ouvrir_sortie(
        &self,
        id: Option<&str>,
        echantillons: FournisseurAudio,
        erreur: SignalErreurAudio,
    ) -> Resultat<Box<dyn FluxAudio>> {
        self.ouvrir(SensAudio::Sortie, id, Rappel::Sortie(echantillons), erreur)
    }
}
