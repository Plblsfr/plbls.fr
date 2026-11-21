use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample, StreamConfig};

use crate::{
    Erreur, ErreurFluxAudio, FluxAudio, FluxSon, FormatAudio, FournisseurAudio, Micro,
    PeripheriqueAudio, ProducteurSon, RecepteurAudio, RecepteurSon, Resultat, SensAudio,
    SignalErreurAudio, SortieSon, SystemeAudio,
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

fn erreur_cpal(e: cpal::Error) -> Erreur {
    match e.kind() {
        cpal::ErrorKind::PermissionDenied => Erreur::Refuse(e.to_string()),
        cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::HostUnavailable => {
            Erreur::Indisponible("périphérique audio")
        }
        _ => echec(e),
    }
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
    match id {
        Some(id) => lister(hote, sens)?
            .into_iter()
            .find(|p| identifiant(p).as_deref() == Some(id))
            .ok_or_else(|| Erreur::Refuse("périphérique audio introuvable".to_string())),
        None => defaut(hote, sens).ok_or(Erreur::Indisponible("périphérique audio")),
    }
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
        let pris_en_charge = peripherique.default_input_config().map_err(erreur_cpal)?;
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
        .map_err(erreur_cpal)?;
        flux.play().map_err(erreur_cpal)?;
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
        let pris_en_charge = peripherique.default_output_config().map_err(erreur_cpal)?;
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
        .map_err(erreur_cpal)?;
        flux.play().map_err(erreur_cpal)?;
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

pub type ChoixPeripherique = Arc<dyn Fn(SensAudio) -> Option<String> + Send + Sync>;

#[derive(Clone)]
pub struct SonPeripheriques {
    audio: Arc<dyn SystemeAudio>,
    choix: ChoixPeripherique,
}

struct FormatPartage(AtomicU64);

impl FormatPartage {
    fn nouveau() -> Arc<Self> {
        Arc::new(Self(AtomicU64::new(0)))
    }

    fn definir(&self, format: FormatAudio) {
        let canaux = u64::from(format.canaux.max(1));
        self.0.store(
            u64::from(format.frequence) << 16 | canaux,
            Ordering::Release,
        );
    }

    fn lire(&self) -> Option<(u32, usize)> {
        let valeur = self.0.load(Ordering::Acquire);
        let frequence = u32::try_from(valeur >> 16).ok()?;
        (valeur != 0).then_some((frequence, (valeur & 0xffff) as usize))
    }
}

impl SonPeripheriques {
    pub fn nouveau(audio: Arc<dyn SystemeAudio>, choix: ChoixPeripherique) -> Self {
        Self { audio, choix }
    }

    pub fn par_defaut(audio: Arc<dyn SystemeAudio>) -> Self {
        Self::nouveau(audio, Arc::new(|_| None))
    }

    fn identifiant(&self, sens: SensAudio) -> Option<String> {
        let id = (self.choix)(sens)?;
        self.audio
            .peripheriques(sens)
            .ok()?
            .iter()
            .any(|p| p.id == id)
            .then_some(id)
    }
}

impl Micro for SonPeripheriques {
    fn disponible(&self) -> bool {
        self.audio.disponible()
    }

    fn ecouter(&self, mut recepteur: RecepteurSon) -> Resultat<FluxSon> {
        let format = FormatPartage::nouveau();
        let lu = format.clone();
        let mut mono = Vec::new();
        let flux = self.audio.ouvrir_entree(
            self.identifiant(SensAudio::Entree).as_deref(),
            Box::new(move |donnees| {
                let Some((frequence, canaux)) = lu.lire() else {
                    return;
                };
                mono.clear();
                mono.extend(
                    donnees
                        .chunks(canaux)
                        .map(|trame| trame.iter().sum::<f32>() / trame.len() as f32),
                );
                recepteur(&mono, frequence);
            }),
            Box::new(|_| {}),
        )?;
        format.definir(flux.format());
        Ok(FluxSon::nouveau(flux))
    }
}

impl SortieSon for SonPeripheriques {
    fn disponible(&self) -> bool {
        self.audio.disponible()
    }

    fn jouer(&self, mut producteur: ProducteurSon) -> Resultat<FluxSon> {
        let format = FormatPartage::nouveau();
        let lu = format.clone();
        let mut mono = Vec::new();
        let flux = self.audio.ouvrir_sortie(
            self.identifiant(SensAudio::Sortie).as_deref(),
            Box::new(move |donnees| {
                let Some((frequence, canaux)) = lu.lire() else {
                    donnees.fill(0.0);
                    return;
                };
                mono.clear();
                mono.resize(donnees.len() / canaux, 0.0);
                producteur(&mut mono, frequence);
                for (trame, e) in donnees.chunks_mut(canaux).zip(&mono) {
                    trame.fill(*e);
                }
            }),
            Box::new(|_| {}),
        )?;
        format.definir(flux.format());
        Ok(FluxSon::nouveau(flux))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct Fac {
        entree: Mutex<Option<RecepteurAudio>>,
        sortie: Mutex<Option<FournisseurAudio>>,
        demandes: Mutex<Vec<Option<String>>>,
    }

    struct FluxFac;

    impl FluxAudio for FluxFac {
        fn format(&self) -> FormatAudio {
            FormatAudio {
                frequence: 44_100,
                canaux: 2,
            }
        }
        fn peripherique(&self) -> &str {
            "micro:usb"
        }
    }

    impl SystemeAudio for Fac {
        fn disponible(&self) -> bool {
            true
        }
        fn peripheriques(&self, _: SensAudio) -> Resultat<Vec<PeripheriqueAudio>> {
            Ok(vec![PeripheriqueAudio {
                id: "micro:usb".into(),
                nom: "USB".into(),
                defaut: false,
            }])
        }
        fn ouvrir_entree(
            &self,
            id: Option<&str>,
            echantillons: RecepteurAudio,
            _: SignalErreurAudio,
        ) -> Resultat<Box<dyn FluxAudio>> {
            self.demandes.lock().unwrap().push(id.map(str::to_string));
            *self.entree.lock().unwrap() = Some(echantillons);
            Ok(Box::new(FluxFac))
        }
        fn ouvrir_sortie(
            &self,
            id: Option<&str>,
            echantillons: FournisseurAudio,
            _: SignalErreurAudio,
        ) -> Resultat<Box<dyn FluxAudio>> {
            self.demandes.lock().unwrap().push(id.map(str::to_string));
            *self.sortie.lock().unwrap() = Some(echantillons);
            Ok(Box::new(FluxFac))
        }
    }

    #[test]
    fn le_son_des_messages_passe_par_le_peripherique_choisi_en_mono() {
        let fac = Arc::new(Fac::default());
        let son = SonPeripheriques::nouveau(
            fac.clone(),
            Arc::new(|sens| match sens {
                SensAudio::Entree => Some("micro:usb".into()),
                SensAudio::Sortie => Some("debranche".into()),
            }),
        );
        let recu = Arc::new(Mutex::new(Vec::new()));
        let copie = recu.clone();
        let _micro = son
            .ecouter(Box::new(move |e, taux| {
                copie.lock().unwrap().push((e.to_vec(), taux))
            }))
            .unwrap();
        (fac.entree.lock().unwrap().as_mut().unwrap())(&[1.0, 0.0, 0.5, 0.5]);
        assert_eq!(*recu.lock().unwrap(), vec![(vec![0.5, 0.5], 44_100)]);
        let _sortie = son
            .jouer(Box::new(|tampon, taux| {
                assert_eq!(taux, 44_100);
                tampon.fill(0.25)
            }))
            .unwrap();
        let mut tampon = [0.0; 4];
        (fac.sortie.lock().unwrap().as_mut().unwrap())(&mut tampon);
        assert_eq!(tampon, [0.25; 4]);
        assert_eq!(
            *fac.demandes.lock().unwrap(),
            vec![Some("micro:usb".to_string()), None]
        );
    }

    #[test]
    fn sans_peripherique_le_micro_et_la_sortie_repondent_sans_paniquer() {
        let son = SonPeripheriques::par_defaut(Arc::new(AudioSysteme));
        let micro = son.ecouter(Box::new(|_, _| {}));
        let sortie = son.jouer(Box::new(|tampon, _| tampon.fill(0.0)));
        println!(
            "micro : {:?}, sortie : {:?}",
            micro.as_ref().err(),
            sortie.as_ref().err()
        );
    }

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
