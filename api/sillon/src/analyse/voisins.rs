use std::collections::{HashMap, HashSet};

use uuid::Uuid;

pub const ECART_MIN: f32 = 0.1;

#[derive(Debug, Default)]
pub struct Voisins {
    ids: Vec<Uuid>,
    rangs: HashMap<Uuid, usize>,
    dimension: usize,
    valeurs: Vec<f32>,
}

fn ecart(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum()
}

impl Voisins {
    pub fn construire(bruts: Vec<(Uuid, Vec<f32>)>) -> Self {
        let dimension = bruts.first().map_or(0, |b| b.1.len());
        let bruts: Vec<_> = bruts
            .into_iter()
            .filter(|(_, v)| v.len() == dimension && v.iter().all(|x| x.is_finite()))
            .collect();
        if dimension == 0 || bruts.is_empty() {
            return Self::default();
        }
        let n = bruts.len() as f64;
        let mut moyennes = vec![0.0f64; dimension];
        let mut carres = vec![0.0f64; dimension];
        for (_, v) in &bruts {
            for (d, x) in v.iter().enumerate() {
                moyennes[d] += f64::from(*x);
                carres[d] += f64::from(*x) * f64::from(*x);
            }
        }
        let echelles: Vec<(f32, f32)> = (0..dimension)
            .map(|d| {
                let m = moyennes[d] / n;
                let e = (carres[d] / n - m * m).max(0.0).sqrt() as f32;
                (m as f32, e.max(ECART_MIN))
            })
            .collect();
        let mut valeurs = Vec::with_capacity(bruts.len() * dimension);
        let mut ids = Vec::with_capacity(bruts.len());
        let mut rangs = HashMap::with_capacity(bruts.len());
        for (id, v) in bruts {
            if rangs.insert(id, ids.len()).is_some() {
                continue;
            }
            ids.push(id);
            valeurs.extend(v.iter().zip(&echelles).map(|(x, (m, e))| (x - m) / e));
        }
        Self {
            ids,
            rangs,
            dimension,
            valeurs,
        }
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }

    fn rang(&self, i: usize) -> &[f32] {
        &self.valeurs[i * self.dimension..(i + 1) * self.dimension]
    }

    fn mesure(&self, somme: f32) -> f32 {
        (somme / self.dimension as f32).sqrt()
    }

    pub fn vecteur(&self, piste: Uuid) -> Option<&[f32]> {
        self.rangs.get(&piste).map(|&i| self.rang(i))
    }

    pub fn distance(&self, a: Uuid, b: Uuid) -> Option<f32> {
        Some(self.mesure(ecart(self.vecteur(a)?, self.vecteur(b)?)))
    }

    pub fn barycentre(&self, pistes: &[Uuid]) -> Option<Vec<f32>> {
        let mut somme = vec![0.0f32; self.dimension];
        let mut n = 0usize;
        for v in pistes.iter().filter_map(|p| self.vecteur(*p)) {
            somme.iter_mut().zip(v).for_each(|(s, x)| *s += x);
            n += 1;
        }
        (n > 0).then(|| somme.into_iter().map(|s| s / n as f32).collect())
    }

    pub fn plus_proches(
        &self,
        vecteur: &[f32],
        n: usize,
        exclure: &HashSet<Uuid>,
    ) -> Vec<(Uuid, f32)> {
        if n == 0 || self.is_empty() || vecteur.len() != self.dimension {
            return Vec::new();
        }
        let mut d: Vec<(f32, usize)> = self
            .valeurs
            .chunks_exact(self.dimension)
            .enumerate()
            .filter(|(i, _)| !exclure.contains(&self.ids[*i]))
            .map(|(i, v)| (ecart(vecteur, v), i))
            .collect();
        let ordre = |a: &(f32, usize), b: &(f32, usize)| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1));
        if d.len() > n {
            d.select_nth_unstable_by(n, ordre);
            d.truncate(n);
        }
        d.sort_unstable_by(ordre);
        d.into_iter()
            .map(|(s, i)| (self.ids[i], self.mesure(s)))
            .collect()
    }

    pub fn voisins(&self, piste: Uuid, n: usize) -> Option<Vec<(Uuid, f32)>> {
        let v = self.vecteur(piste)?;
        Some(self.plus_proches(v, n, &HashSet::from([piste])))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    fn id(i: usize) -> Uuid {
        Uuid::from_u128(i as u128 + 1)
    }

    fn generes(n: usize, dimension: usize) -> Vec<(Uuid, Vec<f32>)> {
        let mut graine = 0x2545_f491_4f6c_dd1du64;
        let mut hasard = move || {
            graine ^= graine << 13;
            graine ^= graine >> 7;
            graine ^= graine << 17;
            (graine >> 40) as f32 / (1u64 << 24) as f32
        };
        (0..n)
            .map(|i| {
                let groupe = (i % 10) as f32;
                (
                    id(i),
                    (0..dimension)
                        .map(|_| groupe + hasard())
                        .collect::<Vec<_>>(),
                )
            })
            .collect()
    }

    #[test]
    fn piste_inconnue_none_jamais_d_erreur() {
        let vide = Voisins::default();
        assert_eq!(vide.voisins(id(1), 5), None);
        assert_eq!(vide.distance(id(1), id(2)), None);
        assert_eq!(vide.vecteur(id(1)), None);
        assert_eq!(vide.barycentre(&[id(1)]), None);
        assert!(vide.plus_proches(&[0.0; 3], 5, &HashSet::new()).is_empty());
        let v = Voisins::construire(generes(20, 4));
        assert_eq!(v.voisins(id(99), 5), None);
        assert_eq!(v.distance(id(0), id(99)), None);
        assert_eq!(v.barycentre(&[id(99)]), None);
        assert!(v.plus_proches(&[0.0; 3], 5, &HashSet::new()).is_empty());
        assert!(v.plus_proches(&[0.0; 4], 0, &HashSet::new()).is_empty());
    }

    #[test]
    fn vecteurs_invalides_ou_en_double_ignores() {
        let v = Voisins::construire(vec![
            (id(0), vec![1.0, 2.0]),
            (id(1), vec![1.0]),
            (id(2), vec![f32::NAN, 0.0]),
            (id(0), vec![9.0, 9.0]),
            (id(3), vec![3.0, 2.0]),
        ]);
        assert_eq!((v.len(), v.dimension()), (2, 2));
        assert!(v.vecteur(id(1)).is_none() && v.vecteur(id(2)).is_none());
    }

    #[test]
    fn normalisation_par_dimension_et_ordre_des_voisins() {
        let v = Voisins::construire(vec![
            (id(0), vec![0.0, 1000.0, 5.0]),
            (id(1), vec![1.0, 1100.0, 5.0]),
            (id(2), vec![10.0, 1050.0, 5.001]),
            (id(3), vec![11.0, 3000.0, 5.0]),
        ]);
        for d in 0..2 {
            let colonne: Vec<f32> = (0..4).map(|i| v.vecteur(id(i)).unwrap()[d]).collect();
            let m = colonne.iter().sum::<f32>() / 4.0;
            let e = (colonne.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / 4.0).sqrt();
            assert!(m.abs() < 1e-4 && (e - 1.0).abs() < 1e-4, "{colonne:?}");
        }
        assert!(v.vecteur(id(2)).unwrap()[2].abs() < 0.02);
        let proches = v.voisins(id(0), 10).unwrap();
        assert_eq!(
            proches.iter().map(|p| p.0).collect::<Vec<_>>(),
            [id(1), id(2), id(3)]
        );
        assert!(proches.windows(2).all(|p| p[0].1 <= p[1].1));
        assert_eq!(v.distance(id(0), id(1)), Some(proches[0].1));
        assert_eq!(v.distance(id(1), id(0)), Some(proches[0].1));
        assert_eq!(v.distance(id(0), id(0)), Some(0.0));
        assert_eq!(v.voisins(id(0), 1).unwrap(), proches[..1]);
        let centre = v.barycentre(&[id(0), id(1), id(99)]).unwrap();
        let attendu: Vec<f32> = (0..3)
            .map(|d| (v.vecteur(id(0)).unwrap()[d] + v.vecteur(id(1)).unwrap()[d]) / 2.0)
            .collect();
        assert_eq!(centre, attendu);
        let sans = v.plus_proches(&centre, 2, &HashSet::from([id(0)]));
        assert_eq!(sans.iter().map(|p| p.0).collect::<Vec<_>>(), [id(1), id(2)]);
    }

    #[test]
    fn rapide_sur_cinquante_mille_pistes() {
        let (n, dimension) = (50_000, crate::analyse::descripteurs::DIMENSION);
        let debut = Instant::now();
        let v = Voisins::construire(generes(n, dimension));
        let construction = debut.elapsed();
        assert_eq!(v.len(), n);
        let requetes = 20;
        let debut = Instant::now();
        for i in 0..requetes {
            let proches = v.voisins(id(i * 2503), 50).unwrap();
            assert_eq!(proches.len(), 50);
            assert!(
                proches
                    .iter()
                    .all(|p| (p.0.as_u128() - 1) % 10 == (i * 2503) as u128 % 10)
            );
        }
        let par_requete = debut.elapsed() / requetes as u32;
        eprintln!("construction {construction:?}, voisins {par_requete:?} par requête");
        assert!(construction.as_millis() < 5_000, "{construction:?}");
        assert!(par_requete.as_millis() < 250, "{par_requete:?}");
    }
}
