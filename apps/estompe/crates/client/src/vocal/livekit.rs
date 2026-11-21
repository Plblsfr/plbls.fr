use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use futures_util::future::BoxFuture;
use futures_util::StreamExt;
use livekit::prelude::{
    ConnectionQuality, LocalAudioTrack, LocalTrack, Participant, RemoteParticipant, RemoteTrack,
    RemoteTrackPublication, Room, RoomEvent, RoomOptions, TrackKind, TrackPublishOptions,
    TrackSource,
};
use livekit::webrtc::audio_frame::AudioFrame;
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_source::{AudioSourceOptions, RtcAudioSource};
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::native::apm::AudioProcessingModule;
use livekit::DisconnectReason;
use livekit_protocol::{ParticipantPermission, TrackSource as SourceProto};
use tokio::sync::mpsc;

use crate::audio::{Apm, MoteurApm, FREQUENCE};

use super::salle::{
    Connecteur, Connexion, ErreurSalle, EvenementSalle, InfoParticipant, JetonSalle, Qualite,
    RaisonDeconnexion, Salle, SortieMicro, SourceAudio,
};

const FILE_MICRO: usize = 50;
const TAMPON_SOURCE_MS: u32 = 100;

#[derive(Debug, Clone, Copy, Default)]
pub struct ConnecteurLiveKit;

struct MoteurWebRtc(AudioProcessingModule);

impl MoteurApm for MoteurWebRtc {
    fn capter(&mut self, trame: &mut [i16]) {
        let _ = self.0.process_stream(trame, FREQUENCE as i32, 1);
    }

    fn jouer(&mut self, trame: &mut [i16]) {
        let _ = self.0.process_reverse_stream(trame, FREQUENCE as i32, 1);
    }
}

pub fn apm() -> Arc<Apm> {
    Apm::nouveau(Box::new(MoteurWebRtc(AudioProcessingModule::new(
        true, true, true, true,
    ))))
}

struct SalleLiveKit {
    salle: Room,
    identite: String,
}

struct SortieLiveKit {
    envoi: mpsc::Sender<Vec<i16>>,
    piste: LocalAudioTrack,
}

fn qualite(q: ConnectionQuality) -> Qualite {
    match q {
        ConnectionQuality::Excellent => Qualite::Excellente,
        ConnectionQuality::Good => Qualite::Bonne,
        ConnectionQuality::Poor => Qualite::Faible,
        ConnectionQuality::Lost => Qualite::Perdue,
    }
}

fn raison(r: DisconnectReason) -> RaisonDeconnexion {
    match r {
        DisconnectReason::ClientInitiated => RaisonDeconnexion::Volontaire,
        DisconnectReason::DuplicateIdentity => RaisonDeconnexion::IdentiteDupliquee,
        DisconnectReason::ParticipantRemoved => RaisonDeconnexion::Retire,
        DisconnectReason::RoomDeleted | DisconnectReason::RoomClosed => {
            RaisonDeconnexion::SalleFermee
        }
        DisconnectReason::SignalClose
        | DisconnectReason::JoinFailure
        | DisconnectReason::StateMismatch
        | DisconnectReason::ServerShutdown
        | DisconnectReason::Migration => RaisonDeconnexion::Reseau,
        _ => RaisonDeconnexion::Inconnue,
    }
}

fn source(s: TrackSource) -> Option<SourceAudio> {
    match s {
        TrackSource::Microphone => Some(SourceAudio::Micro),
        TrackSource::ScreenshareAudio => Some(SourceAudio::Partage),
        _ => None,
    }
}

fn attributs(a: HashMap<String, String>) -> BTreeMap<String, String> {
    a.into_iter().collect()
}

fn info_distant(p: &RemoteParticipant) -> InfoParticipant {
    InfoParticipant {
        identite: p.identity().to_string(),
        nom: p.name(),
        attributs: attributs(p.attributes()),
        qualite: qualite(p.connection_quality()),
        local: false,
    }
}

fn permissions(p: &ParticipantPermission) -> EvenementSalle {
    let micro = SourceProto::Microphone as i32;
    EvenementSalle::Permissions {
        publier_micro: p.can_publish
            && (p.can_publish_sources.is_empty() || p.can_publish_sources.contains(&micro)),
        recevoir: p.can_subscribe,
    }
}

fn abonner(publication: &RemoteTrackPublication) {
    if publication.kind() == TrackKind::Audio && source(publication.source()).is_some() {
        publication.set_subscribed(true);
    }
}

fn piste_audio(
    identite: String,
    publication: &RemoteTrackPublication,
    piste: RemoteTrack,
) -> Option<EvenementSalle> {
    let RemoteTrack::Audio(audio) = piste else {
        return None;
    };
    let source = source(publication.source())?;
    let flux = NativeAudioStream::new(audio.rtc_track(), FREQUENCE as i32, 1);
    Some(EvenementSalle::PisteAudio {
        identite,
        source,
        trames: flux.map(|trame| trame.data.into_owned()).boxed(),
    })
}

fn traduire(evenement: RoomEvent) -> Option<EvenementSalle> {
    Some(match evenement {
        RoomEvent::ParticipantConnected(p) => EvenementSalle::Arrivee(info_distant(&p)),
        RoomEvent::ParticipantDisconnected(p) => EvenementSalle::Depart(p.identity().to_string()),
        RoomEvent::ParticipantAttributesChanged { participant, .. } => EvenementSalle::Attributs {
            identite: participant.identity().to_string(),
            attributs: attributs(participant.attributes()),
        },
        RoomEvent::ConnectionQualityChanged {
            quality,
            participant,
        } => EvenementSalle::Qualite {
            identite: participant.identity().to_string(),
            qualite: qualite(quality),
        },
        RoomEvent::ActiveSpeakersChanged { speakers } => {
            EvenementSalle::Parole(speakers.iter().map(|p| p.identity().to_string()).collect())
        }
        RoomEvent::ParticipantPermissionChanged {
            participant: Participant::Local(_),
            permission: Some(permission),
        } => permissions(&permission),
        RoomEvent::Reconnecting => EvenementSalle::Reconnexion,
        RoomEvent::Reconnected => EvenementSalle::Reconnecte,
        RoomEvent::Disconnected { reason } => EvenementSalle::Deconnecte(raison(reason)),
        RoomEvent::TrackPublished { publication, .. } => {
            abonner(&publication);
            return None;
        }
        RoomEvent::TrackSubscribed {
            track,
            publication,
            participant,
        } => return piste_audio(participant.identity().to_string(), &publication, track),
        RoomEvent::TrackUnsubscribed {
            publication,
            participant,
            ..
        } => EvenementSalle::PisteRetiree {
            identite: participant.identity().to_string(),
            source: source(publication.source())?,
        },
        _ => return None,
    })
}

impl SortieMicro for SortieLiveKit {
    fn envoyer(&self, trame: &[i16]) {
        let _ = self.envoi.try_send(trame.to_vec());
    }

    fn couper(&self, coupe: bool) {
        if coupe {
            self.piste.mute();
        } else {
            self.piste.unmute();
        }
    }
}

impl Salle for SalleLiveKit {
    fn identite(&self) -> String {
        self.identite.clone()
    }

    fn definir_attributs(
        &self,
        attributs: BTreeMap<String, String>,
    ) -> BoxFuture<'_, Result<(), ErreurSalle>> {
        Box::pin(async move {
            self.salle
                .local_participant()
                .set_attributes(attributs.into_iter().collect())
                .await
                .map_err(|e| ErreurSalle::Publication(e.to_string()))
        })
    }

    fn publier_micro(&self) -> BoxFuture<'_, Result<Arc<dyn SortieMicro>, ErreurSalle>> {
        Box::pin(async move {
            let source = NativeAudioSource::new(
                AudioSourceOptions::default(),
                FREQUENCE,
                1,
                TAMPON_SOURCE_MS,
            );
            let piste = LocalAudioTrack::create_audio_track(
                "micro",
                RtcAudioSource::Native(source.clone()),
            );
            self.salle
                .local_participant()
                .publish_track(
                    LocalTrack::Audio(piste.clone()),
                    TrackPublishOptions {
                        source: TrackSource::Microphone,
                        dtx: true,
                        red: true,
                        ..TrackPublishOptions::default()
                    },
                )
                .await
                .map_err(|e| ErreurSalle::Publication(e.to_string()))?;
            let (envoi, mut reception) = mpsc::channel::<Vec<i16>>(FILE_MICRO);
            tokio::spawn(async move {
                while let Some(trame) = reception.recv().await {
                    let image = AudioFrame {
                        samples_per_channel: trame.len() as u32,
                        data: trame.into(),
                        sample_rate: FREQUENCE,
                        num_channels: 1,
                    };
                    if source.capture_frame(&image).await.is_err() {
                        break;
                    }
                }
            });
            let sortie: Arc<dyn SortieMicro> = Arc::new(SortieLiveKit { envoi, piste });
            Ok(sortie)
        })
    }

    fn fermer(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let _ = self.salle.close().await;
        })
    }
}

impl Connecteur for ConnecteurLiveKit {
    fn connecter(&self, jeton: JetonSalle) -> BoxFuture<'static, Result<Connexion, ErreurSalle>> {
        Box::pin(async move {
            let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
            let mut options = RoomOptions::default();
            options.auto_subscribe = false;
            options.adaptive_stream = true;
            options.dynacast = true;
            let (salle, mut recus) = Room::connect(&jeton.url, &jeton.jeton, options)
                .await
                .map_err(|e| ErreurSalle::Connexion(e.to_string()))?;
            let local = salle.local_participant();
            let identite = local.identity().to_string();
            let mut participants = vec![InfoParticipant {
                identite: identite.clone(),
                nom: local.name(),
                attributs: attributs(local.attributes()),
                qualite: qualite(local.connection_quality()),
                local: true,
            }];
            for distant in salle.remote_participants().values() {
                participants.push(info_distant(distant));
                for publication in distant.track_publications().values() {
                    abonner(publication);
                }
            }
            let (envoi, evenements) = mpsc::unbounded_channel();
            tokio::spawn(async move {
                while let Some(evenement) = recus.recv().await {
                    if let Some(traduit) = traduire(evenement) {
                        if envoi.send(traduit).is_err() {
                            break;
                        }
                    }
                }
            });
            Ok(Connexion {
                salle: Arc::new(SalleLiveKit { salle, identite }),
                participants,
                evenements,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::format::Reechantillonneur;
    use crate::audio::{niveau_db, ECHANTILLONS_PAR_TRAME};

    fn voix() -> Vec<f32> {
        let octets = include_bytes!("../../tests/donnees/voix-16k.wav");
        let debut = octets.windows(4).position(|f| f == b"data").unwrap() + 8;
        let voix16: Vec<f32> = octets[debut..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|o| f32::from(i16::from_le_bytes(*o)) / f32::from(i16::MAX))
            .collect();
        let mut voix = Vec::new();
        Reechantillonneur::nouveau(16_000, FREQUENCE).traiter(&voix16, &mut voix);
        voix.truncate(voix.len() / ECHANTILLONS_PAR_TRAME * ECHANTILLONS_PAR_TRAME);
        voix
    }

    #[test]
    fn l_annulation_d_echo_retire_le_son_joue_repris_par_le_micro() {
        let apm = apm();
        apm.activer(true);
        let lointain: Vec<f32> = voix().repeat(3);
        let retard = 3 * ECHANTILLONS_PAR_TRAME;
        let mut echo = vec![0.0f32; lointain.len()];
        for i in retard..lointain.len() {
            echo[i] = 0.5 * lointain[i - retard];
        }
        let mut entree = Vec::new();
        let mut sortie = Vec::new();
        for (joue, capte) in lointain
            .as_chunks::<ECHANTILLONS_PAR_TRAME>()
            .0
            .iter()
            .zip(echo.as_chunks::<ECHANTILLONS_PAR_TRAME>().0)
        {
            apm.jouer(joue);
            let mut trame: Vec<i16> = capte
                .iter()
                .map(|e| crate::audio::format::vers_i16(*e))
                .collect();
            entree.extend(capte.iter().copied());
            apm.capter(&mut trame);
            sortie.extend(trame.iter().map(|e| f32::from(*e) / f32::from(i16::MAX)));
        }
        let fin = 2 * lointain.len() / 3;
        let avant = niveau_db(&entree[fin..]);
        let apres = niveau_db(&sortie[fin..]);
        assert!(
            avant - apres >= 15.0,
            "écho : {avant:.1} dB repris, {apres:.1} dB après annulation"
        );
    }

    #[test]
    #[ignore = "mesure du coût, à lancer en release : cargo test --release --features livekit -- --ignored --nocapture cout"]
    fn mesure_le_cout_de_la_chaine_complete_du_micro() {
        use crate::audio::{ChaineMicro, MesureMicro, ReglagesMicro};
        let apm = apm();
        let mut chaine = ChaineMicro::nouvelle(
            ReglagesMicro {
                detection_voix: true,
                ..ReglagesMicro::default()
            },
            Some(apm.clone()),
            Arc::new(MesureMicro::default()),
        );
        let signal = voix();
        let micro: Vec<i16> = signal
            .iter()
            .map(|e| crate::audio::format::vers_i16(e * 0.3))
            .collect();
        let tours = 10;
        let debut = std::time::Instant::now();
        for _ in 0..tours {
            for (joue, capte) in signal
                .as_chunks::<ECHANTILLONS_PAR_TRAME>()
                .0
                .iter()
                .zip(micro.as_chunks::<ECHANTILLONS_PAR_TRAME>().0)
            {
                apm.jouer(joue);
                std::hint::black_box(chaine.traiter(capte));
            }
        }
        let calcul = debut.elapsed().as_secs_f64();
        let audio = (tours * signal.len()) as f64 / f64::from(FREQUENCE);
        println!(
            "chaîne complète du micro (APM WebRTC, RNNoise, détection de voix, référence d'écho) : {:.2} % d'un cœur ({calcul:.3} s pour {audio:.1} s de son)",
            100.0 * calcul / audio
        );
    }
}
