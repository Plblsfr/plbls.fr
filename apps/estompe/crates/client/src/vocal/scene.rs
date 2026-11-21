use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::video::DemandeVideo;

pub const ATTRIBUT_SPECTATEUR: &str = "watching";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourcePiste {
    Micro,
    Camera,
    Partage,
    SonPartage,
}

pub fn doit_recevoir(source: SourcePiste, regarde: Option<bool>, auto: bool, propre: bool) -> bool {
    if propre && source == SourcePiste::SonPartage {
        return false;
    }
    match source {
        SourcePiste::Partage | SourcePiste::SonPartage => regarde.unwrap_or(auto),
        SourcePiste::Micro | SourcePiste::Camera => true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuileVideo {
    pub piste: String,
    pub utilisateur: String,
    pub nom: String,
    pub partage: bool,
    pub recue: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementScene {
    pub cle: String,
    pub utilisateur: String,
    pub nom: String,
    pub tuile: Option<TuileVideo>,
}

impl ElementScene {
    pub fn partage(&self) -> bool {
        self.tuile.as_ref().is_some_and(|t| t.partage)
    }

    pub fn recue(&self) -> bool {
        self.tuile.as_ref().is_some_and(|t| t.recue)
    }
}

pub fn cle_avatar(utilisateur: &str) -> String {
    format!("u:{utilisateur}")
}

pub fn construire_elements(
    personnes: &[(String, String)],
    tuiles: &[TuileVideo],
) -> Vec<ElementScene> {
    let mut sortie = Vec::new();
    let mut utilisees = vec![false; tuiles.len()];
    for (utilisateur, nom) in personnes {
        let siennes: Vec<usize> = (0..tuiles.len())
            .filter(|i| &tuiles[*i].utilisateur == utilisateur)
            .collect();
        let camera = siennes.iter().copied().find(|i| !tuiles[*i].partage);
        sortie.push(ElementScene {
            cle: camera.map_or_else(|| cle_avatar(utilisateur), |i| tuiles[i].piste.clone()),
            utilisateur: utilisateur.clone(),
            nom: nom.clone(),
            tuile: camera.map(|i| tuiles[i].clone()),
        });
        for i in siennes.iter().copied().filter(|i| tuiles[*i].partage) {
            sortie.push(ElementScene {
                cle: tuiles[i].piste.clone(),
                utilisateur: utilisateur.clone(),
                nom: nom.clone(),
                tuile: Some(tuiles[i].clone()),
            });
        }
        for i in siennes {
            utilisees[i] = true;
        }
    }
    for (tuile, utilisee) in tuiles.iter().zip(utilisees) {
        if !utilisee {
            sortie.push(ElementScene {
                cle: tuile.piste.clone(),
                utilisateur: tuile.utilisateur.clone(),
                nom: tuile.nom.clone(),
                tuile: Some(tuile.clone()),
            });
        }
    }
    sortie
}

pub fn dimensions_grille(nombre: usize, etroit: bool) -> (usize, usize) {
    if nombre <= 1 {
        return (1, 1);
    }
    if etroit && nombre == 2 {
        return (1, 2);
    }
    let colonnes = match (etroit, nombre) {
        (true, _) => 2,
        (false, 0..=4) => 2,
        (false, 5..=9) => 3,
        (false, _) => 4,
    };
    (colonnes, nombre.div_ceil(colonnes))
}

pub type Spectateurs = BTreeMap<String, Vec<(String, String)>>;

pub fn rassembler_spectateurs(personnes: &[(String, String, Option<String>)]) -> Spectateurs {
    let mut sortie = Spectateurs::new();
    for (id, nom, regarde) in personnes {
        for piste in regarde
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
        {
            sortie
                .entry(piste.to_string())
                .or_default()
                .push((id.clone(), nom.clone()));
        }
    }
    sortie
}

pub fn vedette<'a>(
    elements: &'a [ElementScene],
    epinglee: Option<&str>,
    detachee: Option<&str>,
) -> Option<&'a ElementScene> {
    elements
        .iter()
        .find(|e| {
            e.tuile
                .as_ref()
                .is_some_and(|t| Some(t.piste.as_str()) == epinglee)
        })
        .or_else(|| {
            elements
                .iter()
                .find(|e| e.partage() && e.recue() && Some(e.cle.as_str()) != detachee)
        })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Epinglage {
    pub epinglee: Option<String>,
    pub detachee: Option<String>,
}

impl Epinglage {
    pub fn vedette<'a>(&self, elements: &'a [ElementScene]) -> Option<&'a ElementScene> {
        vedette(elements, self.epinglee.as_deref(), self.detachee.as_deref())
    }

    pub fn basculer(&mut self, elements: &[ElementScene], cle: &str) {
        let Some(element) = elements.iter().find(|e| e.cle == cle) else {
            return;
        };
        let piste = element.tuile.as_ref().map(|t| t.piste.clone());
        if self.vedette(elements).is_some_and(|v| v.cle == cle) {
            if piste.is_some() && self.epinglee == piste {
                self.epinglee = None;
            } else {
                self.detachee = Some(cle.to_string());
            }
        } else {
            self.epinglee = piste;
            self.detachee = None;
        }
    }

    pub fn nettoyer(&mut self, elements: &[ElementScene]) {
        if let Some(d) = &self.detachee {
            if !elements.iter().any(|e| &e.cle == d) {
                self.detachee = None;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualiteRecue {
    #[default]
    Auto,
    Haute,
    Moyenne,
    Basse,
}

impl QualiteRecue {
    pub const TOUTES: [Self; 4] = [Self::Auto, Self::Haute, Self::Moyenne, Self::Basse];

    pub fn libelle(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Haute => "1080p",
            Self::Moyenne => "720p",
            Self::Basse => "360p",
        }
    }

    pub fn demande(self, zone: (u32, u32)) -> DemandeVideo {
        let (largeur, hauteur) = match self {
            Self::Auto => zone,
            Self::Haute => (1920, 1080),
            Self::Moyenne => (1280, 720),
            Self::Basse => (640, 360),
        };
        DemandeVideo {
            largeur: largeur.max(1),
            hauteur: hauteur.max(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tuile(piste: &str, utilisateur: &str, partage: bool) -> TuileVideo {
        TuileVideo {
            piste: piste.into(),
            utilisateur: utilisateur.into(),
            nom: utilisateur.into(),
            partage,
            recue: true,
        }
    }

    fn personnes() -> Vec<(String, String)> {
        vec![("a".into(), "alice".into()), ("b".into(), "bob".into())]
    }

    #[test]
    fn construire_donne_une_tuile_par_participant_avatar_si_pas_de_camera() {
        let elements = construire_elements(&personnes(), &[tuile("t1", "b", false)]);
        let resume: Vec<_> = elements
            .iter()
            .map(|e| (e.utilisateur.as_str(), e.tuile.is_some()))
            .collect();
        assert_eq!(resume, [("a", false), ("b", true)]);
    }

    #[test]
    fn construire_ajoute_une_tuile_pour_chaque_partage_d_ecran() {
        let elements = construire_elements(
            &personnes(),
            &[tuile("t1", "a", false), tuile("t2", "a", true)],
        );
        let cles: Vec<_> = elements.iter().map(|e| e.cle.as_str()).collect();
        assert_eq!(cles, ["t1", "t2", "u:b"]);
    }

    #[test]
    fn construire_affiche_un_flux_dont_le_participant_n_est_pas_encore_liste() {
        let elements = construire_elements(&[], &[tuile("t9", "z", false)]);
        assert_eq!(elements[0].utilisateur, "z");
        assert_eq!(elements.len(), 1);
    }

    #[test]
    fn la_grille_suit_le_nombre_de_tuiles() {
        for (n, colonnes, rangees) in [
            (1, 1, 1),
            (2, 2, 1),
            (3, 2, 2),
            (4, 2, 2),
            (5, 3, 2),
            (9, 3, 3),
            (10, 4, 3),
        ] {
            assert_eq!(
                dimensions_grille(n, false),
                (colonnes, rangees),
                "{n} tuiles"
            );
        }
    }

    #[test]
    fn la_grille_empile_deux_tuiles_sur_un_ecran_etroit() {
        assert_eq!(dimensions_grille(2, true), (1, 2));
        assert_eq!(dimensions_grille(3, true), (2, 2));
    }

    #[test]
    fn les_spectateurs_sont_regroupes_par_flux() {
        let s = |v: &str| Some(v.to_string());
        let spectateurs = rassembler_spectateurs(&[
            ("a".into(), "alice".into(), s("t1,t2")),
            ("b".into(), "bob".into(), s("t1")),
            ("c".into(), "carl".into(), s("")),
            ("d".into(), "dan".into(), None),
        ]);
        let paire = |id: &str, nom: &str| (id.to_string(), nom.to_string());
        assert_eq!(
            spectateurs,
            Spectateurs::from([
                ("t1".into(), vec![paire("a", "alice"), paire("b", "bob")]),
                ("t2".into(), vec![paire("a", "alice")]),
            ])
        );
    }

    #[test]
    fn le_micro_et_la_camera_sont_toujours_recus() {
        assert!(doit_recevoir(SourcePiste::Micro, None, false, false));
        assert!(doit_recevoir(
            SourcePiste::Camera,
            Some(false),
            false,
            false
        ));
    }

    #[test]
    fn un_ecran_non_regarde_n_est_pas_recu_par_defaut() {
        assert!(!doit_recevoir(SourcePiste::Partage, None, false, false));
        assert!(!doit_recevoir(SourcePiste::SonPartage, None, false, false));
    }

    #[test]
    fn l_ecran_est_recu_apres_regarder_et_plus_apres_arreter() {
        assert!(doit_recevoir(
            SourcePiste::Partage,
            Some(true),
            false,
            false
        ));
        assert!(!doit_recevoir(
            SourcePiste::Partage,
            Some(false),
            true,
            false
        ));
    }

    #[test]
    fn le_reglage_automatique_est_respecte() {
        assert!(doit_recevoir(SourcePiste::SonPartage, None, true, false));
    }

    #[test]
    fn le_son_de_son_propre_partage_n_est_jamais_rejoue_mais_l_apercu_reste() {
        assert!(!doit_recevoir(
            SourcePiste::SonPartage,
            Some(true),
            true,
            true
        ));
        assert!(doit_recevoir(SourcePiste::Partage, Some(true), true, true));
    }

    #[test]
    fn construire_garde_une_tuile_d_ecran_non_regardee_sans_piste() {
        let t = TuileVideo {
            recue: false,
            ..tuile("s", "a", true)
        };
        let elements = construire_elements(&personnes(), &[t]);
        let resume: Vec<_> = elements
            .iter()
            .map(|e| (e.cle.as_str(), e.recue()))
            .collect();
        assert_eq!(resume, [("u:a", false), ("s", false), ("u:b", false)]);
    }

    #[test]
    fn un_partage_regarde_passe_en_vedette_et_detacher_le_remet_dans_la_grille() {
        let mut elements = construire_elements(
            &personnes(),
            &[tuile("t1", "a", false), tuile("t2", "b", true)],
        );
        let mut epinglage = Epinglage::default();
        assert_eq!(
            epinglage.vedette(&elements).map(|e| e.cle.as_str()),
            Some("t2")
        );
        epinglage.basculer(&elements, "t2");
        assert_eq!(epinglage.vedette(&elements), None);
        epinglage.basculer(&elements, "t1");
        assert_eq!(
            epinglage.vedette(&elements).map(|e| e.cle.as_str()),
            Some("t1")
        );
        epinglage.basculer(&elements, "t1");
        assert_eq!(
            epinglage.vedette(&elements).map(|e| e.cle.as_str()),
            Some("t2"),
            "désépingler rend la vedette au partage"
        );
        epinglage.basculer(&elements, "t2");
        assert_eq!(epinglage.detachee.as_deref(), Some("t2"));
        elements.retain(|e| e.cle != "t2");
        epinglage.nettoyer(&elements);
        assert_eq!(epinglage.detachee, None);
    }

    #[test]
    fn un_partage_non_regarde_ne_passe_pas_en_vedette() {
        let t = TuileVideo {
            recue: false,
            ..tuile("t2", "b", true)
        };
        let elements = construire_elements(&personnes(), &[t]);
        assert_eq!(Epinglage::default().vedette(&elements), None);
    }

    #[test]
    fn la_qualite_recue_choisie_remplace_la_taille_de_la_tuile() {
        assert_eq!(
            QualiteRecue::Auto.demande((400, 225)),
            DemandeVideo {
                largeur: 400,
                hauteur: 225
            }
        );
        assert_eq!(QualiteRecue::Haute.demande((400, 225)).hauteur, 1080);
        assert_eq!(QualiteRecue::Basse.demande((1920, 1080)).hauteur, 360);
        assert_eq!(QualiteRecue::Auto.demande((0, 0)).largeur, 1);
    }
}
