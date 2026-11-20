use estompe_client::http::ErreurApi;
use estompe_client::recherche::{
    retenir, termes_a_surligner, DemandeRecherche, Tri, AIDE, MARGE_DE_SUITE,
};
use estompe_client::types::MessagePage;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::super::navigation::Navigation;
use super::super::projection::{conversation_de_la_route, serveur_courant};
use super::super::recherche::{
    cle_du_message, en_tete, resultats, vide, Action, Cible, EtatRecherche,
};
use super::Pont;
use crate::{
    DonneesCoquille, DonneesRecherche, EntreePalette, MorceauExtrait, PorteeRecherche, Principale,
    ResultatRecherche,
};

fn modele<T: Clone + 'static>(valeurs: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(valeurs))
}

impl Pont {
    pub fn recherche(&self) -> Option<EtatRecherche> {
        self.verrou().recherche.clone()
    }

    fn modifier_recherche<R>(
        &self,
        modification: impl FnOnce(&mut EtatRecherche) -> R,
    ) -> Option<R> {
        self.verrou().recherche.as_mut().map(modification)
    }

    pub fn ouvrir_recherche(&self, f: &Principale) {
        if self.verrou().recherche.is_some() {
            return;
        }
        let route = self.route();
        let recentes = self
            .memoire
            .as_ref()
            .map(|m| m.lire(|p| p.recherches_recentes.clone()))
            .unwrap_or_default();
        let recherche = self.store.lire(|etat| {
            EtatRecherche::nouvelle(
                etat,
                serveur_courant(&route).map(str::to_string),
                conversation_de_la_route(etat, &route),
                recentes,
            )
        });
        self.verrou().recherche = Some(recherche);
        let g = f.global::<DonneesRecherche>();
        g.set_saisie(SharedString::new());
        self.afficher_recherche(f);
        g.set_ouvert(true);
        g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
    }

    pub fn fermer_recherche(&self, f: &Principale) {
        if self.verrou().recherche.take().is_none() {
            return;
        }
        f.global::<DonneesRecherche>().set_ouvert(false);
        if self.messagerie.peut_ecrire() {
            self.messagerie.saisie().demander_focus(f);
        } else {
            let g = f.global::<DonneesCoquille>();
            g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
        }
    }

    pub fn recherche_modifiee(&self, f: &Principale, texte: &str) {
        let sections = self.verrou().sections.clone();
        self.store
            .lire(|etat| self.modifier_recherche(|r| r.modifier(etat, &sections, texte)));
        self.afficher_recherche(f);
    }

    pub fn naviguer_recherche(&self, f: &Principale, pas: i32) {
        if self.modifier_recherche(|r| r.naviguer(pas)).is_some() {
            self.afficher_recherche(f);
        }
    }

    fn action_de_recherche(&self, index: Option<usize>) -> Option<Action> {
        self.modifier_recherche(|r| {
            index
                .or(r.active)
                .and_then(|i| r.entrees.get(i))
                .map(|e| e.action.clone())
        })
        .flatten()
    }

    pub fn completer_recherche(&self, f: &Principale) {
        if let Some(Action::Completer(texte)) = self.action_de_recherche(None) {
            self.inserer_dans_la_recherche(f, &texte);
        }
    }

    fn inserer_dans_la_recherche(&self, f: &Principale, texte: &str) {
        let g = f.global::<DonneesRecherche>();
        g.set_saisie(texte.into());
        g.set_curseur(texte.len() as i32);
        g.set_demande_curseur(g.get_demande_curseur().wrapping_add(1));
        self.recherche_modifiee(f, texte);
        g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
    }

    pub fn valider_recherche(&self, f: &Principale) {
        match self.modifier_recherche(|r| r.active).flatten() {
            Some(i) => self.choisir_entree(f, i as i32),
            None => self.lancer_recherche(f),
        }
    }

    pub fn choisir_entree(&self, f: &Principale, index: i32) {
        match self.action_de_recherche(Some(usize::try_from(index).unwrap_or(usize::MAX))) {
            Some(Action::Completer(texte)) => self.inserer_dans_la_recherche(f, &texte),
            Some(Action::Aller(cible)) => {
                match cible {
                    Cible::Serveur(serveur) => self.ouvrir_serveur(f, &serveur),
                    Cible::Route(route) => self.naviguer(f, route),
                }
                self.fermer_recherche(f);
            }
            Some(Action::NouveauPrive) => {
                self.verrou().recherche = None;
                f.global::<DonneesRecherche>().set_ouvert(false);
                self.ouvrir_nouveau_prive(f);
            }
            Some(Action::Statut(statut)) => {
                self.definir_statut(statut);
                self.fermer_recherche(f);
            }
            None => {}
        }
    }

    pub fn choisir_portee(&self, f: &Principale, index: i32) {
        let sections = self.verrou().sections.clone();
        let demande = self
            .store
            .lire(|etat| {
                self.modifier_recherche(|r| {
                    r.portee = usize::try_from(index)
                        .unwrap_or(0)
                        .min(r.portees.len().saturating_sub(1));
                    let saisie = r.saisie.clone();
                    r.modifier(etat, &sections, &saisie);
                    let (portee, tri) = (r.portee(), r.tri);
                    r.messages.relancer(portee, tri)
                })
            })
            .flatten();
        self.afficher_recherche(f);
        if let Some(demande) = demande {
            self.demander_recherche(demande);
        }
    }

    pub fn choisir_tri(&self, f: &Principale, par_date: bool) {
        let demande = self
            .modifier_recherche(|r| {
                r.tri = if par_date { Tri::Date } else { Tri::Pertinence };
                let (portee, tri) = (r.portee(), r.tri);
                r.messages.relancer(portee, tri)
            })
            .flatten();
        self.afficher_recherche(f);
        if let Some(demande) = demande {
            self.demander_recherche(demande);
        }
    }

    pub fn lancer_recherche(&self, f: &Principale) {
        let demande = self
            .modifier_recherche(|r| {
                let (saisie, portee, tri) = (r.saisie.clone(), r.portee(), r.tri);
                r.decalage = 0.0;
                r.messages.lancer(&saisie, portee, tri)
            })
            .flatten();
        if let Some(demande) = demande {
            f.global::<DonneesRecherche>().set_decalage(0.0);
            self.afficher_recherche(f);
            self.demander_recherche(demande);
        }
    }

    fn demander_recherche(&self, demande: DemandeRecherche) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            let resultat = pont.store.client().chercher_messages(&demande).await;
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.recherche_recue(&f, demande.sequence, resultat));
        });
    }

    pub fn recherche_recue(
        &self,
        f: &Principale,
        sequence: u64,
        resultat: Result<MessagePage, ErreurApi>,
    ) {
        let recentes = self
            .modifier_recherche(|r| {
                if !r.messages.recevoir(sequence, resultat) || !r.messages.erreur.is_empty() {
                    return None;
                }
                let recentes = retenir(&r.recentes, &r.messages.cherche);
                (recentes != r.recentes).then(|| {
                    r.recentes = recentes.clone();
                    recentes
                })
            })
            .flatten();
        if let (Some(recentes), Some(memoire)) = (recentes, &self.memoire) {
            memoire.modifier(|p| p.recherches_recentes = recentes);
        }
        self.afficher_recherche(f);
    }

    pub fn ouvrir_resultat(&self, f: &Principale, id: &str) {
        let cle = self
            .modifier_recherche(|r| {
                r.messages
                    .resultats
                    .iter()
                    .find(|m| m.id.as_deref() == Some(id))
                    .and_then(cle_du_message)
            })
            .flatten();
        let Some(route) = cle.and_then(|cle| {
            self.store
                .lire(|etat| Navigation::route_de_la_conversation(etat, &cle))
        }) else {
            return;
        };
        self.aller(f, route, Some(id.to_string()));
        self.fermer_recherche(f);
    }

    pub fn chercher_recente(&self, f: &Principale, texte: &str) {
        self.inserer_dans_la_recherche(f, texte);
        self.lancer_recherche(f);
    }

    pub fn effacer_recentes(&self, f: &Principale) {
        self.modifier_recherche(|r| r.recentes.clear());
        if let Some(memoire) = &self.memoire {
            memoire.modifier(|p| p.recherches_recentes.clear());
        }
        self.afficher_recherche(f);
    }

    pub fn defile_recherche(&self, f: &Principale, haut: f32, vue: f32, contenu: f32) {
        let demande = self
            .modifier_recherche(|r| {
                r.vue = vue;
                r.decalage = haut;
                (contenu - haut - vue < MARGE_DE_SUITE)
                    .then(|| r.messages.suite())
                    .flatten()
            })
            .flatten();
        if let Some(demande) = demande {
            self.afficher_recherche(f);
            self.demander_recherche(demande);
        }
    }

    fn afficher_recherche(&self, f: &Principale) {
        let Some(r) = self.recherche() else {
            return;
        };
        let g = f.global::<DonneesRecherche>();
        g.set_portees(modele(
            r.portees
                .iter()
                .map(|p| PorteeRecherche {
                    libelle: p.libelle.as_str().into(),
                    court: p.court.as_str().into(),
                })
                .collect(),
        ));
        g.set_portee(r.portee as i32);
        g.set_par_date(r.tri == Tri::Date);
        g.set_entrees(modele(
            r.entrees
                .iter()
                .map(|e| EntreePalette {
                    titre_groupe: e.titre_groupe.into(),
                    icone: e.icone.into(),
                    libelle: e.libelle.as_str().into(),
                    detail: e.detail.as_str().into(),
                })
                .collect(),
        ));
        g.set_active(r.active.map_or(-1, |i| i as i32));
        g.set_active_filtre(r.active_filtre());
        if r.active.is_some() {
            g.set_decalage(-r.decalage);
        }
        let termes = termes_a_surligner(&r.messages.cherche);
        let decalage = self.messagerie.decalage_minutes();
        let lignes = self
            .store
            .lire(|etat| resultats(etat, &r.messages.resultats, &termes, decalage));
        g.set_resultats(modele(
            lignes
                .into_iter()
                .map(|l| ResultatRecherche {
                    titre_groupe: l.titre_groupe.into(),
                    id: l.id.into(),
                    auteur: l.auteur.into(),
                    date: l.date.into(),
                    morceaux: modele(
                        l.segments
                            .into_iter()
                            .map(|s| MorceauExtrait {
                                texte: s.texte.into(),
                                trouve: s.trouve,
                                espace: s.espace,
                            })
                            .collect(),
                    ),
                    fichiers: l.fichiers.into(),
                })
                .collect(),
        ));
        g.set_en_tete(en_tete(&r.messages).into());
        g.set_en_cours(r.messages.en_cours);
        g.set_encore(r.messages.encore);
        let (vide, conseil) = vide(&r.messages, &r.portee());
        g.set_vide(vide.into());
        g.set_conseil(conseil.into());
        g.set_erreur(r.messages.erreur.as_str().into());
        g.set_recentes(modele(
            r.recentes.iter().map(|s| s.as_str().into()).collect(),
        ));
        g.set_aide(AIDE.into());
    }
}
