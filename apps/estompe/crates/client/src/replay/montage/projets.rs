use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::super::galerie::{self, Horodatage};
use super::plan::{duree_finale, sources, verifier};
use super::{Montage, NOM_MAX, VERSION};

const PROJETS: &str = "projets";
const ANCIEN: &str = ".montage.json";

#[derive(Clone, Debug, PartialEq)]
pub struct Resume {
    pub id: String,
    pub nom: String,
    pub date: String,
    pub modifie: i64,
    pub duree_s: f64,
    pub clips: Vec<String>,
    pub manquants: Vec<String>,
}

fn nom_de(chemin: &Path) -> String {
    chemin
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn source_valide(nom: &str) -> bool {
    nom.len() > 4
        && nom.to_ascii_lowercase().ends_with(".mp4")
        && Path::new(nom).file_name().is_some_and(|n| n == nom)
        && !nom.contains(['/', '\\', ':'])
}

fn id_valide(id: &str) -> bool {
    (1..=48).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn fichier_projet(dossier: &Path, id: &str) -> Result<PathBuf, String> {
    if !id_valide(id) {
        return Err("projet inconnu".into());
    }
    Ok(dossier
        .join(galerie::ANNEXES)
        .join(PROJETS)
        .join(format!("{id}.json")))
}

fn maintenant() -> Horodatage {
    let secondes = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    Horodatage::depuis_epoque(secondes)
}

pub fn nouvel_id(dossier: &Path) -> String {
    let date = maintenant().pour_nom().replace(' ', "-");
    (1u32..)
        .map(|n| format!("{date}-{n}"))
        .find(|id| fichier_projet(dossier, id).is_ok_and(|f| !f.exists()))
        .unwrap_or(date)
}

pub fn lire_projet(octets: &[u8], source: Option<&str>) -> Option<Montage> {
    let mut brut: serde_json::Value = serde_json::from_slice(octets).ok()?;
    let ancien = brut.get("version")?.as_u64()? == 1;
    if ancien {
        brut["version"] = VERSION.into();
    }
    let mut m: Montage = serde_json::from_value(brut).ok()?;
    if let Some(source) = source.filter(|_| ancien) {
        m.segments
            .iter_mut()
            .for_each(|s| s.source = source.to_string());
        m.volumes = m
            .volumes
            .into_values()
            .next()
            .map(|v| BTreeMap::from([(source.to_string(), v)]))
            .unwrap_or_default();
        m.nom = source
            .strip_suffix(".mp4")
            .unwrap_or(source)
            .chars()
            .take(NOM_MAX)
            .collect();
    }
    verifier(&m).ok().map(|_| m)
}

fn ecrire_projet(dossier: &Path, id: &str, montage: &Montage) -> Result<(), String> {
    let fichier = fichier_projet(dossier, id)?;
    galerie::creer_annexes(dossier)
        .and_then(|a| fs::create_dir_all(a.join(PROJETS)))
        .map_err(|e| format!("dossier des clips inaccessible : {e}"))?;
    let partiel = fichier.with_extension(galerie::PARTIEL);
    let octets = serde_json::to_vec(montage).map_err(|e| e.to_string())?;
    fs::write(&partiel, octets)
        .and_then(|_| fs::rename(&partiel, &fichier))
        .map_err(|e| format!("montage non enregistré : {e}"))
}

pub fn enregistrer(dossier: &Path, id: &str, montage: &Montage) -> Result<(), String> {
    verifier(montage)?;
    ecrire_projet(dossier, id, montage)
}

pub fn migrer(dossier: &Path) {
    let entrees = fs::read_dir(dossier.join(galerie::ANNEXES))
        .into_iter()
        .flatten()
        .flatten();
    for e in entrees {
        let nom = e.file_name().to_string_lossy().into_owned();
        let Some(source) = nom.strip_suffix(ANCIEN).filter(|s| source_valide(s)) else {
            continue;
        };
        let lu = fs::read(e.path())
            .ok()
            .and_then(|o| lire_projet(&o, Some(source)));
        if lu.is_none_or(|m| ecrire_projet(dossier, &nouvel_id(dossier), &m).is_ok()) {
            let _ = fs::remove_file(e.path());
        }
    }
}

fn fichiers_projets(dossier: &Path) -> Vec<(String, PathBuf, i64)> {
    let entrees = fs::read_dir(dossier.join(galerie::ANNEXES).join(PROJETS))
        .into_iter()
        .flatten()
        .flatten();
    let mut liste: Vec<(String, PathBuf, i64)> = entrees
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let id = nom_de(&e.path());
            let modifie = e
                .metadata()
                .ok()?
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_secs() as i64;
            id_valide(&id).then(|| (id, e.path(), modifie))
        })
        .collect();
    liste.sort_by(|a, b| (b.2, &b.0).cmp(&(a.2, &a.0)));
    liste
}

pub fn lire(dossier: &Path, id: &str) -> Option<Montage> {
    fichier_projet(dossier, id)
        .ok()
        .and_then(|f| fs::read(f).ok())
        .and_then(|o| lire_projet(&o, None))
}

pub fn dernier_pour(dossier: &Path, source: &str) -> Option<(String, Montage)> {
    migrer(dossier);
    fichiers_projets(dossier)
        .into_iter()
        .find_map(|(id, _, _)| {
            lire(dossier, &id)
                .filter(|m| sources(m) == [source])
                .map(|m| (id, m))
        })
}

pub fn projets(dossier: &Path) -> Vec<Resume> {
    migrer(dossier);
    fichiers_projets(dossier)
        .into_iter()
        .filter_map(|(id, fichier, modifie)| {
            Some((id, lire_projet(&fs::read(fichier).ok()?, None)?, modifie))
        })
        .map(|(id, m, modifie)| {
            let clips: Vec<String> = sources(&m).into_iter().map(String::from).collect();
            let manquants = clips
                .iter()
                .filter(|c| !dossier.join(c).is_file())
                .cloned()
                .collect();
            Resume {
                id,
                date: format!("{}Z", Horodatage::depuis_epoque(modifie).iso()),
                modifie,
                duree_s: duree_finale(&m),
                nom: m.nom,
                clips,
                manquants,
            }
        })
        .collect()
}

pub fn renommer_projet(dossier: &Path, id: &str, nom: &str) -> Result<(), String> {
    let fichier = fichier_projet(dossier, id)?;
    let mut m = fs::read(&fichier)
        .ok()
        .and_then(|o| lire_projet(&o, None))
        .ok_or("projet introuvable ou illisible")?;
    m.nom = galerie::nettoyer(nom);
    if m.nom.is_empty() {
        return Err("nom de projet invalide".into());
    }
    ecrire_projet(dossier, id, &m)
}

pub fn supprimer_projet(dossier: &Path, id: &str) -> Result<(), String> {
    fs::remove_file(fichier_projet(dossier, id)?)
        .map_err(|e| format!("suppression impossible : {e}"))
}

pub fn suivre_renommage(dossier: &Path, ancien: &str, nouveau: &str) {
    migrer(dossier);
    for (id, fichier, _) in fichiers_projets(dossier) {
        let Some(mut m) = fs::read(fichier)
            .ok()
            .and_then(|o| lire_projet(&o, None))
            .filter(|m| sources(m).contains(&ancien))
        else {
            continue;
        };
        m.segments
            .iter_mut()
            .filter(|s| s.source == ancien)
            .for_each(|s| s.source = nouveau.to_string());
        if let Some(v) = m.volumes.remove(ancien) {
            m.volumes.insert(nouveau.to_string(), v);
        }
        let _ = ecrire_projet(dossier, &id, &m);
    }
}

#[cfg(test)]
mod tests {
    use super::super::plan::tests::{de, segment, texte, trois_segments, CLIP};
    use super::super::{Cadre, Sortie, Volume};
    use super::*;

    #[test]
    fn projet_serialise_et_repris_de_la_version_1() {
        let complet = Montage {
            textes: vec![texte(2.0, 5.0)],
            cadre: Cadre {
                x: 0.25,
                y: 0.0,
                l: 0.5,
                h: 1.0,
                rapport: "9:16".into(),
            },
            sortie: Sortie {
                hauteur: Some(720),
                ips: Some(30),
            },
            ..trois_segments()
        };
        let json = serde_json::to_value(&complet).unwrap();
        assert_eq!(
            json["segments"][1],
            serde_json::json!({ "source": CLIP, "debut": 2.0, "fin": 3.0, "garde": false, "vitesse": 1.0, "muet": false })
        );
        assert_eq!(
            json["volumes"][CLIP][1],
            serde_json::json!({ "volume": 1.0, "muet": true })
        );
        assert_eq!(
            json["textes"][0],
            serde_json::json!({ "contenu": "Bien joué 🎉", "police": "Bricolage Grotesque", "taille": 0.08, "couleur": "#ffffff", "habillage": "contour", "x": 0.5, "y": 0.8, "debut": 2.0, "fin": 5.0 })
        );
        assert_eq!(
            (
                &json["version"],
                &json["nom"],
                &json["cadre"],
                &json["sortie"]
            ),
            (
                &serde_json::json!(2),
                &serde_json::json!("Jeu"),
                &serde_json::json!({ "x": 0.25, "y": 0.0, "l": 0.5, "h": 1.0, "rapport": "9:16" }),
                &serde_json::json!({ "hauteur": 720, "ips": 30 })
            )
        );
        assert_eq!(
            serde_json::from_value::<Montage>(json.clone()).unwrap(),
            complet
        );
        assert_eq!(
            lire_projet(&serde_json::to_vec(&json).unwrap(), None),
            Some(complet)
        );
        let v1 = br#"{"version":1,"segments":[{"source":"Ancien nom.mp4","debut":0.5,"fin":2,"garde":true},{"source":"Ancien nom.mp4","debut":2,"fin":3,"garde":false}],"volumes":{"Ancien nom.mp4":[{"volume":0.5,"muet":false},{"volume":1,"muet":true}]}}"#;
        let repris = lire_projet(v1, Some(CLIP)).unwrap();
        assert_eq!(
            repris,
            Montage {
                segments: vec![segment(0.5, 2.0, true), segment(2.0, 3.0, false)],
                ..trois_segments()
            }
        );
        assert_eq!(
            (
                repris.version,
                repris.textes.len(),
                repris.cadre.clone(),
                repris.sortie
            ),
            (2, 0, Cadre::default(), Sortie::default())
        );
        for illisible in [
            &b"pas du json"[..],
            b"{}",
            br#"{"version":3,"segments":[]}"#,
            br#"{"version":2,"segments":[]}"#,
            br#"{"version":1,"segments":[{"source":"a.mp4","debut":2,"fin":1,"garde":true}]}"#,
        ] {
            assert_eq!(lire_projet(illisible, Some(CLIP)), None);
        }
    }

    #[test]
    fn projets_a_cote_des_clips() {
        let dossier = tempfile::tempdir().unwrap();
        let d = dossier.path();
        for nom in [CLIP, "Autre.mp4"] {
            fs::write(d.join(nom), b"x").unwrap();
        }
        let annexes = galerie::creer_annexes(d).unwrap();
        fs::write(annexes.join("Jeu.mp4.montage.json"), br#"{"version":1,"segments":[{"source":"Jeu.mp4","debut":0.5,"fin":2,"garde":true}],"volumes":{}}"#).unwrap();
        fs::write(annexes.join("Autre.mp4.montage.json"), b"illisible").unwrap();
        let liste = projets(d);
        assert_eq!(liste.len(), 1);
        let id = liste[0].id.clone();
        assert_eq!(
            (
                liste[0].nom.as_str(),
                liste[0].clips.clone(),
                liste[0].manquants.len(),
                liste[0].duree_s
            ),
            ("Jeu", vec![CLIP.to_string()], 0, 1.5)
        );
        assert!(
            !annexes.join("Jeu.mp4.montage.json").exists()
                && !annexes.join("Autre.mp4.montage.json").exists()
        );
        assert!(annexes.join("projets").join(format!("{id}.json")).is_file());
        assert_eq!(dernier_pour(d, CLIP).map(|(i, _)| i), Some(id.clone()));
        assert_eq!(dernier_pour(d, "Autre.mp4"), None);
        let deux = Montage {
            segments: vec![
                segment(0.0, 1.0, true),
                de("Autre.mp4", 0.0, 2.0, 0.5, false),
            ],
            volumes: BTreeMap::from([(
                "Autre.mp4".to_string(),
                vec![Volume {
                    volume: 2.0,
                    muet: false,
                }],
            )]),
            nom: "Deux clips".into(),
            ..trois_segments()
        };
        enregistrer(d, "deux", &deux).unwrap();
        assert!(enregistrer(d, "../dehors", &deux).is_err() && enregistrer(d, "", &deux).is_err());
        assert!(enregistrer(
            d,
            "deux",
            &Montage {
                version: 1,
                ..deux.clone()
            }
        )
        .is_err());
        assert_eq!(
            serde_json::from_slice::<Montage>(
                &fs::read(annexes.join("projets").join("deux.json")).unwrap()
            )
            .unwrap(),
            deux
        );
        assert_eq!(lire(d, "deux"), Some(deux.clone()));
        let trouve = |id: &str| projets(d).into_iter().find(|p| p.id == id).unwrap();
        assert_eq!(
            (
                projets(d).len(),
                trouve("deux").duree_s,
                trouve("deux").clips.len()
            ),
            (2, 5.0, 2)
        );
        renommer_projet(d, "deux", " Mon: montage ").unwrap();
        assert!(
            renommer_projet(d, "deux", " ").is_err() && renommer_projet(d, "absent", "x").is_err()
        );
        assert_eq!(trouve("deux").nom, "Mon montage");
        fs::rename(d.join("Autre.mp4"), d.join("Renomme.mp4")).unwrap();
        suivre_renommage(d, "Autre.mp4", "Renomme.mp4");
        let suivi = lire(d, "deux").unwrap();
        assert_eq!(
            (
                suivi.segments[1].source.as_str(),
                suivi.volumes.keys().next().map(String::as_str),
                trouve("deux").manquants.len()
            ),
            ("Renomme.mp4", Some("Renomme.mp4"), 0)
        );
        fs::remove_file(d.join("Renomme.mp4")).unwrap();
        assert_eq!(
            (trouve("deux").manquants, trouve(&id).manquants.len()),
            (vec!["Renomme.mp4".to_string()], 0)
        );
        supprimer_projet(d, "deux").unwrap();
        assert!(supprimer_projet(d, "deux").is_err() && supprimer_projet(d, "../../Jeu").is_err());
        assert_eq!(
            (projets(d).len(), fs::read(d.join(CLIP)).unwrap()),
            (1, b"x".to_vec())
        );
        assert!(id_valide(&nouvel_id(d)) && !fichier_projet(d, &nouvel_id(d)).unwrap().exists());
    }
}
