use std::collections::BTreeMap;

use serde_json::Value;

use super::geometrie::{borner_cadre, toile};
use super::texte::texte_de_valeur;
use super::{
    arrondi, borner, max_js, min_js, Cadre, Montage, Segment, Sortie, Source, Sources, Volume,
    CLIPS_MAX, DUREE_MAX_S, HISTORIQUE_MAX, RAPPORTS, TEXTES_MAX, VERSION, VITESSES, VOLUME_MAX,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lieu {
    pub i: usize,
    pub t: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cote {
    Debut,
    Fin,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Suite {
    Aller(Lieu),
    Fin,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReglageSegment {
    pub garde: Option<bool>,
    pub vitesse: Option<f64>,
    pub muet: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReglageVolume {
    pub volume: Option<f64>,
    pub muet: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReglageSortie {
    pub hauteur: Option<Option<u32>>,
    pub ips: Option<Option<u32>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Historique {
    pub passe: Vec<Montage>,
    pub present: Montage,
    pub futur: Vec<Montage>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Lecture,
    Precedente,
    Suivante,
    Couper,
    Annuler,
    Retablir,
    Basculer,
    Entree,
    Sortie,
}

pub const RACCOURCIS: [(&[&str], &str); 8] = [
    (&["Espace"], "Lecture, pause"),
    (&["←", "→"], "Image précédente, suivante"),
    (&["C"], "Poser un point de coupe"),
    (&["I"], "Entrée à la tête de lecture"),
    (&["O"], "Sortie à la tête de lecture"),
    (&["Suppr"], "Retirer ou restaurer le segment"),
    (&["Ctrl", "Z"], "Annuler"),
    (&["Ctrl", "Y"], "Rétablir"),
];

pub fn image_de(t: f64, ips: f64) -> i64 {
    (t * ips + 0.25).floor() as i64
}

pub fn temps_image(image: i64, ips: f64) -> f64 {
    (image as f64 + 0.5) / ips
}

pub fn images(s: &Source) -> u64 {
    arrondi(s.duree_s * s.cadence()).max(1.0) as u64
}

pub fn duree(s: &Source) -> f64 {
    images(s) as f64 / s.cadence()
}

fn rang(t: f64, s: &Source) -> f64 {
    arrondi(t * s.cadence()).max(0.0).min(images(s) as f64)
}

pub fn caler(t: f64, s: &Source) -> f64 {
    rang(if t.is_finite() { t } else { 0.0 }, s) / s.cadence()
}

fn entier(s: &Source) -> Segment {
    Segment {
        source: s.nom.clone(),
        debut: 0.0,
        fin: duree(s),
        garde: true,
        vitesse: 1.0,
        muet: false,
    }
}

fn volumes_neufs(s: &Source) -> Vec<Volume> {
    vec![Volume::default(); s.pistes]
}

pub fn nom_par_defaut(s: &Source) -> String {
    let n = s.nom.len();
    if n >= 4 && s.nom.is_char_boundary(n - 4) && s.nom[n - 4..].eq_ignore_ascii_case(".mp4") {
        s.nom[..n - 4].to_string()
    } else {
        s.nom.clone()
    }
}

pub fn initial(s: &Source, nom: Option<&str>) -> Montage {
    Montage {
        version: VERSION,
        nom: nom.map_or_else(|| nom_par_defaut(s), String::from),
        segments: vec![entier(s)],
        volumes: BTreeMap::from([(s.nom.clone(), volumes_neufs(s))]),
        textes: Vec::new(),
        cadre: Cadre::default(),
        sortie: Sortie::default(),
    }
}

pub fn bloc(m: &Montage, i: usize) -> (usize, usize) {
    let colle = |j: usize| {
        m.segments[j].source == m.segments[j + 1].source
            && m.segments[j].fin == m.segments[j + 1].debut
    };
    let (mut a, mut b) = (i, i + 1);
    while a > 0 && colle(a - 1) {
        a -= 1;
    }
    while b < m.segments.len() && colle(b - 1) {
        b += 1;
    }
    (a, b)
}

pub fn bornes(m: &Montage, i: usize) -> (f64, f64) {
    let (a, b) = bloc(m, i);
    (m.segments[a].debut, m.segments[b - 1].fin)
}

pub fn gardes(m: &Montage) -> Vec<&Segment> {
    m.segments.iter().filter(|s| s.garde).collect()
}

pub fn clips_de(m: &Montage) -> Vec<String> {
    let mut vus: Vec<String> = Vec::new();
    for s in &m.segments {
        if !vus.contains(&s.source) {
            vus.push(s.source.clone());
        }
    }
    vus
}

fn remplacer(m: &Montage, a: usize, z: usize, segments: Vec<Segment>) -> Montage {
    let mut tous = m.segments[..a].to_vec();
    tous.extend(segments);
    tous.extend_from_slice(&m.segments[z..]);
    Montage {
        segments: tous,
        ..m.clone()
    }
}

pub fn rogner(m: &Montage, s: &Source, cote: Cote, t: f64, i: usize) -> Montage {
    if i >= m.segments.len() {
        return m.clone();
    }
    let (a, z) = bloc(m, i);
    let morceau = &m.segments[a..z];
    let (debut, fin) = (morceau[0].debut, morceau[morceau.len() - 1].fin);
    let image = 1.0 / s.cadence();
    match cote {
        Cote::Debut => {
            let d = caler(min_js(t, fin - image), s);
            if d == debut {
                return m.clone();
            }
            let restes: Vec<&Segment> = morceau.iter().filter(|x| x.fin > d).collect();
            let premier = restes[0];
            let mut nouveaux = if d < premier.debut && !premier.garde {
                vec![
                    Segment {
                        debut: d,
                        fin: premier.debut,
                        garde: true,
                        ..premier.clone()
                    },
                    premier.clone(),
                ]
            } else {
                vec![Segment {
                    debut: d,
                    ..premier.clone()
                }]
            };
            nouveaux.extend(restes[1..].iter().map(|x| (*x).clone()));
            remplacer(m, a, z, nouveaux)
        }
        Cote::Fin => {
            let f = caler(max_js(t, debut + image), s);
            if f == fin {
                return m.clone();
            }
            let gardes: Vec<&Segment> = morceau.iter().filter(|x| x.debut < f).collect();
            let dernier = gardes[gardes.len() - 1];
            let mut nouveaux: Vec<Segment> = gardes[..gardes.len() - 1]
                .iter()
                .map(|x| (*x).clone())
                .collect();
            if f > dernier.fin && !dernier.garde {
                nouveaux.push(dernier.clone());
                nouveaux.push(Segment {
                    debut: dernier.fin,
                    fin: f,
                    garde: true,
                    ..dernier.clone()
                });
            } else {
                nouveaux.push(Segment {
                    fin: f,
                    ..dernier.clone()
                });
            }
            remplacer(m, a, z, nouveaux)
        }
    }
}

pub fn couper(m: &Montage, s: &Source, t: f64, i: Option<usize>) -> Montage {
    let c = caler(t, s);
    let dans =
        |x: Option<&Segment>| x.is_some_and(|x| x.source == s.nom && c > x.debut && c < x.fin);
    let j = i.or_else(|| m.segments.iter().position(|x| dans(Some(x))));
    let Some(j) = j.filter(|j| dans(m.segments.get(*j))) else {
        return m.clone();
    };
    let x = &m.segments[j];
    remplacer(
        m,
        j,
        j + 1,
        vec![
            Segment {
                fin: c,
                ..x.clone()
            },
            Segment {
                debut: c,
                ..x.clone()
            },
        ],
    )
}

pub fn regler_segment(m: &Montage, i: usize, reglage: ReglageSegment) -> Montage {
    let Some(x) = m.segments.get(i) else {
        return m.clone();
    };
    let apres = Segment {
        garde: reglage.garde.unwrap_or(x.garde),
        vitesse: reglage.vitesse.unwrap_or(x.vitesse),
        muet: reglage.muet.unwrap_or(x.muet),
        ..x.clone()
    };
    if !VITESSES.contains(&apres.vitesse) || apres == *x {
        return m.clone();
    }
    remplacer(m, i, i + 1, vec![apres])
}

pub fn garder(m: &Montage, i: usize, garde: bool) -> Montage {
    regler_segment(
        m,
        i,
        ReglageSegment {
            garde: Some(garde),
            ..ReglageSegment::default()
        },
    )
}

pub fn retirer(m: &Montage, i: usize) -> Montage {
    garder(m, i, false)
}

pub fn restaurer(m: &Montage, i: usize) -> Montage {
    garder(m, i, true)
}

pub fn deplacer(m: &Montage, de: isize, vers: isize) -> Montage {
    let n = m.segments.len() as isize;
    if de == vers || de < 0 || vers < 0 || de >= n || vers >= n {
        return m.clone();
    }
    let mut segments = m.segments.clone();
    let x = segments.remove(de as usize);
    segments.insert(vers as usize, x);
    Montage {
        segments,
        ..m.clone()
    }
}

pub fn deplacer_clip(m: &Montage, nom: &str, vers: isize) -> Montage {
    let mut ordre = clips_de(m);
    let Some(de) = ordre.iter().position(|c| c == nom) else {
        return m.clone();
    };
    if vers < 0 || vers as usize >= ordre.len() {
        return m.clone();
    }
    let x = ordre.remove(de);
    ordre.insert(vers as usize, x);
    let segments: Vec<Segment> = ordre
        .iter()
        .flat_map(|c| m.segments.iter().filter(move |s| &s.source == c).cloned())
        .collect();
    Montage {
        segments,
        ..m.clone()
    }
}

pub fn ajouter_clip(m: &Montage, s: &Source) -> Montage {
    if clips_de(m).contains(&s.nom) {
        return m.clone();
    }
    let mut apres = m.clone();
    apres.segments.push(entier(s));
    apres.volumes.insert(s.nom.clone(), volumes_neufs(s));
    apres
}

pub fn retirer_clip(m: &Montage, nom: &str) -> Montage {
    let segments: Vec<Segment> = m
        .segments
        .iter()
        .filter(|s| s.source != nom)
        .cloned()
        .collect();
    if segments.is_empty() || segments.len() == m.segments.len() {
        return m.clone();
    }
    let mut volumes = m.volumes.clone();
    volumes.remove(nom);
    Montage {
        segments,
        volumes,
        ..m.clone()
    }
}

pub fn duree_montee(m: &Montage) -> f64 {
    super::plan::duree_finale(m)
}

pub fn refus(avant: &Montage, apres: &Montage) -> Option<String> {
    if clips_de(apres).len() > CLIPS_MAX.max(clips_de(avant).len()) {
        return Some(format!(
            "Un montage réunit {CLIPS_MAX} clips au plus : ce clip n’a pas été ajouté."
        ));
    }
    if duree_montee(apres) > DUREE_MAX_S.max(duree_montee(avant)) {
        return Some(format!(
            "Un montage dure {} minutes au plus : la modification n’a pas été appliquée.",
            DUREE_MAX_S as u32 / 60
        ));
    }
    None
}

pub fn regler(m: &Montage, s: &Source, piste: isize, reglage: ReglageVolume) -> Montage {
    let actuels = m.volumes.get(&s.nom).cloned().unwrap_or_default();
    let avant = usize::try_from(piste)
        .ok()
        .and_then(|p| actuels.get(p).copied())
        .unwrap_or_default();
    let brut = reglage.volume.unwrap_or(avant.volume);
    let apres = Volume {
        volume: if brut.is_finite() {
            brut.clamp(0.0, VOLUME_MAX)
        } else {
            avant.volume
        },
        muet: reglage.muet.unwrap_or(avant.muet),
    };
    if piste < 0 || piste as usize >= s.pistes || apres == avant {
        return m.clone();
    }
    let liste = (0..s.pistes)
        .map(|i| {
            if i == piste as usize {
                apres
            } else {
                actuels.get(i).copied().unwrap_or_default()
            }
        })
        .collect();
    let mut nouveau = m.clone();
    nouveau.volumes.insert(s.nom.clone(), liste);
    nouveau
}

pub fn gain(m: &Montage, s: &Source, piste: usize) -> f64 {
    match m.volumes.get(&s.nom).and_then(|v| v.get(piste)) {
        Some(v) if v.muet => 0.0,
        Some(v) => v.volume,
        None => 1.0,
    }
}

pub fn axe(m: &Montage) -> (Vec<f64>, f64) {
    let mut total = 0.0;
    let debuts = m
        .segments
        .iter()
        .map(|s| {
            let d = total;
            total += s.fin - s.debut;
            d
        })
        .collect();
    (debuts, total)
}

pub fn lieu(m: &Montage, u: f64) -> Lieu {
    let (debuts, _) = axe(m);
    let i = debuts
        .iter()
        .enumerate()
        .position(|(j, d)| u < d + m.segments[j].fin - m.segments[j].debut)
        .unwrap_or(m.segments.len() - 1);
    let s = &m.segments[i];
    Lieu {
        i,
        t: (s.debut + u - debuts[i]).max(s.debut).min(s.fin),
    }
}

pub fn position(m: &Montage, l: Lieu) -> f64 {
    axe(m).0[l.i] + l.t - m.segments[l.i].debut
}

pub fn vers_montage(m: &Montage, u: f64) -> f64 {
    let (debuts, _) = axe(m);
    m.segments
        .iter()
        .enumerate()
        .filter(|(_, s)| s.garde)
        .map(|(i, s)| (s.fin - s.debut).min((u - debuts[i]).max(0.0)) / s.vitesse)
        .sum()
}

pub fn vers_axe(m: &Montage, t: f64) -> f64 {
    let (debuts, _) = axe(m);
    let mut reste = t.max(0.0);
    let mut dernier = 0.0;
    for (i, s) in m.segments.iter().enumerate().filter(|(_, s)| s.garde) {
        let d = (s.fin - s.debut) / s.vitesse;
        if reste < d {
            return debuts[i] + reste * s.vitesse;
        }
        reste -= d;
        dernier = debuts[i] + s.fin - s.debut;
    }
    dernier
}

pub fn suite(m: &Montage, l: Lieu, sources: &Sources) -> Option<Suite> {
    let s = &m.segments[l.i];
    let ips = sources.get(&s.source).map_or(0.0, Source::cadence);
    let image = image_de(l.t, ips) as f64;
    let connue = s.garde && ips > 0.0;
    if connue && image >= arrondi(s.debut * ips) && image < arrondi(s.fin * ips) {
        return None;
    }
    let avant = connue && image < arrondi(s.debut * ips);
    let j = if avant {
        Some(l.i)
    } else {
        m.segments
            .iter()
            .enumerate()
            .position(|(k, x)| k > l.i && x.garde)
    };
    Some(match j {
        None => Suite::Fin,
        Some(j) => Suite::Aller(Lieu {
            i: j,
            t: m.segments[j].debut,
        }),
    })
}

pub fn pas(m: &Montage, l: Lieu, n: i64, sources: &Sources) -> Lieu {
    let s = &m.segments[l.i];
    let ips = sources[&s.source].cadence();
    let (premiere, derniere) = (
        arrondi(s.debut * ips) as i64,
        arrondi(s.fin * ips) as i64 - 1,
    );
    let image = image_de(l.t, ips).clamp(premiere, derniere.max(premiere)) + n;
    if image >= premiere && image <= derniere {
        return Lieu {
            i: l.i,
            t: image as f64 / ips,
        };
    }
    let j = l.i as i64 + n.signum();
    let voisin = usize::try_from(j).ok().and_then(|j| m.segments.get(j));
    let Some(voisin) = voisin else {
        return Lieu {
            i: l.i,
            t: (if n < 0 { premiere } else { derniere }) as f64 / ips,
        };
    };
    let cadence = sources[&voisin.source].cadence();
    Lieu {
        i: j as usize,
        t: if n < 0 {
            (arrondi(voisin.fin * cadence) - 1.0) / cadence
        } else {
            voisin.debut
        },
    }
}

pub fn saccade(vitesse: f64, ips_source: u32, ips_sortie: u32) -> bool {
    f64::from(ips_source) * vitesse < f64::from(ips_sortie)
}

pub fn regler_sortie(m: &Montage, reglage: ReglageSortie) -> Montage {
    let apres = Sortie {
        hauteur: reglage.hauteur.unwrap_or(m.sortie.hauteur),
        ips: reglage.ips.unwrap_or(m.sortie.ips),
    };
    Montage {
        sortie: apres,
        ..m.clone()
    }
}

pub fn historique(m: Montage) -> Historique {
    Historique {
        passe: Vec::new(),
        present: m,
        futur: Vec::new(),
    }
}

pub fn appliquer(h: &Historique, m: Montage) -> Historique {
    if m == h.present {
        return h.clone();
    }
    let mut passe = h.passe.clone();
    passe.push(h.present.clone());
    let surplus = passe.len().saturating_sub(HISTORIQUE_MAX);
    passe.drain(..surplus);
    Historique {
        passe,
        present: m,
        futur: Vec::new(),
    }
}

pub fn annuler(h: &Historique) -> Historique {
    let mut passe = h.passe.clone();
    let Some(precedent) = passe.pop() else {
        return h.clone();
    };
    let mut futur = vec![h.present.clone()];
    futur.extend(h.futur.iter().cloned());
    Historique {
        passe,
        present: precedent,
        futur,
    }
}

pub fn retablir(h: &Historique) -> Historique {
    let Some((suivant, reste)) = h.futur.split_first() else {
        return h.clone();
    };
    let mut passe = h.passe.clone();
    passe.push(h.present.clone());
    Historique {
        passe,
        present: suivant.clone(),
        futur: reste.to_vec(),
    }
}

fn nombre(v: Option<&Value>) -> f64 {
    v.and_then(Value::as_f64).unwrap_or(f64::NAN)
}

pub fn relire(brut: &Value, clips: &[Source], nom: &str) -> Montage {
    let sources: Sources = clips.iter().map(|c| (c.nom.clone(), c.clone())).collect();
    let neuf = initial(&clips[0], Some(nom));
    let Some(p) = brut.as_object() else {
        return neuf;
    };
    let Some(lus) = p
        .get("segments")
        .and_then(Value::as_array)
        .filter(|_| p.get("version").and_then(Value::as_u64) == Some(u64::from(VERSION)))
    else {
        return neuf;
    };
    let segments: Vec<Segment> = lus
        .iter()
        .filter_map(|x| {
            let s = sources.get(x.get("source")?.as_str()?)?;
            let (debut, fin) = (x.get("debut")?.as_f64()?, x.get("fin")?.as_f64()?);
            let (debut, fin) = (caler(debut, s), caler(fin, s));
            let vitesse = x
                .get("vitesse")
                .and_then(Value::as_f64)
                .filter(|v| VITESSES.contains(v));
            (fin > debut).then(|| Segment {
                source: s.nom.clone(),
                debut,
                fin,
                garde: x.get("garde").and_then(Value::as_bool) != Some(false),
                vitesse: vitesse.unwrap_or(1.0),
                muet: x.get("muet").and_then(Value::as_bool) == Some(true),
            })
        })
        .collect();
    if segments.is_empty() {
        return neuf;
    }
    let vides = serde_json::Map::new();
    let volumes_lus = p
        .get("volumes")
        .and_then(Value::as_object)
        .unwrap_or(&vides);
    let mut volumes = BTreeMap::new();
    for s in &segments {
        if volumes.contains_key(&s.source) {
            continue;
        }
        let lus = volumes_lus.get(&s.source).and_then(Value::as_array);
        let liste = (0..sources[&s.source].pistes)
            .map(|i| {
                let v = lus.and_then(|l| l.get(i));
                Volume {
                    volume: borner(
                        nombre(v.and_then(|v| v.get("volume"))),
                        0.0,
                        VOLUME_MAX,
                        1.0,
                    ),
                    muet: v.and_then(|v| v.get("muet")).and_then(Value::as_bool) == Some(true),
                }
            })
            .collect();
        volumes.insert(s.source.clone(), liste);
    }
    let lu_nom = p
        .get("nom")
        .and_then(Value::as_str)
        .filter(|n| !n.trim().is_empty());
    let sortie = p.get("sortie");
    let entier_ou = |cle: &str, min: f64, max: f64, defaut: f64| {
        let v = nombre(sortie.and_then(|s| s.get(cle)));
        (v.is_finite() && v != 0.0).then(|| borner(v, min, max, defaut) as u32)
    };
    let premier = &sources[&segments[0].source];
    let cadre_lu = p.get("cadre").filter(|c| c.is_object());
    let rapport = cadre_lu
        .and_then(|c| c.get("rapport"))
        .and_then(Value::as_str)
        .filter(|r| RAPPORTS.contains(r))
        .unwrap_or(RAPPORTS[0]);
    let champ = |cle: &str, defaut: f64| {
        cadre_lu
            .and_then(|c| c.get(cle))
            .map_or(defaut, |v| v.as_f64().unwrap_or(f64::NAN))
    };
    let cadre = borner_cadre(
        &Cadre {
            x: champ("x", 0.0),
            y: champ("y", 0.0),
            l: champ("l", 1.0),
            h: champ("h", 1.0),
            rapport: rapport.into(),
        },
        toile(premier.largeur, premier.hauteur),
    );
    let textes = p
        .get("textes")
        .and_then(Value::as_array)
        .map(|l| {
            l.iter()
                .filter(|t| t.is_object())
                .take(TEXTES_MAX)
                .map(|t| super::texte::borner_texte(&texte_de_valeur(t), f64::INFINITY))
                .filter(|t| !t.contenu.trim().is_empty())
                .collect()
        })
        .unwrap_or_default();
    Montage {
        nom: lu_nom.map_or(neuf.nom.clone(), String::from),
        segments,
        volumes,
        textes,
        cadre,
        sortie: Sortie {
            hauteur: entier_ou("hauteur", 2.0, 4320.0, 1080.0),
            ips: entier_ou("ips", 1.0, 240.0, 60.0),
        },
        ..neuf
    }
}

pub fn formater_temps(t: f64) -> String {
    let c = arrondi(t * 100.0).max(0.0) as u64;
    format!("{}:{:02},{:02}", c / 6000, c / 100 % 60, c % 100)
}

pub fn lire_temps(texte: &str) -> Option<f64> {
    let propre = texte.trim().replacen(',', ".", 1);
    let parties: Vec<&str> = propre.split(':').collect();
    if parties.is_empty() || parties.len() > 3 {
        return None;
    }
    let valide = |p: &str| {
        let (entiere, decimale) = p.split_once('.').unwrap_or((p, "0"));
        !entiere.is_empty()
            && !decimale.is_empty()
            && entiere.bytes().all(|b| b.is_ascii_digit())
            && decimale.bytes().all(|b| b.is_ascii_digit())
    };
    if !parties.iter().all(|p| valide(p)) {
        return None;
    }
    parties
        .iter()
        .try_fold(0.0, |t, p| p.parse::<f64>().ok().map(|v| t * 60.0 + v))
}

pub fn pics(canaux: &[&[f32]], n: usize) -> Vec<f32> {
    let longueur = canaux.iter().map(|c| c.len()).max().unwrap_or(0);
    let mut sortie = vec![0.0f32; n];
    if longueur == 0 || n < 1 {
        return sortie;
    }
    let saut = (longueur / n / 64).max(1);
    for c in canaux {
        for i in (0..c.len()).step_by(saut) {
            let b = (n - 1).min(((i as f64 / longueur as f64) * n as f64).floor() as usize);
            let v = c[i].abs();
            if v > sortie[b] {
                sortie[b] = v;
            }
        }
    }
    sortie.into_iter().map(|v| v.min(1.0)).collect()
}

pub fn trace_onde(p: &[f32], hauteur: f64) -> String {
    let milieu = hauteur / 2.0;
    let plus_haut = p.iter().copied().fold(0.0f32, f32::max);
    let echelle = hauteur
        / 2.0
        / if plus_haut > 0.0 {
            f64::from(plus_haut)
        } else {
            1.0
        };
    p.iter()
        .enumerate()
        .map(|(i, v)| {
            let h = (f64::from(*v) * echelle).max(0.5);
            format!("M{} {:.1}V{:.1}", i as f64 + 0.5, milieu - h, milieu + h)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Touche<'a> {
    pub touche: &'a str,
    pub ctrl: bool,
    pub meta: bool,
    pub maj: bool,
    pub alt: bool,
}

pub fn action_clavier(e: Touche, champ: bool) -> Option<Action> {
    if champ || e.alt {
        return None;
    }
    let k = if e.touche.chars().count() == 1 {
        e.touche.to_lowercase()
    } else {
        e.touche.to_string()
    };
    if e.ctrl || e.meta {
        return match k.as_str() {
            "z" if e.maj => Some(Action::Retablir),
            "z" => Some(Action::Annuler),
            "y" => Some(Action::Retablir),
            _ => None,
        };
    }
    match k.as_str() {
        " " => Some(Action::Lecture),
        "ArrowLeft" => Some(Action::Precedente),
        "ArrowRight" => Some(Action::Suivante),
        "c" => Some(Action::Couper),
        "i" => Some(Action::Entree),
        "o" => Some(Action::Sortie),
        "Delete" | "Backspace" => Some(Action::Basculer),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::essais::*;
    use super::super::*;
    use super::*;
    use serde_json::json;

    #[test]
    fn part_du_clip_entier_volumes_a_100() {
        let m = initial(&s(), None);
        assert_eq!(
            serde_json::to_value(&m).unwrap(),
            json!({
                "version": VERSION, "nom": "Jeu",
                "segments": [{ "source": "Jeu.mp4", "debut": 0.0, "fin": 10.0, "garde": true, "vitesse": 1.0, "muet": false }],
                "volumes": { "Jeu.mp4": [{ "volume": 1.0, "muet": false }, { "volume": 1.0, "muet": false }] },
                "textes": [], "cadre": { "x": 0.0, "y": 0.0, "l": 1.0, "h": 1.0, "rapport": "libre" }, "sortie": { "hauteur": null, "ips": null }
            })
        );
        assert_eq!(
            (VERSION, images(&s()), duree(&s()), duree_finale(&m)),
            (2, 600, 10.0, 10.0)
        );
        assert_eq!(
            duree(&Source {
                duree_s: 10.004,
                ..s()
            }),
            10.0
        );
        assert_eq!(
            initial(
                &Source {
                    nom: "Clip.MP4".into(),
                    ..s()
                },
                None
            )
            .nom,
            "Clip"
        );
    }

    #[test]
    fn rogne_le_debut_et_la_fin_a_l_image_pres_dans_les_bornes() {
        let m = rogner(
            &rogner(&initial(&s(), None), &s(), Cote::Debut, 1.004, 0),
            &s(),
            Cote::Fin,
            8.26,
            0,
        );
        assert_eq!(bornes(&m, 0), (1.0, 496.0 / 60.0));
        assert!(proche(duree_finale(&m), 436.0 / 60.0));
        assert_eq!(
            bornes(&rogner(&m, &s(), Cote::Debut, 99.0, 0), 0),
            (495.0 / 60.0, 496.0 / 60.0)
        );
        assert_eq!(
            bornes(&rogner(&m, &s(), Cote::Fin, -3.0, 0), 0),
            (1.0, 61.0 / 60.0)
        );
        assert_eq!(
            bornes(
                &rogner(
                    &rogner(&m, &s(), Cote::Debut, -5.0, 0),
                    &s(),
                    Cote::Fin,
                    50.0,
                    0
                ),
                0
            ),
            (0.0, 10.0)
        );
        assert_eq!(rogner(&m, &s(), Cote::Debut, 1.0, 0), m);
        assert_eq!(rogner(&m, &s(), Cote::Fin, 496.0 / 60.0, 0), m);
        assert_eq!(
            bornes(&rogner(&m, &s(), Cote::Debut, f64::NAN, 0), 0).0,
            0.0
        );
        assert_eq!(rogner(&m, &s(), Cote::Debut, 2.0, 9), m);
    }

    #[test]
    fn rogner_au_dela_d_un_point_de_coupe_retire_les_segments_depasses() {
        let m = trois();
        let sv = s();
        assert_eq!(
            plages(&m),
            [
                (0.5, 2.0, true),
                (2.0, 3.0, false),
                (3.0, 5.0, true),
                (5.0, 7.0, false),
                (7.0, 9.5, true)
            ]
        );
        assert_eq!(
            plages(&rogner(&m, &sv, Cote::Debut, 4.0, 0)),
            [(4.0, 5.0, true), (5.0, 7.0, false), (7.0, 9.5, true)]
        );
        assert_eq!(
            plages(&rogner(&m, &sv, Cote::Fin, 6.0, 0)),
            [
                (0.5, 2.0, true),
                (2.0, 3.0, false),
                (3.0, 5.0, true),
                (5.0, 6.0, false)
            ]
        );
        let retire_en_tete = rogner(&m, &sv, Cote::Debut, 2.5, 0);
        assert_eq!(
            plages(&rogner(&retire_en_tete, &sv, Cote::Debut, 1.0, 0)),
            [
                (1.0, 2.5, true),
                (2.5, 3.0, false),
                (3.0, 5.0, true),
                (5.0, 7.0, false),
                (7.0, 9.5, true)
            ]
        );
        assert_eq!(
            plages(&rogner(
                &rogner(&m, &sv, Cote::Fin, 6.0, 0),
                &sv,
                Cote::Fin,
                8.0,
                0
            )),
            [
                (0.5, 2.0, true),
                (2.0, 3.0, false),
                (3.0, 5.0, true),
                (5.0, 6.0, false),
                (6.0, 8.0, true)
            ]
        );
        assert_eq!(
            plages(&rogner(&m, &sv, Cote::Fin, 10.0, 0)).last(),
            Some(&(7.0, 10.0, true))
        );
    }

    #[test]
    fn pose_des_points_de_coupe_retire_et_restaure_un_segment() {
        let sv = s();
        let m = coupes(initial(&sv, None), &[2.004, 5.0]);
        assert_eq!(
            plages(&m),
            [(0.0, 2.0, true), (2.0, 5.0, true), (5.0, 10.0, true)]
        );
        for t in [2.0, 0.0, 10.0, 42.0] {
            assert_eq!(couper(&m, &sv, t, None), m);
        }
        assert_eq!(
            [
                couper(&m, &sv, 3.0, Some(0)),
                couper(&m, &sv, 3.0, Some(9)),
                couper(&m, &h(), 3.0, None)
            ],
            [m.clone(), m.clone(), m.clone()]
        );
        assert_eq!(
            plages(&couper(&m, &sv, 3.0, Some(1))),
            [
                (0.0, 2.0, true),
                (2.0, 3.0, true),
                (3.0, 5.0, true),
                (5.0, 10.0, true)
            ]
        );
        let r = retirer(&m, 1);
        assert_eq!(
            plages(&r),
            [(0.0, 2.0, true), (2.0, 5.0, false), (5.0, 10.0, true)]
        );
        assert_eq!(retirer(&r, 1), r);
        assert_eq!(retirer(&r, 9), r);
        assert_eq!(
            plages(&couper(&r, &sv, 3.0, None)),
            [
                (0.0, 2.0, true),
                (2.0, 3.0, false),
                (3.0, 5.0, false),
                (5.0, 10.0, true)
            ]
        );
        assert_eq!(restaurer(&r, 1), m);
    }

    #[test]
    fn recolle_les_segments_gardes_dans_l_ordre_et_donne_la_duree_finale() {
        let m = trois();
        let g: Vec<(f64, f64)> = gardes(&m).iter().map(|s| (s.debut, s.fin)).collect();
        assert_eq!(g, [(0.5, 2.0), (3.0, 5.0), (7.0, 9.5)]);
        assert_eq!(duree_finale(&m), 6.0);
        assert_eq!(duree_finale(&retirer(&retirer(&retirer(&m, 0), 2), 4)), 0.0);
    }

    #[test]
    fn convertit_la_position_le_lieu_et_le_temps_du_montage() {
        let m = trois();
        assert_eq!(axe(&m), (vec![0.0, 1.5, 2.5, 4.5, 6.5], 9.0));
        let lieux: Vec<(usize, f64)> = [0.0, 1.0, 1.5, 2.0, 2.5, 6.5, 8.99, 9.0, 99.0, -1.0]
            .iter()
            .map(|u| {
                let l = lieu(&m, *u);
                (l.i, (l.t * 1e9).round() / 1e9)
            })
            .collect();
        assert_eq!(
            lieux,
            [
                (0, 0.5),
                (0, 1.5),
                (1, 2.0),
                (1, 2.5),
                (2, 3.0),
                (4, 7.0),
                (4, 9.49),
                (4, 9.5),
                (4, 9.5),
                (0, 0.5)
            ]
        );
        for u in [0.0, 0.75, 1.5, 3.0, 4.5, 8.0] {
            assert!(proche(position(&m, lieu(&m, u)), u));
        }
        let montes: Vec<f64> = [0.0, 0.5, 1.5, 2.0, 2.5, 3.5, 4.5, 6.0, 6.5, 9.0, 99.0]
            .iter()
            .map(|u| vers_montage(&m, *u))
            .collect();
        assert_eq!(
            montes,
            [0.0, 0.5, 1.5, 1.5, 1.5, 2.5, 3.5, 3.5, 3.5, 6.0, 6.0]
        );
        let axes: Vec<f64> = [0.0, 1.0, 1.5, 2.5, 3.5, 5.9, 6.0, 99.0, -1.0]
            .iter()
            .map(|t| (vers_axe(&m, *t) * 1e9).round() / 1e9)
            .collect();
        assert_eq!(axes, [0.0, 1.0, 2.5, 3.5, 6.5, 8.9, 9.0, 9.0, 0.0]);
        for u in [0.0, 1.25, 2.5, 4.0, 6.5, 8.0] {
            assert!(proche(vers_axe(&m, vers_montage(&m, u)), u));
        }
        assert_eq!(vers_axe(&retirer(&initial(&s(), None), 0), 3.0), 0.0);
    }

    #[test]
    fn saute_les_passages_retires_a_la_lecture_et_s_arrete_a_la_fin() {
        let m = trois();
        let src = src();
        let aller = |i, t| Some(Suite::Aller(Lieu { i, t }));
        assert_eq!(
            suite(
                &m,
                Lieu {
                    i: 0,
                    t: temps_image(60, 60.0)
                },
                &src
            ),
            None
        );
        assert_eq!(suite(&m, Lieu { i: 0, t: 0.0 }, &src), aller(0, 0.5));
        assert_eq!(
            suite(
                &m,
                Lieu {
                    i: 0,
                    t: temps_image(119, 60.0)
                },
                &src
            ),
            None
        );
        assert_eq!(suite(&m, Lieu { i: 0, t: 2.0 }, &src), aller(2, 3.0));
        assert_eq!(
            suite(
                &m,
                Lieu {
                    i: 1,
                    t: temps_image(150, 60.0)
                },
                &src
            ),
            aller(2, 3.0)
        );
        assert_eq!(suite(&m, Lieu { i: 2, t: 3.0 }, &src), None);
        assert_eq!(suite(&m, Lieu { i: 3, t: 6.2 }, &src), aller(4, 7.0));
        assert_eq!(suite(&m, Lieu { i: 4, t: 9.5 }, &src), Some(Suite::Fin));
        assert_eq!(suite(&m, Lieu { i: 4, t: 9.99 }, &src), Some(Suite::Fin));
        let deux = deplacer(&tous(), 2, 0);
        assert_eq!(suite(&deux, Lieu { i: 0, t: 5.0 }, &src), aller(1, 0.0));
        assert_eq!(
            suite(&retirer(&deux, 1), Lieu { i: 0, t: 5.0 }, &src),
            aller(2, 0.0)
        );
    }

    #[test]
    fn avance_et_recule_image_par_image_d_un_segment_a_l_autre() {
        assert_eq!(
            [
                image_de(0.0, 60.0),
                image_de(1.0 / 60.0, 60.0),
                image_de(temps_image(7, 60.0), 60.0),
                image_de(2.0, 30.0),
                image_de(119.9999 / 60.0, 60.0)
            ],
            [0, 1, 7, 60, 120]
        );
        let m = deplacer(&tous(), 2, 1);
        let src = src();
        assert_eq!(ordre(&m), "J0 C0 H0");
        let l = |i, t| Lieu { i, t };
        assert_eq!(pas(&m, l(0, 1.0), 1, &src), l(0, 61.0 / 60.0));
        assert_eq!(
            pas(&m, l(0, temps_image(60, 60.0)), -1, &src),
            l(0, 59.0 / 60.0)
        );
        assert_eq!(pas(&m, l(0, 0.0), -1, &src), l(0, 0.0));
        assert_eq!(pas(&m, l(0, 599.0 / 60.0), 1, &src), l(1, 0.0));
        assert_eq!(pas(&m, l(1, 0.0), -1, &src), l(0, 599.0 / 60.0));
        assert_eq!(pas(&m, l(1, 1.0), 1, &src), l(1, 31.0 / 30.0));
        assert_eq!(pas(&m, l(2, 1151.0 / 144.0), 1, &src), l(2, 1151.0 / 144.0));
        assert_eq!(
            [
                caler(1.004, &s()),
                caler(-1.0, &s()),
                caler(99.0, &s()),
                caler(1.02, &Source { ips: 30, ..s() })
            ],
            [1.0, 0.0, 10.0, 31.0 / 30.0]
        );
    }

    #[test]
    fn regle_le_volume_de_0_a_200_et_le_muet_par_piste() {
        let sv = s();
        let vol = |v: f64| ReglageVolume {
            volume: Some(v),
            muet: None,
        };
        let muet = |m: bool| ReglageVolume {
            volume: None,
            muet: Some(m),
        };
        let m = regler(
            &regler(&initial(&sv, None), &sv, 0, vol(0.5)),
            &sv,
            1,
            muet(true),
        );
        assert_eq!(
            m.volumes["Jeu.mp4"],
            [
                Volume {
                    volume: 0.5,
                    muet: false
                },
                Volume {
                    volume: 1.0,
                    muet: true
                }
            ]
        );
        assert_eq!(
            [gain(&m, &sv, 0), gain(&m, &sv, 1), gain(&m, &sv, 5)],
            [0.5, 0.0, 1.0]
        );
        assert_eq!(gain(&regler(&m, &sv, 0, vol(9.0)), &sv, 0), 2.0);
        assert_eq!(gain(&regler(&m, &sv, 0, vol(-1.0)), &sv, 0), 0.0);
        assert_eq!(regler(&m, &sv, 0, vol(0.5)), m);
        assert_eq!(regler(&m, &sv, 2, vol(0.5)), m);
        assert_eq!(regler(&m, &sv, -1, vol(0.7)), m);
        assert_eq!(regler(&m, &sv, 0, vol(f64::NAN)), m);
        assert_eq!(
            regler(&m, &sv, 1, muet(false)).volumes["Jeu.mp4"][1],
            Volume {
                volume: 1.0,
                muet: false
            }
        );
    }

    #[test]
    fn ajoute_des_clips_a_la_suite_volumes_clip_par_clip() {
        let m = tous();
        assert_eq!(ordre(&m), "J0 H0 C0");
        assert_eq!(clips_de(&m), ["Jeu.mp4", "Hevc.mp4", "Carre.mp4"]);
        assert_eq!((duree_finale(&m), axe(&m).0), (23.0, vec![0.0, 10.0, 18.0]));
        assert_eq!(
            m.volumes,
            BTreeMap::from([
                ("Jeu.mp4".to_string(), vec![Volume::default(); 2]),
                ("Hevc.mp4".to_string(), vec![Volume::default()]),
                ("Carre.mp4".to_string(), vec![]),
            ])
        );
        assert_eq!(ajouter_clip(&m, &h()), m);
        let vol = |v: f64| ReglageVolume {
            volume: Some(v),
            muet: None,
        };
        let regle = regler(
            &regler(&m, &h(), 0, vol(1.5)),
            &s(),
            1,
            ReglageVolume {
                volume: None,
                muet: Some(true),
            },
        );
        assert_eq!(
            [
                gain(&regle, &s(), 0),
                gain(&regle, &s(), 1),
                gain(&regle, &h(), 0)
            ],
            [1.0, 0.0, 1.5]
        );
        assert_eq!(regler(&regle, &c(), 0, vol(0.5)), regle);
    }

    #[test]
    fn reordonne_les_segments_et_les_clips_rogne_et_coupe_dans_le_bon_clip() {
        let m = couper(&couper(&tous(), &s(), 4.0, Some(0)), &h(), 2.0, Some(2));
        assert_eq!(ordre(&m), "J0 J4 H0 H2 C0");
        assert_eq!(
            [bloc(&m, 0), bloc(&m, 1), bloc(&m, 3), bloc(&m, 4)],
            [(0, 2), (0, 2), (2, 4), (4, 5)]
        );
        let d = deplacer(&m, 1, 3);
        assert_eq!(ordre(&d), "J0 H0 H2 J4 C0");
        assert_eq!(
            (bloc(&d, 0), bloc(&d, 3), bornes(&d, 3), bornes(&d, 1)),
            ((0, 1), (3, 4), (4.0, 10.0), (0.0, 8.0))
        );
        assert_eq!(
            [deplacer(&m, 1, 1), deplacer(&m, -1, 2), deplacer(&m, 0, 9)],
            [m.clone(), m.clone(), m.clone()]
        );
        assert_eq!(ordre(&deplacer(&d, 4, 0)), "C0 J0 H0 H2 J4");
        assert_eq!(ordre(&deplacer_clip(&d, "Carre.mp4", 0)), "C0 J0 J4 H0 H2");
        assert_eq!(ordre(&deplacer_clip(&d, "Jeu.mp4", 2)), "H0 H2 C0 J0 J4");
        assert_eq!(
            [
                deplacer_clip(&m, "Jeu.mp4", 0),
                deplacer_clip(&m, "Absent.mp4", 1),
                deplacer_clip(&m, "Jeu.mp4", 5)
            ],
            [m.clone(), m.clone(), m.clone()]
        );
        let r = rogner(
            &rogner(&d, &s(), Cote::Debut, 5.0, 3),
            &h(),
            Cote::Fin,
            3.0,
            1,
        );
        let vus: Vec<(String, f64, f64)> = r
            .segments
            .iter()
            .map(|s| (s.source[..1].to_string(), s.debut, s.fin))
            .collect();
        assert_eq!(
            vus,
            [
                ("J".into(), 0.0, 4.0),
                ("H".into(), 0.0, 2.0),
                ("H".into(), 2.0, 3.0),
                ("J".into(), 5.0, 10.0),
                ("C".into(), 0.0, 5.0)
            ]
        );
        assert_eq!(rogner(&d, &c(), Cote::Fin, 99.0, 4), d);
        assert_eq!(ordre(&couper(&d, &s(), 7.0, Some(3))), "J0 H0 H2 J4 J7 C0");
    }

    #[test]
    fn retire_un_clip_du_montage_jamais_le_dernier() {
        let m = regler(
            &couper(&tous(), &h(), 2.0, Some(1)),
            &h(),
            0,
            ReglageVolume {
                volume: Some(0.5),
                muet: None,
            },
        );
        let sans = retirer_clip(&m, "Hevc.mp4");
        assert_eq!(ordre(&sans), "J0 C0");
        assert_eq!(
            sans.volumes.keys().cloned().collect::<Vec<_>>(),
            ["Carre.mp4", "Jeu.mp4"]
        );
        assert_eq!(duree_finale(&sans), 15.0);
        assert_eq!(retirer_clip(&m, "Absent.mp4"), m);
        assert_eq!(
            retirer_clip(&initial(&s(), None), "Jeu.mp4"),
            initial(&s(), None)
        );
    }

    #[test]
    fn refuse_au_dela_de_vingt_clips_ou_de_trente_minutes() {
        let clip = |i: usize, duree_s: f64| Source {
            nom: format!("c{i}.mp4"),
            duree_s,
            ..s()
        };
        let vingt =
            (0..CLIPS_MAX - 1).fold(initial(&s(), None), |m, i| ajouter_clip(&m, &clip(i, 10.0)));
        assert_eq!(
            (clips_de(&vingt).len(), refus(&initial(&s(), None), &vingt)),
            (20, None)
        );
        assert!(refus(&vingt, &ajouter_clip(&vingt, &clip(99, 10.0)))
            .unwrap()
            .contains("20 clips"));
        assert_eq!(refus(&vingt, &retirer_clip(&vingt, "c3.mp4")), None);
        let long = ajouter_clip(&initial(&clip(0, 900.0), None), &clip(1, 900.0));
        assert_eq!(
            (duree_finale(&long), refus(&initial(&s(), None), &long)),
            (1800.0, None)
        );
        assert!(refus(&long, &ajouter_clip(&long, &clip(2, 1.0)))
            .unwrap()
            .contains("30 minutes"));
        let vitesse = |v: f64| ReglageSegment {
            vitesse: Some(v),
            ..ReglageSegment::default()
        };
        assert!(refus(&long, &regler_segment(&long, 0, vitesse(0.5)))
            .unwrap()
            .contains("30 minutes"));
        assert_eq!(refus(&long, &regler_segment(&long, 0, vitesse(2.0))), None);
        let trop = regler_segment(&long, 0, vitesse(0.25));
        assert_eq!(refus(&trop, &regler_segment(&trop, 0, vitesse(0.5))), None);
    }

    #[test]
    fn la_vitesse_change_la_duree_finale_et_le_temps_du_montage() {
        let base = coupes(
            rogner(&initial(&s(), None), &s(), Cote::Fin, 5.0, 0),
            &[1.0, 2.0, 3.0, 4.0],
        );
        let vitesse = |v: f64| ReglageSegment {
            vitesse: Some(v),
            ..ReglageSegment::default()
        };
        let m = [0.5, 2.0, 0.25, 4.0, 1.5]
            .iter()
            .enumerate()
            .fold(base, |x, (i, v)| regler_segment(&x, i, vitesse(*v)));
        assert_eq!(
            m.segments.iter().map(|s| s.vitesse).collect::<Vec<_>>(),
            [0.5, 2.0, 0.25, 4.0, 1.5]
        );
        assert!(proche(duree_finale(&m), 2.0 + 0.5 + 4.0 + 0.25 + 1.0 / 1.5));
        let montes: Vec<f64> = [0.5, 1.0, 1.5, 2.0, 3.0, 5.0]
            .iter()
            .map(|u| vers_montage(&m, *u))
            .collect();
        assert_eq!(montes, [1.0, 2.0, 2.25, 2.5, 6.5, 6.75 + 1.0 / 1.5]);
        let axes: Vec<f64> = [1.0, 2.0, 2.25, 4.5, 6.5]
            .iter()
            .map(|t| vers_axe(&m, *t))
            .collect();
        assert_eq!(axes, [0.5, 1.0, 1.5, 2.5, 3.0]);
        assert!(proche(
            duree_finale(&retirer(&m, 2)),
            2.0 + 0.5 + 0.25 + 1.0 / 1.5
        ));
        assert_eq!(
            [
                regler_segment(&m, 0, vitesse(0.5)),
                regler_segment(&m, 0, vitesse(3.0)),
                regler_segment(&m, 9, vitesse(2.0))
            ],
            [m.clone(), m.clone(), m.clone()]
        );
        let coupe = couper(&m, &s(), 0.5, None);
        assert_eq!(
            (coupe.segments[0].vitesse, coupe.segments[1].vitesse),
            (0.5, 0.5)
        );
    }

    #[test]
    fn coupe_le_son_d_un_segment_et_signale_un_ralenti_saccade() {
        let muet = |b: bool| ReglageSegment {
            muet: Some(b),
            ..ReglageSegment::default()
        };
        let m = regler_segment(&initial(&s(), None), 0, muet(true));
        assert!(m.segments[0].muet);
        assert_eq!(regler_segment(&m, 0, muet(true)), m);
        assert_eq!(regler_segment(&m, 0, muet(false)), initial(&s(), None));
        assert_eq!(
            [
                saccade(0.5, 60, 60),
                saccade(0.5, 144, 60),
                saccade(0.25, 144, 60),
                saccade(1.0, 30, 60),
                saccade(0.5, 60, 30),
                saccade(2.0, 30, 60)
            ],
            [true, false, true, true, false, false]
        );
    }

    type Operation = Box<dyn Fn(&Montage) -> Montage>;

    fn operations_simples() -> Vec<Operation> {
        let sv = s();
        let vol = |v: Option<f64>, m: Option<bool>| ReglageVolume { volume: v, muet: m };
        vec![
            Box::new(move |m| rogner(m, &s(), Cote::Debut, 0.5, 0)),
            Box::new(move |m| rogner(m, &s(), Cote::Fin, 9.5, 0)),
            Box::new(move |m| couper(m, &s(), 2.0, None)),
            Box::new(move |m| couper(m, &s(), 3.0, None)),
            Box::new(|m| retirer(m, 1)),
            Box::new(move |m| regler(m, &sv, 0, vol(Some(0.5), None))),
            Box::new(move |m| regler(m, &s(), 1, vol(None, Some(true)))),
            Box::new(move |m| couper(m, &s(), 5.0, None)),
            Box::new(|m| restaurer(m, 1)),
            Box::new(move |m| rogner(m, &s(), Cote::Debut, 2.5, 0)),
        ]
    }

    fn rejouer(operations: &[Operation]) {
        let mut etats = vec![initial(&s(), None)];
        let mut h = historique(etats[0].clone());
        for op in operations {
            h = appliquer(&h, op(&h.present));
            etats.push(h.present.clone());
        }
        assert_eq!(h.passe.len(), operations.len());
        let distincts: std::collections::HashSet<String> = etats
            .iter()
            .map(|e| serde_json::to_string(e).unwrap())
            .collect();
        assert_eq!(distincts.len(), operations.len() + 1);
        for i in (0..operations.len()).rev() {
            h = annuler(&h);
            assert_eq!(h.present, etats[i]);
        }
        assert_eq!(annuler(&h), h);
        assert_eq!(h.futur.len(), operations.len());
        for etat in etats.iter().skip(1) {
            h = retablir(&h);
            assert_eq!(&h.present, etat);
        }
        assert_eq!(retablir(&h), h);
        assert_eq!(h.passe, etats[..operations.len()]);
    }

    #[test]
    fn restaure_exactement_chaque_etat_sur_dix_operations() {
        rejouer(&operations_simples());
    }

    #[test]
    fn couvre_les_operations_de_l_editeur_avance() {
        let t = (1920, 1080);
        let operations: Vec<Operation> = vec![
            Box::new(|m| ajouter_clip(m, &h())),
            Box::new(|m| ajouter_clip(m, &c())),
            Box::new(|m| couper(m, &s(), 4.0, Some(0))),
            Box::new(|m| deplacer(m, 1, 3)),
            Box::new(|m| deplacer_clip(m, "Carre.mp4", 0)),
            Box::new(|m| {
                regler_segment(
                    m,
                    1,
                    ReglageSegment {
                        vitesse: Some(0.5),
                        ..ReglageSegment::default()
                    },
                )
            }),
            Box::new(|m| {
                regler_segment(
                    m,
                    2,
                    ReglageSegment {
                        muet: Some(true),
                        ..ReglageSegment::default()
                    },
                )
            }),
            Box::new(|m| {
                regler(
                    m,
                    &h(),
                    0,
                    ReglageVolume {
                        volume: Some(1.5),
                        muet: None,
                    },
                )
            }),
            Box::new(move |m| recadrer(m, &cadre_au("9:16", &m.cadre, t), t)),
            Box::new(move |m| {
                recadrer(
                    m,
                    &Cadre {
                        x: 0.1,
                        ..m.cadre.clone()
                    },
                    t,
                )
            }),
            Box::new(|m| ajouter_texte(m, 2.0)),
            Box::new(|m| ajouter_texte(m, 3.0)),
            Box::new(|m| {
                regler_texte(
                    m,
                    0,
                    ReglageTexte {
                        contenu: Some("GG".into()),
                        x: Some(0.2),
                        y: Some(0.3),
                        ..ReglageTexte::default()
                    },
                )
            }),
            Box::new(|m| {
                regler_texte(
                    m,
                    1,
                    ReglageTexte {
                        police: Some("JetBrains Mono".into()),
                        taille: Some(0.12),
                        couleur: Some("#ff0000".into()),
                        habillage: Some("fond".into()),
                        debut: Some(1.0),
                        fin: Some(4.0),
                        ..ReglageTexte::default()
                    },
                )
            }),
            Box::new(|m| retirer_texte(m, 0)),
            Box::new(|m| {
                regler_sortie(
                    m,
                    ReglageSortie {
                        hauteur: Some(Some(720)),
                        ips: Some(Some(30)),
                    },
                )
            }),
            Box::new(|m| rogner(m, &h(), Cote::Fin, 3.0, 2)),
            Box::new(|m| retirer_clip(m, "Jeu.mp4")),
        ];
        let mut h = historique(initial(&s(), None));
        for op in &operations {
            h = appliquer(&h, op(&h.present));
        }
        assert_eq!(
            (
                ordre(&h.present),
                h.present.textes.len(),
                h.present.cadre.rapport.as_str(),
                h.present.sortie
            ),
            (
                "C0 H0".to_string(),
                1,
                "9:16",
                Sortie {
                    hauteur: Some(720),
                    ips: Some(30)
                }
            )
        );
        rejouer(&operations);
    }

    #[test]
    fn ignore_une_operation_sans_effet_et_oublie_le_futur() {
        let sv = s();
        let mut h = appliquer(
            &historique(initial(&sv, None)),
            couper(&initial(&sv, None), &sv, 4.0, None),
        );
        assert_eq!(appliquer(&h, couper(&h.present, &sv, 4.0, None)), h);
        h = annuler(&h);
        h = appliquer(&h, couper(&h.present, &sv, 6.0, None));
        assert_eq!(
            (h.passe.len(), h.futur.len(), h.present.segments[0].fin),
            (1, 0, 6.0)
        );
        for i in 0..250 {
            let v = (i % 2) as f64 + 0.5;
            h = appliquer(
                &h,
                regler(
                    &h.present,
                    &sv,
                    0,
                    ReglageVolume {
                        volume: Some(v),
                        muet: None,
                    },
                ),
            );
        }
        assert_eq!(h.passe.len(), HISTORIQUE_MAX);
    }

    fn complet() -> Montage {
        let t = (1920, 1080);
        let m = regler(
            &tous(),
            &s(),
            0,
            ReglageVolume {
                volume: Some(0.5),
                muet: None,
            },
        );
        let m = deplacer(&couper(&m, &s(), 4.0, Some(0)), 1, 3);
        let m = regler_segment(
            &m,
            1,
            ReglageSegment {
                vitesse: Some(0.25),
                ..ReglageSegment::default()
            },
        );
        let m = recadrer(&m, &cadre_au("9:16", &m.cadre, t), t);
        let m = ajouter_texte(&m, 2.0);
        let m = regler_texte(
            &m,
            0,
            ReglageTexte {
                contenu: Some("Bien joué 🎉".into()),
                habillage: Some("fond".into()),
                ..ReglageTexte::default()
            },
        );
        regler_sortie(
            &m,
            ReglageSortie {
                hauteur: Some(Some(720)),
                ips: Some(Some(30)),
            },
        )
    }

    fn json(m: &Montage) -> Value {
        serde_json::from_str(&serde_json::to_string(m).unwrap()).unwrap()
    }

    #[test]
    fn relit_un_projet_tel_qu_il_a_ete_ecrit() {
        let m = regler(
            &trois(),
            &s(),
            0,
            ReglageVolume {
                volume: Some(0.5),
                muet: None,
            },
        );
        assert_eq!(relire(&json(&m), &[s()], "x"), m);
        assert_eq!(relire(&json(&complet()), &[s(), h(), c()], "x"), complet());
    }

    #[test]
    fn signale_un_clip_manquant_par_son_absence_et_ouvre_le_reste() {
        let m = relire(&json(&complet()), &[s(), c()], "x");
        assert_eq!(ordre(&m), "J0 C0 J4");
        assert_eq!(
            m.volumes.keys().cloned().collect::<Vec<_>>(),
            ["Carre.mp4", "Jeu.mp4"]
        );
        assert_eq!(
            (m.textes.len(), m.cadre.rapport.as_str(), m.sortie),
            (
                1,
                "9:16",
                Sortie {
                    hauteur: Some(720),
                    ips: Some(30)
                }
            )
        );
        let manquants: Vec<String> = clips_de(&complet())
            .into_iter()
            .filter(|c| !clips_de(&m).contains(c))
            .collect();
        assert_eq!(manquants, ["Hevc.mp4"]);
        assert_eq!(
            relire(&json(&complet()), &[c()], "x").segments,
            initial(&c(), None).segments
        );
        assert_eq!(
            relire(&json(&initial(&h(), None)), &[s()], "Secours"),
            initial(&s(), Some("Secours"))
        );
    }

    #[test]
    fn ramene_les_temps_sur_les_images_et_les_reglages_dans_leurs_bornes() {
        let brut = json!({
            "version": 2, "nom": " ",
            "segments": [{ "source": "Jeu.mp4", "debut": 0.504, "fin": 2, "vitesse": 3 }, { "source": "Jeu.mp4", "debut": 2, "fin": 99, "garde": false, "vitesse": 0.5, "muet": true }],
            "volumes": { "Jeu.mp4": [{ "volume": 5, "muet": true }] },
            "textes": [null, { "contenu": "a", "x": 7 }, { "contenu": "" }],
            "cadre": { "x": 0.9, "l": 0.5, "rapport": "faux" },
            "sortie": { "hauteur": 99999, "ips": 0 }
        });
        let m = relire(&brut, &[s()], "Jeu");
        assert_eq!(
            m.segments,
            [
                Segment {
                    source: "Jeu.mp4".into(),
                    debut: 0.5,
                    fin: 2.0,
                    garde: true,
                    vitesse: 1.0,
                    muet: false
                },
                Segment {
                    source: "Jeu.mp4".into(),
                    debut: 2.0,
                    fin: 10.0,
                    garde: false,
                    vitesse: 0.5,
                    muet: true
                },
            ]
        );
        assert_eq!(m.nom, "Jeu");
        assert_eq!(
            m.volumes["Jeu.mp4"],
            [
                Volume {
                    volume: 2.0,
                    muet: true
                },
                Volume::default()
            ]
        );
        assert_eq!(
            m.cadre,
            Cadre {
                x: 0.5,
                y: 0.0,
                l: 0.5,
                h: 1.0,
                rapport: "libre".into()
            }
        );
        assert_eq!(
            m.sortie,
            Sortie {
                hauteur: Some(4320),
                ips: None
            }
        );
        assert_eq!(
            m.textes,
            [Texte {
                contenu: "a".into(),
                police: "Bricolage Grotesque".into(),
                taille: 0.08,
                couleur: "#ffffff".into(),
                habillage: "contour".into(),
                x: 1.0,
                y: 0.5,
                debut: 0.0,
                fin: 3.0
            }]
        );
    }

    #[test]
    fn repart_du_clip_entier_si_le_projet_est_illisible_ou_d_une_autre_version() {
        let seg = |debut: f64, fin: f64| json!({ "source": "Jeu.mp4", "debut": debut, "fin": fin, "garde": true });
        let bruts = [
            Value::Null,
            json!("x"),
            json!(3),
            json!({}),
            json!({ "version": 1, "segments": [seg(0.0, 1.0)] }),
            json!({ "version": 3, "segments": [seg(0.0, 1.0)] }),
            json!({ "version": 2, "segments": [] }),
            json!({ "version": 2, "segments": [seg(2.0, 1.0)] }),
            json!({ "version": 2, "segments": [null] }),
            json!({ "version": 2, "segments": [{ "source": "Jeu.mp4", "debut": "0", "fin": 1 }] }),
            json!({ "version": 2, "segments": [seg(20.0, 30.0)] }),
        ];
        for brut in bruts {
            assert_eq!(relire(&brut, &[s()], "Jeu"), initial(&s(), None), "{brut}");
        }
    }

    #[test]
    fn ecrit_et_lit_un_temps_precis() {
        assert_eq!(
            [
                formater_temps(0.0),
                formater_temps(3.254),
                formater_temps(62.5),
                formater_temps(599.999),
                formater_temps(-1.0)
            ],
            ["0:00,00", "0:03,25", "1:02,50", "10:00,00", "0:00,00"]
        );
        assert_eq!(
            [
                lire_temps("1:02,5"),
                lire_temps("62.5"),
                lire_temps(" 0:03 "),
                lire_temps("1:00:01"),
                lire_temps("7")
            ],
            [Some(62.5), Some(62.5), Some(3.0), Some(3601.0), Some(7.0)]
        );
        for faux in ["", "abc", "1:", "-3", "1:2:3:4", "1e3"] {
            assert_eq!(lire_temps(faux), None, "{faux}");
        }
    }

    #[test]
    fn calcule_la_forme_d_onde_d_une_piste() {
        let gauche: Vec<f32> = (0..6400)
            .map(|i| if i < 3200 { 0.25 } else { -0.5 })
            .collect();
        let droite: Vec<f32> = (0..6400)
            .map(|i| if i >= 4800 { 1.5 } else { 0.0 })
            .collect();
        assert_eq!(pics(&[&gauche, &droite], 4), [0.25, 0.25, 0.5, 1.0]);
        assert_eq!(pics(&[], 3), [0.0, 0.0, 0.0]);
        assert_eq!(pics(&[&[]], 2), [0.0, 0.0]);
        assert_eq!(
            trace_onde(&[0.0, 1.0], 100.0),
            "M0.5 49.5V50.5M1.5 0.0V100.0"
        );
        assert_eq!(
            trace_onde(&[0.0, 0.125], 100.0),
            trace_onde(&[0.0, 1.0], 100.0)
        );
        assert_eq!(
            trace_onde(&[0.0, 0.0], 100.0),
            "M0.5 49.5V50.5M1.5 49.5V50.5"
        );
    }

    #[test]
    fn associe_les_raccourcis_aux_actions_sauf_dans_un_champ() {
        let t = |touche| Touche {
            touche,
            ..Touche::default()
        };
        let actions: Vec<Option<Action>> =
            [" ", "ArrowLeft", "ArrowRight", "c", "C", "Delete", "i", "O"]
                .into_iter()
                .map(|k| action_clavier(t(k), false))
                .collect();
        use Action::*;
        assert_eq!(
            actions,
            [
                Some(Lecture),
                Some(Precedente),
                Some(Suivante),
                Some(Couper),
                Some(Couper),
                Some(Basculer),
                Some(Entree),
                Some(Sortie)
            ]
        );
        let ctrl = |touche, maj| Touche {
            touche,
            ctrl: true,
            maj,
            ..Touche::default()
        };
        assert_eq!(
            [
                action_clavier(ctrl("z", false), false),
                action_clavier(ctrl("y", false), false),
                action_clavier(ctrl("Z", true), false)
            ],
            [Some(Annuler), Some(Retablir), Some(Retablir)]
        );
        assert_eq!(
            [
                action_clavier(ctrl("c", false), false),
                action_clavier(t("z"), false),
                action_clavier(
                    Touche {
                        alt: true,
                        ..t("c")
                    },
                    false
                ),
                action_clavier(t("Enter"), false)
            ],
            [None, None, None, None]
        );
        for k in [" ", "ArrowLeft", "c", "Delete"] {
            assert_eq!(action_clavier(t(k), true), None);
        }
        assert_eq!(action_clavier(ctrl("z", false), true), None);
        assert_eq!(RACCOURCIS.len(), 8);
    }
}
