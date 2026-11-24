use estompe_client::gestion_roles::{
    etat_de, groupes_du_salon, icone_de_role, libelle_du_role, message_de_perte, nom_de_role,
    permissions_filtrees, surcharge_changee, surcharge_du_role, surcharges_apres, BrouillonRole,
    Contexte, EtatPermission, COULEURS_ROLE, MAX_ROLES,
};
use estompe_client::roles::nom_du_membre;
use estompe_client::store::{texte_du_genre, trier_salons, GenreSalon};
use estompe_client::types::{Override, Role};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::super::projection::{initiales, teinte};
use super::{couleur_slint, Pont};
use crate::confirmation::{self, Demande};
use crate::menu::{self, Ancre, Entree};
use crate::{
    DonneesRoles, GroupePermissionsRole, MembreRole, NuanceRole, PermissionRole, PermissionSalon,
    Principale, RoleReglage, SalonReglage,
};

pub const ROLE_ENREGISTRE: &str = "Rôle enregistré.";
pub const ROLE_SUPPRIME: &str = "Rôle supprimé.";

#[derive(Clone, Debug, Default)]
pub struct EtatRoles {
    pub choisi: Option<String>,
    pub brouillon: Option<BrouillonRole>,
    pub filtre: String,
    pub recherche: String,
    pub salon: Option<String>,
    pub role_salon: Option<String>,
    pub surcharges: Vec<Override>,
}

fn id_de(valeur: &Option<String>) -> &str {
    valeur.as_deref().unwrap_or_default()
}

fn modele<T: Clone + 'static>(lignes: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(lignes))
}

fn icone_du_salon(genre: Option<GenreSalon>) -> &'static str {
    match texte_du_genre(genre) {
        "voice" => "vol",
        "cinema" => "film",
        "videos" => "video",
        "music" => "casque",
        _ => "hash",
    }
}

fn groupe_du_salon(genre: Option<GenreSalon>) -> &'static str {
    match texte_du_genre(genre) {
        "voice" => "SALONS VOCAUX",
        "cinema" => "SALONS CINÉMA",
        "videos" | "music" => "ESQUISSES",
        _ => "SALONS TEXTE",
    }
}

fn membre_role(m: &estompe_client::roles::Membre, gerable: bool) -> MembreRole {
    let id = id_de(&m.id).to_string();
    let nom = nom_du_membre(m).to_string();
    MembreRole {
        initiales: initiales(&nom).as_str().into(),
        teinte: teinte(&id),
        id: id.into(),
        nom: nom.into(),
        gerable,
    }
}

impl Pont {
    fn etat_roles(&self) -> Option<(String, EtatRoles)> {
        self.reglages_serveur().map(|e| (e.serveur, e.roles))
    }

    fn modifier_roles<R>(&self, modification: impl FnOnce(&mut EtatRoles) -> R) -> Option<R> {
        self.modifier_reglages(|e| modification(&mut e.roles))
    }

    fn role_du_serveur(&self, serveur: &str, id: &str) -> Option<Role> {
        self.store.lire(|etat| {
            etat.details
                .get(serveur)
                .and_then(|d| d.roles.as_deref())
                .unwrap_or_default()
                .iter()
                .find(|r| id_de(&r.id) == id)
                .cloned()
        })
    }

    fn poser_brouillon(&self, f: &Principale, brouillon: &BrouillonRole) {
        let g = f.global::<DonneesRoles>();
        g.set_nom(brouillon.nom.as_str().into());
        g.set_icone(brouillon.icone.as_str().into());
        g.set_couleur_hex(brouillon.couleur.to_lowercase().as_str().into());
    }

    pub fn choisir_role(&self, f: &Principale, id: &str) {
        let Some((serveur, _)) = self.etat_roles() else {
            return;
        };
        let Some(role) = self.role_du_serveur(&serveur, id) else {
            return;
        };
        let brouillon = BrouillonRole::depuis(&role);
        self.poser_brouillon(f, &brouillon);
        let g = f.global::<DonneesRoles>();
        g.set_filtre(SharedString::new());
        g.set_recherche(SharedString::new());
        self.modifier_roles(|r| {
            r.choisi = Some(id.to_string());
            r.brouillon = Some(brouillon);
            r.filtre.clear();
            r.recherche.clear();
        });
        self.modifier_reglages(|e| {
            e.erreur.clear();
            e.annonce.clear();
        });
        self.projeter_reglages(f);
    }

    fn modifier_brouillon(&self, f: &Principale, modification: impl FnOnce(&mut BrouillonRole)) {
        self.modifier_roles(|r| {
            if let Some(b) = r.brouillon.as_mut() {
                modification(b);
            }
        });
        self.modifier_reglages(|e| e.annonce.clear());
        self.projeter_reglages(f);
    }

    pub fn nom_de_role_modifie(&self, f: &Principale, texte: &str) {
        let nom = nom_de_role(texte);
        if nom != texte {
            f.global::<DonneesRoles>().set_nom(nom.as_str().into());
        }
        self.modifier_brouillon(f, |b| b.nom = nom);
    }

    pub fn icone_de_role_modifiee(&self, f: &Principale, texte: &str) {
        let icone = icone_de_role(texte);
        if icone != texte {
            f.global::<DonneesRoles>().set_icone(icone.as_str().into());
        }
        self.modifier_brouillon(f, |b| b.icone = icone);
    }

    pub fn couleur_de_role_choisie(&self, f: &Principale, hex: &str) {
        let hex = hex.trim().to_lowercase();
        f.global::<DonneesRoles>()
            .set_couleur_hex(hex.as_str().into());
        self.modifier_brouillon(f, |b| b.couleur = hex);
    }

    pub fn couleur_de_role_modifiee(&self, f: &Principale, texte: &str) {
        let hex: String = texte.trim().chars().take(7).collect();
        self.modifier_brouillon(f, |b| b.couleur = hex);
    }

    pub fn separer_role(&self, f: &Principale, separe: bool) {
        self.modifier_brouillon(f, |b| b.separe = separe);
    }

    pub fn basculer_permission_du_role(&self, f: &Principale, cle: &str, actif: bool) {
        let accordable = self
            .etat_roles()
            .map(|(serveur, _)| {
                self.store.lire(|etat| {
                    Contexte::depuis(etat, &serveur).is_some_and(|c| c.permission_accordable(cle))
                })
            })
            .unwrap_or(false);
        if accordable {
            self.modifier_brouillon(f, |b| b.basculer(cle, actif));
        } else {
            self.projeter_reglages(f);
        }
    }

    pub fn filtrer_permissions_du_role(&self, f: &Principale, texte: &str) {
        self.modifier_roles(|r| r.filtre = texte.to_string());
        self.projeter_reglages(f);
    }

    pub fn chercher_membre_du_role(&self, f: &Principale, texte: &str) {
        self.modifier_roles(|r| r.recherche = texte.to_string());
        self.projeter_reglages(f);
    }

    fn apres_les_roles(
        &self,
        f: &Principale,
        tache: impl std::future::Future<Output = Result<Option<String>, estompe_client::http::ErreurApi>>
            + Send
            + 'static,
        annonce: Option<&'static str>,
    ) {
        let Some(serveur) = self.serveur_des_reglages() else {
            return;
        };
        let store = self.store.clone();
        self.executer(
            f,
            async move {
                let choix = tache.await?;
                store.recharger_serveur(&serveur).await?;
                Ok(choix)
            },
            move |p, f, choix| {
                if let Some(a) = annonce {
                    p.modifier_reglages(|e| e.annonce = a.to_string());
                }
                match choix.as_deref() {
                    Some("") => {
                        p.modifier_roles(|r| {
                            r.choisi = None;
                            r.brouillon = None;
                        });
                    }
                    Some(id) => {
                        let annonce = p.reglages_serveur().map(|e| e.annonce).unwrap_or_default();
                        p.choisir_role(f, id);
                        p.modifier_reglages(|e| e.annonce = annonce);
                    }
                    None => {}
                }
            },
        );
    }

    pub fn creer_role(&self, f: &Principale) {
        let Some(serveur) = self.serveur_des_reglages() else {
            return;
        };
        let client = self.store.client().clone();
        let requete = BrouillonRole::nouveau().requete();
        self.apres_les_roles(
            f,
            async move {
                let role = client.creer_role(&serveur, &requete).await?;
                Ok(Some(role.id.unwrap_or_default()))
            },
            None,
        );
    }

    pub fn enregistrer_role(&self, f: &Principale) {
        let Some((serveur, r)) = self.etat_roles() else {
            return;
        };
        let (Some(id), Some(brouillon)) = (r.choisi, r.brouillon) else {
            return;
        };
        let Some(role) = self.role_du_serveur(&serveur, &id) else {
            return;
        };
        if brouillon.erreur().is_some() || !brouillon.differe(&role) {
            return;
        }
        let Some(pertes) = self.store.lire(|etat| {
            Contexte::depuis(etat, &serveur)
                .filter(|c| c.role_modifiable(&role))
                .map(|c| c.pertes(&id, Some(&brouillon.permissions)))
        }) else {
            return;
        };
        let pont = self.clone();
        let envoyer = move |f: &Principale| {
            let client = pont.store.client().clone();
            let requete = brouillon.requete();
            let s = serveur.clone();
            let i = id.clone();
            pont.apres_les_roles(
                f,
                async move {
                    client.modifier_role(&s, &i, &requete).await?;
                    Ok(Some(i))
                },
                Some(ROLE_ENREGISTRE),
            );
        };
        if pertes.is_empty() {
            envoyer(f);
            return;
        }
        confirmation::confirmer(
            f,
            Demande {
                titre: "Perdre des permissions".into(),
                message: message_de_perte(&pertes),
                action: "Enregistrer quand même".into(),
                destructif: true,
                ..Demande::default()
            },
            move |f, ok| {
                if ok {
                    envoyer(f);
                }
            },
        );
    }

    pub fn supprimer_role(&self, f: &Principale) {
        let Some((serveur, r)) = self.etat_roles() else {
            return;
        };
        let Some(id) = r.choisi else {
            return;
        };
        let Some(role) = self
            .role_du_serveur(&serveur, &id)
            .filter(|r| r.is_default != Some(true))
        else {
            return;
        };
        let Some(pertes) = self.store.lire(|etat| {
            Contexte::depuis(etat, &serveur)
                .filter(|c| c.role_modifiable(&role))
                .map(|c| c.pertes(&id, None))
        }) else {
            return;
        };
        let mut message = format!(
            "Supprimer le rôle « {} » ? Ses membres le perdent aussitôt.",
            role.name.as_deref().unwrap_or_default()
        );
        if !pertes.is_empty() {
            message = format!("{message} {}", message_de_perte(&pertes));
        }
        let pont = self.clone();
        confirmation::confirmer(
            f,
            Demande {
                titre: "Supprimer le rôle".into(),
                message,
                action: "Supprimer le rôle".into(),
                destructif: true,
                ..Demande::default()
            },
            move |f, ok| {
                if !ok {
                    return;
                }
                let client = pont.store.client().clone();
                pont.apres_les_roles(
                    f,
                    async move {
                        client.supprimer_role(&serveur, &id).await?;
                        Ok(Some(String::new()))
                    },
                    Some(ROLE_SUPPRIME),
                );
            },
        );
    }

    pub fn deplacer_role(&self, f: &Principale, de: i32, vers: i32) {
        let Some(serveur) = self.serveur_des_reglages() else {
            return;
        };
        let ordre = self.store.lire(|etat| {
            let c = Contexte::depuis(etat, &serveur)?;
            let n = c.roles_ordonnes().len();
            if n == 0 || de < 0 {
                return None;
            }
            c.nouvel_ordre(de as usize, (vers.max(0) as usize).min(n - 1))
        });
        let Some(ordre) = ordre else {
            self.projeter_reglages(f);
            return;
        };
        let client = self.store.client().clone();
        self.apres_les_roles(
            f,
            async move {
                client.ordonner_roles(&serveur, &ordre).await?;
                Ok(None)
            },
            None,
        );
    }

    fn attribuer(&self, f: &Principale, membre: &str, attribue: bool) {
        let Some((serveur, r)) = self.etat_roles() else {
            return;
        };
        let Some(role) = r.choisi else {
            return;
        };
        let (permis, pertes) = self.store.lire(|etat| {
            Contexte::depuis(etat, &serveur)
                .map(|c| {
                    let permis = c.role(&role).is_some_and(|x| c.role_modifiable(x))
                        && c.membre_gerable(membre);
                    let pertes = if attribue {
                        Vec::new()
                    } else {
                        c.pertes_si_retire(&role, membre)
                    };
                    (permis, pertes)
                })
                .unwrap_or_default()
        });
        if !permis {
            return;
        }
        let pont = self.clone();
        let membre = membre.to_string();
        let envoyer = move |f: &Principale| {
            let client = pont.store.client().clone();
            let (s, m, r) = (serveur.clone(), membre.clone(), role.clone());
            pont.modifier_roles(|x| x.recherche.clear());
            f.global::<DonneesRoles>()
                .set_recherche(SharedString::new());
            pont.apres_les_roles(
                f,
                async move {
                    client.attribuer_role(&s, &m, &r, attribue).await?;
                    Ok(None)
                },
                None,
            );
        };
        if pertes.is_empty() {
            envoyer(f);
            return;
        }
        confirmation::confirmer(
            f,
            Demande {
                titre: "Te retirer ce rôle".into(),
                message: message_de_perte(&pertes),
                action: "Retirer quand même".into(),
                destructif: true,
                ..Demande::default()
            },
            move |f, ok| {
                if ok {
                    envoyer(f);
                }
            },
        );
    }

    pub fn ajouter_membre_au_role(&self, f: &Principale, membre: &str) {
        self.attribuer(f, membre, true);
    }

    pub fn retirer_membre_du_role(&self, f: &Principale, membre: &str) {
        self.attribuer(f, membre, false);
    }

    pub fn creer_salon_depuis_reglages(&self, f: &Principale) {
        if let Some(serveur) = self.serveur_des_reglages() {
            self.ouvrir_creation_salon(f, &serveur, GenreSalon::Text, None);
        }
    }

    pub fn basculer_permissions_du_salon(&self, f: &Principale, salon: &str) {
        let Some((serveur, r)) = self.etat_roles() else {
            return;
        };
        if r.salon.as_deref() == Some(salon) {
            self.modifier_roles(|r| {
                r.salon = None;
                r.surcharges.clear();
            });
            self.projeter_reglages(f);
            return;
        }
        let defaut = self.store.lire(|etat| {
            Contexte::depuis(etat, &serveur)
                .and_then(|c| c.role_par_defaut().or(c.roles_ordonnes().first().copied()))
                .map(|r| id_de(&r.id).to_string())
        });
        self.modifier_roles(|r| {
            r.salon = Some(salon.to_string());
            r.surcharges.clear();
            if r.role_salon.is_none() {
                r.role_salon = defaut;
            }
        });
        self.projeter_reglages(f);
        self.charger_surcharges(f);
    }

    pub(super) fn charger_surcharges(&self, f: &Principale) {
        let _ = f;
        let Some((serveur, r)) = self.etat_roles() else {
            return;
        };
        let Some(salon) = r.salon else {
            return;
        };
        let pont = self.clone();
        self.runtime.spawn(async move {
            let Ok(liste) = pont
                .store
                .client()
                .surcharges_du_salon(&serveur, &salon)
                .await
            else {
                return;
            };
            let p = pont.clone();
            let _ = pont
                .fenetre
                .upgrade_in_event_loop(move |f| p.poser_surcharges(&f, &salon, liste));
        });
    }

    pub fn poser_surcharges(&self, f: &Principale, salon: &str, liste: Vec<Override>) {
        let pose = self
            .modifier_roles(|r| {
                if r.salon.as_deref() != Some(salon) {
                    return false;
                }
                r.surcharges = liste;
                true
            })
            .unwrap_or(false);
        if pose {
            self.projeter_reglages(f);
        }
    }

    pub fn menu_role_du_salon(&self, f: &Principale, ancre: Ancre) -> bool {
        let Some((serveur, r)) = self.etat_roles() else {
            return false;
        };
        let entrees: Vec<Entree> = self.store.lire(|etat| {
            Contexte::depuis(etat, &serveur)
                .map(|c| {
                    c.roles_ordonnes()
                        .into_iter()
                        .chain(c.role_par_defaut())
                        .map(|x| {
                            let id = id_de(&x.id);
                            Entree::nouvelle(id, &libelle_du_role(x), "couronne")
                                .cochee(r.role_salon.as_deref() == Some(id))
                        })
                        .collect()
                })
                .unwrap_or_default()
        });
        let pont = self.clone();
        menu::ouvrir(f, "Rôle", ancre, entrees, move |f, id| {
            pont.choisir_role_du_salon(f, id);
            false
        })
    }

    pub fn choisir_role_du_salon(&self, f: &Principale, role: &str) {
        self.modifier_roles(|r| r.role_salon = Some(role.to_string()));
        self.projeter_reglages(f);
    }

    pub fn definir_permission_du_salon(&self, f: &Principale, cle: &str, rang: i32) {
        let Some((serveur, r)) = self.etat_roles() else {
            return;
        };
        let (Some(salon), Some(role)) = (r.salon.clone(), r.role_salon.clone()) else {
            return;
        };
        let etat = EtatPermission::depuis_rang(rang);
        let actuelle = surcharge_du_role(&r.surcharges, &role);
        if etat_de(actuelle, cle) == etat {
            return;
        }
        let nouvelle = surcharge_changee(actuelle, &role, cle, etat);
        let apres = surcharges_apres(&r.surcharges, &nouvelle);
        let (permis, pertes) = self.store.lire(|e| {
            Contexte::depuis(e, &serveur)
                .map(|c| {
                    let permis = c.permission_accordable(cle)
                        && c.role(&role).is_some_and(|x| c.role_modifiable(x));
                    (permis, c.pertes_dans_salon(&r.surcharges, &apres))
                })
                .unwrap_or_default()
        });
        if !permis {
            return;
        }
        let pont = self.clone();
        let envoyer = move |f: &Principale| {
            let client = pont.store.client().clone();
            let (s, c, n) = (serveur.clone(), salon.clone(), nouvelle.clone());
            pont.executer(
                f,
                async move {
                    client.definir_surcharge(&s, &c, &n).await?;
                    client.surcharges_du_salon(&s, &c).await
                },
                move |p, _, liste| {
                    p.modifier_roles(|r| {
                        if r.salon.as_deref() == Some(salon.as_str()) {
                            r.surcharges = liste;
                        }
                    });
                },
            );
        };
        if pertes.is_empty() {
            envoyer(f);
            return;
        }
        confirmation::confirmer(
            f,
            Demande {
                titre: "Perdre des permissions dans ce salon".into(),
                message: message_de_perte(&pertes),
                action: "Appliquer quand même".into(),
                destructif: true,
                ..Demande::default()
            },
            move |f, ok| {
                if ok {
                    envoyer(f);
                }
            },
        );
    }

    pub(super) fn projeter_roles(&self, f: &Principale) {
        let Some(e) = self.reglages_serveur() else {
            return;
        };
        let r = &e.roles;
        let g = f.global::<DonneesRoles>();
        let projection = self.store.lire(|etat| {
            let c = Contexte::depuis(etat, &e.serveur)?;
            let ordonnes = c.roles_ordonnes();
            let mut lignes: Vec<RoleReglage> = ordonnes
                .iter()
                .map(|x| {
                    let couleur = couleur_slint(x.color.as_deref().filter(|c| !c.is_empty()));
                    RoleReglage {
                        id: id_de(&x.id).into(),
                        libelle: libelle_du_role(x).into(),
                        a_couleur: couleur.is_some(),
                        couleur: couleur.unwrap_or_default(),
                        separe: x.hoist == Some(true),
                        deplacable: c.role_deplacable(x),
                        defaut: false,
                    }
                })
                .collect();
            if let Some(d) = c.role_par_defaut() {
                lignes.push(RoleReglage {
                    id: id_de(&d.id).into(),
                    libelle: d.name.clone().unwrap_or_default().into(),
                    a_couleur: false,
                    couleur: Default::default(),
                    separe: false,
                    deplacable: false,
                    defaut: true,
                });
            }
            let choisi = r.choisi.as_deref().and_then(|id| c.role(id));
            let editeur = choisi.map(|role| {
                let id = id_de(&role.id);
                let modifiable = c.role_modifiable(role);
                let groupes: Vec<GroupePermissionsRole> = permissions_filtrees(&r.filtre)
                    .into_iter()
                    .map(|(titre, permissions)| GroupePermissionsRole {
                        titre: titre.into(),
                        permissions: modele(
                            permissions
                                .into_iter()
                                .map(|p| PermissionRole {
                                    cle: p.cle.into(),
                                    libelle: p.libelle.into(),
                                    description: p.description.into(),
                                    allumee: r.brouillon.as_ref().is_some_and(|b| b.a(p.cle)),
                                    modifiable: c.permission_accordable(p.cle),
                                })
                                .collect(),
                        ),
                    })
                    .collect();
                let membres: Vec<MembreRole> = c
                    .membres_du_role(id)
                    .into_iter()
                    .map(|m| membre_role(m, c.membre_gerable(id_de(&m.id))))
                    .collect();
                let proposes: Vec<MembreRole> = if modifiable {
                    c.membres_proposes(id, &r.recherche)
                        .into_iter()
                        .map(|m| membre_role(m, true))
                        .collect()
                } else {
                    Vec::new()
                };
                (
                    id.to_string(),
                    role.clone(),
                    modifiable,
                    role.is_default == Some(true),
                    groupes,
                    membres,
                    proposes,
                )
            });
            let mut salons: Vec<_> = etat.salons(&e.serveur).to_vec();
            trier_salons(&mut salons);
            let ordre = ["text", "videos", "music", "voice", "cinema"];
            salons.sort_by_key(|s| {
                ordre
                    .iter()
                    .position(|o| *o == texte_du_genre(s.kind))
                    .unwrap_or(ordre.len())
            });
            let mut precedent = "";
            let lignes_salons: Vec<SalonReglage> = salons
                .iter()
                .map(|s| {
                    let groupe = groupe_du_salon(s.kind);
                    let titre = if groupe != precedent { groupe } else { "" };
                    precedent = groupe;
                    let id = id_de(&s.id).to_string();
                    SalonReglage {
                        ouvert: r.salon.as_deref() == Some(id.as_str()),
                        id: id.into(),
                        nom: s.name.clone().unwrap_or_default().into(),
                        icone: icone_du_salon(s.kind).into(),
                        groupe: titre.into(),
                        verrouille: s.locked == Some(true),
                        limite: s
                            .user_limit
                            .filter(|l| *l > 0)
                            .map(|l| format!("{l} max"))
                            .unwrap_or_default()
                            .into(),
                    }
                })
                .collect();
            let salon = r.salon.as_deref().and_then(|id| {
                etat.salons(&e.serveur)
                    .iter()
                    .find(|s| id_de(&s.id) == id)
                    .cloned()
            });
            let role_salon = r.role_salon.as_deref().and_then(|id| c.role(id));
            let permissions_salon: Vec<PermissionSalon> = match (&salon, role_salon) {
                (Some(s), Some(role)) => {
                    let surcharge = surcharge_du_role(&r.surcharges, id_de(&role.id));
                    let gerable = c.role_modifiable(role);
                    groupes_du_salon(s.kind)
                        .into_iter()
                        .flat_map(|g| g.permissions.iter())
                        .map(|p| PermissionSalon {
                            cle: p.cle.into(),
                            libelle: p.libelle.into(),
                            etat: etat_de(surcharge, p.cle).rang(),
                            modifiable: gerable && c.permission_accordable(p.cle),
                        })
                        .collect()
                }
                _ => Vec::new(),
            };
            Some((
                lignes,
                ordonnes.len(),
                editeur,
                lignes_salons,
                role_salon.map(libelle_du_role).unwrap_or_default(),
                permissions_salon,
            ))
        });
        let Some((lignes, nombre, editeur, salons, role_salon, permissions_salon)) = projection
        else {
            return;
        };
        let total = lignes.len();
        g.set_compte(format!("RÔLES · {total}").into());
        g.set_peut_creer(nombre < MAX_ROLES);
        g.set_roles(modele(lignes));
        g.set_en_cours(e.en_cours);
        g.set_palette(modele(
            COULEURS_ROLE
                .iter()
                .map(|hex| NuanceRole {
                    hex: (*hex).into(),
                    couleur: couleur_slint(Some(*hex)).unwrap_or_default(),
                })
                .collect(),
        ));
        match editeur {
            Some((id, role, modifiable, defaut, groupes, membres, proposes)) => {
                let brouillon = r
                    .brouillon
                    .clone()
                    .unwrap_or_else(|| BrouillonRole::depuis(&role));
                let couleur = couleur_slint(Some(&brouillon.couleur));
                g.set_choisi(id.into());
                g.set_modifiable(modifiable);
                g.set_defaut(defaut);
                g.set_a_couleur(couleur.is_some());
                g.set_couleur(couleur.unwrap_or_default());
                g.set_separe(brouillon.separe);
                g.set_groupes(modele(groupes));
                g.set_membres(modele(membres));
                g.set_proposes(modele(proposes));
                g.set_erreur_brouillon(brouillon.erreur().unwrap_or_default().into());
                g.set_modifie(brouillon.differe(&role));
            }
            None => {
                if r.choisi.is_some() {
                    self.modifier_roles(|x| {
                        x.choisi = None;
                        x.brouillon = None;
                    });
                }
                g.set_choisi(SharedString::new());
                g.set_groupes(modele(Vec::new()));
                g.set_membres(modele(Vec::new()));
                g.set_proposes(modele(Vec::new()));
                g.set_modifie(false);
            }
        }
        g.set_salons(modele(salons));
        g.set_role_salon(role_salon.into());
        g.set_permissions_salon(modele(permissions_salon));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nomme_et_regroupe_les_salons() {
        assert_eq!(icone_du_salon(Some(GenreSalon::Voice)), "vol");
        assert_eq!(icone_du_salon(Some(GenreSalon::Music)), "casque");
        assert_eq!(icone_du_salon(None), "hash");
        assert_eq!(groupe_du_salon(Some(GenreSalon::Videos)), "ESQUISSES");
        assert_eq!(groupe_du_salon(Some(GenreSalon::Cinema)), "SALONS CINÉMA");
        assert_eq!(groupe_du_salon(Some(GenreSalon::Text)), "SALONS TEXTE");
    }
}
