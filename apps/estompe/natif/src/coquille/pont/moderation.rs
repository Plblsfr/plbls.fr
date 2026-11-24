use estompe_client::creation::{message_d_erreur, MAX_NOM_SALON, MODES_LENTS};
use estompe_client::http::ErreurApi;
use estompe_client::moderation::{
    actions_de_moderation, borner, date_du_journal, erreur_renommage, heures_saisies, icone_et_ton,
    messages_supprimes, nom_de_la_cible, nombre_a_supprimer, peut_moderer_salon, phrase_du_journal,
    pseudo_de_serveur, question_de_suppression, secondes, valide, ActionModeration,
    GenreModeration, Ton, FILTRES_JOURNAL, JOURNAL_VIDE, MAX_PSEUDO, MAX_RAISON, PERSONNALISE,
    PURGES,
};
use estompe_client::store::GenreSalon;
use estompe_client::types::{
    GetServersIdAuditLogReponseElement, PostServersIdMembersUidBanRequete,
};
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};

use super::super::membres::DonneesMembre;
use super::super::projection::{initiales, teinte};
use super::Pont;
use crate::confirmation::{self, Demande, Saisie};
use crate::menu::{self, Ancre, Entree};
use crate::{
    DonneesModeration, DonneesModerationSalon, DonneesReglagesServeur, EntreeJournal, Principale,
    TonJournal,
};

pub const SALON_MIS_A_JOUR: &str = "Salon mis à jour.";
pub const MODERER_SALON: &str = "moderer-salon";

#[derive(Clone, Debug, PartialEq)]
pub struct EtatModerationMembre {
    pub serveur: String,
    pub membre: String,
    pub pseudo: String,
    pub genre: GenreModeration,
    pub duree: i64,
    pub heures: String,
    pub purge: i64,
    pub raison: String,
    pub surnom: String,
    pub en_cours: bool,
    pub fait: bool,
    pub erreur: String,
}

impl EtatModerationMembre {
    pub fn secondes(&self) -> i64 {
        secondes(self.duree, heures_saisies(&self.heures).unwrap_or(0.0))
    }

    pub fn valide(&self) -> bool {
        valide(self.genre, &self.raison, &self.surnom, self.secondes())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EtatModerationSalon {
    pub serveur: String,
    pub salon: String,
    pub nom: String,
    pub mode_lent: i64,
    pub verrouille: bool,
    pub nombre: String,
    pub auteur: usize,
    pub auteurs: Vec<(String, String)>,
    pub en_cours: bool,
    pub erreur: String,
    pub annonce: String,
}

#[derive(Default)]
pub(super) struct EtatModeration {
    membre: Option<EtatModerationMembre>,
    salon: Option<EtatModerationSalon>,
}

pub fn entrees_de_moderation(actions: &[ActionModeration]) -> Vec<Entree> {
    actions
        .iter()
        .map(|a| {
            let entree = Entree::nouvelle(a.id(), a.libelle(), a.icone());
            if a.danger() {
                entree.danger()
            } else {
                entree
            }
        })
        .collect()
}

fn modele<T: Clone + 'static>(lignes: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(lignes))
}

fn ton_slint(ton: Ton) -> TonJournal {
    match ton {
        Ton::Neutre => TonJournal::Neutre,
        Ton::Attention => TonJournal::Attention,
        Ton::Danger => TonJournal::Danger,
    }
}

impl Pont {
    pub fn moderation_membre(&self) -> Option<EtatModerationMembre> {
        self.verrou().moderation.membre.clone()
    }

    pub fn moderation_salon(&self) -> Option<EtatModerationSalon> {
        self.verrou().moderation.salon.clone()
    }

    fn modifier_moderation<R>(
        &self,
        modification: impl FnOnce(&mut EtatModerationMembre) -> R,
    ) -> Option<R> {
        self.verrou().moderation.membre.as_mut().map(modification)
    }

    fn modifier_moderation_salon<R>(
        &self,
        modification: impl FnOnce(&mut EtatModerationSalon) -> R,
    ) -> Option<R> {
        self.verrou().moderation.salon.as_mut().map(modification)
    }

    pub(super) fn actions_sur(&self, serveur: &str, membre: &str) -> Vec<ActionModeration> {
        let maintenant = self.maintenant_ms();
        self.store
            .lire(|etat| actions_de_moderation(etat, serveur, membre, maintenant))
    }

    pub(super) fn agir_sur_membre(
        &self,
        f: &Principale,
        serveur: &str,
        membre: &DonneesMembre,
        action: &str,
    ) -> bool {
        match ActionModeration::depuis_id(action) {
            Some(ActionModeration::LeverExclusion) => {
                self.lever_exclusion(serveur, &membre.utilisateur);
                false
            }
            Some(ActionModeration::Moderer(genre)) => {
                self.ouvrir_moderation(f, serveur, membre, genre);
                true
            }
            None => false,
        }
    }

    fn lever_exclusion(&self, serveur: &str, membre: &str) {
        let pont = self.clone();
        let client = self.store.client().clone();
        let serveur = serveur.to_string();
        let membre = membre.to_string();
        self.runtime.spawn(async move {
            if let Err(erreur) = client.lever_exclusion(&serveur, &membre).await {
                let texte = format!(
                    "Impossible de lever l'exclusion : {}",
                    message_d_erreur(&erreur)
                );
                let saisie = pont.messagerie.saisie().clone();
                let ecrire = pont.messagerie.peut_ecrire();
                let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                    if ecrire {
                        saisie.signaler_erreur(&f, &texte);
                    }
                });
            }
        });
    }

    pub fn menu_moderation_du_membre(
        &self,
        f: &Principale,
        utilisateur: &str,
        ancre: Ancre,
    ) -> bool {
        let Some(serveur) = self.verrou().membres.serveur.clone() else {
            return false;
        };
        let Some(membre) = self.donnees_du_membre(utilisateur) else {
            return false;
        };
        let actions = self.actions_sur(&serveur, utilisateur);
        let pont = self.clone();
        let titre = membre.nom.clone();
        menu::ouvrir(
            f,
            &titre,
            ancre,
            entrees_de_moderation(&actions),
            move |f, action| {
                pont.fermer_carte(f);
                pont.agir_sur_membre(f, &serveur, &membre, action)
            },
        )
    }

    pub fn ouvrir_moderation(
        &self,
        f: &Principale,
        serveur: &str,
        membre: &DonneesMembre,
        genre: GenreModeration,
    ) {
        let (pseudo, surnom) = self.store.lire(|etat| {
            (
                nom_de_la_cible(etat, serveur, &membre.utilisateur),
                pseudo_de_serveur(etat, serveur, &membre.utilisateur),
            )
        });
        self.verrou().moderation.membre = Some(EtatModerationMembre {
            serveur: serveur.to_string(),
            membre: membre.utilisateur.clone(),
            pseudo: pseudo.clone(),
            genre,
            duree: genre.duree_de_depart(),
            heures: "1".into(),
            purge: 0,
            raison: String::new(),
            surnom: surnom.clone(),
            en_cours: false,
            fait: false,
            erreur: String::new(),
        });
        let portrait = membre.avatar.as_ref().and_then(|c| self.profils.avatar(c));
        let g = f.global::<DonneesModeration>();
        g.set_titre(genre.titre(&pseudo).into());
        g.set_pseudo(pseudo.as_str().into());
        g.set_initiales(membre.initiales.as_str().into());
        g.set_teinte(membre.teinte);
        g.set_portrait(portrait.map(Image::from_rgba8).unwrap_or_default());
        g.set_explication(genre.explication(&pseudo).into());
        g.set_durees(modele(
            genre
                .durees()
                .iter()
                .map(|(l, _)| SharedString::from(*l))
                .collect(),
        ));
        g.set_heures_max(format!("{}", genre.heures_max()).into());
        g.set_avec_purge(genre == GenreModeration::Bannir);
        g.set_purges(modele(
            PURGES.iter().map(|(l, _)| SharedString::from(*l)).collect(),
        ));
        g.set_avec_pseudo(genre == GenreModeration::Pseudo);
        g.set_raison_obligatoire(genre == GenreModeration::Avertir);
        g.set_raison("".into());
        g.set_surnom(surnom.as_str().into());
        g.set_heures("1".into());
        g.set_bouton(genre.bouton(&pseudo).into());
        g.set_danger(genre.destructif());
        self.afficher_moderation(f);
        g.set_ouvert(true);
        g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
    }

    pub fn fermer_moderation(&self, f: &Principale) {
        if self.verrou().moderation.membre.take().is_none() {
            return;
        }
        let g = f.global::<DonneesModeration>();
        g.set_ouvert(false);
        g.set_portrait(Image::default());
        self.rendre_le_focus(f);
    }

    pub fn choisir_duree_de_moderation(&self, f: &Principale, index: i32) {
        self.modifier_moderation(|e| {
            if let Some((_, v)) = usize::try_from(index)
                .ok()
                .and_then(|i| e.genre.durees().get(i))
            {
                e.duree = *v;
                e.erreur.clear();
            }
        });
        self.afficher_moderation(f);
    }

    pub fn heures_de_moderation_modifiees(&self, f: &Principale, texte: &str) {
        self.modifier_moderation(|e| e.heures = texte.to_string());
        self.afficher_moderation(f);
    }

    pub fn choisir_purge(&self, f: &Principale, index: i32) {
        self.modifier_moderation(|e| {
            if let Some((_, v)) = usize::try_from(index).ok().and_then(|i| PURGES.get(i)) {
                e.purge = *v;
            }
        });
        self.afficher_moderation(f);
    }

    pub fn raison_de_moderation_modifiee(&self, f: &Principale, texte: &str) {
        let raison = borner(texte, MAX_RAISON);
        if raison != texte {
            f.global::<DonneesModeration>()
                .set_raison(raison.as_str().into());
        }
        self.modifier_moderation(|e| e.raison = raison);
        self.afficher_moderation(f);
    }

    pub fn surnom_de_moderation_modifie(&self, f: &Principale, texte: &str) {
        let surnom = borner(texte, MAX_PSEUDO);
        if surnom != texte {
            f.global::<DonneesModeration>()
                .set_surnom(surnom.as_str().into());
        }
        self.modifier_moderation(|e| e.surnom = surnom);
        self.afficher_moderation(f);
    }

    pub fn valider_moderation(&self, f: &Principale) {
        let demande = self
            .modifier_moderation(|e| {
                if e.en_cours || e.fait || !e.valide() {
                    return None;
                }
                e.en_cours = true;
                e.erreur.clear();
                Some(e.clone())
            })
            .flatten();
        let Some(e) = demande else {
            return;
        };
        self.afficher_moderation(f);
        let client = self.store.client().clone();
        let pont = self.clone();
        self.runtime.spawn(async move {
            let (s, m) = (e.serveur.as_str(), e.membre.as_str());
            let raison = e.raison.trim();
            let resultat = match e.genre {
                GenreModeration::Expulser => client.expulser_membre(s, m, raison).await,
                GenreModeration::Bannir => {
                    let ban = PostServersIdMembersUidBanRequete {
                        reason: Some(raison.to_string()),
                        duration: Some(e.secondes()),
                        delete_messages: Some(e.purge),
                    };
                    client.bannir_membre(s, m, &ban).await
                }
                GenreModeration::Exclure => client.exclure_membre(s, m, e.secondes(), raison).await,
                GenreModeration::Avertir => client.avertir_membre(s, m, raison).await,
                GenreModeration::Pseudo => client.renommer_membre(s, m, e.surnom.trim()).await,
            };
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                p.modifier_moderation(|x| {
                    x.en_cours = false;
                    match resultat {
                        Ok(()) => x.fait = true,
                        Err(erreur) => x.erreur = message_d_erreur(&erreur),
                    }
                });
                p.afficher_moderation(&f);
            });
        });
    }

    fn afficher_moderation(&self, f: &Principale) {
        let Some(e) = self.moderation_membre() else {
            return;
        };
        let g = f.global::<DonneesModeration>();
        g.set_duree(
            e.genre
                .durees()
                .iter()
                .position(|(_, v)| *v == e.duree)
                .map_or(-1, |i| i as i32),
        );
        g.set_personnalise(e.duree == PERSONNALISE);
        g.set_purge(
            PURGES
                .iter()
                .position(|(_, v)| *v == e.purge)
                .map_or(0, |i| i as i32),
        );
        g.set_compte_raison(format!("{} / {MAX_RAISON}", e.raison.chars().count()).into());
        g.set_compte_surnom(format!("{} / {MAX_PSEUDO}", e.surnom.chars().count()).into());
        g.set_peut_valider(e.valide() && !e.en_cours && !e.fait);
        g.set_en_cours(e.en_cours);
        g.set_erreur(e.erreur.as_str().into());
        g.set_annonce(if e.fait { e.genre.fait() } else { "" }.into());
    }

    pub(super) fn entree_moderation_salon(&self, serveur: &str, salon: &str) -> Option<Entree> {
        let (gerer, nettoyer) = self.store.lire(|etat| {
            let texte = etat
                .salon(salon)
                .is_some_and(|(_, s)| s.kind == Some(GenreSalon::Text));
            let (g, n) = peut_moderer_salon(etat, serveur, salon);
            (texte && g, texte && n)
        });
        (gerer || nettoyer)
            .then(|| Entree::nouvelle(MODERER_SALON, "Modération du salon…", "bouclier").separee())
    }

    pub fn ouvrir_moderation_salon(&self, f: &Principale, serveur: &str, salon: &str) {
        let Some((nom, mode_lent, verrouille, auteurs, (gerer, nettoyer))) =
            self.store.lire(|etat| {
                let (_, s) = etat.salon(salon)?;
                let auteurs: Vec<(String, String)> = etat
                    .membres(serveur)
                    .iter()
                    .map(|m| {
                        (
                            m.id.clone().unwrap_or_default(),
                            m.pseudo.clone().unwrap_or_default(),
                        )
                    })
                    .collect();
                Some((
                    s.name.clone().unwrap_or_default(),
                    s.slowmode.unwrap_or(0),
                    s.locked.unwrap_or(false),
                    auteurs,
                    peut_moderer_salon(etat, serveur, salon),
                ))
            })
        else {
            return;
        };
        if !gerer && !nettoyer {
            return;
        }
        self.verrou().moderation.salon = Some(EtatModerationSalon {
            serveur: serveur.to_string(),
            salon: salon.to_string(),
            nom: nom.clone(),
            mode_lent,
            verrouille,
            nombre: "10".into(),
            auteur: 0,
            auteurs,
            en_cours: false,
            erreur: String::new(),
            annonce: String::new(),
        });
        let g = f.global::<DonneesModerationSalon>();
        g.set_titre(format!("Modération · {nom}").into());
        g.set_gerer(gerer);
        g.set_nettoyer(nettoyer);
        g.set_nom(nom.as_str().into());
        g.set_nombre("10".into());
        g.set_modes_lents(modele(
            MODES_LENTS
                .iter()
                .map(|(l, _)| SharedString::from(*l))
                .collect(),
        ));
        self.afficher_moderation_salon(f);
        g.set_ouvert(true);
        g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
    }

    pub fn fermer_moderation_salon(&self, f: &Principale) {
        if self.verrou().moderation.salon.take().is_none() {
            return;
        }
        f.global::<DonneesModerationSalon>().set_ouvert(false);
        self.rendre_le_focus(f);
    }

    fn modifier_salon_et_afficher(
        &self,
        f: &Principale,
        modification: impl FnOnce(&mut EtatModerationSalon),
    ) {
        self.modifier_moderation_salon(|e| {
            modification(e);
            e.annonce.clear();
        });
        self.afficher_moderation_salon(f);
    }

    pub fn nom_de_moderation_modifie(&self, f: &Principale, texte: &str) {
        self.modifier_salon_et_afficher(f, |e| {
            e.nom = texte.to_string();
            e.erreur.clear();
        });
    }

    pub fn choisir_mode_lent_du_salon(&self, f: &Principale, index: i32) {
        self.modifier_salon_et_afficher(f, |e| {
            if let Some((_, v)) = usize::try_from(index).ok().and_then(|i| MODES_LENTS.get(i)) {
                e.mode_lent = *v;
            }
        });
    }

    pub fn verrouiller_salon(&self, f: &Principale, verrouille: bool) {
        self.modifier_salon_et_afficher(f, |e| e.verrouille = verrouille);
    }

    pub fn nombre_a_supprimer_modifie(&self, f: &Principale, texte: &str) {
        self.modifier_salon_et_afficher(f, |e| e.nombre = texte.to_string());
    }

    pub fn menu_auteur_a_supprimer(&self, f: &Principale, ancre: Ancre) -> bool {
        let Some(e) = self.moderation_salon() else {
            return false;
        };
        let entrees: Vec<Entree> = std::iter::once("Tout le monde".to_string())
            .chain(e.auteurs.iter().map(|(_, p)| p.clone()))
            .enumerate()
            .map(|(i, libelle)| {
                Entree::nouvelle(
                    &i.to_string(),
                    &libelle,
                    if i == 0 { "gens" } else { "perso" },
                )
                .cochee(i == e.auteur)
            })
            .collect();
        let pont = self.clone();
        menu::ouvrir(f, "Auteur", ancre, entrees, move |f, id| {
            if let Ok(i) = id.parse::<usize>() {
                pont.modifier_salon_et_afficher(f, |e| e.auteur = i.min(e.auteurs.len()));
            }
            false
        })
    }

    fn executer_moderation_salon<T: Send + 'static>(
        &self,
        f: &Principale,
        tache: impl std::future::Future<Output = Result<T, ErreurApi>> + Send + 'static,
        suite: impl FnOnce(&Pont, &Principale, T) + Send + 'static,
    ) {
        let libre = self
            .modifier_moderation_salon(|e| {
                if e.en_cours {
                    return false;
                }
                e.en_cours = true;
                e.erreur.clear();
                e.annonce.clear();
                true
            })
            .unwrap_or(false);
        if !libre {
            return;
        }
        self.afficher_moderation_salon(f);
        let pont = self.clone();
        self.runtime.spawn(async move {
            let resultat = tache.await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                if p.modifier_moderation_salon(|e| e.en_cours = false)
                    .is_none()
                {
                    return;
                }
                match resultat {
                    Ok(valeur) => suite(&p, &f, valeur),
                    Err(erreur) => {
                        p.modifier_moderation_salon(|e| e.erreur = message_d_erreur(&erreur));
                    }
                }
                p.afficher_moderation_salon(&f);
            });
        });
    }

    pub fn enregistrer_moderation_salon(&self, f: &Principale) {
        let Some(e) = self.moderation_salon() else {
            return;
        };
        let (erreur, actuel) = self.store.lire(|etat| {
            (
                erreur_renommage(etat, &e.serveur, &e.salon, &e.nom),
                etat.salon(&e.salon)
                    .and_then(|(_, s)| s.name.clone())
                    .unwrap_or_default(),
            )
        });
        if !erreur.is_empty() {
            return;
        }
        let client = self.store.client().clone();
        let store = self.store.clone();
        let nom = e.nom.trim().to_string();
        self.executer_moderation_salon(
            f,
            async move {
                if nom != actuel {
                    client.renommer_salon(&e.serveur, &e.salon, &nom).await?;
                }
                client
                    .moderer_salon(&e.serveur, &e.salon, e.mode_lent, e.verrouille)
                    .await?;
                store.recharger_serveur(&e.serveur).await
            },
            |p, _, _| {
                p.modifier_moderation_salon(|e| e.annonce = SALON_MIS_A_JOUR.to_string());
            },
        );
    }

    pub fn supprimer_messages_du_salon(&self, f: &Principale) {
        let Some(e) = self.moderation_salon() else {
            return;
        };
        let Some(nombre) = nombre_a_supprimer(&e.nombre) else {
            return;
        };
        let auteur = e
            .auteur
            .checked_sub(1)
            .and_then(|i| e.auteurs.get(i))
            .map(|(id, _)| id.clone());
        let nom = self.store.lire(|etat| {
            etat.salon(&e.salon)
                .and_then(|(_, s)| s.name.clone())
                .unwrap_or_default()
        });
        let pont = self.clone();
        confirmation::confirmer(
            f,
            Demande {
                titre: "Supprimer des messages".into(),
                message: question_de_suppression(nombre, &nom, auteur.is_some()),
                action: "Supprimer les messages".into(),
                destructif: true,
                ..Demande::default()
            },
            move |f, ok| {
                if !ok {
                    return;
                }
                let client = pont.store.client().clone();
                pont.executer_moderation_salon(
                    f,
                    async move {
                        client
                            .supprimer_derniers_messages(&e.salon, nombre, auteur.as_deref())
                            .await
                    },
                    |p, _, n| {
                        p.modifier_moderation_salon(|e| e.annonce = messages_supprimes(n));
                    },
                );
            },
        );
    }

    pub fn supprimer_salon_moderation(&self, f: &Principale) {
        let Some(e) = self.moderation_salon() else {
            return;
        };
        let (serveur, salon) = (e.serveur, e.salon);
        self.demander_suppression_salon(f, &serveur, &salon, move |p, f| {
            p.fermer_moderation_salon(f)
        });
    }

    pub(super) fn demander_suppression_salon(
        &self,
        f: &Principale,
        serveur: &str,
        salon: &str,
        apres: impl FnOnce(&Pont, &Principale) + Send + 'static,
    ) {
        let nom = self.store.lire(|etat| {
            etat.salon(salon)
                .and_then(|(_, s)| s.name.clone())
                .unwrap_or_default()
        });
        let pont = self.clone();
        let (serveur, salon) = (serveur.to_string(), salon.to_string());
        confirmation::confirmer(
            f,
            Demande {
                titre: "Supprimer le salon".into(),
                message: format!("Supprimer le salon « {nom} » et tous ses messages ?"),
                action: "Supprimer le salon".into(),
                destructif: true,
                ..Demande::default()
            },
            move |_, ok| {
                if !ok {
                    return;
                }
                let p = pont.clone();
                pont.runtime.spawn(async move {
                    let client = p.store.client().clone();
                    let resultat = match client.supprimer_salon(&serveur, &salon).await {
                        Ok(()) => p.store.recharger_serveur(&serveur).await,
                        Err(e) => Err(e),
                    };
                    let q = p.clone();
                    let _ = p.fenetre.upgrade_in_event_loop(move |f| match resultat {
                        Ok(()) => apres(&q, &f),
                        Err(erreur) => q.erreur_de_moderation(&f, &erreur),
                    });
                });
            },
        );
    }

    pub fn renommer_salon_des_reglages(&self, f: &Principale, salon: &str) {
        let Some(serveur) = self.serveur_des_reglages() else {
            return;
        };
        let Some(nom) = self
            .store
            .lire(|etat| etat.salon(salon).and_then(|(_, s)| s.name.clone()))
        else {
            return;
        };
        let pont = self.clone();
        let salon = salon.to_string();
        confirmation::saisir(
            f,
            Demande {
                titre: format!("Renommer « {nom} »"),
                action: "Renommer".into(),
                saisie: Some(Saisie {
                    libelle: "Nouveau nom".into(),
                    valeur: nom.clone(),
                    exiger: None,
                }),
                ..Demande::default()
            },
            move |f, valeur| {
                let Some(valeur) = valeur.map(|v| v.trim().to_string()) else {
                    return;
                };
                if valeur == nom {
                    return;
                }
                let erreur = pont
                    .store
                    .lire(|etat| erreur_renommage(etat, &serveur, &salon, &valeur));
                if !erreur.is_empty() {
                    pont.modifier_reglages(|e| e.erreur = erreur.to_string());
                    pont.projeter_reglages(f);
                    return;
                }
                let client = pont.store.client().clone();
                let store = pont.store.clone();
                pont.executer(
                    f,
                    async move {
                        client.renommer_salon(&serveur, &salon, &valeur).await?;
                        store.recharger_serveur(&serveur).await
                    },
                    |_, _, ()| {},
                );
            },
        );
    }

    pub fn supprimer_salon_des_reglages(&self, f: &Principale, salon: &str) {
        let Some(serveur) = self.serveur_des_reglages() else {
            return;
        };
        self.demander_suppression_salon(f, &serveur, salon, |p, f| p.projeter_reglages(f));
    }

    fn erreur_de_moderation(&self, f: &Principale, erreur: &ErreurApi) {
        let texte = message_d_erreur(erreur);
        if self
            .modifier_moderation_salon(|e| e.erreur = texte.clone())
            .is_some()
        {
            self.afficher_moderation_salon(f);
        } else if self
            .modifier_reglages(|e| e.erreur = texte.clone())
            .is_some()
        {
            self.projeter_reglages(f);
        }
    }

    fn afficher_moderation_salon(&self, f: &Principale) {
        let Some(e) = self.moderation_salon() else {
            return;
        };
        let erreur = self
            .store
            .lire(|etat| erreur_renommage(etat, &e.serveur, &e.salon, &e.nom));
        let g = f.global::<DonneesModerationSalon>();
        g.set_erreur_nom(erreur.into());
        g.set_compte_nom(format!("{} / {MAX_NOM_SALON}", e.nom.trim().chars().count()).into());
        g.set_mode_lent(
            MODES_LENTS
                .iter()
                .position(|(_, v)| *v == e.mode_lent)
                .map_or(0, |i| i as i32),
        );
        g.set_verrouille(e.verrouille);
        g.set_auteur(
            e.auteur
                .checked_sub(1)
                .and_then(|i| e.auteurs.get(i))
                .map_or("Tout le monde", |(_, p)| p.as_str())
                .into(),
        );
        g.set_peut_enregistrer(erreur.is_empty() && !e.en_cours);
        g.set_peut_supprimer(nombre_a_supprimer(&e.nombre).is_some() && !e.en_cours);
        g.set_en_cours(e.en_cours);
        g.set_erreur(e.erreur.as_str().into());
        g.set_annonce(e.annonce.as_str().into());
    }

    pub(super) fn charger_journal(&self, f: &Principale) {
        let Some((serveur, filtre)) = self
            .verrou()
            .reglages
            .as_ref()
            .map(|e| (e.serveur.clone(), e.filtre_journal.clone()))
        else {
            return;
        };
        let _ = f;
        let pont = self.clone();
        self.runtime.spawn(async move {
            let Ok(liste) = pont
                .store
                .client()
                .journal_du_serveur(&serveur, &filtre)
                .await
            else {
                return;
            };
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.poser_journal(&f, &serveur, &filtre, liste));
        });
    }

    pub fn poser_journal(
        &self,
        f: &Principale,
        serveur: &str,
        filtre: &str,
        liste: Vec<GetServersIdAuditLogReponseElement>,
    ) {
        let pose = self
            .modifier_reglages(|e| {
                if e.serveur != serveur || e.filtre_journal != filtre {
                    return false;
                }
                e.journal = liste;
                true
            })
            .unwrap_or(false);
        if pose {
            self.projeter_reglages(f);
        }
    }

    pub fn menu_filtre_journal(&self, f: &Principale, ancre: Ancre) -> bool {
        let Some(actuel) = self.reglages_serveur().map(|e| e.filtre_journal) else {
            return false;
        };
        let entrees: Vec<Entree> = FILTRES_JOURNAL
            .iter()
            .map(|(id, libelle)| {
                let id = if id.is_empty() { "tout" } else { id };
                Entree::nouvelle(id, libelle, "filtre").cochee(
                    FILTRES_JOURNAL
                        .iter()
                        .any(|(v, l)| v == &actuel && l == libelle),
                )
            })
            .collect();
        let pont = self.clone();
        menu::ouvrir(f, "Type d'action", ancre, entrees, move |f, id| {
            let filtre = if id == "tout" { "" } else { id };
            pont.modifier_reglages(|e| {
                e.filtre_journal = filtre.to_string();
                e.journal.clear();
            });
            pont.projeter_reglages(f);
            pont.charger_journal(f);
            false
        })
    }

    pub(super) fn projeter_journal(&self, f: &Principale) {
        let Some(e) = self.reglages_serveur() else {
            return;
        };
        let decalage = self.messagerie.decalage_minutes();
        let g = f.global::<DonneesReglagesServeur>();
        g.set_filtre_journal(
            FILTRES_JOURNAL
                .iter()
                .find(|(v, _)| *v == e.filtre_journal)
                .map_or("", |(_, l)| *l)
                .into(),
        );
        g.set_journal_vide(JOURNAL_VIDE.into());
        g.set_journal(modele(
            e.journal
                .iter()
                .map(|x| {
                    let (icone, ton) = icone_et_ton(x.action);
                    let acteur = x.actor.clone().unwrap_or_default();
                    EntreeJournal {
                        date: date_du_journal(x, decalage).into(),
                        initiales: initiales(&acteur).as_str().into(),
                        teinte: teinte(x.actor_id.as_deref().unwrap_or(&acteur)),
                        acteur: acteur.into(),
                        phrase: phrase_du_journal(x).into(),
                        icone: icone.into(),
                        ton: ton_slint(ton),
                    }
                })
                .collect(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_actions_de_moderation_gardent_l_ordre_et_le_danger_du_front() {
        let entrees = entrees_de_moderation(&[
            ActionModeration::LeverExclusion,
            ActionModeration::Moderer(GenreModeration::Avertir),
            ActionModeration::Moderer(GenreModeration::Pseudo),
            ActionModeration::Moderer(GenreModeration::Expulser),
            ActionModeration::Moderer(GenreModeration::Bannir),
        ]);
        let ids: Vec<&str> = entrees.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["untimeout", "warn", "nickname", "kick", "ban"]);
        let dangers: Vec<bool> = entrees.iter().map(|e| e.danger).collect();
        assert_eq!(dangers, [false, false, false, true, true]);
    }

    #[test]
    fn une_duree_personnalisee_se_lit_en_heures() {
        let mut e = EtatModerationMembre {
            serveur: "s1".into(),
            membre: "b".into(),
            pseudo: "bob".into(),
            genre: GenreModeration::Exclure,
            duree: PERSONNALISE,
            heures: "2,5".into(),
            purge: 0,
            raison: String::new(),
            surnom: String::new(),
            en_cours: false,
            fait: false,
            erreur: String::new(),
        };
        assert_eq!(e.secondes(), 9000);
        assert!(e.valide());
        e.heures = "700".into();
        assert!(!e.valide());
        e.heures = "x".into();
        assert!(!e.valide());
    }
}
