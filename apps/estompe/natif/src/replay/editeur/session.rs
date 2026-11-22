use std::collections::{BTreeMap, HashMap};

use estompe_client::replay::galerie::nettoyer;
use estompe_client::replay::montage::{
    ajouter_clip, annuler, appliquer, axe, bloc, cadences_possibles, cadre_au, couper, deplacer,
    duree_finale, format_de, formater_temps, garder, hauteurs_possibles, historique, initial, lieu,
    nom_piste, pas, position, recadrer, refus, regler, regler_segment, regler_sortie, relire,
    retablir, retirer_clip, rogner, saccade, toile, trace_onde, vers_axe, vers_montage, Cote,
    Historique, Infos, Lieu, Montage, ReglageSegment, ReglageSortie, ReglageVolume, Source,
    Sources, RAPPORTS, VITESSES,
};

pub const BARRES_ONDE: usize = 200;
pub const HAUTEUR_ONDE: f64 = 14.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Operation {
    Couper,
    Basculer,
    Entree,
    Sortie,
    Poignee {
        debut: bool,
        fraction: f64,
    },
    Vitesse(usize),
    MuetSegment(bool),
    Volume(usize, f64),
    MuetPiste(usize, bool),
    Annuler,
    Retablir,
    Renommer(String),
    Rapport(usize),
    RetirerClip,
    Deplacer(isize),
    AjouterClip(Source, Infos),
    Format {
        hauteur: Option<u32>,
        ips: Option<u32>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct VueSegment {
    pub debut: f32,
    pub largeur: f32,
    pub garde: bool,
    pub choisi: bool,
    pub libelle: String,
    pub clip: String,
    pub muet: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VuePiste {
    pub nom: String,
    pub volume: f32,
    pub muet: bool,
    pub onde: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VueEditeur {
    pub nom: String,
    pub duree: String,
    pub temps: String,
    pub segments: Vec<VueSegment>,
    pub tete: f32,
    pub entree: f32,
    pub sortie: f32,
    pub pistes: Vec<VuePiste>,
    pub clip_choisi: String,
    pub garde: bool,
    pub muet: bool,
    pub vitesse: usize,
    pub saccade: bool,
    pub peut_annuler: bool,
    pub peut_retablir: bool,
    pub rapport: usize,
    pub plusieurs: bool,
    pub peut_reculer: bool,
    pub peut_avancer: bool,
    pub textes: usize,
    pub manquants: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VueExport {
    pub hauteurs: Vec<(String, Option<u32>, bool)>,
    pub cadences: Vec<(String, u32, bool)>,
    pub format: String,
}

pub fn libelle_vitesse(v: f64) -> String {
    format!("{}×", v.to_string().replace('.', ","))
}

pub struct SessionMontage {
    pub id: String,
    pub sources: Sources,
    pub infos: BTreeMap<String, Infos>,
    pub manquants: Vec<String>,
    pub historique: Historique,
    pub choisi: usize,
    pub tete: f64,
    pub ondes: HashMap<(String, usize), Vec<f32>>,
    pub modifie: bool,
}

impl SessionMontage {
    pub fn nouvelle(
        id: String,
        clips: Vec<(String, Infos)>,
        manquants: Vec<String>,
        projet: Option<&Montage>,
    ) -> Option<Self> {
        let liste: Vec<Source> = clips.iter().map(|(n, i)| Source::de(n, i)).collect();
        let premier = liste.first()?;
        let montage = match projet {
            Some(p) => relire(&serde_json::to_value(p).unwrap_or_default(), &liste, &p.nom),
            None => initial(premier, None),
        };
        Some(Self {
            id,
            sources: liste.iter().map(|s| (s.nom.clone(), s.clone())).collect(),
            infos: clips.into_iter().collect(),
            manquants,
            historique: historique(montage),
            choisi: 0,
            tete: 0.0,
            ondes: HashMap::new(),
            modifie: false,
        })
    }

    pub fn montage(&self) -> &Montage {
        &self.historique.present
    }

    pub fn lieu(&self) -> Lieu {
        lieu(self.montage(), self.tete)
    }

    fn source_de(&self, i: usize) -> Option<&Source> {
        self.sources.get(&self.montage().segments.get(i)?.source)
    }

    fn total(&self) -> f64 {
        axe(self.montage()).1
    }

    pub fn temps_monte(&self) -> f64 {
        vers_montage(self.montage(), self.tete)
    }

    pub fn placer(&mut self, fraction: f64) {
        self.tete = (fraction.clamp(0.0, 1.0) * self.total()).min(self.total());
        self.choisi = self.lieu().i;
    }

    pub fn placer_au_temps(&mut self, monte: f64) {
        self.tete = vers_axe(self.montage(), monte);
        self.choisi = self.lieu().i;
    }

    pub fn choisir(&mut self, i: usize) {
        let Some(s) = self.montage().segments.get(i) else {
            return;
        };
        let debut = s.debut;
        self.choisi = i;
        self.tete = position(self.montage(), Lieu { i, t: debut });
    }

    pub fn image_voisine(&mut self, n: i64) {
        let l = pas(self.montage(), self.lieu(), n, &self.sources);
        self.tete = position(self.montage(), l);
        self.choisi = l.i;
    }

    fn modifier(&mut self, apres: Montage) -> Result<bool, String> {
        if let Some(message) = refus(self.montage(), &apres) {
            return Err(message);
        }
        let avant = self.montage().clone();
        let ancien = self.temps_monte();
        self.historique = appliquer(&self.historique, apres);
        let change = *self.montage() != avant;
        if change {
            self.recaler(ancien);
        }
        Ok(change)
    }

    fn recaler(&mut self, monte: f64) {
        self.modifie = true;
        let n = self.montage().segments.len();
        self.choisi = self.choisi.min(n.saturating_sub(1));
        self.tete = vers_axe(self.montage(), monte).min(self.total());
    }

    pub fn executer(&mut self, op: Operation) -> Result<bool, String> {
        let m = self.montage().clone();
        let l = self.lieu();
        let choisi = self.choisi.min(m.segments.len() - 1);
        let source = self.source_de(choisi).cloned();
        let ici = self.source_de(l.i).cloned();
        let apres = match op {
            Operation::Annuler | Operation::Retablir => {
                let ancien = self.temps_monte();
                let avant = self.historique.clone();
                self.historique = if op == Operation::Annuler {
                    annuler(&avant)
                } else {
                    retablir(&avant)
                };
                let change = self.historique != avant;
                if change {
                    self.recaler(ancien);
                }
                return Ok(change);
            }
            Operation::Couper => match ici {
                Some(s) => couper(&m, &s, l.t, Some(l.i)),
                None => m.clone(),
            },
            Operation::Basculer => garder(&m, choisi, !m.segments[choisi].garde),
            Operation::Entree | Operation::Sortie => match ici {
                Some(s) => {
                    self.choisi = l.i;
                    let cote = if op == Operation::Entree {
                        Cote::Debut
                    } else {
                        Cote::Fin
                    };
                    rogner(&m, &s, cote, l.t, l.i)
                }
                None => m.clone(),
            },
            Operation::Poignee { debut, fraction } => match source {
                Some(s) => {
                    let (a, _) = bloc(&m, choisi);
                    let u = fraction.max(0.0) * self.total();
                    let t = m.segments[a].debut + u - axe(&m).0[a];
                    let cote = if debut { Cote::Debut } else { Cote::Fin };
                    rogner(&m, &s, cote, t, choisi)
                }
                None => m.clone(),
            },
            Operation::Vitesse(i) => match VITESSES.get(i) {
                Some(v) => regler_segment(
                    &m,
                    choisi,
                    ReglageSegment {
                        vitesse: Some(*v),
                        ..ReglageSegment::default()
                    },
                ),
                None => m.clone(),
            },
            Operation::MuetSegment(muet) => regler_segment(
                &m,
                choisi,
                ReglageSegment {
                    muet: Some(muet),
                    ..ReglageSegment::default()
                },
            ),
            Operation::Volume(piste, volume) => match source {
                Some(s) => regler(
                    &m,
                    &s,
                    piste as isize,
                    ReglageVolume {
                        volume: Some(volume),
                        muet: None,
                    },
                ),
                None => m.clone(),
            },
            Operation::MuetPiste(piste, muet) => match source {
                Some(s) => regler(
                    &m,
                    &s,
                    piste as isize,
                    ReglageVolume {
                        volume: None,
                        muet: Some(muet),
                    },
                ),
                None => m.clone(),
            },
            Operation::Renommer(nom) => {
                let propre = nettoyer(&nom);
                if propre.is_empty() {
                    m.clone()
                } else {
                    Montage {
                        nom: propre,
                        ..m.clone()
                    }
                }
            }
            Operation::Rapport(i) => {
                let premier = &self.sources[&m.segments[0].source];
                let t = toile(premier.largeur, premier.hauteur);
                match RAPPORTS.get(i) {
                    Some(r) => recadrer(&m, &cadre_au(r, &m.cadre, t), t),
                    None => m.clone(),
                }
            }
            Operation::RetirerClip => match source {
                Some(s) => retirer_clip(&m, &s.nom),
                None => m.clone(),
            },
            Operation::Deplacer(sens) => {
                let vers = choisi as isize + sens;
                let apres = deplacer(&m, choisi as isize, vers);
                if apres != m {
                    self.choisi = vers as usize;
                }
                apres
            }
            Operation::AjouterClip(s, infos) => {
                let apres = ajouter_clip(&m, &s);
                if apres != m {
                    if let Some(message) = refus(&m, &apres) {
                        return Err(message);
                    }
                    self.infos.insert(s.nom.clone(), infos);
                    self.sources.insert(s.nom.clone(), s);
                }
                apres
            }
            Operation::Format { hauteur, ips } => regler_sortie(
                &m,
                ReglageSortie {
                    hauteur: Some(hauteur),
                    ips: Some(ips),
                },
            ),
        };
        self.modifier(apres)
    }

    pub fn vue(&self) -> VueEditeur {
        let m = self.montage();
        let (debuts, total) = axe(m);
        let total_f = total.max(f64::MIN_POSITIVE);
        let choisi = self.choisi.min(m.segments.len() - 1);
        let segment = &m.segments[choisi];
        let source = self.sources.get(&segment.source);
        let (a, b) = bloc(m, choisi);
        let format = format_de(m, &self.sources);
        let pistes = source
            .map(|s| {
                (0..s.pistes)
                    .map(|p| {
                        let v = m
                            .volumes
                            .get(&s.nom)
                            .and_then(|v| v.get(p))
                            .copied()
                            .unwrap_or_default();
                        VuePiste {
                            nom: nom_piste(p),
                            volume: (v.volume * 100.0) as f32,
                            muet: v.muet,
                            onde: self
                                .ondes
                                .get(&(s.nom.clone(), p))
                                .map(|o| trace_onde(o, HAUTEUR_ONDE))
                                .unwrap_or_default(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        let plusieurs =
            self.sources.len() > 1 || m.segments.iter().any(|s| s.source != segment.source);
        VueEditeur {
            nom: m.nom.clone(),
            duree: formater_temps(duree_finale(m)),
            temps: formater_temps(self.temps_monte()),
            segments: m
                .segments
                .iter()
                .enumerate()
                .map(|(i, s)| VueSegment {
                    debut: (debuts[i] / total_f) as f32,
                    largeur: ((s.fin - s.debut) / total_f) as f32,
                    garde: s.garde,
                    choisi: i == choisi,
                    libelle: if s.vitesse == 1.0 {
                        String::new()
                    } else {
                        libelle_vitesse(s.vitesse)
                    },
                    clip: s.source.trim_end_matches(".mp4").to_string(),
                    muet: s.muet,
                })
                .collect(),
            tete: (self.tete / total_f) as f32,
            entree: (debuts[a] / total_f) as f32,
            sortie: ((debuts[b - 1] + m.segments[b - 1].fin - m.segments[b - 1].debut) / total_f)
                as f32,
            pistes,
            clip_choisi: segment.source.trim_end_matches(".mp4").to_string(),
            garde: segment.garde,
            muet: segment.muet,
            vitesse: VITESSES
                .iter()
                .position(|v| *v == segment.vitesse)
                .unwrap_or(2),
            saccade: segment.garde
                && source.is_some_and(|s| saccade(segment.vitesse, s.ips, format.ips)),
            peut_annuler: !self.historique.passe.is_empty(),
            peut_retablir: !self.historique.futur.is_empty(),
            rapport: RAPPORTS
                .iter()
                .position(|r| *r == m.cadre.rapport)
                .unwrap_or(0),
            plusieurs,
            peut_reculer: choisi > 0,
            peut_avancer: choisi + 1 < m.segments.len(),
            textes: m.textes.len(),
            manquants: if self.manquants.is_empty() {
                String::new()
            } else {
                format!(
                    "Clip introuvable, ses passages sont écartés : {}.",
                    self.manquants.join(", ")
                )
            },
        }
    }

    pub fn vue_export(&self) -> VueExport {
        let m = self.montage();
        let format = format_de(m, &self.sources);
        let mut hauteurs = vec![(
            format!("Cadre ({} × {})", format.cadre.0, format.cadre.1),
            None,
            m.sortie.hauteur.is_none(),
        )];
        hauteurs.extend(
            hauteurs_possibles(&format)
                .into_iter()
                .map(|h| (format!("{h}p"), Some(h), m.sortie.hauteur == Some(h))),
        );
        if !hauteurs.iter().any(|h| h.2) {
            hauteurs[0].2 = true;
        }
        let cadences = cadences_possibles(m, &self.sources)
            .into_iter()
            .map(|c| (format!("{c} i/s"), c, c == format.ips))
            .collect();
        VueExport {
            hauteurs,
            cadences,
            format: format!(
                "{} × {}, {} i/s, {}",
                format.largeur,
                format.hauteur,
                format.ips,
                formater_temps(duree_finale(m))
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use estompe_client::replay::montage::Volume;

    fn infos(duree_s: f64, ips: u32, pistes_audio: u32) -> Infos {
        Infos {
            duree_s,
            codec: None,
            largeur: 1920,
            hauteur: 1080,
            ips,
            pistes_audio,
        }
    }

    fn session() -> SessionMontage {
        SessionMontage::nouvelle(
            "p".into(),
            vec![("Jeu.mp4".into(), infos(10.0, 60, 2))],
            Vec::new(),
            None,
        )
        .unwrap()
    }

    #[test]
    fn coupe_retire_rogne_et_regle_depuis_la_tete_de_lecture() {
        let mut s = session();
        assert_eq!(
            (
                s.montage().nom.as_str(),
                s.vue().duree.as_str(),
                s.vue().temps.as_str()
            ),
            ("Jeu", "0:10,00", "0:00,00")
        );
        s.placer(0.2);
        assert!(s.executer(Operation::Couper).unwrap());
        s.placer(0.5);
        assert!(s.executer(Operation::Couper).unwrap());
        assert_eq!(s.montage().segments.len(), 3);
        s.placer(0.3);
        assert_eq!(s.choisi, 1);
        s.executer(Operation::Basculer).unwrap();
        let v = s.vue();
        assert_eq!(v.duree, "0:07,00");
        assert!(!v.garde && v.segments[1].choisi && !v.segments[1].garde);
        assert_eq!((v.segments[1].debut, v.segments[1].largeur), (0.2, 0.3));
        s.placer(0.05);
        s.executer(Operation::Entree).unwrap();
        assert_eq!(s.montage().segments[0].debut, 0.5);
        s.placer(8.5 / 9.5);
        s.executer(Operation::Sortie).unwrap();
        assert_eq!(s.montage().segments[2].fin, 9.0);
        s.choisir(2);
        s.executer(Operation::Poignee {
            debut: false,
            fraction: 9.0 / 8.5,
        })
        .unwrap();
        assert_eq!(s.montage().segments[2].fin, 9.5);
        s.executer(Operation::Vitesse(1)).unwrap();
        s.executer(Operation::MuetSegment(true)).unwrap();
        let v = s.vue();
        assert_eq!(
            (v.vitesse, v.muet, v.segments[2].libelle.as_str(), v.saccade),
            (1, true, "0,5×", true)
        );
        s.executer(Operation::Volume(0, 1.5)).unwrap();
        s.executer(Operation::MuetPiste(1, true)).unwrap();
        assert_eq!(
            s.montage().volumes["Jeu.mp4"],
            [
                Volume {
                    volume: 1.5,
                    muet: false
                },
                Volume {
                    volume: 1.0,
                    muet: true
                }
            ]
        );
        let v = s.vue();
        assert_eq!(
            (
                v.pistes[0].nom.as_str(),
                v.pistes[0].volume,
                v.pistes[1].nom.as_str(),
                v.pistes[1].muet
            ),
            ("Jeu", 150.0, "Micro", true)
        );
        assert!(!s.executer(Operation::Volume(0, 1.5)).unwrap());
        assert!(s.modifie && v.peut_annuler && !v.peut_retablir);
    }

    #[test]
    fn annule_retablit_et_garde_la_tete_dans_le_montage() {
        let mut s = session();
        s.placer(0.5);
        s.executer(Operation::Couper).unwrap();
        s.executer(Operation::Basculer).unwrap();
        s.placer(1.0);
        assert_eq!(s.lieu().i, 1);
        assert!(s.executer(Operation::Annuler).unwrap());
        assert!(s.montage().segments.iter().all(|x| x.garde));
        assert!(s.executer(Operation::Retablir).unwrap());
        assert!(!s.executer(Operation::Retablir).unwrap());
        assert!(s.tete <= axe(s.montage()).1);
        s.executer(Operation::Annuler).unwrap();
        s.executer(Operation::Annuler).unwrap();
        assert!(!s.executer(Operation::Annuler).unwrap());
        assert_eq!(s.montage().segments.len(), 1);
    }

    #[test]
    fn image_par_image_et_temps_du_montage() {
        let mut s = session();
        s.image_voisine(1);
        assert_eq!((s.lieu().t, s.temps_monte()), (1.0 / 60.0, 1.0 / 60.0));
        s.image_voisine(-5);
        assert_eq!(s.tete, 0.0);
        s.placer_au_temps(4.0);
        assert_eq!(s.vue().temps, "0:04,00");
    }

    #[test]
    fn plusieurs_clips_ordre_cadre_et_format_de_sortie() {
        let mut s = session();
        let carre = Source {
            nom: "Carre.mp4".into(),
            duree_s: 5.0,
            ips: 30,
            pistes: 0,
            largeur: 1000,
            hauteur: 1000,
        };
        s.executer(Operation::AjouterClip(carre.clone(), infos(5.0, 30, 0)))
            .unwrap();
        assert!(s.vue().plusieurs && s.vue().peut_avancer);
        s.choisir(1);
        assert!(s.vue().pistes.is_empty());
        s.executer(Operation::Deplacer(-1)).unwrap();
        assert_eq!(
            (s.montage().segments[0].source.as_str(), s.choisi),
            ("Carre.mp4", 0)
        );
        s.executer(Operation::RetirerClip).unwrap();
        assert_eq!(s.montage().segments.len(), 1);
        s.executer(Operation::Rapport(3)).unwrap();
        assert_eq!(
            (s.montage().cadre.rapport.as_str(), s.vue().rapport),
            ("9:16", 3)
        );
        let e = s.vue_export();
        assert_eq!(
            e.hauteurs.iter().map(|h| h.0.as_str()).collect::<Vec<_>>(),
            ["Cadre (608 × 1080)", "720p", "480p"]
        );
        assert_eq!(e.cadences.iter().map(|c| c.1).collect::<Vec<_>>(), [60, 30]);
        assert_eq!(e.format, "594 × 1056, 60 i/s, 0:10,00");
        s.executer(Operation::Format {
            hauteur: Some(720),
            ips: Some(30),
        })
        .unwrap();
        let e = s.vue_export();
        assert!(e.hauteurs[1].2 && e.cadences[1].2);
        assert_eq!(e.format, "396 × 704, 30 i/s, 0:10,00");
        s.executer(Operation::Renommer(" Mon / montage ".into()))
            .unwrap();
        assert_eq!(s.vue().nom, "Mon montage");
        assert!(!s.executer(Operation::Renommer("  ".into())).unwrap());
    }

    #[test]
    fn refuse_un_ralenti_au_dela_de_trente_minutes() {
        let mut s = SessionMontage::nouvelle(
            "p".into(),
            vec![("Long.mp4".into(), infos(1500.0, 30, 1))],
            vec!["Absent.mp4".into()],
            None,
        )
        .unwrap();
        assert!(s
            .executer(Operation::Vitesse(1))
            .unwrap_err()
            .contains("30 minutes"));
        assert_eq!(
            s.vue().manquants,
            "Clip introuvable, ses passages sont écartés : Absent.mp4."
        );
        assert_eq!(libelle_vitesse(0.25), "0,25×");
    }

    #[test]
    fn rouvre_un_projet_enregistre() {
        let mut s = session();
        s.placer(0.5);
        s.executer(Operation::Couper).unwrap();
        let projet = s.montage().clone();
        let r = SessionMontage::nouvelle(
            "p".into(),
            vec![("Jeu.mp4".into(), infos(10.0, 60, 2))],
            Vec::new(),
            Some(&projet),
        )
        .unwrap();
        assert_eq!(r.montage(), &projet);
        assert!(SessionMontage::nouvelle("p".into(), Vec::new(), Vec::new(), None).is_none());
    }
}
