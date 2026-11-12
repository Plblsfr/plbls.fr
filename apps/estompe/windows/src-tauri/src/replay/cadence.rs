use super::tampon::UNITE_TEMPS;

pub const RATTRAPAGE_MAX: u64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Perte {
    Retard,
    Encodeur,
    Conversion,
}

impl Perte {
    pub fn erreur(self) -> (&'static str, &'static str) {
        match self {
            Self::Retard => ("capture-en-retard", "L'instant replay perd des images : la capture prend du retard."),
            Self::Encodeur => ("encodeur-sature", "L'instant replay perd des images : l'encodeur vidéo ne suit pas. Baisse sa résolution ou sa cadence."),
            Self::Conversion => ("conversion", "L'instant replay perd des images : la conversion sur la carte graphique échoue."),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Entree {
    #[default]
    Reserve,
    Textures,
    Memoire,
}

impl Entree {
    pub fn suivante(self) -> Option<Self> {
        match self {
            Self::Reserve => Some(Self::Textures),
            Self::Textures => Some(Self::Memoire),
            Self::Memoire => None,
        }
    }

    pub fn nom(self) -> &'static str {
        match self {
            Self::Reserve => "textures de la réserve Media Foundation",
            Self::Textures => "textures individuelles",
            Self::Memoire => "mémoire vive",
        }
    }
}

pub fn encodeur_muet(ips: u32, premiere: Option<u64>, courante: u64, encodees: u64) -> bool {
    encodees == 0 && premiere.is_some_and(|p| courante.saturating_sub(p) >= 2 * ips.max(1) as u64)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pertes {
    pub retard: u64,
    pub encodeur: u64,
    pub conversion: u64,
}

impl Pertes {
    pub fn total(&self) -> u64 {
        self.retard + self.encodeur + self.conversion
    }

    pub fn depuis(&self, avant: &Pertes) -> Pertes {
        Pertes { retard: self.retard - avant.retard, encodeur: self.encodeur - avant.encodeur, conversion: self.conversion - avant.conversion }
    }

    pub fn alerte(&self, images: u64) -> Option<Perte> {
        if self.total() * 5 <= images {
            return None;
        }
        Some(if self.conversion >= self.retard && self.conversion >= self.encodeur {
            Perte::Conversion
        } else if self.encodeur >= self.retard {
            Perte::Encodeur
        } else {
            Perte::Retard
        })
    }
}

#[derive(Debug)]
pub struct Cadence {
    ips: u32,
    emises: u64,
    pertes: Pertes,
}

impl Cadence {
    pub fn nouvelle(ips: u32) -> Self {
        Self { ips: ips.max(1), emises: 0, pertes: Pertes::default() }
    }

    pub fn horodatage(&self, n: u64) -> i64 {
        (n as u128 * UNITE_TEMPS as u128).div_ceil(self.ips as u128) as i64
    }

    pub fn duree(&self, n: u64) -> i64 {
        self.horodatage(n + 1) - self.horodatage(n)
    }

    pub fn echeance(&self) -> i64 {
        self.horodatage(self.emises)
    }

    pub fn dues(&mut self, ecoule: i64) -> Vec<u64> {
        if ecoule < self.echeance() {
            return Vec::new();
        }
        let attendues = (ecoule as i128 * self.ips as i128 / UNITE_TEMPS as i128) as u64 + 1;
        let retard = attendues - self.emises;
        let sautees = retard.saturating_sub(RATTRAPAGE_MAX);
        self.pertes.retard += sautees;
        let premiere = self.emises + sautees;
        self.emises = attendues;
        (premiere..attendues).collect()
    }

    pub fn perdre(&mut self, cause: Perte) {
        match cause {
            Perte::Retard => self.pertes.retard += 1,
            Perte::Encodeur => self.pertes.encodeur += 1,
            Perte::Conversion => self.pertes.conversion += 1,
        }
    }

    pub fn bilan(&self) -> (u64, Pertes) {
        (self.emises, self.pertes)
    }
}

#[derive(Debug)]
pub struct Alignement {
    frequence: u32,
    ecrits: u64,
}

impl Alignement {
    pub fn nouveau(frequence: u32) -> Self {
        Self { frequence, ecrits: 0 }
    }

    pub fn silence(&mut self, horloge: i64, recus: u64) -> u64 {
        let fin = (horloge.max(0) as i128 * self.frequence as i128 / UNITE_TEMPS as i128) as u64;
        let debut = fin.saturating_sub(recus);
        let seuil = self.frequence as u64 * 3 / 100;
        let silence = if debut > self.ecrits + seuil { debut - self.ecrits } else { 0 };
        self.ecrits += silence + recus;
        silence
    }

    #[cfg(test)]
    pub fn ecrits(&self) -> u64 {
        self.ecrits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cadence_constante_sans_derive() {
        let mut c = Cadence::nouvelle(144);
        assert_eq!(c.dues(0), [0]);
        assert!(c.dues(c.echeance() - 1).is_empty());
        let mut total = 1;
        while c.echeance() <= 300 * UNITE_TEMPS {
            total += c.dues(c.echeance()).len();
        }
        assert_eq!(total, 144 * 300 + 1);
        assert_eq!(c.horodatage(144 * 300), 300 * UNITE_TEMPS);
        assert_eq!(c.bilan(), (144 * 300 + 1, Pertes::default()));
        assert!((69_444..=69_445).contains(&c.duree(7)));
    }

    #[test]
    fn repete_puis_compte_les_images_perdues() {
        let mut c = Cadence::nouvelle(60);
        assert_eq!(c.dues(0), [0]);
        assert_eq!(c.dues(c.horodatage(3)), [1, 2, 3]);
        assert_eq!(c.dues(c.horodatage(63)), [61, 62, 63]);
        assert_eq!(c.bilan(), (64, Pertes { retard: 57, encodeur: 0, conversion: 0 }));
        c.perdre(Perte::Encodeur);
        c.perdre(Perte::Conversion);
        c.perdre(Perte::Conversion);
        assert_eq!(c.bilan().1, Pertes { retard: 57, encodeur: 1, conversion: 2 });
        assert_eq!(c.bilan().1.total(), 60);
    }

    #[test]
    fn alerte_selon_la_cause_dominante() {
        let avant = Pertes { retard: 5, encodeur: 5, conversion: 5 };
        let fenetre = Pertes { retard: 6, encodeur: 105, conversion: 25 }.depuis(&avant);
        assert_eq!(fenetre, Pertes { retard: 1, encodeur: 100, conversion: 20 });
        assert_eq!(fenetre.alerte(600), Some(Perte::Encodeur));
        assert_eq!(fenetre.alerte(605), None);
        assert_eq!(Pertes { retard: 200, encodeur: 0, conversion: 0 }.alerte(600), Some(Perte::Retard));
        assert_eq!(Pertes { retard: 0, encodeur: 0, conversion: 200 }.alerte(600), Some(Perte::Conversion));
        assert_eq!(Pertes::default().alerte(600), None);
        let codes = [Perte::Retard, Perte::Encodeur, Perte::Conversion].map(|p| p.erreur().0);
        assert_eq!(codes, ["capture-en-retard", "encodeur-sature", "conversion"]);
    }

    #[test]
    fn encodeur_muet_apres_deux_secondes_sans_image_encodee() {
        assert!(!encodeur_muet(120, None, 10_000, 0));
        assert!(!encodeur_muet(120, Some(5), 244, 0));
        assert!(encodeur_muet(120, Some(5), 245, 0));
        assert!(!encodeur_muet(120, Some(5), 10_000, 1));
        assert!(encodeur_muet(0, Some(0), 2, 0));
    }

    #[test]
    fn entrees_de_l_encodeur_essayees_dans_l_ordre() {
        assert_eq!(Entree::default(), Entree::Reserve);
        assert_eq!(Entree::Reserve.suivante(), Some(Entree::Textures));
        assert_eq!(Entree::Textures.suivante(), Some(Entree::Memoire));
        assert_eq!(Entree::Memoire.suivante(), None);
    }

    #[test]
    fn comble_les_silences_du_son() {
        let mut a = Alignement::nouveau(48_000);
        assert_eq!(a.silence(UNITE_TEMPS / 100, 480), 0);
        assert_eq!(a.silence(2 * UNITE_TEMPS / 100, 480), 0);
        assert_eq!(a.silence(2 * UNITE_TEMPS / 100 + 50_000, 480), 0);
        assert_eq!(a.ecrits(), 1440);
        assert_eq!(a.silence(UNITE_TEMPS, 480), 48_000 - 480 - 1440);
        assert_eq!(a.ecrits(), 48_000);
        assert_eq!(a.silence(UNITE_TEMPS, 480), 0);
    }
}
