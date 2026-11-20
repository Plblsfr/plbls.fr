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
use livekit::DisconnectReason;
use livekit_protocol::{ParticipantPermission, TrackSource as SourceProto};
use tokio::sync::mpsc;

use crate::audio::FREQUENCE;

use super::salle::{
    Connecteur, Connexion, ErreurSalle, EvenementSalle, InfoParticipant, JetonSalle, Qualite,
    RaisonDeconnexion, Salle, SortieMicro, SourceAudio,
};

const FILE_MICRO: usize = 50;
const TAMPON_SOURCE_MS: u32 = 100;

#[derive(Debug, Clone, Copy, Default)]
pub struct ConnecteurLiveKit;

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
