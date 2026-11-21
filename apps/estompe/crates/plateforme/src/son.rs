use std::sync::mpsc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, SampleFormat, SizedSample, Stream};

use crate::traits::{FluxSon, Micro, ProducteurSon, RecepteurSon, SortieSon};
use crate::{Erreur, Resultat};

#[derive(Debug, Clone, Copy, Default)]
pub struct SonSysteme;

fn erreur(appareil: &'static str, e: cpal::Error) -> Erreur {
    match e.kind() {
        ErrorKind::PermissionDenied => Erreur::Refuse(e.to_string()),
        ErrorKind::DeviceNotAvailable | ErrorKind::HostUnavailable => {
            Erreur::Indisponible(appareil)
        }
        _ => Erreur::Echec(e.to_string()),
    }
}

fn lancer(
    nom: &str,
    construire: impl FnOnce() -> Resultat<Stream> + Send + 'static,
) -> Resultat<FluxSon> {
    let (pret, attente) = mpsc::sync_channel(1);
    let (arret, signal) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name(nom.into())
        .spawn(move || match construire() {
            Ok(flux) => {
                let _ = pret.send(Ok(()));
                let _ = signal.recv();
                drop(flux);
            }
            Err(e) => {
                let _ = pret.send(Err(e));
            }
        })
        .map_err(|e| Erreur::Echec(e.to_string()))?;
    attente
        .recv()
        .map_err(|_| Erreur::Echec("fil du son arrêté".into()))??;
    Ok(FluxSon::nouveau(arret))
}

fn entree<T: SizedSample>(
    appareil: &cpal::Device,
    config: cpal::StreamConfig,
    mut recepteur: RecepteurSon,
) -> Result<Stream, cpal::Error>
where
    f32: FromSample<T>,
{
    let canaux = usize::from(config.channels.max(1));
    let taux = config.sample_rate;
    let mut mono = Vec::new();
    appareil.build_input_stream(
        config,
        move |donnees: &[T], _: &_| {
            mono.clear();
            mono.extend(donnees.chunks(canaux).map(|trame| {
                trame.iter().map(|e| e.to_sample::<f32>()).sum::<f32>() / canaux as f32
            }));
            recepteur(&mono, taux);
        },
        |_| {},
        None,
    )
}

fn sortie<T: SizedSample + FromSample<f32>>(
    appareil: &cpal::Device,
    config: cpal::StreamConfig,
    mut producteur: ProducteurSon,
) -> Result<Stream, cpal::Error> {
    let canaux = usize::from(config.channels.max(1));
    let taux = config.sample_rate;
    let mut mono = Vec::new();
    appareil.build_output_stream(
        config,
        move |donnees: &mut [T], _: &_| {
            mono.resize(donnees.len() / canaux, 0.0);
            producteur(&mut mono, taux);
            for (trame, e) in donnees.chunks_mut(canaux).zip(&mono) {
                trame.fill(T::from_sample(*e));
            }
        },
        |_| {},
        None,
    )
}

impl Micro for SonSysteme {
    fn disponible(&self) -> bool {
        true
    }

    fn ecouter(&self, recepteur: RecepteurSon) -> Resultat<FluxSon> {
        lancer("estompe-micro", move || {
            let appareil = cpal::default_host()
                .default_input_device()
                .ok_or(Erreur::Indisponible("micro"))?;
            let reglage = appareil
                .default_input_config()
                .map_err(|e| erreur("micro", e))?;
            let config = reglage.config();
            let flux = match reglage.sample_format() {
                SampleFormat::I16 => entree::<i16>(&appareil, config, recepteur),
                SampleFormat::U16 => entree::<u16>(&appareil, config, recepteur),
                SampleFormat::I32 => entree::<i32>(&appareil, config, recepteur),
                _ => entree::<f32>(&appareil, config, recepteur),
            }
            .map_err(|e| erreur("micro", e))?;
            flux.play().map_err(|e| erreur("micro", e))?;
            Ok(flux)
        })
    }
}

impl SortieSon for SonSysteme {
    fn disponible(&self) -> bool {
        true
    }

    fn jouer(&self, producteur: ProducteurSon) -> Resultat<FluxSon> {
        lancer("estompe-sortie-son", move || {
            let appareil = cpal::default_host()
                .default_output_device()
                .ok_or(Erreur::Indisponible("sortie du son"))?;
            let reglage = appareil
                .default_output_config()
                .map_err(|e| erreur("sortie du son", e))?;
            let config = reglage.config();
            let flux = match reglage.sample_format() {
                SampleFormat::I16 => sortie::<i16>(&appareil, config, producteur),
                SampleFormat::U16 => sortie::<u16>(&appareil, config, producteur),
                SampleFormat::I32 => sortie::<i32>(&appareil, config, producteur),
                _ => sortie::<f32>(&appareil, config, producteur),
            }
            .map_err(|e| erreur("sortie du son", e))?;
            flux.play().map_err(|e| erreur("sortie du son", e))?;
            Ok(flux)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sans_peripherique_le_micro_et_la_sortie_repondent_sans_paniquer() {
        let micro = SonSysteme.ecouter(Box::new(|_, _| {}));
        let sortie = SonSysteme.jouer(Box::new(|tampon, _| tampon.fill(0.0)));
        println!(
            "micro : {:?}, sortie : {:?}",
            micro.as_ref().err(),
            sortie.as_ref().err()
        );
    }
}
