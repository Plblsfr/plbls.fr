use super::tampon::UNITE_TEMPS;

pub const RATTRAPAGE_MAX: u64 = 3;

#[derive(Debug)]
pub struct Cadence {
    ips: u32,
    emises: u64,
    perdues: u64,
}

impl Cadence {
    pub fn nouvelle(ips: u32) -> Self {
        Self { ips: ips.max(1), emises: 0, perdues: 0 }
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
        self.perdues += sautees;
        let premiere = self.emises + sautees;
        self.emises = attendues;
        (premiere..attendues).collect()
    }

    pub fn perdre(&mut self, n: u64) {
        self.perdues += n;
    }

    pub fn bilan(&self) -> (u64, u64) {
        (self.emises, self.perdues)
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
        assert_eq!(c.bilan(), (144 * 300 + 1, 0));
        assert!((69_444..=69_445).contains(&c.duree(7)));
    }

    #[test]
    fn repete_puis_compte_les_images_perdues() {
        let mut c = Cadence::nouvelle(60);
        assert_eq!(c.dues(0), [0]);
        assert_eq!(c.dues(c.horodatage(3)), [1, 2, 3]);
        assert_eq!(c.dues(c.horodatage(63)), [61, 62, 63]);
        assert_eq!(c.bilan(), (64, 57));
        c.perdre(1);
        assert_eq!(c.bilan().1, 58);
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
