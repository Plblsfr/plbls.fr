#[path = "../tests/jeton_livekit/mod.rs"]
mod jeton_livekit;

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use estompe_client::audio::factice::SystemeFactice;
use estompe_client::audio::{FormatAudio, SensAudio, ECHANTILLONS_PAR_TRAME, FREQUENCE};
use estompe_client::mesures::{
    construire_export, maintenant_ms, Marqueur, Releveur, Systeme, INTERVALLE_MS,
};
use estompe_client::vocal::livekit::{apm, ConnecteurLiveKit};
use estompe_client::vocal::{Dependances, Phase, Reglages, SessionVocale};
use estompe_plateforme::{MesureProcessus, MesureSysteme};
use jeton_livekit::ApiDeTest;

const VOIX: &[u8] = include_bytes!("../tests/donnees/voix-16k.wav");
const MICRO: &str = "micro";
const SORTIE: &str = "hp";
const SILENCE: u8 = 0;
const PAROLE: u8 = 1;
const ATTENTE_CONNEXION: Duration = Duration::from_secs(30);

struct Options {
    url: String,
    salle: String,
    identite: String,
    parle: bool,
    duree: u64,
    phases: Vec<(String, u64)>,
    sortie: Option<String>,
}

fn valeur(arguments: &[String], nom: &str) -> Option<String> {
    arguments
        .iter()
        .position(|a| a == nom)
        .and_then(|i| arguments.get(i + 1).cloned())
}

fn options() -> Result<Options, String> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let phases = valeur(&arguments, "--phases")
        .unwrap_or_else(|| "repos:30,silence:60,parole:60".into())
        .split(',')
        .map(|p| {
            let (nom, s) = p.split_once(':').ok_or(format!("phase « {p} »"))?;
            let s = s.parse::<u64>().map_err(|_| format!("phase « {p} »"))?;
            Ok((nom.to_string(), s))
        })
        .collect::<Result<Vec<_>, String>>()?;
    if phases
        .iter()
        .any(|(nom, _)| !["repos", "silence", "parole"].contains(&nom.as_str()))
    {
        return Err("phases possibles : repos, silence, parole".into());
    }
    Ok(Options {
        url: valeur(&arguments, "--url")
            .or_else(|| std::env::var("ESTOMPE_LIVEKIT_URL").ok())
            .ok_or("--url ou ESTOMPE_LIVEKIT_URL")?,
        salle: valeur(&arguments, "--salle").unwrap_or_else(|| "mesure".into()),
        identite: valeur(&arguments, "--identite").unwrap_or_else(|| "mesure".into()),
        parle: arguments.iter().any(|a| a == "--parle"),
        duree: valeur(&arguments, "--duree")
            .map(|d| d.parse::<u64>().map_err(|_| "--duree en secondes"))
            .transpose()?
            .unwrap_or(180),
        phases,
        sortie: valeur(&arguments, "--sortie"),
    })
}

fn voix_48k() -> Vec<f32> {
    let debut = VOIX
        .windows(4)
        .position(|w| w == b"data")
        .map_or(44, |i| i + 8);
    VOIX[debut..]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|o| f32::from(i16::from_le_bytes(*o)) / 32_768.0)
        .flat_map(|e| [e; 3])
        .collect()
}

fn bruit(graine: &mut u32) -> f32 {
    *graine = graine.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    ((*graine >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.002
}

fn audio() -> SystemeFactice {
    let audio = SystemeFactice::nouveau();
    let format = FormatAudio {
        frequence: FREQUENCE,
        canaux: 1,
    };
    audio.brancher(SensAudio::Entree, MICRO, "Micro", format);
    audio.brancher(SensAudio::Sortie, SORTIE, "Haut-parleurs", format);
    audio
}

fn horloge_audio(
    audio: SystemeFactice,
    mode: Arc<AtomicU8>,
    arret: Arc<AtomicBool>,
    recus: Arc<AtomicUsize>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let voix = voix_48k();
        let mut position = 0;
        let mut graine = 1u32;
        let pas = Duration::from_millis(10);
        let mut prochaine = Instant::now();
        let mut trame = vec![0f32; ECHANTILLONS_PAR_TRAME];
        while !arret.load(Ordering::Acquire) {
            let parle = mode.load(Ordering::Acquire) == PAROLE;
            for e in trame.iter_mut() {
                *e = bruit(&mut graine);
                if parle {
                    *e += voix[position];
                    position = (position + 1) % voix.len();
                }
            }
            audio.pousser(MICRO, &trame);
            if let Some(joue) = audio.tirer(SORTIE, ECHANTILLONS_PAR_TRAME) {
                if joue.iter().any(|e| e.abs() > 0.01) {
                    recus.fetch_add(1, Ordering::Relaxed);
                }
            }
            prochaine += pas;
            if let Some(reste) = prochaine.checked_duration_since(Instant::now()) {
                std::thread::sleep(reste);
            }
        }
    })
}

async fn attendre_connexion(session: &SessionVocale) -> Result<(), String> {
    let mut etat = session.etat();
    tokio::time::timeout(ATTENTE_CONNEXION, async {
        loop {
            if etat.borrow_and_update().phase == Phase::Connecte {
                return;
            }
            if etat.changed().await.is_err() {
                return;
            }
        }
    })
    .await
    .map_err(|_| format!("connexion impossible : {:?}", session.instantane().erreur))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), String> {
    let o = options()?;
    let source: Arc<dyn MesureProcessus> = Arc::new(MesureSysteme);
    let releveur = match &o.sortie {
        Some(_) => {
            Some(Releveur::demarrer(source.clone(), INTERVALLE_MS).map_err(|e| e.to_string())?)
        }
        None => None,
    };
    let audio = audio();
    let session = SessionVocale::demarrer(
        &tokio::runtime::Handle::current(),
        Dependances {
            api: Arc::new(ApiDeTest {
                url: o.url.clone(),
                cle: std::env::var("ESTOMPE_LIVEKIT_CLE").unwrap_or_else(|_| "devkey".into()),
                secret: std::env::var("ESTOMPE_LIVEKIT_SECRET").unwrap_or_else(|_| "secret".into()),
                identite: o.identite.clone(),
            }),
            connecteur: Arc::new(ConnecteurLiveKit),
            audio: Arc::new(audio.clone()),
            cameras: Arc::new(estompe_client::vocal::CamerasFactices::nouvelles()),
            ecrans: Arc::new(estompe_plateforme::EcransFactices::nouveaux()),
            sons: Arc::new(estompe_plateforme::Indisponible),
            apm: Some(apm()),
            reglages: Reglages {
                micro: Some(MICRO.into()),
                sortie: Some(SORTIE.into()),
                ..Reglages::default()
            },
        },
    );
    let mode = Arc::new(AtomicU8::new(if o.parle { PAROLE } else { SILENCE }));
    let arret = Arc::new(AtomicBool::new(false));
    let recus = Arc::new(AtomicUsize::new(0));
    let fil = horloge_audio(audio, mode.clone(), arret.clone(), recus.clone());

    let mut marqueurs = Vec::new();
    if releveur.is_none() {
        session.rejoindre(&o.salle);
        attendre_connexion(&session).await?;
        tokio::time::sleep(Duration::from_secs(o.duree)).await;
    } else {
        for (nom, secondes) in &o.phases {
            if nom != "repos" && session.instantane().phase != Phase::Connecte {
                session.rejoindre(&o.salle);
                attendre_connexion(&session).await?;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            mode.store(
                if nom == "parole" { PAROLE } else { SILENCE },
                Ordering::Release,
            );
            let debut = maintenant_ms();
            tokio::time::sleep(Duration::from_secs(*secondes)).await;
            marqueurs.push(Marqueur {
                nom: nom.clone(),
                debut,
                fin: maintenant_ms(),
            });
        }
    }
    let participants = session.instantane().participants.len();
    session.fermer().await;
    arret.store(true, Ordering::Release);
    let _ = fil.join();

    if let (Some(releveur), Some(sortie)) = (releveur, &o.sortie) {
        let export = construire_export(
            releveur.arreter(),
            "banc vocal natif",
            Systeme::de_la_machine(source.machine(), None),
            marqueurs,
            maintenant_ms(),
        );
        println!(
            "{} : {participants} participants vus, {} trames de son reçues",
            o.identite,
            recus.load(Ordering::Relaxed)
        );
        println!("| Phase | Mémoire privée, médiane (Mo) | Working set, médiane (Mo) | Working set, pic (Mo) | CPU (% d'un cœur) |");
        println!("|---|---:|---:|---:|---:|");
        for (nom, s) in export
            .marqueurs
            .iter()
            .filter_map(|m| Some((&m.nom, export.statistiques.scenarios.get(&m.nom)?)))
        {
            println!(
                "| {nom} | {} | {} | {} | {} |",
                s.total.prive.mediane_mo, s.total.ws.mediane_mo, s.total.ws.pic_mo, s.total.cpu_pct
            );
        }
        let texte = serde_json::to_string_pretty(&export).map_err(|e| e.to_string())?;
        std::fs::write(sortie, format!("{texte}\n")).map_err(|e| e.to_string())?;
    }
    Ok(())
}
