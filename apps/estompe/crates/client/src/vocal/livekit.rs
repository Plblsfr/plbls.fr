use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use futures_util::future::BoxFuture;
use futures_util::StreamExt;
use livekit::options::VideoEncoding;
use livekit::prelude::{
    ConnectionQuality, LocalAudioTrack, LocalTrack, LocalVideoTrack, Participant,
    RemoteParticipant, RemoteTrack, RemoteTrackPublication, Room, RoomEvent, RoomOptions,
    TrackKind, TrackPublication, TrackPublishOptions, TrackSource,
};
use livekit::track::TrackDimension;
use livekit::webrtc::audio_frame::AudioFrame;
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_source::{AudioSourceOptions, RtcAudioSource};
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::native::apm::AudioProcessingModule;
use livekit::webrtc::native::yuv_helper::abgr_to_i420;
use livekit::webrtc::prelude::{
    DegradationPreference, I420Buffer, VideoBuffer, VideoFormatType, VideoFrame, VideoResolution,
    VideoRotation,
};
use livekit::webrtc::stats::{QualityLimitationReason, RtcStats};
use livekit::webrtc::video_frame::native::VideoFrameBufferExt;
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::RtcVideoSource;
use livekit::webrtc::video_stream::native::NativeVideoStream;
use livekit::DisconnectReason;
use livekit_protocol::{ParticipantPermission, TrackSource as SourceProto};
use tokio::sync::mpsc;

use crate::audio::{Apm, MoteurApm, FREQUENCE};

use estompe_plateforme::Image;

use super::camera::FormatVideo;
use super::partage::{taille_reduite, Degradation};
use super::salle::{
    est_partage, Connecteur, Connexion, ErreurSalle, EvenementSalle, InfoParticipant, JetonSalle,
    Limitation, Qualite, RaisonDeconnexion, Salle, SortieMicro, SortieVideo, SourceAudio,
};
use super::video::{DemandeVideo, ImageI420, ImageVideo, SourceVideo, TrameVideo};

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

struct SortieVideoLiveKit {
    source: NativeVideoSource,
    debut: std::time::Instant,
    piste: String,
}

impl SortieVideoLiveKit {
    fn capturer(&self, tampon: I420Buffer) {
        let mut trame = VideoFrame::new(VideoRotation::VideoRotation0, tampon);
        trame.timestamp_us = self.debut.elapsed().as_micros() as i64;
        self.source.capture_frame(&trame);
    }
}

fn copier_plan(source: &[u8], largeur: usize, hauteur: usize, cible: &mut [u8], pas: usize) {
    for y in 0..hauteur {
        cible[y * pas..y * pas + largeur].copy_from_slice(&source[y * largeur..(y + 1) * largeur]);
    }
}

impl SortieVideo for SortieVideoLiveKit {
    fn envoyer(&self, image: &ImageI420) {
        if !image.valide() {
            return;
        }
        let mut tampon = I420Buffer::new(image.largeur, image.hauteur);
        let (pas_y, pas_u, pas_v) = tampon.strides();
        let (l, h) = (image.largeur as usize, image.hauteur as usize);
        let (cl, ch) = (
            image.largeur_chroma() as usize,
            image.hauteur_chroma() as usize,
        );
        let (y, u, v) = tampon.data_mut();
        copier_plan(&image.y, l, h, y, pas_y as usize);
        copier_plan(&image.u, cl, ch, u, pas_u as usize);
        copier_plan(&image.v, cl, ch, v, pas_v as usize);
        self.capturer(tampon);
    }

    fn envoyer_rgba(&self, image: &Image, hauteur_max: u32) {
        let (l, h) = (image.largeur & !1, image.hauteur & !1);
        if l < 2 || h < 2 || image.rgba.len() < (image.largeur * image.hauteur * 4) as usize {
            return;
        }
        let mut tampon = I420Buffer::new(l, h);
        let (pas_y, pas_u, pas_v) = tampon.strides();
        let (y, u, v) = tampon.data_mut();
        abgr_to_i420(
            &image.rgba,
            image.largeur * 4,
            y,
            pas_y,
            u,
            pas_u,
            v,
            pas_v,
            l as i32,
            h as i32,
        );
        let (cl, ch) = taille_reduite(l, h, hauteur_max);
        if (cl, ch) != (l, h) {
            tampon = tampon.scale(cl as i32, ch as i32);
        }
        self.capturer(tampon);
    }

    fn piste(&self) -> String {
        self.piste.clone()
    }
}

fn degradation(d: Degradation) -> Option<DegradationPreference> {
    match d {
        Degradation::Auto => None,
        Degradation::GarderCadence => Some(DegradationPreference::MaintainFramerate),
        Degradation::GarderResolution => Some(DegradationPreference::MaintainResolution),
    }
}

fn limitation(stats: &[RtcStats]) -> Limitation {
    let raisons: Vec<QualityLimitationReason> = stats
        .iter()
        .filter_map(|s| match s {
            RtcStats::OutboundRtp(o) => Some(o.outbound.quality_limitation_reason),
            _ => None,
        })
        .collect();
    if raisons.contains(&QualityLimitationReason::Bandwidth) {
        Limitation::Debit
    } else if raisons.contains(&QualityLimitationReason::Cpu) {
        Limitation::Processeur
    } else if raisons.contains(&QualityLimitationReason::Other) {
        Limitation::Autre
    } else {
        Limitation::Aucune
    }
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

fn source_video(s: TrackSource) -> Option<SourceVideo> {
    match s {
        TrackSource::Camera => Some(SourceVideo::Camera),
        TrackSource::Screenshare => Some(SourceVideo::Partage),
        _ => None,
    }
}

fn piste_de(source: SourceVideo) -> TrackSource {
    match source {
        SourceVideo::Camera => TrackSource::Camera,
        SourceVideo::Partage => TrackSource::Screenshare,
    }
}

struct TrameLiveKit {
    tampon: Box<dyn VideoBuffer>,
}

fn en_i420(tampon: &dyn VideoBuffer) -> Option<I420Buffer> {
    if let Some(t) = tampon.as_i420() {
        return Some(t.to_i420());
    }
    if let Some(t) = tampon.as_nv12() {
        return Some(t.to_i420());
    }
    if let Some(t) = tampon.as_i420a() {
        return Some(t.to_i420());
    }
    if let Some(t) = tampon.as_i422() {
        return Some(t.to_i420());
    }
    if let Some(t) = tampon.as_i444() {
        return Some(t.to_i420());
    }
    if let Some(t) = tampon.as_i010() {
        return Some(t.to_i420());
    }
    tampon.as_native().map(|t| t.to_i420())
}

impl TrameVideo for TrameLiveKit {
    fn largeur(&self) -> u32 {
        self.tampon.width()
    }

    fn hauteur(&self) -> u32 {
        self.tampon.height()
    }

    fn vers_rgba(&self, largeur: u32, hauteur: u32, sortie: &mut [u8]) {
        let Some(mut image) = en_i420(&*self.tampon) else {
            return;
        };
        if sortie.len() < (largeur * hauteur * 4) as usize || largeur == 0 || hauteur == 0 {
            return;
        }
        if (largeur, hauteur) != (image.width(), image.height()) {
            image = image.scale(largeur as i32, hauteur as i32);
        }
        image.to_argb(
            VideoFormatType::ABGR,
            sortie,
            largeur * 4,
            largeur as i32,
            hauteur as i32,
        );
    }
}

fn video_publiee(identite: String, publication: &RemoteTrackPublication) -> Option<EvenementSalle> {
    if publication.kind() != TrackKind::Video || publication.is_muted() {
        return None;
    }
    Some(EvenementSalle::VideoPubliee {
        identite,
        source: source_video(publication.source())?,
        piste: publication.sid().to_string(),
    })
}

fn son_de_partage(identite: &str, s: TrackSource) -> bool {
    s == TrackSource::ScreenshareAudio || (est_partage(identite) && source(s).is_some())
}

fn publication_distante(
    identite: String,
    publication: &RemoteTrackPublication,
) -> Option<EvenementSalle> {
    if publication.kind() == TrackKind::Audio {
        if son_de_partage(&identite, publication.source()) {
            return Some(EvenementSalle::AudioPubliee {
                identite,
                source: SourceAudio::Partage,
            });
        }
        if source(publication.source()).is_some() {
            publication.set_subscribed(true);
        }
        return None;
    }
    video_publiee(identite, publication)
}

fn bascule_video(
    participant: &Participant,
    publication: &TrackPublication,
    muette: bool,
) -> Option<EvenementSalle> {
    let Participant::Remote(distant) = participant else {
        return None;
    };
    if publication.kind() != TrackKind::Video {
        return None;
    }
    let identite = distant.identity().to_string();
    let source = source_video(publication.source())?;
    Some(if muette {
        EvenementSalle::VideoRetiree { identite, source }
    } else {
        EvenementSalle::VideoPubliee {
            identite,
            source,
            piste: publication.sid().to_string(),
        }
    })
}

fn piste_video(
    identite: String,
    publication: &RemoteTrackPublication,
    piste: RemoteTrack,
) -> Option<EvenementSalle> {
    let RemoteTrack::Video(video) = piste else {
        return None;
    };
    let source = source_video(publication.source())?;
    let flux = NativeVideoStream::new(video.rtc_track());
    Some(EvenementSalle::PisteVideo {
        identite,
        source,
        images: flux
            .map(|image| {
                let trame: ImageVideo = Arc::new(TrameLiveKit {
                    tampon: image.buffer,
                });
                trame
            })
            .boxed(),
    })
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
        RoomEvent::TrackPublished {
            publication,
            participant,
        } => return publication_distante(participant.identity().to_string(), &publication),
        RoomEvent::TrackUnpublished {
            publication,
            participant,
        } if publication.kind() == TrackKind::Video => EvenementSalle::VideoRetiree {
            identite: participant.identity().to_string(),
            source: source_video(publication.source())?,
        },
        RoomEvent::TrackUnpublished {
            publication,
            participant,
        } if son_de_partage(participant.identity().as_str(), publication.source()) => {
            EvenementSalle::AudioRetiree {
                identite: participant.identity().to_string(),
                source: SourceAudio::Partage,
            }
        }
        RoomEvent::TrackMuted {
            participant,
            publication,
        } => return bascule_video(&participant, &publication, true),
        RoomEvent::TrackUnmuted {
            participant,
            publication,
        } => return bascule_video(&participant, &publication, false),
        RoomEvent::TrackSubscribed {
            track,
            publication,
            participant,
        } => {
            let identite = participant.identity().to_string();
            return match track {
                RemoteTrack::Video(_) => piste_video(identite, &publication, track),
                RemoteTrack::Audio(_) => piste_audio(identite, &publication, track),
            };
        }
        RoomEvent::TrackUnsubscribed {
            publication,
            participant,
            ..
        } if publication.kind() == TrackKind::Video => EvenementSalle::PisteVideoRetiree {
            identite: participant.identity().to_string(),
            source: source_video(publication.source())?,
        },
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

    fn publier_video(
        &self,
        source: SourceVideo,
        format: FormatVideo,
    ) -> BoxFuture<'_, Result<Arc<dyn SortieVideo>, ErreurSalle>> {
        Box::pin(async move {
            let video = NativeVideoSource::new(
                VideoResolution {
                    width: format.largeur,
                    height: format.hauteur,
                },
                source == SourceVideo::Partage && format.ecran,
            );
            let nom = match source {
                SourceVideo::Camera => "camera",
                SourceVideo::Partage => "screen",
            };
            let piste =
                LocalVideoTrack::create_video_track(nom, RtcVideoSource::Native(video.clone()));
            let publication = self
                .salle
                .local_participant()
                .publish_track(
                    LocalTrack::Video(piste),
                    TrackPublishOptions {
                        source: piste_de(source),
                        simulcast: format.simulcast,
                        video_encoding: Some(VideoEncoding {
                            max_bitrate: format.debit,
                            max_framerate: f64::from(format.images_par_seconde),
                        }),
                        degradation_preference: degradation(format.degradation),
                        ..TrackPublishOptions::default()
                    },
                )
                .await
                .map_err(|e| ErreurSalle::Publication(e.to_string()))?;
            let sortie: Arc<dyn SortieVideo> = Arc::new(SortieVideoLiveKit {
                source: video,
                debut: std::time::Instant::now(),
                piste: publication.sid().to_string(),
            });
            Ok(sortie)
        })
    }

    fn retirer_video(&self, source: SourceVideo) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let local = self.salle.local_participant();
            let sids: Vec<_> = local
                .track_publications()
                .into_iter()
                .filter(|(_, p)| p.source() == piste_de(source))
                .map(|(sid, _)| sid)
                .collect();
            for sid in sids {
                let _ = local.unpublish_track(&sid).await;
            }
        })
    }

    fn limitation_video(&self, source: SourceVideo) -> BoxFuture<'_, Limitation> {
        Box::pin(async move {
            let pistes: Vec<LocalVideoTrack> = self
                .salle
                .local_participant()
                .track_publications()
                .into_values()
                .filter(|p| p.source() == piste_de(source))
                .filter_map(|p| match p.track() {
                    Some(LocalTrack::Video(piste)) => Some(piste),
                    _ => None,
                })
                .collect();
            let mut stats = Vec::new();
            for piste in pistes {
                stats.extend(piste.get_stats().await.unwrap_or_default());
            }
            limitation(&stats)
        })
    }

    fn regler_video(&self, identite: &str, source: SourceVideo, demande: Option<DemandeVideo>) {
        let participants = self.salle.remote_participants();
        let Some(distant) = participants
            .values()
            .find(|p| p.identity().as_str() == identite)
        else {
            return;
        };
        for publication in distant.track_publications().values() {
            if publication.kind() != TrackKind::Video || publication.source() != piste_de(source) {
                continue;
            }
            publication.set_subscribed(demande.is_some());
            if let Some(d) = demande {
                publication.update_video_dimensions(TrackDimension(d.largeur, d.hauteur));
            }
        }
    }

    fn regler_audio(&self, identite: &str, source: SourceAudio, recevoir: bool) {
        let participants = self.salle.remote_participants();
        let Some(distant) = participants
            .values()
            .find(|p| p.identity().as_str() == identite)
        else {
            return;
        };
        for publication in distant.track_publications().values() {
            if publication.kind() != TrackKind::Audio {
                continue;
            }
            let partage = son_de_partage(identite, publication.source());
            if partage == (source == SourceAudio::Partage) {
                publication.set_subscribed(recevoir);
            }
        }
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
            let (envoi, evenements) = mpsc::unbounded_channel();
            for distant in salle.remote_participants().values() {
                participants.push(info_distant(distant));
                for publication in distant.track_publications().values() {
                    if let Some(e) =
                        publication_distante(distant.identity().to_string(), publication)
                    {
                        let _ = envoi.send(e);
                    }
                }
            }
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
