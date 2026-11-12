#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Jeu {
    pub fenetre: isize,
    pub pid: u32,
    pub nom: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pause {
    PremierPlan,
    Partage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Etat {
    Inactif,
    Arme,
    Pause(Pause),
    Indisponible(String),
}

impl Etat {
    pub fn nom(&self) -> &'static str {
        match self {
            Etat::Inactif => "inactif",
            Etat::Arme => "arme",
            Etat::Pause(_) => "pause",
            Etat::Indisponible(_) => "indisponible",
        }
    }

    pub fn raison(&self) -> Option<String> {
        match self {
            Etat::Pause(Pause::PremierPlan) => Some("le jeu n'est plus au premier plan".into()),
            Etat::Pause(Pause::Partage) => Some("le partage d'écran est prioritaire".into()),
            Etat::Indisponible(r) => Some(r.clone()),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct Automate {
    actif: bool,
    sans_encodeur: Option<String>,
    jeu: Option<Jeu>,
    premier_plan: bool,
    partage: bool,
    suspendu: bool,
    refus: Option<String>,
}

impl Automate {
    pub fn etat(&self) -> Etat {
        match (&self.sans_encodeur, &self.jeu, &self.refus) {
            _ if !self.actif => Etat::Inactif,
            (Some(r), ..) => Etat::Indisponible(r.clone()),
            (_, None, _) => Etat::Inactif,
            (.., Some(r)) => Etat::Indisponible(r.clone()),
            _ if self.suspendu => Etat::Pause(Pause::Partage),
            _ if !self.premier_plan => Etat::Pause(Pause::PremierPlan),
            _ => Etat::Arme,
        }
    }

    pub fn jeu(&self) -> Option<&Jeu> {
        self.jeu.as_ref().filter(|_| self.actif)
    }

    pub fn materiel(&mut self, sans_encodeur: Option<String>) {
        self.sans_encodeur = sans_encodeur;
    }

    pub fn configurer(&mut self, actif: bool, vider: bool) -> bool {
        let vide = vider || (self.actif && !actif);
        self.actif = actif;
        if vide {
            self.refus = None;
        }
        vide
    }

    pub fn premier_plan(&mut self, jeu: Option<Jeu>) -> bool {
        match (jeu, &self.jeu) {
            (None, _) => {
                self.premier_plan = false;
                false
            }
            (Some(j), Some(connu)) if j.pid == connu.pid && j.fenetre == connu.fenetre => {
                self.premier_plan = true;
                false
            }
            (Some(j), ancien) => {
                let vide = ancien.is_some();
                (self.jeu, self.premier_plan, self.refus) = (Some(j), true, None);
                vide
            }
        }
    }

    pub fn jeu_ferme(&mut self, pid: u32) -> bool {
        let ferme = self.jeu.as_ref().is_some_and(|j| j.pid == pid);
        if ferme {
            (self.jeu, self.premier_plan, self.refus) = (None, false, None);
        }
        ferme
    }

    pub fn partage(&mut self, actif: bool, suspendre: bool) {
        self.partage = actif;
        self.suspendu = actif && (suspendre || self.suspendu);
    }

    pub fn echec(&mut self, raison: String) -> bool {
        if self.partage {
            self.suspendu = true;
            false
        } else {
            self.refus = Some(raison);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jeu(pid: u32) -> Option<Jeu> {
        Some(Jeu { fenetre: pid as isize * 10, pid, nom: format!("Jeu {pid}") })
    }

    fn actif() -> Automate {
        let mut a = Automate::default();
        a.configurer(true, false);
        a
    }

    #[test]
    fn inactif_tant_que_desactive_ou_sans_jeu() {
        let mut a = Automate::default();
        assert!(!a.premier_plan(jeu(1)));
        assert_eq!(a.etat(), Etat::Inactif);
        assert!(a.jeu().is_none());
        a.configurer(true, false);
        assert_eq!(a.etat(), Etat::Arme);
        assert!(a.configurer(false, false));
        assert_eq!(a.etat(), Etat::Inactif);
        assert_eq!(actif().etat(), Etat::Inactif);
    }

    #[test]
    fn pause_hors_premier_plan_et_reprise_sans_vider() {
        let mut a = actif();
        assert!(!a.premier_plan(jeu(1)));
        assert_eq!(a.etat(), Etat::Arme);
        assert!(!a.premier_plan(None));
        assert_eq!(a.etat(), Etat::Pause(Pause::PremierPlan));
        assert_eq!(a.etat().nom(), "pause");
        assert!(!a.premier_plan(jeu(1)));
        assert_eq!(a.etat(), Etat::Arme);
    }

    #[test]
    fn vide_a_la_fermeture_ou_au_changement_de_jeu() {
        let mut a = actif();
        a.premier_plan(jeu(1));
        assert!(a.premier_plan(jeu(2)));
        assert_eq!(a.jeu().unwrap().pid, 2);
        assert!(!a.jeu_ferme(1));
        a.premier_plan(None);
        assert!(a.jeu_ferme(2));
        assert_eq!(a.etat(), Etat::Inactif);
        assert!(a.configurer(true, true));
    }

    #[test]
    fn partage_prioritaire() {
        let mut a = actif();
        a.premier_plan(jeu(1));
        a.partage(true, false);
        assert_eq!(a.etat(), Etat::Arme);
        assert!(!a.echec("session refusée".into()));
        assert_eq!(a.etat(), Etat::Pause(Pause::Partage));
        a.partage(true, false);
        assert_eq!(a.etat(), Etat::Pause(Pause::Partage));
        a.partage(false, false);
        assert_eq!(a.etat(), Etat::Arme);
        a.partage(true, true);
        assert_eq!(a.etat().raison().as_deref(), Some("le partage d'écran est prioritaire"));
        a.partage(false, true);
        assert_eq!(a.etat(), Etat::Arme);
    }

    #[test]
    fn indisponible_avec_raison() {
        let mut a = actif();
        a.premier_plan(jeu(1));
        assert!(a.echec("capture refusée par le jeu".into()));
        assert_eq!(a.etat(), Etat::Indisponible("capture refusée par le jeu".into()));
        a.premier_plan(None);
        a.premier_plan(jeu(1));
        assert_eq!(a.etat().nom(), "indisponible");
        a.premier_plan(jeu(2));
        assert_eq!(a.etat(), Etat::Arme);
        a.materiel(Some("aucun encodeur matériel".into()));
        assert_eq!(a.etat().raison().as_deref(), Some("aucun encodeur matériel"));
        a.materiel(None);
        assert_eq!(a.etat(), Etat::Arme);
    }
}
