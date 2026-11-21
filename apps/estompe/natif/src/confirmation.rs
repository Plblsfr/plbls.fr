use std::cell::RefCell;

use slint::ComponentHandle;

use crate::{DemandeConfirmation, DonneesConfirmation, Principale};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Saisie {
    pub libelle: String,
    pub valeur: String,
    pub exiger: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Demande {
    pub titre: String,
    pub message: String,
    pub action: String,
    pub destructif: bool,
    pub saisie: Option<Saisie>,
    pub case: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reponse {
    pub ok: bool,
    pub valeur: String,
}

impl Reponse {
    pub fn annulee() -> Self {
        Self::default()
    }
}

pub fn peut_valider(demande: &Demande, valeur: &str, coche: bool) -> bool {
    if let Some(exige) = demande.saisie.as_ref().and_then(|s| s.exiger.as_deref()) {
        if valeur.trim() != exige {
            return false;
        }
    }
    demande.case.is_none() || coche
}

pub struct File<S> {
    courante: Option<(Demande, S)>,
}

impl<S> Default for File<S> {
    fn default() -> Self {
        Self { courante: None }
    }
}

impl<S> File<S> {
    pub fn demander(&mut self, demande: Demande, suite: S) -> Option<S> {
        self.courante
            .replace((demande, suite))
            .map(|(_, precedente)| precedente)
    }

    pub fn repondre(&mut self) -> Option<S> {
        self.courante.take().map(|(_, suite)| suite)
    }

    pub fn courante(&self) -> Option<&Demande> {
        self.courante.as_ref().map(|(d, _)| d)
    }
}

type Suite = Box<dyn FnOnce(&Principale, Reponse)>;

thread_local! {
    static FILE: RefCell<File<Suite>> = RefCell::new(File::default());
}

fn demander(f: &Principale, demande: Demande, suite: Suite) {
    let g = f.global::<DonneesConfirmation>();
    g.set_demande(DemandeConfirmation {
        titre: demande.titre.as_str().into(),
        message: demande.message.as_str().into(),
        action: demande.action.as_str().into(),
        destructif: demande.destructif,
        saisie: demande.saisie.is_some(),
        saisie_libelle: demande
            .saisie
            .as_ref()
            .map(|s| s.libelle.as_str())
            .unwrap_or_default()
            .into(),
        case: demande.case.is_some(),
        case_libelle: demande.case.as_deref().unwrap_or_default().into(),
    });
    g.set_valeur(
        demande
            .saisie
            .as_ref()
            .map(|s| s.valeur.as_str())
            .unwrap_or_default()
            .into(),
    );
    g.set_coche(false);
    let precedente = FILE.with(|file| file.borrow_mut().demander(demande, suite));
    g.set_ouverte(true);
    if let Some(precedente) = precedente {
        precedente(f, Reponse::annulee());
    }
}

pub fn confirmer(
    f: &Principale,
    demande: Demande,
    suite: impl FnOnce(&Principale, bool) + 'static,
) {
    demander(f, demande, Box::new(move |f, r| suite(f, r.ok)));
}

pub fn saisir(
    f: &Principale,
    demande: Demande,
    suite: impl FnOnce(&Principale, Option<String>) + 'static,
) {
    demander(
        f,
        demande,
        Box::new(move |f, r| suite(f, r.ok.then_some(r.valeur))),
    );
}

pub fn ouverte() -> Option<Demande> {
    FILE.with(|file| file.borrow().courante().cloned())
}

pub fn repondre(f: &Principale, ok: bool) {
    let g = f.global::<DonneesConfirmation>();
    let valeur = g.get_valeur().to_string();
    let Some(suite) = FILE.with(|file| file.borrow_mut().repondre()) else {
        return;
    };
    g.set_ouverte(false);
    suite(f, Reponse { ok, valeur });
}

pub fn brancher(f: &Principale) {
    let g = f.global::<DonneesConfirmation>();
    g.on_peut_valider(|_, valeur, coche| {
        FILE.with(|file| {
            file.borrow()
                .courante()
                .is_some_and(|d| peut_valider(d, &valeur, coche))
        })
    });
    let faible = f.as_weak();
    g.on_repondre(move |ok| {
        if let Some(f) = faible.upgrade() {
            repondre(&f, ok);
        }
    });
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    type Recu = Rc<RefCell<Vec<(String, Reponse)>>>;

    fn suite(recu: &Recu, nom: &str) -> Box<dyn FnOnce(Reponse)> {
        let recu = recu.clone();
        let nom = nom.to_string();
        Box::new(move |r| recu.borrow_mut().push((nom, r)))
    }

    fn titre(t: &str) -> Demande {
        Demande {
            titre: t.into(),
            ..Demande::default()
        }
    }

    fn valider(ok: bool, valeur: &str) -> Reponse {
        Reponse {
            ok,
            valeur: valeur.into(),
        }
    }

    #[test]
    fn renvoie_vrai_quand_on_valide_et_faux_quand_on_annule() {
        let recu: Recu = Rc::default();
        let mut file = File::default();
        assert!(file
            .demander(titre("Supprimer"), suite(&recu, "a"))
            .is_none());
        file.repondre().unwrap()(valider(true, ""));
        assert!(file.courante().is_none());
        file.demander(
            Demande {
                destructif: true,
                ..titre("Supprimer")
            },
            suite(&recu, "b"),
        );
        file.repondre().unwrap()(valider(false, ""));
        let oks: Vec<bool> = recu.borrow().iter().map(|(_, r)| r.ok).collect();
        assert_eq!(oks, [true, false]);
    }

    #[test]
    fn annule_la_demande_precedente_quand_une_autre_arrive() {
        let recu: Recu = Rc::default();
        let mut file = File::default();
        file.demander(titre("A"), suite(&recu, "a"));
        let precedente = file.demander(titre("B"), suite(&recu, "b")).unwrap();
        precedente(Reponse::annulee());
        assert_eq!(file.courante().unwrap().titre, "B");
        file.repondre().unwrap()(valider(true, ""));
        assert_eq!(
            *recu.borrow(),
            [
                ("a".to_string(), valider(false, "")),
                ("b".to_string(), valider(true, ""))
            ]
        );
    }

    #[test]
    fn saisir_renvoie_le_texte_ou_rien_si_annule() {
        let recu: Rc<RefCell<Vec<Option<String>>>> = Rc::default();
        let mut file: File<Box<dyn FnOnce(Reponse)>> = File::default();
        let d = Demande {
            saisie: Some(Saisie {
                libelle: "Nom".into(),
                valeur: "x".into(),
                exiger: None,
            }),
            ..titre("Renommer")
        };
        for ok in [true, false] {
            let r = recu.clone();
            file.demander(
                d.clone(),
                Box::new(move |rep: Reponse| r.borrow_mut().push(rep.ok.then_some(rep.valeur))),
            );
            file.repondre().unwrap()(valider(ok, "y"));
        }
        assert_eq!(*recu.borrow(), [Some("y".to_string()), None]);
    }

    #[test]
    fn exige_le_nom_exact_pour_la_double_confirmation() {
        let double = Demande {
            saisie: Some(Saisie {
                libelle: "Nom".into(),
                valeur: String::new(),
                exiger: Some("Mon serveur".into()),
            }),
            ..titre("Supprimer")
        };
        assert!(!peut_valider(&double, "", false));
        assert!(!peut_valider(&double, "mon serveur", false));
        assert!(peut_valider(&double, " Mon serveur ", false));
    }

    #[test]
    fn exige_la_case_quand_elle_est_demandee() {
        let d = Demande {
            case: Some("Je comprends".into()),
            ..titre("x")
        };
        assert!(!peut_valider(&d, "", false));
        assert!(peut_valider(&d, "", true));
    }

    #[test]
    fn valide_d_office_sans_condition() {
        assert!(peut_valider(&titre("x"), "", false));
    }
}
