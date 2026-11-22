use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use estompe_client::replay::automate::Jeu;
use estompe_client::replay::cadence::{encodeur_muet, Alignement, Cadence, Perte, Pertes};
use estompe_client::replay::garde::Garde;
use estompe_client::replay::image::vers_nv12;
use estompe_client::replay::micro::{Traitement, FREQUENCE};
use estompe_client::replay::mp4::{avec_parametres, en_longueurs};
use estompe_client::replay::reglages::{self, Materiel, AUDIO_JEU_KBPS, AUDIO_MICRO_KBPS, GOP_S};
use estompe_client::replay::son::{avec_silence, Conversion};
use estompe_client::replay::tampon::{Format, Piste, Tampon, Unite, UNITE_TEMPS};
use estompe_client::replay::types::{Codec, ErreurReplay, DUREE_MIN_S};
use estompe_plateforme::{
    CibleSon, EncodeurVideo, EvenementCapture, FluxCapture, FluxSon, FormatAudio, IdSource, Image,
    ImageNv12, MaterielEncodage, Micro, OptionsCapture, Paquet, ParametresAudio, ParametresVideo,
};

use super::{Dependances, Message, ReglagesMoteur, ENCODEUR_MUET};

pub const IMAGE_CHANGEE: &str = "image-changee";
pub const ATTENTE_PREMIERE_IMAGE: Duration = Duration::from_secs(3);
const PAS_MAX: Duration = Duration::from_millis(20);
const SANS_IMAGE: &str =
    "le jeu ne fournit aucune image à la capture (plein écran exclusif ou protection)";

type Derniere = Arc<Mutex<Option<Arc<Image>>>>;
type Refus = (&'static str, String);

pub(super) struct Ouverture<'a> {
    pub numero: u64,
    pub jeu: &'a Jeu,
    pub reglages: &'a ReglagesMoteur,
    pub materiel: &'a MaterielEncodage,
    pub d: &'a Dependances,
    pub tampon: Arc<Mutex<Tampon>>,
    pub tx: Sender<Message>,
    pub garde: Arc<Garde>,
}

pub(super) struct Session {
    pub numero: u64,
    pub format: Format,
    pub encodeur: String,
    pub source: (u32, u32),
    arret: Arc<AtomicBool>,
    capture: Option<FluxCapture>,
    sons: Vec<FluxSon>,
    rythme: Option<JoinHandle<()>>,
}

impl Drop for Session {
    fn drop(&mut self) {
        self.arret.store(true, Ordering::Relaxed);
        self.sons.clear();
        self.capture.take();
        if let Some(rythme) = self.rythme.take() {
            let _ = rythme.join();
        }
    }
}

fn temps(horloge: Instant) -> i64 {
    (horloge.elapsed().as_nanos() / 100) as i64
}

fn depot(
    tampon: Arc<Mutex<Tampon>>,
    arret: Arc<AtomicBool>,
    piste: Piste,
    base: i64,
) -> impl FnMut(Paquet) + Send + 'static {
    move |p: Paquet| {
        if arret.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(mut t) = tampon.lock() {
            t.pousser(
                piste,
                Unite {
                    pts: base + p.pts,
                    duree: p.duree,
                    cle: p.cle,
                    donnees: p.donnees.into(),
                },
            );
        }
    }
}

fn depot_video(
    tampon: Arc<Mutex<Tampon>>,
    arret: Arc<AtomicBool>,
    codec: Codec,
    base: i64,
) -> impl FnMut(Paquet) + Send + 'static {
    let mut entete = Vec::new();
    let mut pousser = depot(tampon, arret, Piste::Video, base);
    move |p: Paquet| {
        let mut donnees = en_longueurs(&p.donnees);
        if donnees.is_empty() {
            return;
        }
        if p.cle {
            donnees = avec_parametres(codec, donnees, &mut entete);
        }
        pousser(Paquet { donnees, ..p });
    }
}

fn recepteur_capture(
    numero: u64,
    pid: u32,
    tx: Sender<Message>,
    derniere: Derniere,
    premiere: Sender<(u32, u32)>,
) -> estompe_plateforme::RecepteurCapture {
    let mut taille = None;
    Box::new(move |evenement| match evenement {
        EvenementCapture::Trame(image) => {
            let dimensions = (image.largeur, image.hauteur);
            match taille {
                None => {
                    taille = Some(dimensions);
                    let _ = premiere.send(dimensions);
                }
                Some(t) if t != dimensions => {
                    let _ = tx.send(Message::Echec(numero, IMAGE_CHANGEE, String::new()));
                    return false;
                }
                _ => {}
            }
            if let Ok(mut d) = derniere.lock() {
                *d = Some(Arc::new(image));
            }
            true
        }
        EvenementCapture::Fermee => {
            let _ = tx.send(Message::JeuFerme(pid));
            false
        }
        EvenementCapture::Echec(raison) => {
            let _ = tx.send(Message::Echec(numero, "capture-refusee", raison));
            false
        }
        EvenementCapture::Reduite | EvenementCapture::Restauree => true,
    })
}

struct Pistes {
    tampon: Arc<Mutex<Tampon>>,
    arret: Arc<AtomicBool>,
    base: i64,
    horloge: Instant,
}

impl Pistes {
    fn jeu(&self, d: &Dependances, pid: u32) -> Result<FluxSon, String> {
        let mut aac = d
            .encodage
            .audio(
                ParametresAudio {
                    frequence: FREQUENCE,
                    canaux: 2,
                    debit_kbps: AUDIO_JEU_KBPS,
                },
                Box::new(depot(
                    self.tampon.clone(),
                    self.arret.clone(),
                    Piste::Jeu,
                    self.base,
                )),
            )
            .map_err(|e| e.to_string())?;
        let mut conversion = Conversion::nouvelle(2);
        let mut alignement = Alignement::nouveau(FREQUENCE);
        let horloge = self.horloge;
        d.sons
            .demarrer(
                CibleSon::Application(pid),
                Box::new(move |entrelace, format| {
                    let pcm = conversion.convertir(entrelace, format);
                    let silence = alignement.silence(temps(horloge), (pcm.len() / 2) as u64);
                    let _ = aac.soumettre(&avec_silence(silence, 2, &pcm));
                }),
            )
            .map_err(|e| e.to_string())
    }

    fn micro(
        &self,
        d: &Dependances,
        micro: &Arc<dyn Micro + Send + Sync>,
        reglages: &ReglagesMoteur,
    ) -> Result<FluxSon, String> {
        if !micro.disponible() {
            return Err("aucun micro".into());
        }
        let mut aac = d
            .encodage
            .audio(
                ParametresAudio {
                    frequence: FREQUENCE,
                    canaux: 1,
                    debit_kbps: AUDIO_MICRO_KBPS,
                },
                Box::new(depot(
                    self.tampon.clone(),
                    self.arret.clone(),
                    Piste::Micro,
                    self.base,
                )),
            )
            .map_err(|e| e.to_string())?;
        let mut conversion = Conversion::nouvelle(1);
        let mut traitement = Traitement::nouveau(reglages.debruiter, reglages.gain_micro);
        let mut alignement = Alignement::nouveau(FREQUENCE);
        let horloge = self.horloge;
        micro
            .ecouter(Box::new(move |mono, frequence| {
                let format = FormatAudio {
                    frequence,
                    canaux: 1,
                };
                let pcm = conversion.convertir(mono, format);
                let silence = alignement.silence(temps(horloge), pcm.len() as u64);
                let mut sortie = vec![0i16; silence as usize];
                sortie.extend(traitement.traiter(&pcm));
                let _ = aac.soumettre(&sortie);
            }))
            .map_err(|e| e.to_string())
    }
}

impl Session {
    pub(super) fn ouvrir(o: Ouverture<'_>) -> Result<Self, Refus> {
        let r = &o.reglages.replay;
        let numero = o.numero;
        let arret = Arc::new(AtomicBool::new(false));
        let derniere: Derniere = Arc::default();
        let (premiere, attente) = mpsc::channel();
        let recepteur =
            recepteur_capture(numero, o.jeu.pid, o.tx.clone(), derniere.clone(), premiere);
        let source_capture = IdSource::Fenetre(o.jeu.fenetre as u32).to_string();
        let options = OptionsCapture {
            ips: r.ips,
            curseur: r.curseur,
        };
        let capture =
            o.d.ecrans
                .demarrer(&source_capture, options, recepteur)
                .map_err(|e| ("capture-refusee", e.to_string()))?;
        let source = attente
            .recv_timeout(ATTENTE_PREMIERE_IMAGE)
            .map_err(|_| ("capture-refusee", SANS_IMAGE.to_string()))?;
        let m = Materiel::de(o.materiel, source);
        let (dimensions, ips, codec) =
            reglages::effectifs(r, &m).map_err(|e| ("sans-encodeur", e))?;
        let debit = reglages::debit_kbps(dimensions, ips, r.qualite);
        let total = debit + reglages::audio_kbps(r);
        let duree = r
            .duree_s
            .min(reglages::duree_max_s(total, m.ram_libre))
            .max(DUREE_MIN_S);
        let format = Format {
            codec,
            largeur: dimensions.0,
            hauteur: dimensions.1,
            ips,
            micro: r.micro,
        };
        let base = {
            let mut t = o
                .tampon
                .lock()
                .map_err(|_| ("tampon", "tampon inaccessible".to_string()))?;
            t.preparer(
                format,
                i64::from(duree) * UNITE_TEMPS,
                reglages::memoire_octets(total, duree),
            );
            t.reprise()
        };
        o.garde.debut();
        let arret_encodeur = o.tx.clone();
        let video = o.d.encodage.video(
            ParametresVideo {
                codec,
                largeur: dimensions.0,
                hauteur: dimensions.1,
                ips,
                debit_kbps: debit,
                images_par_groupe: ips * GOP_S,
            },
            Box::new(depot_video(o.tampon.clone(), arret.clone(), codec, base)),
            Box::new(move |raison| {
                let _ = arret_encodeur.send(Message::Echec(numero, "encodeur-arrete", raison));
            }),
        );
        let video = video.map_err(|e| {
            o.garde.relacher();
            ("sans-encodeur", e.to_string())
        })?;
        let encodeur = video.nom().to_string();
        let horloge = Instant::now();
        let pistes = Pistes {
            tampon: o.tampon.clone(),
            arret: arret.clone(),
            base,
            horloge,
        };
        let alerte = |code: &'static str, message: String| {
            let _ =
                o.tx.send(Message::Alerte(ErreurReplay::nouvelle(code, message)));
        };
        let mut sons = Vec::new();
        match pistes.jeu(o.d, o.jeu.pid) {
            Ok(flux) => sons.push(flux),
            Err(e) => alerte(
                "son-indisponible",
                format!("Son du jeu non enregistré : {e}."),
            ),
        }
        if r.micro {
            let ouvert = match &o.d.micro {
                Some(micro) => pistes.micro(o.d, micro, o.reglages),
                None => Err("aucun micro".into()),
            };
            match ouvert {
                Ok(flux) => sons.push(flux),
                Err(e) => alerte("micro-indisponible", format!("Micro non enregistré : {e}.")),
            }
        }
        let rythme = Rythme {
            video,
            derniere,
            cadence: Cadence::nouvelle(ips),
            ips,
            dimensions,
            horloge,
            arret: arret.clone(),
            tx: o.tx.clone(),
            numero,
            garde: o.garde.clone(),
        };
        let rythme = std::thread::Builder::new()
            .name("replay-rythme".into())
            .spawn(move || rythme.tourner())
            .map_err(|e| ("rythme", e.to_string()))?;
        Ok(Self {
            numero,
            format,
            encodeur,
            source,
            arret,
            capture: Some(capture),
            sons,
            rythme: Some(rythme),
        })
    }
}

struct Rythme {
    video: Box<dyn EncodeurVideo>,
    derniere: Derniere,
    cadence: Cadence,
    ips: u32,
    dimensions: (u32, u32),
    horloge: Instant,
    arret: Arc<AtomicBool>,
    tx: Sender<Message>,
    numero: u64,
    garde: Arc<Garde>,
}

impl Rythme {
    fn echec(&self, code: &'static str, raison: &str) {
        let _ = self
            .tx
            .send(Message::Echec(self.numero, code, raison.to_string()));
    }

    fn tourner(mut self) {
        let ips = u64::from(self.ips.max(1));
        let fenetre = 10 * ips;
        let (mut sans_image, mut releve) = (0u64, (0u64, Pertes::default()));
        let (mut premiere, mut validee) = (None, false);
        let mut convertie: Option<(Arc<Image>, ImageNv12)> = None;
        while !self.arret.load(Ordering::Relaxed) {
            let ecoule = temps(self.horloge);
            let attente = self.cadence.echeance() - ecoule;
            if attente > 0 {
                std::thread::sleep(Duration::from_nanos(attente as u64 * 100).min(PAS_MAX));
                continue;
            }
            for n in self.cadence.dues(ecoule) {
                let image = self.derniere.lock().ok().and_then(|d| d.clone());
                let Some(image) = image else {
                    sans_image += 1;
                    continue;
                };
                sans_image = 0;
                let nv12 = match &convertie {
                    Some((source, nv12)) if Arc::ptr_eq(source, &image) => nv12.clone(),
                    _ => {
                        let nv12 =
                            vers_nv12(&image.rgba, image.largeur, image.hauteur, self.dimensions);
                        convertie = Some((image, nv12.clone()));
                        nv12
                    }
                };
                let pts = self.cadence.horodatage(n);
                match self.video.soumettre(nv12, pts, self.cadence.duree(n)) {
                    Ok(true) => {
                        premiere.get_or_insert(n);
                    }
                    Ok(false) => self.cadence.perdre(Perte::Encodeur),
                    Err(_) => self.cadence.perdre(Perte::Conversion),
                }
            }
            if sans_image > 5 * ips {
                self.echec("capture-refusee", SANS_IMAGE);
                break;
            }
            let (emises, pertes) = self.cadence.bilan();
            let encodees = self.video.encodees();
            if encodees > 0 && !validee {
                validee = true;
                self.garde.valider();
            }
            if encodeur_muet(self.ips, premiere, emises, encodees) {
                self.echec(ENCODEUR_MUET, "");
                break;
            }
            if emises - releve.0 >= fenetre {
                if let Some(cause) = pertes.depuis(&releve.1).alerte(emises - releve.0) {
                    let (code, message) = cause.erreur();
                    let _ = self
                        .tx
                        .send(Message::Alerte(ErreurReplay::nouvelle(code, message)));
                }
                releve = (emises, pertes);
            }
        }
    }
}
