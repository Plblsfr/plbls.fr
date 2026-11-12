use std::collections::VecDeque;
use std::sync::Arc;

use super::types::Codec;

pub const UNITE_TEMPS: i64 = 10_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piste {
    Video,
    Jeu,
    Micro,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Unite {
    pub pts: i64,
    pub duree: i64,
    pub cle: bool,
    pub donnees: Arc<[u8]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    pub codec: Codec,
    pub largeur: u32,
    pub hauteur: u32,
    pub ips: u32,
    pub micro: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Instantane {
    pub format: Format,
    pub video: Vec<Unite>,
    pub jeu: Vec<Unite>,
    pub micro: Vec<Unite>,
}

impl Instantane {
    pub fn duree(&self) -> i64 {
        match (self.video.first(), self.video.last()) {
            (Some(a), Some(b)) => b.pts + b.duree - a.pts,
            _ => 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct Tampon {
    format: Option<Format>,
    video: VecDeque<Unite>,
    jeu: VecDeque<Unite>,
    micro: VecDeque<Unite>,
    octets: u64,
    duree_cible: i64,
    plafond_octets: u64,
    debordements: u64,
}

impl Tampon {
    pub fn preparer(&mut self, format: Format, duree_cible: i64, plafond_octets: u64) -> bool {
        let vide = self.format != Some(format);
        if vide {
            self.vider();
        }
        (self.format, self.duree_cible, self.plafond_octets) = (Some(format), duree_cible, plafond_octets);
        vide
    }

    pub fn vider(&mut self) {
        self.video.clear();
        self.jeu.clear();
        self.micro.clear();
        self.octets = 0;
    }

    pub fn octets(&self) -> u64 {
        self.octets
    }

    pub fn debordements(&self) -> u64 {
        self.debordements
    }

    pub fn debut(&self) -> Option<i64> {
        self.video.front().map(|u| u.pts)
    }

    pub fn fin(&self) -> i64 {
        self.video.back().map_or(0, |u| u.pts + u.duree)
    }

    pub fn duree(&self) -> i64 {
        self.debut().map_or(0, |d| self.fin() - d)
    }

    pub fn reprise(&self) -> i64 {
        let fin = |file: &VecDeque<Unite>| file.back().map_or(0, |u| u.pts + u.duree);
        self.fin().max(fin(&self.jeu)).max(fin(&self.micro))
    }

    pub fn pousser(&mut self, piste: Piste, unite: Unite) {
        let Some(debut) = self.debut().or((piste == Piste::Video && unite.cle).then_some(unite.pts)) else { return };
        if piste != Piste::Video && unite.pts < debut {
            return;
        }
        self.octets += unite.donnees.len() as u64;
        match piste {
            Piste::Video => self.video.push_back(unite),
            Piste::Jeu => self.jeu.push_back(unite),
            Piste::Micro => self.micro.push_back(unite),
        }
        self.evincer();
    }

    fn evincer(&mut self) {
        loop {
            let suivante = self.video.iter().skip(1).position(|u| u.cle).map(|i| i + 1);
            let trop = self.octets > self.plafond_octets;
            match suivante {
                Some(i) if trop || self.fin() - self.video[i].pts >= self.duree_cible => {
                    let retire: u64 = self.video.drain(..i).map(|u| u.donnees.len() as u64).sum();
                    self.octets -= retire;
                    let debut = self.video[0].pts;
                    self.octets -= couper(&mut self.jeu, debut) + couper(&mut self.micro, debut);
                }
                None if trop => {
                    self.debordements += 1;
                    self.vider();
                    return;
                }
                _ => return,
            }
        }
    }

    pub fn instantane(&self, duree: i64) -> Option<Instantane> {
        let format = self.format?;
        let voulu = self.fin() - duree;
        let depart = self.video.iter().rposition(|u| u.cle && u.pts <= voulu).unwrap_or(0);
        let video: Vec<Unite> = self.video.iter().skip(depart).cloned().collect();
        let debut = video.first()?.pts;
        let depuis = |file: &VecDeque<Unite>| file.iter().filter(|u| u.pts >= debut).cloned().collect();
        Some(Instantane { format, jeu: depuis(&self.jeu), micro: depuis(&self.micro), video })
    }
}

fn couper(file: &mut VecDeque<Unite>, debut: i64) -> u64 {
    let mut retire = 0;
    while file.front().is_some_and(|u| u.pts < debut) {
        retire += file.pop_front().map_or(0, |u| u.donnees.len() as u64);
    }
    retire
}

#[cfg(test)]
mod tests {
    use super::*;

    const IPS: i64 = 10;
    const IMAGE: i64 = UNITE_TEMPS / IPS;
    const FORMAT: Format = Format { codec: Codec::H264, largeur: 1920, hauteur: 1080, ips: 10, micro: false };

    fn image(n: i64, taille: usize) -> Unite {
        Unite { pts: n * IMAGE, duree: IMAGE, cle: n % IPS == 0, donnees: vec![0u8; taille].into() }
    }

    fn son(n: i64) -> Unite {
        Unite { pts: n * IMAGE / 2, duree: IMAGE / 2, cle: true, donnees: vec![0u8; 10].into() }
    }

    fn tampon(secondes: i64, plafond: u64) -> Tampon {
        let mut t = Tampon::default();
        assert!(t.preparer(FORMAT, secondes * UNITE_TEMPS, plafond));
        t
    }

    fn remplir(t: &mut Tampon, images: std::ops::Range<i64>, taille: usize) {
        for n in images {
            t.pousser(Piste::Video, image(n, taille));
            t.pousser(Piste::Jeu, son(2 * n));
            t.pousser(Piste::Jeu, son(2 * n + 1));
        }
    }

    #[test]
    fn commence_toujours_sur_une_image_cle() {
        let mut t = tampon(3, u64::MAX);
        t.pousser(Piste::Jeu, son(0));
        remplir(&mut t, 3..10, 100);
        assert_eq!((t.octets(), t.duree()), (0, 0));
        remplir(&mut t, 10..200, 100);
        for _ in 0..3 {
            let i = t.instantane(3 * UNITE_TEMPS).unwrap();
            assert!(i.video[0].cle);
            assert!(i.jeu[0].pts >= i.video[0].pts && i.jeu[0].pts < i.video[0].pts + IMAGE);
        }
    }

    #[test]
    fn evince_par_groupe_entier_selon_la_duree() {
        let mut t = tampon(3, u64::MAX);
        for n in 0..500 {
            remplir(&mut t, n..n + 1, 100);
            assert!(t.video[0].cle);
            assert!(t.duree() < 4 * UNITE_TEMPS + IMAGE);
            if n >= 40 {
                assert!(t.duree() >= 3 * UNITE_TEMPS);
            }
            assert!(t.jeu.front().map_or(true, |s| s.pts >= t.video[0].pts));
            let somme: usize = t.video.iter().chain(&t.jeu).map(|u| u.donnees.len()).sum();
            assert_eq!(t.octets(), somme as u64);
        }
        assert_eq!(t.debut(), Some(470 * IMAGE));
    }

    #[test]
    fn respecte_le_plafond_en_octets() {
        let mut t = tampon(300, 5000);
        for n in 0..1000 {
            remplir(&mut t, n..n + 1, 100);
            assert!(t.octets() <= 5000, "{}", t.octets());
            assert!(t.video.front().map_or(true, |u| u.cle));
        }
        assert!(t.duree() >= 3 * UNITE_TEMPS && t.debordements() == 0);
        let mut t = tampon(300, 500);
        remplir(&mut t, 0..9, 100);
        assert_eq!((t.octets(), t.debordements()), (0, 1));
    }

    #[test]
    fn instantane_de_la_duree_voulue_sans_arreter_le_tampon() {
        let mut t = tampon(5, u64::MAX);
        remplir(&mut t, 0..25, 100);
        let court = t.instantane(5 * UNITE_TEMPS).unwrap();
        assert_eq!((court.video[0].pts, court.duree()), (0, 25 * IMAGE));
        remplir(&mut t, 25..207, 100);
        let i = t.instantane(5 * UNITE_TEMPS).unwrap();
        assert!(i.duree() >= 5 * UNITE_TEMPS && i.duree() < 6 * UNITE_TEMPS);
        assert_eq!(i.video.last().unwrap().pts, 206 * IMAGE);
        remplir(&mut t, 207..210, 100);
        assert_eq!(t.fin(), 210 * IMAGE);
        assert_eq!(i.video.len(), 57);
        let bref = t.instantane(UNITE_TEMPS).unwrap();
        assert_eq!(bref.video[0].pts, 200 * IMAGE);
        assert!(Tampon::default().instantane(UNITE_TEMPS).is_none());
    }

    #[test]
    fn garde_le_tampon_si_le_format_ne_change_pas() {
        let mut t = tampon(5, u64::MAX);
        remplir(&mut t, 0..30, 100);
        assert!(!t.preparer(FORMAT, 5 * UNITE_TEMPS, u64::MAX));
        assert_eq!(t.fin(), 30 * IMAGE);
        t.pousser(Piste::Micro, son(61));
        assert_eq!((t.reprise(), t.fin()), (31 * IMAGE, 30 * IMAGE));
        assert!(t.preparer(Format { largeur: 1280, ..FORMAT }, 5 * UNITE_TEMPS, u64::MAX));
        assert_eq!((t.fin(), t.octets()), (0, 0));
    }
}
