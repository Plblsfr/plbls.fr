use std::time::{Duration, Instant};

pub const IPS_MAX: u32 = 240;

#[derive(Debug, Clone)]
pub struct Cadence {
    intervalle: Duration,
    prochaine: Instant,
}

impl Cadence {
    pub fn nouvelle(ips: u32, depart: Instant) -> Self {
        Self {
            intervalle: Duration::from_micros(1_000_000 / u64::from(ips.clamp(1, IPS_MAX))),
            prochaine: depart,
        }
    }

    pub fn intervalle(&self) -> Duration {
        self.intervalle
    }

    pub fn attente(&self, t: Instant) -> Duration {
        self.prochaine
            .checked_sub(self.intervalle / 4)
            .map_or(Duration::ZERO, |seuil| seuil.saturating_duration_since(t))
    }

    pub fn due(&mut self, t: Instant) -> bool {
        if !self.attente(t).is_zero() {
            return false;
        }
        self.prochaine =
            (self.prochaine + self.intervalle).max(t.checked_sub(self.intervalle).unwrap_or(t));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cadence_laisse_passer_une_image_par_intervalle() {
        let depart = Instant::now();
        let mut cadence = Cadence::nouvelle(30, depart);
        let pas = Duration::from_micros(1_000_000 / 60);
        let passees = (0..120u32)
            .filter(|i| cadence.due(depart + pas * *i))
            .count();
        assert_eq!(passees, 60);
    }

    #[test]
    fn cadence_tolere_une_image_legerement_en_avance() {
        let depart = Instant::now();
        let mut cadence = Cadence::nouvelle(60, depart);
        assert!(cadence.due(depart));
        assert!(cadence.due(depart + Duration::from_millis(15)));
        assert!(!cadence.due(depart + Duration::from_millis(20)));
    }

    #[test]
    fn cadence_borne_le_rattrapage_apres_un_silence() {
        let depart = Instant::now();
        let mut cadence = Cadence::nouvelle(30, depart);
        assert!(cadence.due(depart));
        let reprise = depart + Duration::from_secs(5);
        let passees = (0..20u64)
            .filter(|i| cadence.due(reprise + Duration::from_millis(*i)))
            .count();
        assert_eq!(passees, 3);
    }

    #[test]
    fn cadence_donne_le_delai_avant_la_prochaine_image() {
        let depart = Instant::now();
        let mut cadence = Cadence::nouvelle(40, depart);
        assert!(cadence.due(depart));
        let attente = cadence.attente(depart);
        assert_eq!(attente, Duration::from_micros(18_750));
        assert!(cadence.due(depart + attente));
    }

    #[test]
    fn cadence_borne_le_nombre_d_images_par_seconde() {
        let depart = Instant::now();
        assert_eq!(
            Cadence::nouvelle(0, depart).intervalle(),
            Duration::from_secs(1)
        );
        assert_eq!(
            Cadence::nouvelle(10_000, depart).intervalle(),
            Duration::from_micros(1_000_000 / u64::from(IPS_MAX))
        );
    }
}
