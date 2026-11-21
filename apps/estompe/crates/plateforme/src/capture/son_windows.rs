use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

use wasapi::{initialize_mta, AudioClient, Direction, SampleType, StreamMode, WaveFormat};

use super::son::{flottants, CANAUX_SON, FREQUENCE_SON};
use crate::traits::{CibleSon, FluxSon, FormatAudio, RecepteurSonCapture};
use crate::{Erreur, Resultat};

const ECHANTILLONS_PAR_PAQUET: usize = (FREQUENCE_SON / 100) as usize * CANAUX_SON as usize;

struct Garde {
    arret: Arc<AtomicBool>,
    fil: Option<JoinHandle<()>>,
}

impl Drop for Garde {
    fn drop(&mut self) {
        self.arret.store(true, Ordering::Relaxed);
        if let Some(fil) = self.fil.take() {
            if fil.thread().id() != std::thread::current().id() {
                let _ = fil.join();
            }
        }
    }
}

fn ouvrir(cible: CibleSon) -> Result<AudioClient, String> {
    let (processus, arborescence) = match cible {
        CibleSon::Application(pid) => (pid, true),
        CibleSon::SystemeSauf(pid) => (pid, false),
    };
    AudioClient::new_application_loopback_client(processus, arborescence).map_err(|e| e.to_string())
}

fn capturer(
    cible: CibleSon,
    arret: &AtomicBool,
    pret: mpsc::Sender<Result<(), String>>,
    mut recepteur: RecepteurSonCapture,
) {
    let demarrer = || -> Result<_, String> {
        initialize_mta().ok().map_err(|e| e.to_string())?;
        let format = WaveFormat::new(
            32,
            32,
            SampleType::Float,
            FREQUENCE_SON as usize,
            CANAUX_SON as usize,
            None,
        );
        let mut client = ouvrir(cible)?;
        client
            .initialize_client(
                &format,
                Direction::Capture,
                StreamMode::EventsShared {
                    autoconvert: true,
                    buffer_duration_hns: 0,
                },
                None,
            )
            .map_err(|e| e.to_string())?;
        let evenement = client.set_get_eventhandle().map_err(|e| e.to_string())?;
        let capture = client.get_audiocaptureclient().map_err(|e| e.to_string())?;
        client.start_stream().map_err(|e| e.to_string())?;
        Ok((client, evenement, capture))
    };
    let (client, evenement, capture) = match demarrer() {
        Ok(v) => v,
        Err(e) => {
            let _ = pret.send(Err(e));
            return;
        }
    };
    let _ = pret.send(Ok(()));
    let format = FormatAudio {
        frequence: FREQUENCE_SON,
        canaux: CANAUX_SON,
    };
    let octets_par_paquet = ECHANTILLONS_PAR_PAQUET * 4;
    let mut file = VecDeque::<u8>::new();
    let mut echantillons = Vec::with_capacity(ECHANTILLONS_PAR_PAQUET);
    while !arret.load(Ordering::Relaxed) {
        let _ = evenement.wait_for_event(100);
        while capture.get_next_packet_size().ok().flatten().unwrap_or(0) > 0 {
            if capture.read_from_device_to_deque(&mut file).is_err() {
                return;
            }
        }
        while file.len() >= octets_par_paquet {
            let octets: Vec<u8> = file.drain(..octets_par_paquet).collect();
            echantillons.clear();
            echantillons.extend(flottants(&octets));
            recepteur(&echantillons, format);
        }
    }
    let _ = client.stop_stream();
}

pub fn demarrer(cible: CibleSon, recepteur: RecepteurSonCapture) -> Resultat<FluxSon> {
    let arret = Arc::new(AtomicBool::new(false));
    let (pret, reponse) = mpsc::channel();
    let drapeau = arret.clone();
    let fil = std::thread::Builder::new()
        .name("estompe-son-partage".into())
        .spawn(move || capturer(cible, &drapeau, pret, recepteur))
        .map_err(|e| Erreur::Echec(e.to_string()))?;
    let garde = Garde {
        arret,
        fil: Some(fil),
    };
    match reponse.recv() {
        Ok(Ok(())) => Ok(FluxSon::nouveau(garde)),
        Ok(Err(e)) => Err(Erreur::Echec(format!(
            "capture du son impossible (Windows 10 2004 ou plus requis) : {e}"
        ))),
        Err(e) => Err(Erreur::Echec(e.to_string())),
    }
}
