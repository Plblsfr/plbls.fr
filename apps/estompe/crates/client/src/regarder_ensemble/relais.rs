use std::sync::{Arc, Mutex};

use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::http::{Client, ErreurApi};

use super::avance::{Avance, Fragment};

const TETE_MAX: usize = 16 * 1024;

pub fn chemin_d_adresse(uri: &str) -> String {
    let bas = uri.to_ascii_lowercase();
    if bas.starts_with("http://") || bas.starts_with("https://") {
        let reste = &uri[uri.find("//").map_or(0, |i| i + 2)..];
        return reste
            .find('/')
            .map_or("/".to_string(), |i| reste[i..].to_string());
    }
    uri.to_string()
}

fn resoudre(uri: &str, liste: &str) -> String {
    let uri = chemin_d_adresse(uri);
    if uri.starts_with('/') {
        return uri;
    }
    let sans_requete = liste.split(['?', '#']).next().unwrap_or(liste);
    let dossier = sans_requete.rsplit_once('/').map_or("", |(d, _)| d);
    format!("{dossier}/{uri}")
}

pub fn lire_liste(texte: &str, chemin: &str) -> (Vec<Fragment>, String) {
    let mut fragments = Vec::new();
    let mut debut = 0.0;
    let mut duree = None;
    let mut sortie = String::with_capacity(texte.len());
    for ligne in texte.lines() {
        let l = ligne.trim();
        if let Some(reste) = l.strip_prefix("#EXTINF:") {
            duree = reste
                .split(',')
                .next()
                .and_then(|d| d.trim().parse::<f64>().ok());
            sortie.push_str(ligne);
        } else if !l.is_empty() && !l.starts_with('#') {
            let url = resoudre(l, chemin);
            if let Some(d) = duree.take() {
                fragments.push(Fragment {
                    url: url.clone(),
                    debut,
                    duree: d,
                });
                debut += d;
            }
            sortie.push_str(&url);
        } else {
            sortie.push_str(ligne);
        }
        sortie.push('\n');
    }
    (fragments, sortie)
}

type AvancePartagee = Arc<Mutex<Option<Avance>>>;

pub struct Relais {
    adresse: String,
    avance: AvancePartagee,
    tache: JoinHandle<()>,
}

impl Relais {
    pub async fn demarrer(client: Client) -> std::io::Result<Self> {
        let ecoute = TcpListener::bind("127.0.0.1:0").await?;
        Self::ecouter(ecoute, client, &tokio::runtime::Handle::current())
    }

    pub fn demarrer_sur(runtime: &tokio::runtime::Handle, client: Client) -> std::io::Result<Self> {
        let ecoute = std::net::TcpListener::bind("127.0.0.1:0")?;
        ecoute.set_nonblocking(true)?;
        let _contexte = runtime.enter();
        Self::ecouter(TcpListener::from_std(ecoute)?, client, runtime)
    }

    fn ecouter(
        ecoute: TcpListener,
        client: Client,
        runtime: &tokio::runtime::Handle,
    ) -> std::io::Result<Self> {
        let adresse = format!("http://{}", ecoute.local_addr()?);
        let avance: AvancePartagee = Arc::default();
        let partagee = avance.clone();
        let tache = runtime.spawn(async move {
            while let Ok((flux, _)) = ecoute.accept().await {
                tokio::spawn(servir(flux, client.clone(), partagee.clone()));
            }
        });
        Ok(Self {
            adresse,
            avance,
            tache,
        })
    }

    pub fn adresse(&self) -> &str {
        &self.adresse
    }

    pub fn definir_avance(&self, avance: Option<Avance>) {
        let ancienne = std::mem::replace(
            &mut *self.avance.lock().unwrap_or_else(|e| e.into_inner()),
            avance,
        );
        if let Some(a) = ancienne {
            a.detruire();
        }
    }

    pub fn avance(&self) -> Option<Avance> {
        self.avance
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl Drop for Relais {
    fn drop(&mut self) {
        self.tache.abort();
        self.definir_avance(None);
    }
}

async fn lire_tete(flux: &mut TcpStream) -> Option<(String, bool)> {
    let mut recu = Vec::new();
    let mut tampon = [0u8; 4096];
    while !recu.windows(4).any(|f| f == b"\r\n\r\n") {
        let lus = flux.read(&mut tampon).await.ok()?;
        if lus == 0 || recu.len() > TETE_MAX {
            return None;
        }
        recu.extend_from_slice(&tampon[..lus]);
    }
    let tete = String::from_utf8_lossy(&recu);
    let mut lignes = tete.split("\r\n");
    let mut premiere = lignes.next()?.split(' ');
    let (methode, chemin) = (premiere.next()?, premiere.next()?);
    if methode != "GET" || !chemin.starts_with('/') {
        return None;
    }
    let plage = lignes.any(|l| {
        l.split_once(':')
            .is_some_and(|(n, _)| n.trim().eq_ignore_ascii_case("range"))
    });
    Some((chemin.to_string(), plage))
}

async fn repondre(flux: &mut TcpStream, statut: &str, type_mime: &str, corps: &[u8]) {
    let tete = format!(
        "HTTP/1.1 {statut}\r\nContent-Type: {type_mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        corps.len()
    );
    let _ = flux.write_all(tete.as_bytes()).await;
    let _ = flux.write_all(corps).await;
    let _ = flux.shutdown().await;
}

fn statut_d_erreur(e: &ErreurApi) -> &'static str {
    match e {
        ErreurApi::Introuvable(_) => "404 Not Found",
        ErreurApi::NonAutorise(_) | ErreurApi::Interdit(_) => "403 Forbidden",
        _ => "502 Bad Gateway",
    }
}

async fn servir(mut flux: TcpStream, client: Client, avance: AvancePartagee) {
    let Some((chemin, plage)) = lire_tete(&mut flux).await else {
        repondre(&mut flux, "400 Bad Request", "text/plain", b"").await;
        return;
    };
    let sans_requete = chemin.split(['?', '#']).next().unwrap_or(&chemin);
    let a = avance.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if sans_requete.ends_with(".m3u8") {
        match client.octets(&chemin).await {
            Ok(o) => {
                let (fragments, liste) = lire_liste(&String::from_utf8_lossy(&o), &chemin);
                if let Some(a) = &a {
                    a.definir_fragments(fragments);
                }
                repondre(
                    &mut flux,
                    "200 OK",
                    "application/vnd.apple.mpegurl",
                    liste.as_bytes(),
                )
                .await;
            }
            Err(e) => repondre(&mut flux, statut_d_erreur(&e), "text/plain", b"").await,
        }
        return;
    }
    let par_avance = a.filter(|a| !plage && (a.active() || a.contient(&chemin)));
    let resultat: Result<Bytes, &'static str> = match par_avance {
        Some(a) => match a.obtenir(&chemin).await {
            Ok(o) => Ok(o),
            Err(_) => client
                .octets(&chemin)
                .await
                .map_err(|e| statut_d_erreur(&e)),
        },
        None => client
            .octets(&chemin)
            .await
            .map_err(|e| statut_d_erreur(&e)),
    };
    match resultat {
        Ok(o) => repondre(&mut flux, "200 OK", "video/mp2t", &o).await,
        Err(statut) => repondre(&mut flux, statut, "text/plain", b"").await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regarder_ensemble::avance::{Budget, OptionsAvance, TelechargeurHttp};
    use crate::serveur_factice::{Reponse, ServeurFactice};
    use std::time::Duration;

    const LISTE: &str = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4.000,\nseg0.ts\n#EXTINF:4.000,\nhttps://entracte.test/stream/j/seg1.ts\n#EXTINF:2.5,\n/stream/j/seg2.ts\n#EXT-X-ENDLIST\n";

    #[test]
    fn la_liste_donne_les_segments_et_ramene_les_adresses_au_relais() {
        let (fragments, liste) = lire_liste(LISTE, "/stream/j/hls.m3u8?x=1");
        assert_eq!(
            fragments
                .iter()
                .map(|f| (f.url.as_str(), f.debut, f.duree))
                .collect::<Vec<_>>(),
            [
                ("/stream/j/seg0.ts", 0.0, 4.0),
                ("/stream/j/seg1.ts", 4.0, 4.0),
                ("/stream/j/seg2.ts", 8.0, 2.5)
            ]
        );
        assert!(!liste.contains("https://"));
        assert!(liste.contains("#EXT-X-ENDLIST"));
        assert_eq!(chemin_d_adresse("https://h.test"), "/");
    }

    async fn obtenir(adresse: &str) -> (u16, Vec<u8>) {
        let r = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(adresse)
            .send()
            .await
            .unwrap();
        (r.status().as_u16(), r.bytes().await.unwrap().to_vec())
    }

    #[tokio::test]
    async fn le_relais_sert_les_segments_precharges_et_passe_le_reste_a_entracte() {
        let serveur = ServeurFactice::demarrer([Reponse::vide(404)]).await;
        serveur.router_dynamique("GET", "/stream/j/hls.m3u8", |_| {
            Reponse::brute(200, "application/vnd.apple.mpegurl", LISTE.as_bytes())
        });
        for n in 0..3 {
            serveur.router_dynamique("GET", &format!("/stream/j/seg{n}.ts"), move |_| {
                Reponse::brute(200, "video/mp2t", &[n as u8; 8])
            });
        }
        let client = serveur.client();
        let relais = Relais::demarrer(client.clone()).await.unwrap();
        let (statut, _) = obtenir(&format!("{}/stream/j/seg0.ts", relais.adresse())).await;
        assert_eq!(statut, 200);
        let avance = Avance::nouvelle(
            || {
                Some(Budget {
                    secondes: 100.0,
                    octets: 1 << 20,
                })
            },
            || 0.0,
            Arc::new(TelechargeurHttp::nouveau(client)),
            OptionsAvance::default(),
        );
        relais.definir_avance(Some(avance.clone()));
        let (statut, liste) = obtenir(&format!("{}/stream/j/hls.m3u8", relais.adresse())).await;
        assert_eq!(statut, 200);
        assert!(String::from_utf8(liste)
            .unwrap()
            .contains("/stream/j/seg1.ts"));
        let mut essais = 0;
        while avance.plages() != [(0.0, 10.5)] {
            essais += 1;
            assert!(
                essais < 500,
                "préchargement inachevé : {:?}",
                avance.plages()
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let (statut, corps) = obtenir(&format!("{}/stream/j/seg1.ts", relais.adresse())).await;
        assert_eq!((statut, corps), (200, vec![1u8; 8]));
        let lignes: Vec<String> = serveur
            .requetes()
            .iter()
            .map(|r| r.chemin.clone())
            .collect();
        assert_eq!(
            lignes
                .iter()
                .filter(|c| c.starts_with("/stream/j/seg1.ts"))
                .count(),
            1
        );
        assert!(lignes.contains(&"/stream/j/seg1.ts?avance=1".to_string()));
        let (statut, _) = obtenir(&format!("{}/stream/j/absent.ts", relais.adresse())).await;
        assert_eq!(statut, 404);
        drop(relais);
        assert_eq!(avance.octets(), 0);
    }
}
