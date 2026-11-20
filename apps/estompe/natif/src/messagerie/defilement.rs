use std::collections::HashMap;

use estompe_client::virtualisation::{fenetre_de_hauteurs, Fenetre};

pub const MARGE: f64 = 600.0;
pub const SEUIL_BAS: f64 = 80.0;
pub const SEUIL_CHARGEMENT: f64 = 1500.0;
pub const RETRAIT_DU_HAUT: f64 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alignement {
    Haut,
    Centre,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ancre {
    pub cle: String,
    pub decalage: f64,
}

#[derive(Debug, Clone)]
pub struct Defilement {
    cles: Vec<String>,
    estimations: Vec<f64>,
    index: HashMap<String, usize>,
    mesures: HashMap<String, f64>,
    largeur: f64,
    vue: f64,
    position: f64,
    colle: bool,
    ancre: Option<Ancre>,
}

impl Default for Defilement {
    fn default() -> Self {
        Self {
            cles: Vec::new(),
            estimations: Vec::new(),
            index: HashMap::new(),
            mesures: HashMap::new(),
            largeur: 0.0,
            vue: 0.0,
            position: 0.0,
            colle: true,
            ancre: None,
        }
    }
}

impl Defilement {
    pub fn hauteur(&self, i: usize) -> f64 {
        self.mesures
            .get(&self.cles[i])
            .copied()
            .unwrap_or(self.estimations[i])
    }

    pub fn nombre(&self) -> usize {
        self.cles.len()
    }

    pub fn cle(&self, i: usize) -> &str {
        &self.cles[i]
    }

    pub fn indice(&self, cle: &str) -> Option<usize> {
        self.index.get(cle).copied()
    }

    pub fn position(&self) -> f64 {
        self.position
    }

    pub fn vue(&self) -> f64 {
        self.vue
    }

    pub fn largeur(&self) -> f64 {
        self.largeur
    }

    pub fn colle(&self) -> bool {
        self.colle
    }

    pub fn ancre(&self) -> Option<&Ancre> {
        self.ancre.as_ref()
    }

    pub fn mesuree(&self, cle: &str) -> bool {
        self.mesures.contains_key(cle)
    }

    pub fn mesures(&self) -> usize {
        self.mesures.len()
    }

    pub fn hauteur_totale(&self) -> f64 {
        (0..self.cles.len()).map(|i| self.hauteur(i)).sum()
    }

    pub fn y_de(&self, cle: &str) -> Option<f64> {
        let i = self.indice(cle)?;
        Some((0..i).map(|j| self.hauteur(j)).sum())
    }

    pub fn position_max(&self) -> f64 {
        (self.hauteur_totale() - self.vue).max(0.0)
    }

    pub fn en_bas(&self) -> bool {
        self.position_max() - self.position < SEUIL_BAS
    }

    pub fn pres_du_haut(&self) -> bool {
        self.position < SEUIL_CHARGEMENT
    }

    pub fn pres_du_bas(&self) -> bool {
        self.position_max() - self.position < SEUIL_CHARGEMENT
    }

    pub fn fenetre(&self) -> Fenetre {
        fenetre_de_hauteurs(
            self.cles.len(),
            |i| self.hauteur(i),
            self.position,
            self.vue.max(1.0),
            MARGE,
        )
    }

    fn borner(&mut self) {
        self.position = self.position.clamp(0.0, self.position_max());
    }

    fn noter_ancre(&mut self) {
        let mut y = 0.0;
        self.ancre = None;
        for i in 0..self.cles.len() {
            let h = self.hauteur(i);
            if y + h > self.position || i + 1 == self.cles.len() {
                self.ancre = Some(Ancre {
                    cle: self.cles[i].clone(),
                    decalage: self.position - y,
                });
                return;
            }
            y += h;
        }
    }

    fn retrouver(&mut self) {
        if self.colle {
            self.position = self.position_max();
            self.noter_ancre();
            return;
        }
        let place = self
            .ancre
            .as_ref()
            .and_then(|a| self.y_de(&a.cle).map(|y| y + a.decalage));
        match place {
            Some(position) => {
                self.position = position;
                self.borner();
            }
            None => {
                self.borner();
                self.noter_ancre();
            }
        }
    }

    pub fn remplacer_lignes(&mut self, lignes: Vec<(String, f64)>) {
        let (cles, estimations): (Vec<String>, Vec<f64>) = lignes.into_iter().unzip();
        self.index = cles
            .iter()
            .enumerate()
            .map(|(i, c)| (c.clone(), i))
            .collect();
        self.mesures.retain(|c, _| self.index.contains_key(c));
        if let Some(ancre) = &self.ancre {
            if !self.index.contains_key(&ancre.cle) {
                let ancien = ancre.cle.clone();
                let suivante = self
                    .cles
                    .iter()
                    .skip_while(|c| **c != ancien)
                    .find(|c| self.index.contains_key(*c))
                    .cloned();
                self.ancre = suivante.map(|cle| Ancre { cle, decalage: 0.0 });
            }
        }
        self.cles = cles;
        self.estimations = estimations;
        self.retrouver();
    }

    pub fn estimer(&mut self, cle: &str, hauteur: f64) {
        if let Some(i) = self.indice(cle) {
            self.estimations[i] = hauteur;
            if !self.mesures.contains_key(cle) {
                self.retrouver();
            }
        }
    }

    pub fn mesurer(&mut self, cle: &str, hauteur: f64) -> bool {
        if hauteur <= 0.0 || !self.index.contains_key(cle) {
            return false;
        }
        if self
            .mesures
            .get(cle)
            .is_some_and(|h| (h - hauteur).abs() < 0.5)
        {
            return false;
        }
        self.mesures.insert(cle.to_string(), hauteur);
        self.retrouver();
        true
    }

    pub fn defiler(&mut self, position: f64) {
        self.position = position.max(0.0);
        self.borner();
        self.colle = self.en_bas();
        self.noter_ancre();
    }

    pub fn redimensionner(&mut self, largeur: f64, vue: f64) -> bool {
        let largeur_change = (largeur - self.largeur).abs() >= 0.5;
        let vue_change = (vue - self.vue).abs() >= 0.5;
        if !largeur_change && !vue_change {
            return false;
        }
        if largeur_change {
            self.mesures.clear();
        }
        self.largeur = largeur;
        self.vue = vue;
        self.retrouver();
        true
    }

    pub fn aller_en_bas(&mut self) {
        self.colle = true;
        self.retrouver();
    }

    pub fn aller_a(&mut self, cle: &str, alignement: Alignement) -> bool {
        let (Some(y), Some(i)) = (self.y_de(cle), self.indice(cle)) else {
            return false;
        };
        let h = self.hauteur(i);
        let voulue = match alignement {
            Alignement::Haut => y - RETRAIT_DU_HAUT,
            Alignement::Centre => y - (self.vue - h).max(0.0) / 2.0,
        };
        self.colle = false;
        self.position = voulue;
        self.borner();
        self.colle = self.position >= self.position_max() && self.en_bas();
        self.ancre = Some(Ancre {
            cle: cle.to_string(),
            decalage: self.position - y,
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lignes(debut: usize, fin: usize, h: f64) -> Vec<(String, f64)> {
        (debut..fin).map(|i| (format!("l{i:05}"), h)).collect()
    }

    fn avec(nombre: usize) -> Defilement {
        let mut d = Defilement::default();
        d.redimensionner(800.0, 600.0);
        d.remplacer_lignes(lignes(0, nombre, 50.0));
        d
    }

    #[test]
    fn colle_en_bas_par_defaut_et_suit_les_ajouts_en_fin() {
        let mut d = avec(100);
        assert!(d.colle());
        assert_eq!(d.position(), 100.0 * 50.0 - 600.0);
        d.remplacer_lignes(lignes(0, 110, 50.0));
        assert_eq!(d.position(), 110.0 * 50.0 - 600.0);
        assert!(d.en_bas());
    }

    #[test]
    fn un_ajout_en_fin_ne_deplace_pas_une_lecture_de_l_historique() {
        let mut d = avec(100);
        d.defiler(1000.0);
        assert!(!d.colle());
        d.remplacer_lignes(lignes(0, 120, 50.0));
        assert_eq!(d.position(), 1000.0);
        assert!(!d.en_bas());
    }

    #[test]
    fn un_ajout_en_tete_garde_la_ligne_sous_les_yeux() {
        let mut d = avec(100);
        d.defiler(1010.0);
        let ancre = d.ancre().cloned().unwrap();
        assert_eq!((ancre.cle.as_str(), ancre.decalage), ("l00020", 10.0));
        let mut toutes = lignes(0, 100, 50.0);
        let mut tete: Vec<(String, f64)> = (0..30).map(|i| (format!("a{i:05}"), 70.0)).collect();
        tete.append(&mut toutes);
        d.remplacer_lignes(tete);
        assert_eq!(d.position(), 30.0 * 70.0 + 1010.0);
        assert_eq!(d.y_de("l00020").unwrap() + 10.0, d.position());
    }

    #[test]
    fn une_mesure_au_dessus_de_l_ancre_compense_le_defilement() {
        let mut d = avec(100);
        d.defiler(1010.0);
        assert!(d.mesurer("l00005", 150.0));
        assert_eq!(d.position(), 1110.0);
        assert!(!d.mesurer("l00005", 150.2));
        assert!(d.mesurer("l00030", 10.0));
        assert_eq!(d.position(), 1110.0);
        assert!(!d.mesurer("inconnue", 10.0));
        assert!(!d.mesurer("l00031", 0.0));
    }

    #[test]
    fn la_fenetre_ne_couvre_que_l_ecran_et_sa_marge() {
        let mut d = avec(50_000);
        d.defiler(1_000_000.0);
        let f = d.fenetre();
        assert_eq!(f.debut, ((1_000_000.0 - MARGE) / 50.0) as usize);
        assert!(f.fin - f.debut <= ((600.0 + 2.0 * MARGE) / 50.0) as usize + 2);
        assert_eq!(f.haut, f.debut as f64 * 50.0);
    }

    #[test]
    fn aller_a_centre_la_ligne_et_la_garde_apres_mesure() {
        let mut d = avec(100);
        assert!(d.aller_a("l00040", Alignement::Centre));
        assert_eq!(d.position(), 40.0 * 50.0 - 275.0);
        assert!(!d.colle());
        d.mesurer("l00039", 90.0);
        assert_eq!(d.y_de("l00040").unwrap() - d.position(), 275.0);
        assert!(d.aller_a("l00010", Alignement::Haut));
        assert_eq!(d.position(), 10.0 * 50.0 - RETRAIT_DU_HAUT);
        assert!(!d.aller_a("absente", Alignement::Haut));
        d.aller_en_bas();
        assert!(d.colle() && d.en_bas());
    }

    #[test]
    fn une_ligne_d_ancre_retiree_passe_a_la_suivante() {
        let mut d = avec(100);
        d.defiler(1010.0);
        let sans: Vec<(String, f64)> = lignes(0, 100, 50.0)
            .into_iter()
            .filter(|(c, _)| c != "l00020")
            .collect();
        d.remplacer_lignes(sans);
        assert_eq!(d.ancre().unwrap().cle, "l00021");
        assert_eq!(d.position(), 1000.0);
    }

    #[test]
    fn un_changement_de_largeur_oublie_les_mesures_et_garde_l_ancre() {
        let mut d = avec(100);
        d.defiler(1010.0);
        d.mesurer("l00001", 80.0);
        assert_eq!(d.mesures(), 1);
        assert!(d.redimensionner(500.0, 600.0));
        assert_eq!(d.mesures(), 0);
        assert_eq!(d.position(), 1010.0);
        assert!(!d.redimensionner(500.0, 600.0));
        let mut courte = Defilement::default();
        courte.redimensionner(800.0, 600.0);
        courte.remplacer_lignes(lignes(0, 3, 50.0));
        assert_eq!(courte.position(), 0.0);
        assert!(courte.en_bas() && courte.pres_du_haut() && courte.pres_du_bas());
    }
}
