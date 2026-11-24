use chrono::{Duration, NaiveDateTime};
use estompe_client::niveaux_notification::{
    apercu, avec_apercu, avec_general, avec_ignore_groupe, avec_niveau, avec_son, avec_sourdine,
    description_effective, general, ignore_groupe, niveau_de, niveau_effectif, salons_regles, son,
    sourdine, DureeSourdine, Lieu, Niveau, DUREES,
};
use estompe_client::parametres::{avec_type_de_notification, types_de_notification};
use estompe_client::store::{est_vocal, Changement, Etat, Presence};
use estompe_client::temps_reel::evenements::StatutPresence;
use estompe_client::types::NotifySettings;
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use super::super::projection::serveur_courant;
use super::Pont;
use crate::menu::{self, Ancre, Entree};
use crate::{DonneesParametres, LigneNotifSalon, Principale};

pub const ERREUR_NOTIFICATIONS: &str = "Impossible d'enregistrer les réglages.";
pub const LEVER_SOURDINE: &str = "lever-sourdine";
pub const SOURDINE: &str = "sourdine";
pub const REGLER_NOTIFICATIONS: &str = "notifications";

pub fn libelle_sourdine(maintenant: NaiveDateTime, maintenant_ms: i64, fin: i64) -> String {
    if fin == 0 {
        return "En sourdine jusqu'à réactivation".to_string();
    }
    let a = maintenant + Duration::milliseconds(fin - maintenant_ms);
    let heure = a.format("%H:%M");
    let jours = (a.date() - maintenant.date()).num_days();
    match jours {
        0 => format!("En sourdine jusqu'à {heure}"),
        1 => format!("En sourdine jusqu'à demain {heure}"),
        _ => format!("En sourdine jusqu'au {} à {heure}", a.format("%d/%m")),
    }
}

pub fn entrees_de_sourdine(fin: Option<i64>) -> Vec<Entree> {
    match fin {
        Some(_) => vec![Entree::nouvelle(
            LEVER_SOURDINE,
            "Réactiver les notifications",
            "cloche",
        )],
        None => vec![Entree::nouvelle(
            SOURDINE,
            "Mettre en sourdine…",
            "volcoupe",
        )],
    }
}

pub fn entrees_de_duree() -> Vec<Entree> {
    DUREES
        .iter()
        .map(|d| {
            let icone = if *d == DureeSourdine::Toujours {
                "interdit"
            } else {
                "horloge"
            };
            Entree::nouvelle(d.id(), d.libelle(), icone)
        })
        .collect()
}

pub fn touche_les_notifications(changements: &[Changement], moi: &str) -> bool {
    changements.iter().any(|c| match c {
        Changement::ReglagesNotification
        | Changement::Compte
        | Changement::ServeurAjoute { .. }
        | Changement::ServeurRetire { .. }
        | Changement::Serveur { .. }
        | Changement::OrdreServeurs
        | Changement::SalonAjoute { .. }
        | Changement::SalonRetire { .. }
        | Changement::Salon { .. }
        | Changement::OrdreSalons { .. } => true,
        Changement::Presence { utilisateur } => utilisateur == moi,
        _ => false,
    })
}

fn salons_texte(etat: &Etat, serveur: &str) -> Vec<(String, String)> {
    etat.salons(serveur)
        .iter()
        .filter(|s| !est_vocal(s.kind))
        .filter_map(|s| Some((s.id.clone()?, s.name.clone().unwrap_or_default())))
        .collect()
}

fn poser_lignes(g: &DonneesParametres, lignes: Vec<LigneNotifSalon>) {
    let modele = g.get_salons_notif();
    let Some(v) = modele.as_any().downcast_ref::<VecModel<LigneNotifSalon>>() else {
        g.set_salons_notif(ModelRc::new(VecModel::from(lignes)));
        return;
    };
    if v.row_count() != lignes.len() {
        v.set_vec(lignes);
        return;
    }
    for (i, l) in lignes.into_iter().enumerate() {
        if v.row_data(i).as_ref() != Some(&l) {
            v.set_row_data(i, l);
        }
    }
}

impl Pont {
    fn serveur_des_notifications(&self, etat: &Etat, choisi: &str) -> Option<String> {
        let ids = etat.ids_serveurs_ordonnes();
        if ids.iter().any(|s| s == choisi) {
            return Some(choisi.to_string());
        }
        serveur_courant(&self.route())
            .filter(|s| ids.iter().any(|i| i == s))
            .map(str::to_string)
            .or_else(|| ids.first().cloned())
    }

    fn sourdine_lisible(&self, fin: Option<i64>) -> String {
        fin.map(|fin| libelle_sourdine((self.horloge)(), self.maintenant_ms(), fin))
            .unwrap_or_default()
    }

    pub fn projeter_parametres(&self, f: &Principale) {
        let maintenant = self.maintenant_ms();
        let g = f.global::<DonneesParametres>();
        let choisi = g.get_serveur_notif().to_string();
        let (r, ndd, serveur, nom, plusieurs, salons) = self.store.lire(|e| {
            let serveur = self.serveur_des_notifications(e, &choisi);
            let nom = serveur
                .as_deref()
                .and_then(|s| e.serveur(s))
                .and_then(|s| s.name.clone())
                .unwrap_or_default();
            let salons = serveur
                .as_deref()
                .map(|s| salons_texte(e, s))
                .unwrap_or_default();
            (
                e.reglages_notification.clone(),
                e.presence_de(e.moi()) == Presence::NePasDeranger,
                serveur,
                nom,
                e.ids_serveurs_ordonnes().len() > 1,
                salons,
            )
        });
        let [prive, mention, salon, vocal] = types_de_notification(&r);
        g.set_type_prive(prive);
        g.set_type_mention(mention);
        g.set_type_salon(salon);
        g.set_type_vocal(vocal);
        g.set_ne_pas_deranger(ndd);
        g.set_apercu_notifications(apercu(&r));
        g.set_son_notifications(son(&r));
        g.set_niveau_general(Niveau::rang(general(&r)));
        let (n, o) = niveau_effectif(&r, Lieu::default(), maintenant);
        g.set_effectif_general(description_effective(n, o).into());
        g.set_a_sourdines(r.mutes.as_ref().is_some_and(|m| !m.is_empty()));
        g.set_plusieurs_serveurs(plusieurs);
        let Some(serveur) = serveur else {
            g.set_serveur_notif("".into());
            g.set_nom_serveur_notif("".into());
            poser_lignes(&g, Vec::new());
            return;
        };
        let lieu_serveur = Lieu {
            serveur: Some(&serveur),
            ..Lieu::default()
        };
        let (n, o) = niveau_effectif(&r, lieu_serveur, maintenant);
        g.set_serveur_notif(serveur.as_str().into());
        g.set_nom_serveur_notif(nom.into());
        g.set_niveau_serveur(Niveau::rang(niveau_de(&r, &serveur)));
        g.set_effectif_serveur(description_effective(n, o).into());
        g.set_sourdine_serveur(
            self.sourdine_lisible(sourdine(&r, &serveur, maintenant))
                .into(),
        );
        g.set_ignore_groupe(ignore_groupe(&r, &serveur));
        let regles = salons_regles(&r, salons.iter().map(|(id, _)| id.as_str()), maintenant);
        g.set_autres_salons(regles.len() < salons.len());
        let lignes: Vec<LigneNotifSalon> = salons
            .iter()
            .filter(|(id, _)| regles.contains(&id.as_str()))
            .map(|(id, nom)| {
                let (n, o) = niveau_effectif(
                    &r,
                    Lieu {
                        salon: Some(id),
                        serveur: Some(&serveur),
                        prive: None,
                    },
                    maintenant,
                );
                LigneNotifSalon {
                    id: id.as_str().into(),
                    nom: format!("#{nom}").into(),
                    rang: Niveau::rang(niveau_de(&r, id)),
                    effectif: description_effective(n, o).into(),
                    sourdine: self.sourdine_lisible(sourdine(&r, id, maintenant)).into(),
                }
            })
            .collect();
        poser_lignes(&g, lignes);
    }

    pub fn enregistrer_notifications(
        &self,
        f: &Principale,
        modifier: impl FnOnce(&NotifySettings) -> Option<NotifySettings>,
    ) {
        let Some((avant, reglages)) = self.store.appliquer_reglages_notification(modifier) else {
            return;
        };
        f.global::<DonneesParametres>()
            .set_erreur_notifications("".into());
        let store = self.store.clone();
        let pont = self.clone();
        let fenetre = self.fenetre.clone();
        self.runtime.spawn(async move {
            let echec = store
                .envoyer_reglages_notification(reglages, avant)
                .await
                .is_err();
            let _ = fenetre.upgrade_in_event_loop(move |f| {
                if echec {
                    f.global::<DonneesParametres>()
                        .set_erreur_notifications(ERREUR_NOTIFICATIONS.into());
                }
                pont.projeter_parametres(&f);
            });
        });
    }

    pub fn definir_type_notification(&self, f: &Principale, genre: &str, actif: bool) {
        self.enregistrer_notifications(f, |r| avec_type_de_notification(r, genre, actif));
    }

    pub fn definir_niveau_general(&self, f: &Principale, rang: i32) {
        self.enregistrer_notifications(f, |r| Some(avec_general(r, Niveau::du_rang(rang))));
    }

    pub fn definir_niveau(&self, f: &Principale, id: &str, rang: i32) {
        self.enregistrer_notifications(f, |r| Some(avec_niveau(r, id, Niveau::du_rang(rang))));
    }

    pub fn definir_apercu_notifications(&self, f: &Principale, actif: bool) {
        self.enregistrer_notifications(f, |r| Some(avec_apercu(r, actif)));
    }

    pub fn definir_son_notifications(&self, f: &Principale, actif: bool) {
        self.enregistrer_notifications(f, |r| Some(avec_son(r, actif)));
    }

    pub fn definir_ignore_groupe(&self, f: &Principale, actif: bool) {
        let serveur = f.global::<DonneesParametres>().get_serveur_notif();
        if serveur.is_empty() {
            return;
        }
        self.enregistrer_notifications(f, |r| Some(avec_ignore_groupe(r, &serveur, actif)));
    }

    pub fn definir_ne_pas_deranger(&self, f: &Principale, actif: bool) {
        f.global::<DonneesParametres>().set_ne_pas_deranger(actif);
        self.definir_statut(if actif {
            StatutPresence::Dnd
        } else {
            StatutPresence::Online
        });
    }

    pub fn mettre_en_sourdine(&self, f: &Principale, id: &str, duree: Option<DureeSourdine>) {
        let maintenant = self.maintenant_ms();
        self.enregistrer_notifications(f, |r| Some(avec_sourdine(r, id, duree, maintenant)));
    }

    pub fn sourdine_de(&self, id: &str) -> Option<i64> {
        let maintenant = self.maintenant_ms();
        self.store
            .lire(|e| sourdine(&e.reglages_notification, id, maintenant))
    }

    pub fn entrees_de_sourdine_de(&self, id: &str) -> Vec<Entree> {
        entrees_de_sourdine(self.sourdine_de(id))
    }

    pub fn menu_duree_sourdine(&self, f: &Principale, id: &str, titre: &str, ancre: Ancre) -> bool {
        let pont = self.clone();
        let id = id.to_string();
        menu::ouvrir(
            f,
            &format!("Sourdine : {titre}"),
            ancre,
            entrees_de_duree(),
            move |f, action| {
                if let Some(d) = DureeSourdine::depuis_id(action) {
                    pont.mettre_en_sourdine(f, &id, Some(d));
                }
                false
            },
        )
    }

    pub fn action_de_sourdine(
        &self,
        f: &Principale,
        id: &str,
        titre: &str,
        ancre: Ancre,
        action: &str,
    ) -> bool {
        match action {
            SOURDINE => self.menu_duree_sourdine(f, id, titre, ancre),
            LEVER_SOURDINE => {
                self.mettre_en_sourdine(f, id, None);
                true
            }
            _ => false,
        }
    }

    pub fn menu_sourdine_parametres(&self, f: &Principale, id: &str, ancre: Ancre) -> bool {
        let titre = if f.global::<DonneesParametres>().get_serveur_notif() == id {
            f.global::<DonneesParametres>()
                .get_nom_serveur_notif()
                .to_string()
        } else {
            f.global::<DonneesParametres>()
                .get_salons_notif()
                .iter()
                .find(|l| l.id == id)
                .map(|l| l.nom.to_string())
                .unwrap_or_default()
        };
        self.menu_duree_sourdine(f, id, &titre, ancre)
    }

    pub fn menu_serveur_notif(&self, f: &Principale, ancre: Ancre) -> bool {
        let courant = f.global::<DonneesParametres>().get_serveur_notif();
        let serveurs: Vec<(String, String)> = self.store.lire(|e| {
            e.serveurs_ordonnes()
                .into_iter()
                .filter_map(|s| Some((s.id.clone()?, s.name.clone().unwrap_or_default())))
                .collect()
        });
        let entrees = serveurs
            .iter()
            .map(|(id, nom)| Entree::nouvelle(id, nom, "globe").cochee(courant == id.as_str()))
            .collect();
        let pont = self.clone();
        menu::ouvrir(f, "Serveur", ancre, entrees, move |f, action| {
            f.global::<DonneesParametres>()
                .set_serveur_notif(action.into());
            pont.projeter_parametres(f);
            false
        })
    }

    pub fn menu_ajout_salon(&self, f: &Principale, ancre: Ancre) -> bool {
        let serveur = f
            .global::<DonneesParametres>()
            .get_serveur_notif()
            .to_string();
        let maintenant = self.maintenant_ms();
        let libres: Vec<(String, String)> = self.store.lire(|e| {
            let salons = salons_texte(e, &serveur);
            let regles: Vec<String> = salons_regles(
                &e.reglages_notification,
                salons.iter().map(|(id, _)| id.as_str()),
                maintenant,
            )
            .into_iter()
            .map(str::to_string)
            .collect();
            salons
                .into_iter()
                .filter(|(id, _)| !regles.contains(id))
                .collect()
        });
        let entrees = libres
            .iter()
            .map(|(id, nom)| Entree::nouvelle(id, nom, "hash"))
            .collect();
        let pont = self.clone();
        menu::ouvrir(f, "Régler un salon", ancre, entrees, move |f, action| {
            pont.definir_niveau(f, action, Niveau::rang(Some(Niveau::Tous)));
            false
        })
    }

    pub fn definir_apercus(&self, f: &Principale, apercus: bool) {
        if let Some(memoire) = &self.memoire {
            memoire.modifier(|p| p.apercus = apercus);
        }
        f.global::<DonneesParametres>().set_apercus(apercus);
        self.messagerie.definir_apercus(apercus);
        self.messagerie.rafraichir_maintenant(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn matin() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 19)
            .unwrap()
            .and_hms_opt(9, 30, 0)
            .unwrap()
    }

    #[test]
    fn la_fin_d_une_sourdine_se_lit_en_heure_locale() {
        let t = 1_000_000;
        assert_eq!(
            libelle_sourdine(matin(), t, t + 15 * 60_000),
            "En sourdine jusqu'à 09:45"
        );
        assert_eq!(
            libelle_sourdine(matin(), t, t + 24 * 3_600_000),
            "En sourdine jusqu'à demain 09:30"
        );
        assert_eq!(
            libelle_sourdine(matin(), t, t + 72 * 3_600_000),
            "En sourdine jusqu'au 22/11 à 09:30"
        );
        assert_eq!(
            libelle_sourdine(matin(), t, 0),
            "En sourdine jusqu'à réactivation"
        );
    }

    #[test]
    fn le_menu_propose_la_sourdine_ou_sa_levee() {
        let ids = |e: Vec<Entree>| e.into_iter().map(|e| e.id).collect::<Vec<_>>();
        assert_eq!(ids(entrees_de_sourdine(None)), [SOURDINE]);
        assert_eq!(ids(entrees_de_sourdine(Some(0))), [LEVER_SOURDINE]);
        assert_eq!(
            ids(entrees_de_duree()),
            [
                "sourdine-15m",
                "sourdine-1h",
                "sourdine-8h",
                "sourdine-24h",
                "sourdine-toujours"
            ]
        );
    }

    #[test]
    fn la_page_suit_les_reglages_les_salons_et_mon_statut() {
        assert!(touche_les_notifications(
            &[Changement::ReglagesNotification],
            "u1"
        ));
        assert!(touche_les_notifications(
            &[Changement::Presence {
                utilisateur: "u1".into()
            }],
            "u1"
        ));
        assert!(!touche_les_notifications(
            &[Changement::Presence {
                utilisateur: "u2".into()
            }],
            "u1"
        ));
    }
}
