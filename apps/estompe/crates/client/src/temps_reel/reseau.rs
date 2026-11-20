use std::time::Duration;

use tokio::sync::watch;
use tokio::time::Instant;

pub const BANDEAU_APRES: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatutConnexion {
    Connexion,
    Ouverte,
    Fermee,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EtatReseau {
    Connecte,
    Reconnexion,
    HorsLigne,
}

#[derive(Debug, Clone)]
pub struct CoupureDifferee {
    delai: Duration,
    echeance: Option<Instant>,
    coupe: bool,
}

impl CoupureDifferee {
    pub fn nouvelle(delai: Duration) -> Self {
        Self {
            delai,
            echeance: None,
            coupe: false,
        }
    }

    pub fn coupe(&self) -> bool {
        self.coupe
    }

    pub fn echeance(&self) -> Option<Instant> {
        self.echeance
    }

    fn poser(&mut self, valeur: bool) -> Option<bool> {
        if valeur == self.coupe {
            return None;
        }
        self.coupe = valeur;
        Some(valeur)
    }

    pub fn signaler(
        &mut self,
        statut: Option<StatutConnexion>,
        maintenant: Instant,
    ) -> Option<bool> {
        if matches!(statut, None | Some(StatutConnexion::Ouverte)) {
            self.echeance = None;
            return self.poser(false);
        }
        if self.echeance.is_none() && !self.coupe {
            self.echeance = Some(maintenant + self.delai);
        }
        None
    }

    pub fn avancer(&mut self, maintenant: Instant) -> Option<bool> {
        match self.echeance {
            Some(echeance) if echeance <= maintenant => {
                self.echeance = None;
                self.poser(true)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeReseau {
    Wifi,
    Cellulaire,
    Inconnu,
}

impl TypeReseau {
    pub fn depuis_annonce(annonce: Option<&str>) -> Self {
        match annonce {
            Some("wifi" | "ethernet") => Self::Wifi,
            Some("cellular") => Self::Cellulaire,
            _ => Self::Inconnu,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnnonceReseau {
    annonce: watch::Receiver<Option<String>>,
}

impl AnnonceReseau {
    pub fn inconnue() -> Self {
        Self::depuis(watch::channel(None).1)
    }

    pub fn depuis(annonce: watch::Receiver<Option<String>>) -> Self {
        Self { annonce }
    }

    pub fn genre(&self) -> TypeReseau {
        TypeReseau::depuis_annonce(self.annonce.borrow().as_deref())
    }

    pub fn suivre(&self) -> SuiviReseau {
        let mut annonce = self.annonce.clone();
        annonce.mark_unchanged();
        SuiviReseau { annonce }
    }
}

#[derive(Debug)]
pub struct SuiviReseau {
    annonce: watch::Receiver<Option<String>>,
}

impl SuiviReseau {
    pub async fn changement(&mut self) -> Option<TypeReseau> {
        self.annonce.changed().await.ok()?;
        Some(TypeReseau::depuis_annonce(
            self.annonce.borrow_and_update().as_deref(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bandeau_seulement_apres_une_coupure_durable_et_masque_a_la_reconnexion() {
        let debut = Instant::now();
        let a = |ms: u64| debut + Duration::from_millis(ms);
        let mut vus = Vec::new();
        let mut coupure = CoupureDifferee::nouvelle(Duration::from_millis(3_000));
        let mut noter = |v: Option<bool>| vus.extend(v);
        noter(coupure.signaler(Some(StatutConnexion::Fermee), a(0)));
        noter(coupure.signaler(Some(StatutConnexion::Connexion), a(0)));
        noter(coupure.avancer(a(2_999)));
        noter(coupure.signaler(Some(StatutConnexion::Ouverte), a(2_999)));
        noter(coupure.avancer(a(12_999)));
        assert_eq!(coupure.echeance(), None);
        noter(coupure.signaler(Some(StatutConnexion::Fermee), a(12_999)));
        noter(coupure.avancer(a(14_999)));
        noter(coupure.signaler(Some(StatutConnexion::Connexion), a(14_999)));
        noter(coupure.avancer(a(15_999)));
        noter(coupure.signaler(Some(StatutConnexion::Fermee), a(15_999)));
        noter(coupure.signaler(Some(StatutConnexion::Ouverte), a(15_999)));
        assert_eq!(vus, [true, false]);
    }

    #[test]
    fn bandeau_affiche_a_l_echeance_exacte() {
        let debut = Instant::now();
        let mut coupure = CoupureDifferee::nouvelle(BANDEAU_APRES);
        coupure.signaler(Some(StatutConnexion::Fermee), debut);
        assert_eq!(
            coupure.avancer(debut + BANDEAU_APRES - Duration::from_millis(1)),
            None
        );
        assert_eq!(coupure.avancer(debut + BANDEAU_APRES), Some(true));
        assert!(coupure.coupe());
    }

    #[test]
    fn arret_volontaire_bandeau_masque_et_minuteur_annule() {
        let debut = Instant::now();
        let mut coupure = CoupureDifferee::nouvelle(Duration::from_millis(3_000));
        assert_eq!(coupure.signaler(Some(StatutConnexion::Fermee), debut), None);
        assert_eq!(coupure.signaler(None, debut), None);
        assert_eq!(coupure.echeance(), None);
        assert_eq!(coupure.avancer(debut + Duration::from_secs(10)), None);
        assert!(!coupure.coupe());
    }

    #[tokio::test]
    async fn reste_inconnu_quand_le_systeme_ne_dit_rien() {
        let annonce = AnnonceReseau::inconnue();
        assert_eq!(annonce.genre(), TypeReseau::Inconnu);
        assert_eq!(TypeReseau::depuis_annonce(None), TypeReseau::Inconnu);
        assert_eq!(TypeReseau::depuis_annonce(Some("4g")), TypeReseau::Inconnu);
        assert_eq!(annonce.suivre().changement().await, None);
    }

    #[tokio::test]
    async fn distingue_le_wifi_des_donnees_mobiles_et_suit_les_changements() {
        let (source, recepteur) = watch::channel(Some("cellular".to_string()));
        let annonce = AnnonceReseau::depuis(recepteur);
        assert_eq!(annonce.genre(), TypeReseau::Cellulaire);
        source.send_replace(Some("wifi".into()));
        assert_eq!(annonce.genre(), TypeReseau::Wifi);
        source.send_replace(Some("ethernet".into()));
        assert_eq!(annonce.genre(), TypeReseau::Wifi);
        source.send_replace(Some("bluetooth".into()));
        assert_eq!(annonce.genre(), TypeReseau::Inconnu);
        let mut suivi = annonce.suivre();
        assert_eq!(source.receiver_count(), 2);
        source.send_replace(Some("cellular".into()));
        assert_eq!(suivi.changement().await, Some(TypeReseau::Cellulaire));
        drop(suivi);
        assert_eq!(source.receiver_count(), 1);
    }
}
