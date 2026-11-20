use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, StreamConfig};

use crate::{
    Erreur, ErreurFluxAudio, FluxAudio, FormatAudio, FournisseurAudio, PeripheriqueAudio,
    RecepteurAudio, Resultat, SensAudio, SignalErreurAudio, SystemeAudio,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct AudioSysteme;

struct FluxCpal {
    _flux: cpal::Stream,
    format: FormatAudio,
    peripherique: String,
}

impl FluxAudio for FluxCpal {
    fn format(&self) -> FormatAudio {
        self.format
    }
    fn peripherique(&self) -> &str {
        &self.peripherique
    }
}

fn echec(erreur: impl std::fmt::Display) -> Erreur {
    Erreur::Echec(erreur.to_string())
}

fn identifiant(peripherique: &cpal::Device) -> Option<String> {
    peripherique.id().ok().map(|id| id.to_string())
}

fn nom(peripherique: &cpal::Device) -> String {
    peripherique
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| identifiant(peripherique).unwrap_or_default())
}

fn defaut(hote: &cpal::Host, sens: SensAudio) -> Option<cpal::Device> {
    match sens {
        SensAudio::Entree => hote.default_input_device(),
        SensAudio::Sortie => hote.default_output_device(),
    }
}

fn lister(hote: &cpal::Host, sens: SensAudio) -> Resultat<Vec<cpal::Device>> {
    Ok(match sens {
        SensAudio::Entree => hote.input_devices().map_err(echec)?.collect(),
        SensAudio::Sortie => hote.output_devices().map_err(echec)?.collect(),
    })
}

fn choisir(hote: &cpal::Host, sens: SensAudio, id: Option<&str>) -> Resultat<cpal::Device> {
    let trouve = match id {
        Some(id) => lister(hote, sens)?
            .into_iter()
            .find(|p| identifiant(p).as_deref() == Some(id)),
        None => defaut(hote, sens),
    };
    trouve.ok_or_else(|| Erreur::Refuse("périphérique audio introuvable".to_string()))
}

fn signal(mut erreur: SignalErreurAudio) -> impl FnMut(cpal::Error) + Send + 'static {
    move |e: cpal::Error| match e.kind() {
        cpal::ErrorKind::DeviceChanged => {}
        cpal::ErrorKind::DeviceNotAvailable => erreur(ErreurFluxAudio::Debranche),
        _ => erreur(ErreurFluxAudio::Echec(e.to_string())),
    }
}

fn entree<T>(
    peripherique: &cpal::Device,
    config: StreamConfig,
    mut recepteur: RecepteurAudio,
    erreur: SignalErreurAudio,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let mut tampon = Vec::new();
    peripherique.build_input_stream::<T, _, _>(
        config,
        move |donnees: &[T], _| {
            tampon.clear();
            tampon.extend(donnees.iter().map(|e| f32::from_sample(*e)));
            recepteur(&tampon);
        },
        signal(erreur),
        None,
    )
}

fn sortie<T>(
    peripherique: &cpal::Device,
    config: StreamConfig,
    mut fournisseur: FournisseurAudio,
    erreur: SignalErreurAudio,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let mut tampon = Vec::new();
    peripherique.build_output_stream::<T, _, _>(
        config,
        move |donnees: &mut [T], _| {
            tampon.clear();
            tampon.resize(donnees.len(), 0.0);
            fournisseur(&mut tampon);
            for (d, e) in donnees.iter_mut().zip(&tampon) {
                *d = T::from_sample(*e);
            }
        },
        signal(erreur),
        None,
    )
}

impl SystemeAudio for AudioSysteme {
    fn disponible(&self) -> bool {
        true
    }

    fn peripheriques(&self, sens: SensAudio) -> Resultat<Vec<PeripheriqueAudio>> {
        let hote = cpal::default_host();
        let par_defaut = defaut(&hote, sens).and_then(|p| identifiant(&p));
        Ok(lister(&hote, sens)?
            .iter()
            .filter_map(|p| {
                let id = identifiant(p)?;
                Some(PeripheriqueAudio {
                    defaut: par_defaut.as_deref() == Some(id.as_str()),
                    nom: nom(p),
                    id,
                })
            })
            .collect())
    }

    fn ouvrir_entree(
        &self,
        id: Option<&str>,
        echantillons: RecepteurAudio,
        erreur: SignalErreurAudio,
    ) -> Resultat<Box<dyn FluxAudio>> {
        let peripherique = choisir(&cpal::default_host(), SensAudio::Entree, id)?;
        let pris_en_charge = peripherique.default_input_config().map_err(echec)?;
        let config = pris_en_charge.config();
        let flux = match pris_en_charge.sample_format() {
            SampleFormat::F32 => entree::<f32>(&peripherique, config, echantillons, erreur),
            SampleFormat::I16 => entree::<i16>(&peripherique, config, echantillons, erreur),
            SampleFormat::I32 => entree::<i32>(&peripherique, config, echantillons, erreur),
            SampleFormat::U16 => entree::<u16>(&peripherique, config, echantillons, erreur),
            autre => {
                return Err(echec(format!(
                    "format d'échantillons {autre} non pris en charge"
                )))
            }
        }
        .map_err(echec)?;
        flux.play().map_err(echec)?;
        Ok(Box::new(FluxCpal {
            _flux: flux,
            format: FormatAudio {
                frequence: config.sample_rate,
                canaux: config.channels,
            },
            peripherique: identifiant(&peripherique).unwrap_or_default(),
        }))
    }

    fn ouvrir_sortie(
        &self,
        id: Option<&str>,
        echantillons: FournisseurAudio,
        erreur: SignalErreurAudio,
    ) -> Resultat<Box<dyn FluxAudio>> {
        let peripherique = choisir(&cpal::default_host(), SensAudio::Sortie, id)?;
        let pris_en_charge = peripherique.default_output_config().map_err(echec)?;
        let config = pris_en_charge.config();
        let flux = match pris_en_charge.sample_format() {
            SampleFormat::F32 => sortie::<f32>(&peripherique, config, echantillons, erreur),
            SampleFormat::I16 => sortie::<i16>(&peripherique, config, echantillons, erreur),
            SampleFormat::I32 => sortie::<i32>(&peripherique, config, echantillons, erreur),
            SampleFormat::U16 => sortie::<u16>(&peripherique, config, echantillons, erreur),
            autre => {
                return Err(echec(format!(
                    "format d'échantillons {autre} non pris en charge"
                )))
            }
        }
        .map_err(echec)?;
        flux.play().map_err(echec)?;
        Ok(Box::new(FluxCpal {
            _flux: flux,
            format: FormatAudio {
                frequence: config.sample_rate,
                canaux: config.channels,
            },
            peripherique: identifiant(&peripherique).unwrap_or_default(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_liste_des_peripheriques_ne_panique_pas_sans_carte_son() {
        for sens in [SensAudio::Entree, SensAudio::Sortie] {
            if let Ok(liste) = AudioSysteme.peripheriques(sens) {
                assert!(liste.iter().filter(|p| p.defaut).count() <= 1);
                assert!(liste.iter().all(|p| !p.id.is_empty()));
            }
        }
    }

    #[test]
    fn un_peripherique_inconnu_est_refuse_sans_panique() {
        let ouverture = AudioSysteme.ouvrir_entree(
            Some("inexistant:aucun"),
            Box::new(|_| {}),
            Box::new(|_| {}),
        );
        assert!(ouverture.is_err());
    }
}
