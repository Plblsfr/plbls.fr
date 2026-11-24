use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::acces::{
    acteur, categories, cellule, cible, compter_effectifs, correspond_au_filtre, effectifs_par_app,
    exceptions, expire_ou_jamais, extra_journal, initiales, libelle_action, libelle_permission,
    libelles_permissions, message_copie, mot_effet, nom_compte, note_role, obsoletes, origine,
    permissions_actives, permissions_modifiees, pour_qui, quand, qui_a_le_role, ressource_journal,
    resume_compte, resume_effectif, resume_groupe, resume_modifications, teinte, ton_action,
    type_de, types_listables, vers_iso, Catalogue, Compte, CompteDetail, CompteResume, Copie,
    Decision, Detenteur, EffectifsExpliques, Effet, EntreeJournal, Etat, FiltreEffectifs,
    GroupeDetail, GroupeResume, LiaisonVue, ModeCopie, NomsRessources, NouvelleLiaison,
    NouvelleRegle, PageComptes, PermissionVue, RegleSaisie, RegleVue, Ressource, RessourceJournal,
    RoleDetail, RoleResume, Sujet, Systeme, ACTIONS, ECHEC, FILTRES_EFFECTIFS, LIMITE_CHOIX_COMPTE,
    LIMITE_HISTORIQUE, LIMITE_JOURNAL, ONGLETS, PAGE_COMPTES,
};
use estompe_client::creation::message_d_erreur;
use estompe_client::http::{Client, ErreurApi};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use super::admin::Decalage;
use crate::confirmation::{self, Demande};
use crate::menu::{self, Ancre, Entree};
use crate::{
    CaseRoleAcces, ChargementAcces, ColonneRoleAcces, DetenteurAcces, DonneesAcces, ExceptionAcces,
    LigneCatalogueAcces, LigneCompteAcces, LigneDroitAcces, LigneGrilleAcces, LigneJournalAcces,
    LigneLiaisonAcces, LigneMembreAcces, LigneQuiAcces, LigneRegleAcces, LigneRegleRoleAcces,
    MaillonAcces, OptionAcces, Principale, PuceAcces, RangeePucesAcces,
};

pub const ATTENTE_SAISIE: Duration = Duration::from_millis(250);
pub const HAUTEUR_COMPTE: f32 = 52.0;
pub const RESERVE_DEFILEMENT: f32 = 20.0 * HAUTEUR_COMPTE;
const PERMISSIONS_PROPOSEES: usize = 8;

#[derive(Debug, Clone)]
struct Charge<T> {
    donnees: Option<T>,
    erreur: String,
}

impl<T> Default for Charge<T> {
    fn default() -> Self {
        Self {
            donnees: None,
            erreur: String::new(),
        }
    }
}

impl<T> Charge<T> {
    fn pret(donnees: T) -> Self {
        Self {
            donnees: Some(donnees),
            erreur: String::new(),
        }
    }

    fn etat(&self) -> ChargementAcces {
        ChargementAcces {
            charge: self.donnees.is_some(),
            erreur: self.erreur.clone().into(),
        }
    }

    fn recevoir(&mut self, r: Result<T, ErreurApi>) {
        match r {
            Ok(d) => *self = Self::pret(d),
            Err(e) => {
                self.donnees = None;
                self.erreur = erreur_lisible(&e);
            }
        }
    }
}

fn erreur_lisible(e: &ErreurApi) -> String {
    match e {
        ErreurApi::Reseau(_) | ErreurApi::DelaiDepasse => message_d_erreur(e),
        ErreurApi::ReponseInvalide(_) | ErreurApi::RequeteInvalide(_) => ECHEC.into(),
        _ if !e.message().is_empty() => e.message().to_string(),
        _ => ECHEC.into(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ActionPuce {
    Liaison(LiaisonVue),
    Groupe(String, String),
    Regle(RegleVue),
}

#[derive(Debug, Clone, Default)]
struct Fiche {
    sub: String,
    detail: Charge<CompteDetail>,
    effectifs: Charge<EffectifsExpliques>,
    historique: bool,
    journal: Charge<Vec<EntreeJournal>>,
    filtre: FiltreEffectifs,
    ouverte: Option<String>,
    essai: Option<Ressource>,
    decision: Option<Decision>,
    source: Option<Compte>,
    mode: ModeCopie,
}

#[derive(Debug, Clone, Default)]
struct Editeur {
    id: String,
    role: Charge<RoleDetail>,
    joker: BTreeMap<String, Effet>,
    precises: Vec<RegleSaisie>,
    depart: Vec<RegleSaisie>,
    ajout_pour: Option<String>,
    ajout_effet: Effet,
    cible: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct FormLiaison {
    role: Option<RoleResume>,
    portee: Option<(String, String)>,
    ressource: Option<Ressource>,
}

#[derive(Debug, Clone, Default)]
struct FormRegle {
    permission: Option<String>,
    refus: bool,
    ressource: Option<Ressource>,
    recherche: String,
}

#[derive(Debug, Default)]
struct EtatAcces {
    generation: u64,
    onglet: &'static str,
    message: String,
    erreur: String,
    en_cours: bool,
    catalogue: Charge<Catalogue>,
    noms: HashMap<String, PermissionVue>,
    ressources: NomsRessources,
    noms_demandes: HashSet<(String, String)>,
    recherche: String,
    jeton_recherche: u64,
    comptes: Charge<Vec<CompteResume>>,
    total: i64,
    from_keycloak: i64,
    suite_en_cours: bool,
    version_comptes: u64,
    version_projetee: u64,
    choisi_projete: Option<String>,
    fiche: Option<Fiche>,
    formulaire: String,
    liaison: FormLiaison,
    regle: FormRegle,
    groupes_libres: Charge<Vec<GroupeResume>>,
    groupe_libre: Option<GroupeResume>,
    contexte_compte: &'static str,
    comptes_trouves: Option<PageComptes>,
    jeton_compte: u64,
    contexte_ressource: Option<(&'static str, String)>,
    ressources_trouvees: Option<Vec<Ressource>>,
    jeton_ressource: u64,
    roles: Charge<Vec<RoleResume>>,
    details: HashMap<String, RoleDetail>,
    app: usize,
    editeur: Option<Editeur>,
    groupes: Charge<Vec<GroupeResume>>,
    groupe_choisi: Option<String>,
    groupe: Charge<GroupeDetail>,
    depliee: Option<String>,
    detenteurs: Charge<Vec<Detenteur>>,
    action: String,
    journal: Charge<Vec<EntreeJournal>>,
    largeur_puces: f32,
}

#[derive(Default)]
pub struct DonneesAccesChargees {
    pub onglet: &'static str,
    pub catalogue: Option<Catalogue>,
    pub comptes: Option<PageComptes>,
    pub compte: Option<(CompteDetail, EffectifsExpliques)>,
    pub roles: Option<Vec<RoleDetail>>,
    pub role: Option<String>,
    pub groupes: Option<Vec<GroupeResume>>,
    pub groupe: Option<GroupeDetail>,
    pub journal: Option<Vec<EntreeJournal>>,
    pub ressources: Vec<(String, String, String)>,
    pub erreur_comptes: Option<String>,
}

#[derive(Clone)]
pub struct PontAcces {
    interne: Arc<Mutex<EtatAcces>>,
    client: Client,
    fenetre: slint::Weak<Principale>,
    runtime: tokio::runtime::Handle,
    decalage: Decalage,
}

fn modele<T: Clone + 'static>(lignes: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(lignes))
}

fn choix(id: &str, libelle: &str, note: &str) -> OptionAcces {
    OptionAcces {
        id: id.into(),
        libelle: libelle.into(),
        note: note.into(),
    }
}

fn largeur_estimee(p: &PuceAcces) -> f32 {
    let lettres = p.cle.chars().count() + p.nom.chars().count() + p.details.chars().count();
    let pieces = [&p.cle, &p.details]
        .iter()
        .filter(|t| !t.is_empty())
        .count();
    20.0 + 7.2 * lettres as f32 + 6.0 * pieces as f32 + if p.retirable { 32.0 } else { 0.0 }
}

pub fn ranger_puces(puces: Vec<PuceAcces>, largeur: f32) -> Vec<Vec<PuceAcces>> {
    let mut rangees: Vec<Vec<PuceAcces>> = Vec::new();
    let mut courante = 0.0;
    for p in puces {
        let l = largeur_estimee(&p);
        match rangees.last_mut() {
            Some(r) if courante + 8.0 + l <= largeur.max(1.0) => {
                courante += 8.0 + l;
                r.push(p);
            }
            _ => {
                courante = l;
                rangees.push(vec![p]);
            }
        }
    }
    rangees
}

impl PontAcces {
    pub fn nouveau(
        client: Client,
        fenetre: slint::Weak<Principale>,
        runtime: tokio::runtime::Handle,
        decalage: Decalage,
    ) -> Self {
        Self {
            interne: Arc::new(Mutex::new(EtatAcces {
                onglet: "comptes",
                largeur_puces: 600.0,
                ..EtatAcces::default()
            })),
            client,
            fenetre,
            runtime,
            decalage,
        }
    }

    fn verrou(&self) -> MutexGuard<'_, EtatAcces> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn decalage(&self) -> i32 {
        (self.decalage)()
    }

    pub fn onglet(&self) -> &'static str {
        self.verrou().onglet
    }

    pub fn nombre_de_comptes(&self) -> usize {
        self.verrou().comptes.donnees.as_ref().map_or(0, Vec::len)
    }

    pub fn chargement_en_cours(&self) -> bool {
        self.verrou().suite_en_cours
    }

    pub fn oublier(&self) {
        let mut e = self.verrou();
        e.generation += 1;
        e.suite_en_cours = false;
        e.en_cours = false;
    }

    pub fn ouvrir(&self, f: &Principale) {
        let manque_catalogue = {
            let mut e = self.verrou();
            e.generation += 1;
            e.message.clear();
            e.erreur.clear();
            e.suite_en_cours = false;
            e.en_cours = false;
            e.catalogue.donnees.is_none()
        };
        if manque_catalogue {
            self.charger_catalogue();
        }
        self.charger_onglet(f);
        self.projeter(f);
    }

    fn lancer<T: Send + 'static>(
        &self,
        tache: impl Future<Output = Result<T, ErreurApi>> + Send + 'static,
        suite: impl FnOnce(&PontAcces, &Principale, Result<T, ErreurApi>) + Send + 'static,
    ) {
        let generation = self.verrou().generation;
        let pont = self.clone();
        self.runtime.spawn(async move {
            let resultat = tache.await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                if p.verrou().generation != generation {
                    return;
                }
                suite(&p, &f, resultat);
                p.projeter(&f);
            });
        });
    }

    fn agir<T: Send + 'static>(
        &self,
        f: &Principale,
        tache: impl Future<Output = Result<T, ErreurApi>> + Send + 'static,
        fait: &'static str,
        suite: impl FnOnce(&PontAcces, &Principale, T) + Send + 'static,
    ) {
        {
            let mut e = self.verrou();
            if e.en_cours {
                return;
            }
            e.en_cours = true;
            e.message.clear();
            e.erreur.clear();
        }
        self.projeter(f);
        let pont = self.clone();
        self.runtime.spawn(async move {
            let resultat = tache.await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                p.verrou().en_cours = false;
                match resultat {
                    Ok(v) => {
                        if !fait.is_empty() {
                            p.verrou().message = fait.into();
                        }
                        suite(&p, &f, v);
                    }
                    Err(e) => p.verrou().erreur = erreur_lisible(&e),
                }
                p.projeter(&f);
            });
        });
    }

    fn confirmer(
        &self,
        f: &Principale,
        titre: &str,
        message: String,
        action: &str,
        suite: impl FnOnce(&PontAcces, &Principale) + 'static,
    ) {
        let pont = self.clone();
        confirmation::confirmer(
            f,
            Demande {
                titre: titre.into(),
                message,
                action: action.into(),
                destructif: true,
                ..Demande::default()
            },
            move |f, ok| {
                if ok {
                    suite(&pont, f);
                }
            },
        );
    }

    fn charger_catalogue(&self) {
        self.verrou().catalogue.erreur.clear();
        let client = self.client.clone();
        self.lancer(async move { client.catalogue_acces().await }, |p, f, r| {
            let mut e = p.verrou();
            e.catalogue.recevoir(r);
            e.noms = e
                .catalogue
                .donnees
                .as_ref()
                .map(libelles_permissions)
                .unwrap_or_default();
            drop(e);
            p.demander_noms(f);
        });
    }

    fn charger_onglet(&self, f: &Principale) {
        match self.onglet() {
            "comptes" => {
                self.charger_comptes(false);
                if self.verrou().fiche.is_some() {
                    self.recharger_fiche();
                }
            }
            "roles" => {
                self.charger_roles();
                if self.verrou().editeur.is_some() {
                    self.charger_role();
                }
            }
            "groupes" => {
                self.charger_groupes();
                if self.verrou().groupe_choisi.is_some() {
                    self.charger_groupe();
                }
            }
            "catalogue" => {
                if self.verrou().depliee.is_some() {
                    self.charger_detenteurs();
                }
            }
            "journal" => self.charger_journal(),
            _ => {}
        }
        let _ = f;
    }

    pub fn choisir_onglet(&self, f: &Principale, id: &str) {
        let Some((retenu, _)) = ONGLETS.iter().find(|(o, _)| *o == id) else {
            return;
        };
        {
            let mut e = self.verrou();
            e.onglet = retenu;
            e.message.clear();
            e.erreur.clear();
            e.formulaire.clear();
            e.contexte_ressource = None;
        }
        self.charger_onglet(f);
        self.projeter(f);
    }

    pub fn deplacer_onglet(&self, f: &Principale, pas: i32) {
        let actuel = self.onglet();
        let n = ONGLETS.len() as i32;
        let i = ONGLETS.iter().position(|(o, _)| *o == actuel).unwrap_or(0) as i32;
        self.choisir_onglet(f, ONGLETS[(i + pas).rem_euclid(n) as usize].0);
    }

    pub fn recharger(&self, f: &Principale, cle: &str) {
        match cle {
            "catalogue" => self.charger_catalogue(),
            "comptes" => self.charger_comptes(false),
            "fiche" | "effectifs" => self.recharger_fiche(),
            "historique" => self.charger_historique(),
            "groupes-libres" => self.charger_groupes_libres(),
            "roles" => self.charger_roles(),
            "role" => self.charger_role(),
            "groupes" => self.charger_groupes(),
            "groupe" => self.charger_groupe(),
            "detenteurs" => self.charger_detenteurs(),
            "journal" => self.charger_journal(),
            _ => {}
        }
        self.projeter(f);
    }

    fn charger_comptes(&self, suite: bool) {
        let (recherche, decalage) = {
            let mut e = self.verrou();
            if suite {
                if e.suite_en_cours {
                    return;
                }
                e.suite_en_cours = true;
            } else {
                e.comptes.erreur.clear();
                e.suite_en_cours = false;
            }
            let n = if suite {
                e.comptes.donnees.as_ref().map_or(0, Vec::len)
            } else {
                0
            };
            (e.recherche.clone(), n)
        };
        let client = self.client.clone();
        let cherche = recherche.clone();
        self.lancer(
            async move { client.comptes_acces(&cherche, PAGE_COMPTES, decalage).await },
            move |p, _, r| {
                let mut e = p.verrou();
                if e.recherche != recherche {
                    return;
                }
                e.suite_en_cours = false;
                match r {
                    Ok(page) => {
                        e.total = page.total;
                        if decalage == 0 {
                            e.from_keycloak = page.from_keycloak;
                            e.comptes = Charge::pret(page.accounts);
                        } else if let Some(l) = e.comptes.donnees.as_mut() {
                            if l.len() == decalage {
                                l.extend(page.accounts);
                            }
                        }
                        e.version_comptes += 1;
                    }
                    Err(err) if decalage == 0 => {
                        e.comptes.donnees = None;
                        e.comptes.erreur = erreur_lisible(&err);
                        e.version_comptes += 1;
                    }
                    Err(err) => e.erreur = erreur_lisible(&err),
                }
            },
        );
    }

    pub fn chercher_comptes(&self, f: &Principale, texte: &str) {
        let jeton = {
            let mut e = self.verrou();
            e.jeton_recherche += 1;
            e.jeton_recherche
        };
        let t = texte.trim().to_string();
        let pont = self.clone();
        self.runtime.spawn(async move {
            tokio::time::sleep(ATTENTE_SAISIE).await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                {
                    let mut e = p.verrou();
                    if e.jeton_recherche != jeton || e.recherche == t {
                        return;
                    }
                    e.recherche = t;
                    e.comptes = Charge::default();
                    e.version_comptes += 1;
                }
                f.global::<DonneesAcces>().set_defilement_comptes(0.0);
                p.charger_comptes(false);
                p.projeter(&f);
            });
        });
        let _ = f;
    }

    pub fn comptes_defiles(&self, f: &Principale, defilement: f32, vue: f32) {
        let doit = {
            let e = self.verrou();
            let n = e.comptes.donnees.as_ref().map_or(0, Vec::len);
            !e.suite_en_cours
                && (n as i64) < e.total
                && n > 0
                && defilement + vue + RESERVE_DEFILEMENT >= n as f32 * HAUTEUR_COMPTE
        };
        if doit {
            self.charger_comptes(true);
            self.projeter(f);
        }
    }

    pub fn ouvrir_compte(&self, f: &Principale, sub: &str) {
        {
            let mut e = self.verrou();
            if e.fiche.as_ref().is_some_and(|x| x.sub == sub) {
                return;
            }
            e.fiche = Some(Fiche {
                sub: sub.to_string(),
                ..Fiche::default()
            });
            e.formulaire.clear();
            e.contexte_ressource = None;
            e.message.clear();
        }
        self.recharger_fiche();
        self.projeter(f);
    }

    pub fn fermer_compte(&self, f: &Principale) {
        {
            let mut e = self.verrou();
            e.fiche = None;
            e.formulaire.clear();
            e.contexte_ressource = None;
        }
        self.projeter(f);
    }

    fn sub_de_la_fiche(&self) -> Option<String> {
        self.verrou().fiche.as_ref().map(|x| x.sub.clone())
    }

    fn recharger_fiche(&self) {
        let Some(sub) = self.sub_de_la_fiche() else {
            return;
        };
        let historique = {
            let mut e = self.verrou();
            let Some(x) = e.fiche.as_mut() else {
                return;
            };
            x.detail.erreur.clear();
            x.effectifs.erreur.clear();
            x.historique
        };
        let client = self.client.clone();
        let s = sub.clone();
        self.lancer(
            async move { client.compte_acces(&s).await },
            move |p, f, r| {
                if let Some(x) = p.verrou().fiche.as_mut().filter(|x| x.sub == sub) {
                    x.detail.recevoir(r);
                }
                p.demander_noms(f);
            },
        );
        let client = self.client.clone();
        let sub = self.sub_de_la_fiche().unwrap_or_default();
        let s = sub.clone();
        self.lancer(
            async move { client.effectifs_acces(&s).await },
            move |p, f, r| {
                if let Some(x) = p.verrou().fiche.as_mut().filter(|x| x.sub == sub) {
                    x.effectifs.recevoir(r);
                }
                p.demander_noms(f);
            },
        );
        if historique {
            self.charger_historique();
        }
    }

    fn charger_historique(&self) {
        let Some(sub) = self.sub_de_la_fiche() else {
            return;
        };
        if let Some(x) = self.verrou().fiche.as_mut() {
            x.journal.erreur.clear();
        }
        let client = self.client.clone();
        let s = sub.clone();
        self.lancer(
            async move { client.journal_acces("", Some(&s), LIMITE_HISTORIQUE).await },
            move |p, f, r| {
                if let Some(x) = p.verrou().fiche.as_mut().filter(|x| x.sub == sub) {
                    x.journal.recevoir(r);
                }
                p.demander_noms(f);
            },
        );
    }

    fn apres_modification_du_compte(&self) {
        {
            let mut e = self.verrou();
            e.formulaire.clear();
            e.contexte_ressource = None;
            e.liaison = FormLiaison::default();
            e.regle = FormRegle::default();
            e.groupe_libre = None;
        }
        self.recharger_fiche();
        self.charger_comptes(false);
    }

    pub fn basculer_historique(&self, f: &Principale) {
        let charger = {
            let mut e = self.verrou();
            let Some(x) = e.fiche.as_mut() else {
                return;
            };
            x.historique = !x.historique;
            x.historique
        };
        if charger {
            self.charger_historique();
        }
        self.projeter(f);
    }

    pub fn basculer_formulaire(&self, f: &Principale, quel: &str) {
        let ouvert = {
            let mut e = self.verrou();
            if e.formulaire == quel {
                e.formulaire.clear();
            } else {
                e.formulaire = quel.to_string();
            }
            e.liaison = FormLiaison::default();
            e.regle = FormRegle::default();
            e.groupe_libre = None;
            e.contexte_ressource = None;
            e.comptes_trouves = None;
            if let Some(x) = e.fiche.as_mut() {
                x.source = None;
                x.mode = ModeCopie::Ajouter;
            }
            e.formulaire.clone()
        };
        let g = f.global::<DonneesAcces>();
        g.set_liaison_expire("".into());
        g.set_regle_expire("".into());
        g.set_regle_note("".into());
        g.set_regle_recherche("".into());
        g.set_groupe_expire("".into());
        g.set_recherche_compte("".into());
        g.set_recherche_ressource("".into());
        match ouvert.as_str() {
            "role" => {
                if self.verrou().roles.donnees.is_none() {
                    self.charger_roles();
                }
            }
            "groupe" => self.charger_groupes_libres(),
            "regle" => self.chercher_permissions(f, ""),
            "copie" => self.verrou().contexte_compte = "copie",
            "donner" => self.verrou().contexte_compte = "donner",
            _ => {}
        }
        self.projeter(f);
    }

    fn charger_groupes_libres(&self) {
        self.verrou().groupes_libres = Charge::default();
        let client = self.client.clone();
        self.lancer(async move { client.groupes_acces().await }, |p, _, r| {
            p.verrou().groupes_libres.recevoir(r)
        });
    }

    fn puces(&self, e: &EtatAcces) -> Vec<(PuceAcces, Option<ActionPuce>)> {
        let Some(d) = e.fiche.as_ref().and_then(|x| x.detail.donnees.as_ref()) else {
            return Vec::new();
        };
        let dec = self.decalage();
        let jusqu = |x: &Option<String>| {
            x.as_deref()
                .map(|d| format!("jusqu'au {}", quand(Some(d), dec)))
        };
        let joindre = |morceaux: Vec<Option<String>>| {
            morceaux
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ")
        };
        let mut puces = vec![(
            PuceAcces {
                genre: "base".into(),
                nom: "Tout le monde".into(),
                ..PuceAcces::default()
            },
            None,
        )];
        for l in &d.bindings {
            let portee = l
                .scope_id
                .as_deref()
                .map(|s| format!("sur {}", e.ressources.nom(l.scope_type.as_deref(), s)));
            puces.push((
                PuceAcces {
                    genre: "role".into(),
                    cle: "rôle".into(),
                    nom: l.role.clone().into(),
                    details: joindre(vec![portee, jusqu(&l.expires_at)]).into(),
                    retirable: true,
                    ..PuceAcces::default()
                },
                Some(ActionPuce::Liaison(l.clone())),
            ));
        }
        for g in &d.groups {
            puces.push((
                PuceAcces {
                    genre: "groupe".into(),
                    cle: "groupe".into(),
                    nom: g.name.clone().into(),
                    details: joindre(vec![jusqu(&g.expires_at)]).into(),
                    retirable: true,
                    ..PuceAcces::default()
                },
                Some(ActionPuce::Groupe(g.group_id.clone(), g.name.clone())),
            ));
        }
        for l in &d.group_bindings {
            puces.push((
                PuceAcces {
                    genre: "herite".into(),
                    cle: "rôle".into(),
                    nom: l.role.clone().into(),
                    details: format!("via {}", l.subject_label.as_deref().unwrap_or("un groupe"))
                        .into(),
                    note: "Hérité du groupe : se règle dans l'onglet Groupes".into(),
                    ..PuceAcces::default()
                },
                None,
            ));
        }
        for r in &d.rules {
            let portee = (r.resource != "*").then(|| {
                format!(
                    "sur {}",
                    e.ressources
                        .nom(type_de(&e.noms, &r.permission).as_deref(), &r.resource)
                )
            });
            let accord = r.effect == Effet::Allow;
            puces.push((
                PuceAcces {
                    genre: if accord { "accord" } else { "refus" }.into(),
                    cle: if accord {
                        "accord direct"
                    } else {
                        "refus direct"
                    }
                    .into(),
                    nom: libelle_permission(&e.noms, &r.permission).into(),
                    details: joindre(vec![portee, jusqu(&r.expires_at)]).into(),
                    note: r.note.clone().into(),
                    retirable: true,
                    obsolete: r.obsolete,
                    ..PuceAcces::default()
                },
                Some(ActionPuce::Regle(r.clone())),
            ));
        }
        for (i, (p, _)) in puces.iter_mut().enumerate() {
            p.index = i as i32;
        }
        puces
    }

    pub fn puces_mesurees(&self, f: &Principale, largeur: f32) {
        let change = {
            let mut e = self.verrou();
            let change = (e.largeur_puces - largeur).abs() > 1.0;
            e.largeur_puces = largeur;
            change
        };
        if change {
            self.projeter(f);
        }
    }

    pub fn retirer_puce(&self, f: &Principale, i: usize) {
        let action = {
            let e = self.verrou();
            self.puces(&e).get(i).and_then(|(_, a)| a.clone())
        };
        let Some(sub) = self.sub_de_la_fiche() else {
            return;
        };
        match action {
            Some(ActionPuce::Liaison(l)) => self.confirmer(
                f,
                "Retirer le rôle",
                format!("Retirer le rôle « {} » à ce compte ?", l.role),
                "Retirer",
                move |p, f| {
                    let client = p.client.clone();
                    p.agir(
                        f,
                        async move { client.supprimer_liaison(&l.id).await },
                        "Rôle retiré.",
                        |p, _, ()| p.apres_modification_du_compte(),
                    );
                },
            ),
            Some(ActionPuce::Groupe(gid, nom)) => self.confirmer(
                f,
                "Retirer du groupe",
                format!("Retirer ce compte du groupe « {nom} » ?"),
                "Retirer",
                move |p, f| {
                    let client = p.client.clone();
                    p.agir(
                        f,
                        async move { client.retirer_du_groupe(&gid, &sub).await },
                        "Retiré du groupe.",
                        |p, _, ()| p.apres_modification_du_compte(),
                    );
                },
            ),
            Some(ActionPuce::Regle(r)) => {
                let nom = libelle_permission(&self.verrou().noms, &r.permission);
                let verbe = if r.effect == Effet::Allow {
                    "Retirer"
                } else {
                    "Lever le refus de"
                };
                self.confirmer(
                    f,
                    "Supprimer la règle",
                    format!("{verbe} « {nom} » ?"),
                    "Supprimer",
                    move |p, f| {
                        let client = p.client.clone();
                        p.agir(
                            f,
                            async move { client.supprimer_regle(&r.id).await },
                            "Règle supprimée.",
                            |p, _, ()| p.apres_modification_du_compte(),
                        );
                    },
                )
            }
            None => {}
        }
    }

    pub fn menu_compte(&self, f: &Principale, x: f32, haut: f32, bas: f32) {
        let Some(sub) = self.sub_de_la_fiche() else {
            return;
        };
        let pont = self.clone();
        menu::ouvrir(
            f,
            "Autres actions",
            Ancre::Element { x, haut, bas },
            vec![
                Entree::nouvelle("copier", "Copier l'identifiant", "copie"),
                Entree::nouvelle("purger", "Tout retirer…", "trash")
                    .danger()
                    .separee(),
            ],
            move |f, id| {
                match id {
                    "copier" => {
                        menu::copier_texte(f, &sub);
                        pont.verrou().message = "Identifiant copié.".into();
                        pont.projeter(f);
                    }
                    "purger" => pont.purger_compte(f),
                    _ => {}
                }
                false
            },
        );
    }

    pub fn purger_compte(&self, f: &Principale) {
        let Some(sub) = self.sub_de_la_fiche() else {
            return;
        };
        let nom = {
            let e = self.verrou();
            nom_compte(
                e.fiche
                    .as_ref()
                    .and_then(|x| x.detail.donnees.as_ref())
                    .map(|d| &d.account),
                &sub,
            )
        };
        self.confirmer(
            f,
            "Retirer tous les droits",
            format!("Retirer à {nom} tous ses rôles, groupes et règles directes ? Il gardera ce que donne « Tout le monde »."),
            "Tout retirer",
            move |p, f| {
                let client = p.client.clone();
                p.agir(
                    f,
                    async move { client.retirer_droits(&sub).await },
                    "Droits retirés.",
                    |p, _, ()| p.apres_modification_du_compte(),
                );
            },
        );
    }

    pub fn choisir_filtre(&self, f: &Principale, i: usize) {
        if let Some(x) = self.verrou().fiche.as_mut() {
            x.filtre = FILTRES_EFFECTIFS.get(i).copied().unwrap_or_default();
        }
        self.projeter(f);
    }

    pub fn afficher_heritees(&self, f: &Principale) {
        self.choisir_filtre(f, 3);
    }

    pub fn pourquoi(&self, f: &Principale, permission: &str) {
        let ouvrir = {
            let mut e = self.verrou();
            let type_ = type_de(&e.noms, permission);
            let Some(x) = e.fiche.as_mut() else {
                return;
            };
            x.essai = None;
            x.decision = None;
            x.ouverte = if x.ouverte.as_deref() == Some(permission) {
                None
            } else {
                Some(permission.to_string())
            };
            let ouvrir = x.ouverte.is_some();
            e.contexte_ressource = match (ouvrir, type_) {
                (true, Some(t)) => Some(("essai", t)),
                _ => None,
            };
            e.ressources_trouvees = None;
            ouvrir
        };
        f.global::<DonneesAcces>()
            .set_recherche_ressource("".into());
        if ouvrir {
            self.simuler(None);
            self.chercher_ressource(f, "");
        }
        self.projeter(f);
    }

    fn simuler(&self, ressource: Option<String>) {
        let Some((sub, permission)) = self
            .verrou()
            .fiche
            .as_ref()
            .and_then(|x| x.ouverte.clone().map(|p| (x.sub.clone(), p)))
        else {
            return;
        };
        let client = self.client.clone();
        let perm = permission.clone();
        self.lancer(
            async move {
                client
                    .simuler_acces(&sub, &perm, ressource.as_deref())
                    .await
            },
            move |p, _, r| {
                let mut e = p.verrou();
                match r {
                    Ok(d) => {
                        if let Some(x) = e
                            .fiche
                            .as_mut()
                            .filter(|x| x.ouverte.as_deref() == Some(permission.as_str()))
                        {
                            x.decision = Some(d);
                        }
                    }
                    Err(err) => e.erreur = erreur_lisible(&err),
                }
            },
        );
    }

    pub fn essai_partout(&self, f: &Principale) {
        if let Some(x) = self.verrou().fiche.as_mut() {
            x.essai = None;
            x.decision = None;
        }
        self.simuler(None);
        self.projeter(f);
    }

    pub fn copier(&self, f: &Principale) {
        let Some((sub, source, mode)) = self
            .verrou()
            .fiche
            .as_ref()
            .and_then(|x| x.source.clone().map(|s| (x.sub.clone(), s, x.mode)))
        else {
            return;
        };
        let client = self.client.clone();
        let (s, d) = (sub.clone(), source.sub.clone());
        self.agir(
            f,
            async move { client.copier_droits(&s, &d, mode, true).await },
            "",
            move |p, f, essai: Copie| {
                let nom = nom_compte(Some(&source), &source.sub);
                let pont = p.clone();
                confirmation::confirmer(
                    f,
                    Demande {
                        titre: "Copier les droits".into(),
                        message: message_copie(&essai, &nom, mode),
                        action: "Copier".into(),
                        destructif: mode == ModeCopie::Remplacer,
                        ..Demande::default()
                    },
                    move |f, ok| {
                        if !ok {
                            return;
                        }
                        let client = pont.client.clone();
                        pont.agir(
                            f,
                            async move { client.copier_droits(&sub, &source.sub, mode, false).await },
                            "Droits copiés.",
                            |p, _, _| p.apres_modification_du_compte(),
                        );
                    },
                );
            },
        );
    }

    pub fn changer_source(&self, f: &Principale) {
        if let Some(x) = self.verrou().fiche.as_mut() {
            x.source = None;
        }
        self.projeter(f);
    }

    pub fn donner_liaison(&self, f: &Principale) {
        let (liaison, sujet, sujet_id) = {
            let e = self.verrou();
            let (sujet, id) = if e.onglet == "groupes" {
                (Sujet::Group, e.groupe_choisi.clone())
            } else {
                (Sujet::Account, e.fiche.as_ref().map(|x| x.sub.clone()))
            };
            (e.liaison.clone(), sujet, id)
        };
        let (Some(role), Some(sujet_id)) = (liaison.role, sujet_id) else {
            return;
        };
        let portee = match (liaison.portee, liaison.ressource) {
            (Some((t, _)), Some(r)) => Some((t, r.id)),
            (Some(_), None) => return,
            _ => None,
        };
        let expire = match vers_iso(
            &f.global::<DonneesAcces>().get_liaison_expire(),
            self.decalage(),
        ) {
            Ok(x) => x,
            Err(m) => {
                self.verrou().erreur = m;
                self.projeter(f);
                return;
            }
        };
        let l = NouvelleLiaison {
            role_id: role.id,
            sujet,
            sujet_id,
            portee,
            expire,
        };
        let client = self.client.clone();
        self.agir(
            f,
            async move { client.creer_liaison(&l).await },
            "Rôle donné.",
            move |p, f, ()| {
                f.global::<DonneesAcces>().set_liaison_expire("".into());
                p.apres_ajout(sujet);
            },
        );
    }

    fn apres_ajout(&self, sujet: Sujet) {
        if sujet == Sujet::Group {
            {
                let mut e = self.verrou();
                e.formulaire.clear();
                e.contexte_ressource = None;
                e.liaison = FormLiaison::default();
                e.regle = FormRegle::default();
            }
            self.charger_groupe();
            self.charger_groupes();
        } else {
            self.apres_modification_du_compte();
        }
    }

    pub fn changer_ressource_liaison(&self, f: &Principale) {
        let type_ = {
            let mut e = self.verrou();
            e.liaison.ressource = None;
            e.liaison.portee.clone().map(|(t, _)| t)
        };
        if let Some(t) = type_ {
            self.verrou().contexte_ressource = Some(("liaison", t));
            self.chercher_ressource(f, "");
        }
        self.projeter(f);
    }

    pub fn chercher_permissions(&self, f: &Principale, texte: &str) {
        self.verrou().regle.recherche = texte.to_string();
        self.projeter(f);
    }

    pub fn choisir_permission(&self, f: &Principale, i: i32) {
        let type_ = {
            let mut e = self.verrou();
            if i < 0 {
                e.regle = FormRegle::default();
                e.contexte_ressource = None;
                None
            } else {
                let trouvees = Self::permissions_trouvees(&e);
                let Some(p) = trouvees.get(i as usize).cloned() else {
                    return;
                };
                e.regle.permission = Some(p.name.clone());
                e.regle.ressource = None;
                let t = type_de(&e.noms, &p.name);
                e.contexte_ressource = t.clone().map(|t| ("regle", t));
                t
            }
        };
        f.global::<DonneesAcces>()
            .set_recherche_ressource("".into());
        if type_.is_some() {
            self.chercher_ressource(f, "");
        }
        self.projeter(f);
    }

    fn permissions_trouvees(e: &EtatAcces) -> Vec<PermissionVue> {
        e.catalogue
            .donnees
            .as_ref()
            .map(|c| {
                permissions_actives(c)
                    .into_iter()
                    .filter(|p| correspond_au_filtre(p, &e.regle.recherche))
                    .take(PERMISSIONS_PROPOSEES)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn regle_partout(&self, f: &Principale) {
        let type_ = {
            let mut e = self.verrou();
            e.regle.ressource = None;
            e.regle
                .permission
                .as_ref()
                .and_then(|p| type_de(&e.noms, p))
        };
        if let Some(t) = type_ {
            self.verrou().contexte_ressource = Some(("regle", t));
        }
        self.projeter(f);
    }

    pub fn ajouter_regle(&self, f: &Principale) {
        let (regle, sujet, sujet_id) = {
            let e = self.verrou();
            let (sujet, id) = if e.onglet == "groupes" {
                (Sujet::Group, e.groupe_choisi.clone())
            } else {
                (Sujet::Account, e.fiche.as_ref().map(|x| x.sub.clone()))
            };
            (e.regle.clone(), sujet, id)
        };
        let (Some(permission), Some(sujet_id)) = (regle.permission, sujet_id) else {
            return;
        };
        let g = f.global::<DonneesAcces>();
        let expire = match vers_iso(&g.get_regle_expire(), self.decalage()) {
            Ok(x) => x,
            Err(m) => {
                self.verrou().erreur = m;
                self.projeter(f);
                return;
            }
        };
        let effet = if regle.refus {
            Effet::Deny
        } else {
            Effet::Allow
        };
        let r = NouvelleRegle {
            sujet,
            sujet_id,
            permission,
            ressource: regle.ressource.map_or_else(|| "*".into(), |r| r.id),
            effet,
            expire,
            note: g.get_regle_note().trim().chars().take(200).collect(),
        };
        let client = self.client.clone();
        self.agir(
            f,
            async move { client.creer_regle(&r).await },
            if effet == Effet::Allow {
                "Permission accordée."
            } else {
                "Permission refusée."
            },
            move |p, f, ()| {
                let g = f.global::<DonneesAcces>();
                g.set_regle_expire("".into());
                g.set_regle_note("".into());
                p.apres_ajout(sujet);
            },
        );
    }

    pub fn rejoindre_groupe(&self, f: &Principale) {
        let Some((sub, groupe)) = ({
            let e = self.verrou();
            e.fiche
                .as_ref()
                .map(|x| x.sub.clone())
                .zip(e.groupe_libre.clone())
        }) else {
            return;
        };
        let expire = match vers_iso(
            &f.global::<DonneesAcces>().get_groupe_expire(),
            self.decalage(),
        ) {
            Ok(x) => x,
            Err(m) => {
                self.verrou().erreur = m;
                self.projeter(f);
                return;
            }
        };
        let client = self.client.clone();
        self.agir(
            f,
            async move {
                client
                    .ajouter_au_groupe(&groupe.id, &sub, expire.as_deref())
                    .await
            },
            "Ajouté au groupe.",
            |p, f, ()| {
                f.global::<DonneesAcces>().set_groupe_expire("".into());
                p.apres_modification_du_compte();
            },
        );
    }

    pub fn chercher_compte(&self, f: &Principale, texte: &str) {
        let jeton = {
            let mut e = self.verrou();
            e.jeton_compte += 1;
            e.comptes_trouves = None;
            e.jeton_compte
        };
        let t = texte.trim().to_string();
        self.projeter(f);
        if t.chars().count() < 2 {
            return;
        }
        let pont = self.clone();
        let client = self.client.clone();
        let generation = self.verrou().generation;
        self.runtime.spawn(async move {
            tokio::time::sleep(ATTENTE_SAISIE).await;
            if pont.verrou().jeton_compte != jeton {
                return;
            }
            let r = client.comptes_acces(&t, LIMITE_CHOIX_COMPTE, 0).await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                {
                    let mut e = p.verrou();
                    if e.jeton_compte != jeton || e.generation != generation {
                        return;
                    }
                    match r {
                        Ok(page) => e.comptes_trouves = Some(page),
                        Err(err) => {
                            e.erreur = format!(
                                "Recherche de comptes impossible : {}",
                                erreur_lisible(&err)
                            )
                        }
                    }
                }
                p.projeter(&f);
            });
        });
    }

    pub fn choisir_compte(&self, f: &Principale, i: usize) {
        let (compte, contexte) = {
            let mut e = self.verrou();
            let Some(c) = e
                .comptes_trouves
                .as_ref()
                .and_then(|p| p.accounts.get(i))
                .map(|c| c.compte.clone())
            else {
                return;
            };
            e.comptes_trouves = None;
            e.jeton_compte += 1;
            (c, e.contexte_compte)
        };
        f.global::<DonneesAcces>().set_recherche_compte("".into());
        match contexte {
            "copie" => {
                if let Some(x) = self.verrou().fiche.as_mut() {
                    x.source = (x.sub != compte.sub).then_some(compte);
                }
            }
            "donner" => self.donner_role_a(f, compte),
            "membre" => self.ajouter_membre(f, compte),
            _ => {}
        }
        self.projeter(f);
    }

    pub fn chercher_ressource(&self, f: &Principale, texte: &str) {
        let Some((_, type_)) = self.verrou().contexte_ressource.clone() else {
            return;
        };
        let jeton = {
            let mut e = self.verrou();
            e.jeton_ressource += 1;
            e.ressources_trouvees = None;
            e.jeton_ressource
        };
        let t = texte.trim().to_string();
        let pont = self.clone();
        let client = self.client.clone();
        let generation = self.verrou().generation;
        self.runtime.spawn(async move {
            tokio::time::sleep(ATTENTE_SAISIE).await;
            if pont.verrou().jeton_ressource != jeton {
                return;
            }
            let r = client.ressources_acces(&type_, &t).await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                {
                    let mut e = p.verrou();
                    if e.jeton_ressource != jeton || e.generation != generation {
                        return;
                    }
                    match r {
                        Ok(l) => {
                            for x in &l {
                                e.ressources.retenir(&type_, &x.id, &x.name);
                            }
                            e.ressources_trouvees = Some(l);
                        }
                        Err(err) => {
                            e.erreur = format!(
                                "Liste des ressources indisponible : {}",
                                erreur_lisible(&err)
                            )
                        }
                    }
                }
                p.projeter(&f);
            });
        });
        self.projeter(f);
    }

    pub fn choisir_ressource(&self, f: &Principale, i: usize) {
        let contexte = {
            let mut e = self.verrou();
            let Some(r) = e
                .ressources_trouvees
                .as_ref()
                .and_then(|l| l.get(i))
                .cloned()
            else {
                return;
            };
            let contexte = e.contexte_ressource.take().map(|(c, _)| c);
            e.ressources_trouvees = None;
            match contexte {
                Some("liaison") => e.liaison.ressource = Some(r),
                Some("regle") => e.regle.ressource = Some(r),
                Some("essai") => {
                    if let Some(x) = e.fiche.as_mut() {
                        x.essai = Some(r.clone());
                        x.decision = None;
                    }
                    e.contexte_ressource = e
                        .fiche
                        .as_ref()
                        .and_then(|x| x.ouverte.as_ref())
                        .and_then(|p| type_de(&e.noms, p))
                        .map(|t| ("essai", t));
                }
                Some("exception") => {
                    if let Some(ed) = e.editeur.as_mut() {
                        if let Some(perm) = ed.ajout_pour.take() {
                            ed.precises
                                .retain(|x| !(x.permission == perm && x.resource == r.id));
                            ed.precises.push(RegleSaisie {
                                permission: perm,
                                resource: r.id.clone(),
                                effect: ed.ajout_effet,
                            });
                        }
                    }
                }
                _ => {}
            }
            contexte
        };
        f.global::<DonneesAcces>()
            .set_recherche_ressource("".into());
        if contexte == Some("essai") {
            let id = self
                .verrou()
                .fiche
                .as_ref()
                .and_then(|x| x.essai.as_ref().map(|r| r.id.clone()));
            self.simuler(id);
        }
        self.projeter(f);
    }

    pub fn ouvrir_choix(&self, f: &Principale, quel: &str, x: f32, haut: f32, bas: f32) {
        let (entrees, titre): (Vec<Entree>, &str) = {
            let e = self.verrou();
            match quel {
                "liaison-role" => (
                    e.roles
                        .donnees
                        .iter()
                        .flatten()
                        .filter(|r| r.system != Some(Systeme::ToutLeMonde))
                        .map(|r| {
                            Entree::nouvelle(&r.id, &r.name, "")
                                .cochee(e.liaison.role.as_ref().is_some_and(|c| c.id == r.id))
                        })
                        .collect(),
                    "Rôle",
                ),
                "liaison-portee" => {
                    let mut l = vec![
                        Entree::nouvelle("", "Partout", "").cochee(e.liaison.portee.is_none())
                    ];
                    for (cle, nom) in e
                        .catalogue
                        .donnees
                        .as_ref()
                        .map(types_listables)
                        .unwrap_or_default()
                    {
                        let coche = e.liaison.portee.as_ref().is_some_and(|(c, _)| *c == cle);
                        l.push(
                            Entree::nouvelle(&cle, &format!("Une ressource : {nom}"), "")
                                .cochee(coche),
                        );
                    }
                    (l, "Portée")
                }
                "regle-effet" => (
                    vec![
                        Entree::nouvelle("allow", "Accorder", "ok").cochee(!e.regle.refus),
                        Entree::nouvelle("deny", "Refuser", "x").cochee(e.regle.refus),
                    ],
                    "Effet",
                ),
                "copie-mode" => {
                    let mode = e.fiche.as_ref().map(|x| x.mode).unwrap_or_default();
                    (
                        vec![
                            Entree::nouvelle("ajouter", "Ajouter à l'existant", "")
                                .cochee(mode == ModeCopie::Ajouter),
                            Entree::nouvelle("remplacer", "Remplacer l'existant", "")
                                .cochee(mode == ModeCopie::Remplacer),
                        ],
                        "Mode",
                    )
                }
                "groupe-libre" => (
                    Self::groupes_libres(&e)
                        .iter()
                        .map(|g| {
                            Entree::nouvelle(&g.id, &g.name, "")
                                .cochee(e.groupe_libre.as_ref().is_some_and(|c| c.id == g.id))
                        })
                        .collect(),
                    "Groupe",
                ),
                "exception-effet" => {
                    let effet = e.editeur.as_ref().map(|x| x.ajout_effet);
                    (
                        vec![
                            Entree::nouvelle("deny", "Refuser sur…", "x")
                                .cochee(effet == Some(Effet::Deny)),
                            Entree::nouvelle("allow", "Autoriser sur…", "ok")
                                .cochee(effet == Some(Effet::Allow)),
                        ],
                        "Effet",
                    )
                }
                "action" => {
                    let mut l =
                        vec![Entree::nouvelle("", "Toutes", "").cochee(e.action.is_empty())];
                    for (cle, nom) in ACTIONS {
                        l.push(Entree::nouvelle(cle, nom, "").cochee(e.action == cle));
                    }
                    (l, "Action")
                }
                _ => return,
            }
        };
        let pont = self.clone();
        let quel = quel.to_string();
        menu::ouvrir(
            f,
            titre,
            Ancre::Element { x, haut, bas },
            entrees,
            move |f, id| {
                pont.choix_fait(f, &quel, id);
                false
            },
        );
    }

    fn groupes_libres(e: &EtatAcces) -> Vec<GroupeResume> {
        let membre_de: Vec<&str> = e
            .fiche
            .as_ref()
            .and_then(|x| x.detail.donnees.as_ref())
            .map(|d| d.groups.iter().map(|g| g.group_id.as_str()).collect())
            .unwrap_or_default();
        e.groupes_libres
            .donnees
            .iter()
            .flatten()
            .filter(|g| !membre_de.contains(&g.id.as_str()))
            .cloned()
            .collect()
    }

    pub fn choix_fait(&self, f: &Principale, quel: &str, id: &str) {
        let mut charger_journal = false;
        let mut chercher = false;
        {
            let mut e = self.verrou();
            match quel {
                "liaison-role" => {
                    e.liaison.role = e
                        .roles
                        .donnees
                        .iter()
                        .flatten()
                        .find(|r| r.id == id)
                        .cloned();
                }
                "liaison-portee" => {
                    e.liaison.ressource = None;
                    e.liaison.portee = types_listables(
                        e.catalogue
                            .donnees
                            .as_ref()
                            .unwrap_or(&Catalogue::default()),
                    )
                    .into_iter()
                    .find(|(c, _)| c == id);
                    e.contexte_ressource = e.liaison.portee.clone().map(|(t, _)| ("liaison", t));
                    chercher = e.contexte_ressource.is_some();
                }
                "regle-effet" => e.regle.refus = id == "deny",
                "copie-mode" => {
                    if let Some(x) = e.fiche.as_mut() {
                        x.mode = if id == "remplacer" {
                            ModeCopie::Remplacer
                        } else {
                            ModeCopie::Ajouter
                        };
                    }
                }
                "groupe-libre" => {
                    e.groupe_libre = Self::groupes_libres(&e).into_iter().find(|g| g.id == id);
                }
                "exception-effet" => {
                    if let Some(x) = e.editeur.as_mut() {
                        x.ajout_effet = if id == "allow" {
                            Effet::Allow
                        } else {
                            Effet::Deny
                        };
                    }
                }
                "action" => {
                    e.action = id.to_string();
                    charger_journal = true;
                }
                _ => {}
            }
        }
        if chercher {
            f.global::<DonneesAcces>()
                .set_recherche_ressource("".into());
            self.chercher_ressource(f, "");
        }
        if charger_journal {
            self.verrou().journal = Charge::default();
            self.charger_journal();
        }
        self.projeter(f);
    }

    fn charger_roles(&self) {
        self.verrou().roles.erreur.clear();
        let client = self.client.clone();
        self.lancer(
            async move {
                let roles = client.roles_acces().await?;
                let mut details = Vec::new();
                for r in &roles {
                    if let Ok(d) = client.role_acces(&r.id).await {
                        details.push(d);
                    }
                }
                Ok((roles, details))
            },
            |p, _, r| {
                let mut e = p.verrou();
                match r {
                    Ok((roles, details)) => {
                        e.roles = Charge::pret(roles);
                        e.details = details
                            .into_iter()
                            .map(|d| (d.resume.id.clone(), d))
                            .collect();
                    }
                    Err(err) => {
                        e.roles.donnees = None;
                        e.roles.erreur = erreur_lisible(&err);
                    }
                }
            },
        );
    }

    pub fn choisir_app(&self, f: &Principale, i: usize) {
        self.verrou().app = i;
        self.projeter(f);
    }

    pub fn ouvrir_role(&self, f: &Principale, id: &str, cible: &str) {
        {
            let mut e = self.verrou();
            let cible = (!cible.is_empty()).then(|| cible.to_string());
            if let Some(c) = &cible {
                if let Some(i) = e.catalogue.donnees.as_ref().and_then(|cat| {
                    cat.apps
                        .iter()
                        .position(|a| a.permissions.iter().any(|p| &p.name == c))
                }) {
                    e.app = i;
                }
            }
            e.editeur = Some(Editeur {
                id: id.to_string(),
                cible,
                ajout_effet: Effet::Deny,
                ..Editeur::default()
            });
            e.formulaire.clear();
            e.contexte_ressource = None;
            e.contexte_compte = "donner";
        }
        let g = f.global::<DonneesAcces>();
        g.set_role_filtre("".into());
        g.set_role_seulement_defini(false);
        self.charger_role();
        self.projeter(f);
    }

    fn charger_role(&self) {
        let Some(id) = self.verrou().editeur.as_ref().map(|x| x.id.clone()) else {
            return;
        };
        if let Some(x) = self.verrou().editeur.as_mut() {
            x.role.erreur.clear();
        }
        let client = self.client.clone();
        let i = id.clone();
        self.lancer(
            async move { client.role_acces(&i).await },
            move |p, f, r| {
                let recu = {
                    let mut e = p.verrou();
                    let Some(x) = e.editeur.as_mut().filter(|x| x.id == id) else {
                        return;
                    };
                    match r {
                        Ok(d) => {
                            x.joker = d
                                .rules
                                .iter()
                                .filter(|r| r.resource == "*")
                                .map(|r| (r.permission.clone(), r.effect))
                                .collect();
                            x.precises = d
                                .rules
                                .iter()
                                .filter(|r| r.resource != "*")
                                .map(|r| RegleSaisie {
                                    permission: r.permission.clone(),
                                    resource: r.resource.clone(),
                                    effect: r.effect,
                                })
                                .collect();
                            x.depart = d
                                .rules
                                .iter()
                                .map(|r| RegleSaisie {
                                    permission: r.permission.clone(),
                                    resource: r.resource.clone(),
                                    effect: r.effect,
                                })
                                .collect();
                            x.ajout_pour = None;
                            let noms = (d.resume.name.clone(), d.resume.description.clone());
                            x.role = Charge::pret(d);
                            Some(noms)
                        }
                        Err(err) => {
                            x.role.donnees = None;
                            x.role.erreur = erreur_lisible(&err);
                            None
                        }
                    }
                };
                if let Some((nom, description)) = recu {
                    let g = f.global::<DonneesAcces>();
                    g.set_role_nom(nom.into());
                    g.set_role_description(description.into());
                }
                p.demander_noms(f);
            },
        );
    }

    pub fn fermer_role(&self, f: &Principale) {
        self.verrou().editeur = None;
        self.charger_roles();
        self.projeter(f);
    }

    pub fn creer_role(&self, f: &Principale) {
        let nom = f.global::<DonneesAcces>().get_nom_role().trim().to_string();
        if nom.is_empty() {
            return;
        }
        let client = self.client.clone();
        self.agir(
            f,
            async move { client.creer_role_acces(&nom).await },
            "Rôle créé.",
            |p, f, r: RoleDetail| {
                f.global::<DonneesAcces>().set_nom_role("".into());
                p.charger_roles();
                p.ouvrir_role(f, &r.resume.id, "");
            },
        );
    }

    fn regles_saisies(x: &Editeur) -> Vec<RegleSaisie> {
        x.joker
            .iter()
            .map(|(p, e)| RegleSaisie {
                permission: p.clone(),
                resource: "*".into(),
                effect: *e,
            })
            .chain(x.precises.iter().cloned())
            .collect()
    }

    pub fn regler(&self, f: &Principale, permission: &str, choix: i32) {
        if let Some(x) = self.verrou().editeur.as_mut() {
            match choix {
                1 => {
                    x.joker.insert(permission.to_string(), Effet::Allow);
                }
                2 => {
                    x.joker.insert(permission.to_string(), Effet::Deny);
                }
                _ => {
                    x.joker.remove(permission);
                }
            }
        }
        self.projeter(f);
    }

    pub fn tout_regler(&self, f: &Principale, categorie: &str, choix: i32) {
        let perms: Vec<String> = {
            let e = self.verrou();
            let g = f.global::<DonneesAcces>();
            self.lignes_editeur(&e, &g.get_role_filtre(), g.get_role_seulement_defini())
                .into_iter()
                .filter(|(cat, _)| cat == categorie)
                .flat_map(|(_, perms)| perms.into_iter().map(|p| p.name))
                .collect()
        };
        for p in perms {
            if let Some(x) = self.verrou().editeur.as_mut() {
                if choix == 1 {
                    x.joker.insert(p, Effet::Allow);
                } else {
                    x.joker.remove(&p);
                }
            }
        }
        self.projeter(f);
    }

    pub fn ouvrir_exception(&self, f: &Principale, permission: &str) {
        let type_ = {
            let mut e = self.verrou();
            let t = type_de(&e.noms, permission);
            if let Some(x) = e.editeur.as_mut() {
                x.ajout_pour = Some(permission.to_string());
            }
            e.contexte_ressource = t.clone().map(|t| ("exception", t));
            t
        };
        f.global::<DonneesAcces>()
            .set_recherche_ressource("".into());
        if type_.is_some() {
            self.chercher_ressource(f, "");
        }
        self.projeter(f);
    }

    pub fn retirer_exception(&self, f: &Principale, permission: &str, i: usize) {
        if let Some(x) = self.verrou().editeur.as_mut() {
            let indices: Vec<usize> = x
                .precises
                .iter()
                .enumerate()
                .filter(|(_, r)| r.permission == permission)
                .map(|(j, _)| j)
                .collect();
            if let Some(&j) = indices.get(i) {
                x.precises.remove(j);
            }
        }
        self.projeter(f);
    }

    pub fn annuler_role(&self, f: &Principale) {
        self.charger_role();
        self.projeter(f);
    }

    pub fn enregistrer_role(&self, f: &Principale) {
        let Some((id, regles)) = self
            .verrou()
            .editeur
            .as_ref()
            .map(|x| (x.id.clone(), Self::regles_saisies(x)))
        else {
            return;
        };
        let g = f.global::<DonneesAcces>();
        let (nom, description) = (
            g.get_role_nom().to_string(),
            g.get_role_description().to_string(),
        );
        let client = self.client.clone();
        self.agir(
            f,
            async move {
                client
                    .modifier_role_acces(&id, &nom, &description, &regles)
                    .await
            },
            "Rôle enregistré.",
            |p, _, _| {
                p.charger_role();
                p.charger_roles();
            },
        );
    }

    pub fn supprimer_role(&self, f: &Principale) {
        let Some((id, nom, n)) = self.verrou().editeur.as_ref().map(|x| {
            (
                x.id.clone(),
                x.role
                    .donnees
                    .as_ref()
                    .map(|r| r.resume.name.clone())
                    .unwrap_or_default(),
                x.role.donnees.as_ref().map_or(0, |r| r.bindings.len()),
            )
        }) else {
            return;
        };
        self.confirmer(
            f,
            "Supprimer le rôle",
            format!("Supprimer « {nom} » ? Ses {n} liaison(s) disparaissent."),
            "Supprimer",
            move |p, f| {
                let client = p.client.clone();
                p.agir(
                    f,
                    async move { client.supprimer_role_acces(&id).await },
                    "Rôle supprimé.",
                    |p, f, ()| p.fermer_role(f),
                );
            },
        );
    }

    pub fn retirer_qui(&self, f: &Principale, id: &str) {
        let Some(l) = self.verrou().editeur.as_ref().and_then(|x| {
            x.role
                .donnees
                .as_ref()
                .and_then(|r| r.bindings.iter().find(|l| l.id == id).cloned())
        }) else {
            return;
        };
        let qui = l
            .subject_label
            .clone()
            .unwrap_or_else(|| l.subject_id.clone());
        self.confirmer(
            f,
            "Retirer le rôle",
            format!("Retirer le rôle « {} » à {qui} ?", l.role),
            "Retirer",
            move |p, f| {
                let client = p.client.clone();
                p.agir(
                    f,
                    async move { client.supprimer_liaison(&l.id).await },
                    "Rôle retiré.",
                    |p, _, ()| {
                        p.charger_role();
                        p.charger_roles();
                    },
                );
            },
        );
    }

    fn donner_role_a(&self, f: &Principale, compte: Compte) {
        let Some(id) = self.verrou().editeur.as_ref().map(|x| x.id.clone()) else {
            return;
        };
        let l = NouvelleLiaison {
            role_id: id,
            sujet: Sujet::Account,
            sujet_id: compte.sub,
            portee: None,
            expire: None,
        };
        let client = self.client.clone();
        self.agir(
            f,
            async move { client.creer_liaison(&l).await },
            "Rôle donné.",
            |p, _, ()| {
                p.verrou().formulaire.clear();
                p.charger_role();
                p.charger_roles();
            },
        );
    }

    fn charger_groupes(&self) {
        self.verrou().groupes.erreur.clear();
        let client = self.client.clone();
        self.lancer(async move { client.groupes_acces().await }, |p, _, r| {
            p.verrou().groupes.recevoir(r)
        });
    }

    pub fn choisir_groupe(&self, f: &Principale, i: usize) {
        {
            let mut e = self.verrou();
            let Some(g) = e.groupes.donnees.as_ref().and_then(|l| l.get(i)).cloned() else {
                return;
            };
            e.groupe_choisi = Some(g.id);
            e.groupe = Charge::default();
            e.formulaire.clear();
            e.contexte_ressource = None;
            e.contexte_compte = "membre";
            e.comptes_trouves = None;
        }
        f.global::<DonneesAcces>().set_recherche_compte("".into());
        self.charger_groupe();
        self.projeter(f);
    }

    fn charger_groupe(&self) {
        let Some(id) = self.verrou().groupe_choisi.clone() else {
            return;
        };
        self.verrou().groupe.erreur.clear();
        let client = self.client.clone();
        let i = id.clone();
        self.lancer(
            async move { client.groupe_acces(&i).await },
            move |p, f, r| {
                let mut e = p.verrou();
                if e.groupe_choisi.as_deref() == Some(id.as_str()) {
                    e.groupe.recevoir(r);
                }
                drop(e);
                p.demander_noms(f);
            },
        );
    }

    pub fn creer_groupe(&self, f: &Principale) {
        let nom = f
            .global::<DonneesAcces>()
            .get_nom_groupe()
            .trim()
            .to_string();
        if nom.is_empty() {
            return;
        }
        let client = self.client.clone();
        self.agir(
            f,
            async move { client.creer_groupe_acces(&nom).await },
            "Groupe créé.",
            |p, f, g: GroupeDetail| {
                f.global::<DonneesAcces>().set_nom_groupe("".into());
                {
                    let mut e = p.verrou();
                    e.groupe_choisi = Some(g.resume.id.clone());
                    e.groupe = Charge::pret(g);
                    e.contexte_compte = "membre";
                }
                p.charger_groupes();
            },
        );
    }

    pub fn supprimer_groupe(&self, f: &Principale) {
        let Some(g) = self.verrou().groupe.donnees.clone() else {
            return;
        };
        self.confirmer(
            f,
            "Supprimer le groupe",
            format!(
                "Supprimer « {} » ? Ses {} membre(s) perdent les rôles et règles du groupe.",
                g.resume.name,
                g.members.len()
            ),
            "Supprimer",
            move |p, f| {
                let client = p.client.clone();
                let id = g.resume.id.clone();
                p.agir(
                    f,
                    async move { client.supprimer_groupe_acces(&id).await },
                    "Groupe supprimé.",
                    |p, _, ()| {
                        {
                            let mut e = p.verrou();
                            e.groupe_choisi = None;
                            e.groupe = Charge::default();
                        }
                        p.charger_groupes();
                    },
                );
            },
        );
    }

    fn ajouter_membre(&self, f: &Principale, compte: Compte) {
        let Some(gid) = self.verrou().groupe_choisi.clone() else {
            return;
        };
        let expire = match vers_iso(
            &f.global::<DonneesAcces>().get_membre_expire(),
            self.decalage(),
        ) {
            Ok(x) => x,
            Err(m) => {
                self.verrou().erreur = m;
                self.projeter(f);
                return;
            }
        };
        let client = self.client.clone();
        self.agir(
            f,
            async move {
                client
                    .ajouter_au_groupe(&gid, &compte.sub, expire.as_deref())
                    .await
            },
            "Membre ajouté.",
            |p, f, ()| {
                f.global::<DonneesAcces>().set_membre_expire("".into());
                p.charger_groupe();
                p.charger_groupes();
            },
        );
    }

    pub fn retirer_membre(&self, f: &Principale, i: usize) {
        let Some((gid, m)) = ({
            let e = self.verrou();
            e.groupe_choisi.clone().zip(
                e.groupe
                    .donnees
                    .as_ref()
                    .and_then(|g| g.members.get(i).cloned()),
            )
        }) else {
            return;
        };
        let qui = nom_compte(Some(&m.account), &m.account.sub);
        self.confirmer(
            f,
            "Retirer du groupe",
            format!("Retirer {qui} du groupe ?"),
            "Retirer",
            move |p, f| {
                let client = p.client.clone();
                p.agir(
                    f,
                    async move { client.retirer_du_groupe(&gid, &m.account.sub).await },
                    "Membre retiré.",
                    |p, _, ()| {
                        p.charger_groupe();
                        p.charger_groupes();
                    },
                );
            },
        );
    }

    pub fn retirer_liaison(&self, f: &Principale, i: usize) {
        let Some(l) = self
            .verrou()
            .groupe
            .donnees
            .as_ref()
            .and_then(|g| g.bindings.get(i).cloned())
        else {
            return;
        };
        let qui = l
            .subject_label
            .clone()
            .unwrap_or_else(|| l.subject_id.clone());
        self.confirmer(
            f,
            "Retirer le rôle",
            format!("Retirer le rôle « {} » à {qui} ?", l.role),
            "Retirer",
            move |p, f| {
                let client = p.client.clone();
                p.agir(
                    f,
                    async move { client.supprimer_liaison(&l.id).await },
                    "Rôle retiré.",
                    |p, _, ()| {
                        p.charger_groupe();
                        p.charger_groupes();
                    },
                );
            },
        );
    }

    pub fn retirer_regle(&self, f: &Principale, i: usize) {
        let Some(r) = self
            .verrou()
            .groupe
            .donnees
            .as_ref()
            .and_then(|g| g.rules.get(i).cloned())
        else {
            return;
        };
        let nom = libelle_permission(&self.verrou().noms, &r.permission);
        let verbe = if r.effect == Effet::Allow {
            "Retirer"
        } else {
            "Lever le refus de"
        };
        let sur = if r.resource != "*" {
            format!(" sur {}", r.resource)
        } else {
            String::new()
        };
        self.confirmer(
            f,
            "Supprimer la règle",
            format!("{verbe} « {nom} »{sur} ?"),
            "Supprimer",
            move |p, f| {
                let client = p.client.clone();
                p.agir(
                    f,
                    async move { client.supprimer_regle(&r.id).await },
                    "Règle supprimée.",
                    |p, _, ()| {
                        p.charger_groupe();
                        p.charger_groupes();
                    },
                );
            },
        );
    }

    pub fn purger(&self, f: &Principale) {
        let n = self
            .verrou()
            .catalogue
            .donnees
            .as_ref()
            .map_or(0, obsoletes);
        self.confirmer(
            f,
            "Purger les permissions obsolètes",
            format!(
                "Supprimer {n} permission(s) retirée(s) par leur application, et leurs règles ?"
            ),
            "Purger",
            |p, f| {
                let client = p.client.clone();
                p.agir(
                    f,
                    async move { client.purger_permissions_obsoletes().await },
                    "Permissions purgées.",
                    |p, _, ()| p.charger_catalogue(),
                );
            },
        );
    }

    pub fn deplier(&self, f: &Principale, permission: &str) {
        let charger = {
            let mut e = self.verrou();
            e.depliee = if e.depliee.as_deref() == Some(permission) {
                None
            } else {
                Some(permission.to_string())
            };
            e.detenteurs = Charge::default();
            e.depliee.is_some()
        };
        if charger {
            self.charger_detenteurs();
        }
        self.projeter(f);
    }

    fn charger_detenteurs(&self) {
        let Some(perm) = self.verrou().depliee.clone() else {
            return;
        };
        self.verrou().detenteurs.erreur.clear();
        let client = self.client.clone();
        let p2 = perm.clone();
        self.lancer(
            async move { client.detenteurs_acces(&p2).await },
            move |p, f, r| {
                let mut e = p.verrou();
                if e.depliee.as_deref() == Some(perm.as_str()) {
                    e.detenteurs.recevoir(r);
                }
                drop(e);
                p.demander_noms(f);
            },
        );
    }

    fn charger_journal(&self) {
        let action = {
            let mut e = self.verrou();
            e.journal.erreur.clear();
            e.action.clone()
        };
        let client = self.client.clone();
        let a = action.clone();
        self.lancer(
            async move { client.journal_acces(&a, None, LIMITE_JOURNAL).await },
            move |p, f, r| {
                let mut e = p.verrou();
                if e.action == action {
                    e.journal.recevoir(r);
                }
                drop(e);
                p.demander_noms(f);
            },
        );
    }

    fn noms_manquants(&self, e: &EtatAcces) -> Vec<(String, String)> {
        let mut manquants = Vec::new();
        let mut voir = |t: Option<String>, id: &str| {
            if let Some(t) = t {
                if id != "*"
                    && !e.ressources.connu(&t, id)
                    && !e.noms_demandes.contains(&(t.clone(), id.to_string()))
                {
                    manquants.push((t, id.to_string()));
                }
            }
        };
        let regles = |rs: &[RegleVue], voir: &mut dyn FnMut(Option<String>, &str)| {
            for r in rs {
                voir(type_de(&e.noms, &r.permission), &r.resource);
            }
        };
        let liaisons = |ls: &[LiaisonVue], voir: &mut dyn FnMut(Option<String>, &str)| {
            for l in ls {
                if let Some(s) = &l.scope_id {
                    voir(l.scope_type.clone(), s);
                }
            }
        };
        if let Some(x) = &e.fiche {
            if let Some(d) = &x.detail.donnees {
                liaisons(&d.bindings, &mut voir);
                regles(&d.rules, &mut voir);
            }
            if let Some(ef) = &x.effectifs.donnees {
                for p in &ef.permissions {
                    let t = type_de(&e.noms, &p.permission);
                    for id in p.allow.iter().chain(&p.deny) {
                        voir(t.clone(), id);
                    }
                    for r in &p.rules {
                        voir(t.clone(), &r.resource);
                    }
                }
            }
            if let Some(j) = &x.journal.donnees {
                for l in j {
                    match ressource_journal(l, &e.noms) {
                        Some(
                            RessourceJournal::Regle { type_, id }
                            | RessourceJournal::Portee { type_, id },
                        ) => voir(type_, &id),
                        None => {}
                    }
                }
            }
        }
        if let Some(x) = &e.editeur {
            if let Some(r) = &x.role.donnees {
                liaisons(&r.bindings, &mut voir);
            }
            for r in &x.precises {
                voir(type_de(&e.noms, &r.permission), &r.resource);
            }
        }
        if let Some(g) = &e.groupe.donnees {
            liaisons(&g.bindings, &mut voir);
            regles(&g.rules, &mut voir);
        }
        if let Some(d) = &e.detenteurs.donnees {
            for x in d {
                voir(type_de(&e.noms, &x.rule.permission), &x.rule.resource);
            }
        }
        if let Some(j) = &e.journal.donnees {
            for l in j {
                match ressource_journal(l, &e.noms) {
                    Some(
                        RessourceJournal::Regle { type_, id }
                        | RessourceJournal::Portee { type_, id },
                    ) => voir(type_, &id),
                    None => {}
                }
            }
        }
        manquants.sort();
        manquants.dedup();
        manquants
    }

    fn demander_noms(&self, _f: &Principale) {
        let manquants = {
            let mut e = self.verrou();
            let m = self.noms_manquants(&e);
            for x in &m {
                e.noms_demandes.insert(x.clone());
            }
            m
        };
        let mut par_type: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (t, id) in manquants {
            par_type.entry(t).or_default().push(id);
        }
        for (t, ids) in par_type {
            let client = self.client.clone();
            let t2 = t.clone();
            self.lancer(
                async move { client.noms_de_ressources(&t2, &ids).await },
                move |p, _, r| {
                    if let Ok(l) = r {
                        let mut e = p.verrou();
                        for x in l {
                            e.ressources.retenir(&t, &x.id, &x.name);
                        }
                    }
                },
            );
        }
    }

    pub fn installer(&self, f: &Principale, d: DonneesAccesChargees) {
        {
            let mut e = self.verrou();
            e.generation += 1;
            e.onglet = if d.onglet.is_empty() {
                "comptes"
            } else {
                d.onglet
            };
            e.erreur.clear();
            e.message.clear();
            if let Some(c) = d.catalogue {
                e.noms = libelles_permissions(&c);
                e.catalogue = Charge::pret(c);
            }
            if let Some(page) = d.comptes {
                e.total = page.total;
                e.from_keycloak = page.from_keycloak;
                e.comptes = Charge::pret(page.accounts);
                e.version_comptes += 1;
            }
            if let Some((detail, effectifs)) = d.compte {
                e.fiche = Some(Fiche {
                    sub: detail.account.sub.clone(),
                    detail: Charge::pret(detail),
                    effectifs: Charge::pret(effectifs),
                    ..Fiche::default()
                });
            }
            if let Some(roles) = d.roles {
                e.roles = Charge::pret(roles.iter().map(|r| r.resume.clone()).collect());
                e.details = roles
                    .into_iter()
                    .map(|r| (r.resume.id.clone(), r))
                    .collect();
            }
            if let Some(id) = d.role {
                if let Some(r) = e.details.get(&id).cloned() {
                    e.editeur = Some(Editeur {
                        id,
                        joker: r
                            .rules
                            .iter()
                            .filter(|x| x.resource == "*")
                            .map(|x| (x.permission.clone(), x.effect))
                            .collect(),
                        precises: r
                            .rules
                            .iter()
                            .filter(|x| x.resource != "*")
                            .map(|x| RegleSaisie {
                                permission: x.permission.clone(),
                                resource: x.resource.clone(),
                                effect: x.effect,
                            })
                            .collect(),
                        depart: r
                            .rules
                            .iter()
                            .map(|x| RegleSaisie {
                                permission: x.permission.clone(),
                                resource: x.resource.clone(),
                                effect: x.effect,
                            })
                            .collect(),
                        ajout_effet: Effet::Deny,
                        role: Charge::pret(r.clone()),
                        ..Editeur::default()
                    });
                    let g = f.global::<DonneesAcces>();
                    g.set_role_nom(r.resume.name.clone().into());
                    g.set_role_description(r.resume.description.clone().into());
                }
            }
            if let Some(groupes) = d.groupes {
                e.groupes = Charge::pret(groupes);
            }
            if let Some(g) = d.groupe {
                e.groupe_choisi = Some(g.resume.id.clone());
                e.groupe = Charge::pret(g);
            }
            if let Some(j) = d.journal {
                e.journal = Charge::pret(j);
            }
            for (t, id, nom) in d.ressources {
                e.ressources.retenir(&t, &id, &nom);
            }
            if let Some(m) = d.erreur_comptes {
                e.comptes = Charge {
                    donnees: None,
                    erreur: m,
                };
                e.version_comptes += 1;
            }
        }
        self.projeter(f);
    }

    fn lignes_editeur(
        &self,
        e: &EtatAcces,
        filtre: &str,
        seulement: bool,
    ) -> Vec<(String, Vec<PermissionVue>)> {
        let Some(x) = &e.editeur else {
            return Vec::new();
        };
        let Some(cat) = e.catalogue.donnees.as_ref() else {
            return Vec::new();
        };
        let regles = Self::regles_saisies(x);
        let touchees = permissions_modifiees(&x.depart, &regles);
        let defini =
            |p: &str| x.joker.contains_key(p) || x.precises.iter().any(|r| r.permission == p);
        let app = cat.apps.get(e.app).or(cat.apps.first());
        categories(app, |p| {
            correspond_au_filtre(p, filtre)
                && (!seulement || defini(&p.name) || touchees.contains(&p.name))
        })
        .into_iter()
        .map(|c| (c.nom, c.permissions.into_iter().cloned().collect()))
        .collect()
    }

    fn projeter_comptes(&self, f: &Principale, e: &mut EtatAcces) {
        let g = f.global::<DonneesAcces>();
        let choisi = e.fiche.as_ref().map(|x| x.sub.clone());
        let dec = self.decalage();
        let ligne = |c: &CompteResume, choisi: &Option<String>| LigneCompteAcces {
            sub: c.compte.sub.clone().into(),
            nom: nom_compte(Some(&c.compte), &c.compte.sub).into(),
            resume: resume_compte(c, dec).into(),
            initiales: initiales(&c.compte).into(),
            teinte: teinte(&c.compte.sub),
            supprime: !c.compte.in_keycloak,
            choisi: choisi.as_deref() == Some(c.compte.sub.as_str()),
        };
        let liste = e.comptes.donnees.as_deref().unwrap_or_default();
        if e.version_projetee != e.version_comptes {
            let modele = g.get_comptes();
            let existant = modele.as_any().downcast_ref::<VecModel<LigneCompteAcces>>();
            let deja = existant.map_or(0, |m| m.row_count());
            let prolonge = existant.is_some()
                && deja > 0
                && deja <= liste.len()
                && (0..deja).all(|i| {
                    modele
                        .row_data(i)
                        .is_some_and(|l| l.sub.as_str() == liste[i].compte.sub)
                });
            match existant {
                Some(m) if prolonge => {
                    m.extend(liste[deja..].iter().map(|c| ligne(c, &choisi)));
                }
                _ => g.set_comptes(modele_de(liste.iter().map(|c| ligne(c, &choisi)).collect())),
            }
            e.version_projetee = e.version_comptes;
            e.choisi_projete = choisi;
            return;
        }
        if e.choisi_projete != choisi {
            let modele = g.get_comptes();
            for sub in [e.choisi_projete.clone(), choisi.clone()]
                .into_iter()
                .flatten()
            {
                if let Some(i) = liste.iter().position(|c| c.compte.sub == sub) {
                    if let Some(c) = liste.get(i) {
                        modele.set_row_data(i, ligne(c, &choisi));
                    }
                }
            }
            e.choisi_projete = choisi;
        }
    }

    fn ligne_journal(&self, e: &EtatAcces, l: &EntreeJournal) -> LigneJournalAcces {
        let dec = self.decalage();
        LigneJournalAcces {
            date: quand(Some(&l.at), dec).into(),
            acteur: acteur(l).into(),
            action: libelle_action(&l.action).into(),
            ton: ton_action(&l.action).into(),
            cible: cible(l, &e.noms).into(),
            pour: pour_qui(l).unwrap_or_default().into(),
            ressource: match ressource_journal(l, &e.noms) {
                Some(
                    RessourceJournal::Regle { type_, id } | RessourceJournal::Portee { type_, id },
                ) => {
                    format!("sur {}", e.ressources.nom(type_.as_deref(), &id))
                }
                None => String::new(),
            }
            .into(),
            extra: extra_journal(l, dec).into(),
        }
    }

    pub fn projeter(&self, f: &Principale) {
        let g = f.global::<DonneesAcces>();
        let mut e = self.verrou();
        let dec = self.decalage();
        g.set_onglet(e.onglet.into());
        g.set_catalogue(e.catalogue.etat());
        g.set_en_cours(e.en_cours);
        g.set_message(e.message.clone().into());
        g.set_erreur(e.erreur.clone().into());
        g.set_formulaire(e.formulaire.clone().into());
        let Some(catalogue) = e.catalogue.donnees.clone() else {
            return;
        };
        let nom_res = |e: &EtatAcces, t: Option<&str>, id: &str| e.ressources.nom(t, id);

        self.projeter_comptes(f, &mut e);
        g.set_comptes_etat(e.comptes.etat());
        let n = e.comptes.donnees.as_ref().map_or(0, Vec::len);
        g.set_note_comptes(if e.from_keycloak > 0 {
            format!(
                "{} compte(s) trouvé(s) dans Keycloak et ajoutés à l'annuaire.",
                e.from_keycloak
            )
            .into()
        } else {
            "".into()
        });
        g.set_vide_comptes(if e.recherche.is_empty() {
            "L'annuaire est vide : les comptes y entrent à leur première connexion ou par la synchronisation Keycloak.".into()
        } else {
            "Aucun compte trouvé.".into()
        });
        g.set_suite_comptes(if (n as i64) < e.total && n > 0 {
            if e.suite_en_cours {
                format!("{n} sur {} · chargement de la suite…", e.total).into()
            } else {
                format!(
                    "{n} sur {} : faites défiler pour charger la suite.",
                    e.total
                )
                .into()
            }
        } else if n > 0 {
            format!("{n} compte{}", if n > 1 { "s" } else { "" }).into()
        } else {
            "".into()
        });

        g.set_fiche_ouverte(e.fiche.is_some());
        let puces = self.puces(&e);
        let largeur = e.largeur_puces;
        let contexte_ressource = e.contexte_ressource.as_ref().map_or("", |(c, _)| c);
        g.set_contexte_ressource(contexte_ressource.into());
        if let Some(x) = &e.fiche {
            g.set_fiche_etat(x.detail.etat());
            g.set_historique(x.historique);
            g.set_historique_etat(x.journal.etat());
            g.set_historique_lignes(modele_de(
                x.journal
                    .donnees
                    .iter()
                    .flatten()
                    .map(|l| self.ligne_journal(&e, l))
                    .collect(),
            ));
            if let Some(d) = &x.detail.donnees {
                let c = &d.account;
                g.set_fiche_nom(nom_compte(Some(c), &x.sub).into());
                let mut note = String::new();
                if let (Some(n), Some(_)) = (&c.display_name, &c.email) {
                    note.push_str(&format!("{n} · "));
                }
                note.push_str(&format!("vu {}", quand(c.last_seen_at.as_deref(), dec)));
                g.set_fiche_note(note.into());
                g.set_fiche_initiales(initiales(c).into());
                g.set_fiche_teinte(teinte(&c.sub));
                g.set_fiche_supprime(!c.in_keycloak);
            }
            g.set_rangees_puces(modele_de(
                ranger_puces(puces.into_iter().map(|(p, _)| p).collect(), largeur)
                    .into_iter()
                    .map(|r| RangeePucesAcces {
                        puces: modele_de(r),
                    })
                    .collect(),
            ));
            g.set_effectifs_etat(x.effectifs.etat());
            let perms = x
                .effectifs
                .donnees
                .as_ref()
                .map(|d| d.permissions.as_slice())
                .unwrap_or_default();
            let compteurs = compter_effectifs(perms);
            g.set_filtres(modele_de(
                FILTRES_EFFECTIFS
                    .iter()
                    .zip(compteurs)
                    .map(|(f, n)| format!("{} · {n}", f.libelle()).into())
                    .collect(),
            ));
            g.set_filtre(
                FILTRES_EFFECTIFS
                    .iter()
                    .position(|f| *f == x.filtre)
                    .unwrap_or(0) as i32,
            );
            let mut droits = Vec::new();
            for groupe in effectifs_par_app(&catalogue, perms, x.filtre) {
                droits.push(LigneDroitAcces {
                    en_tete: true,
                    titre: groupe.nom.to_uppercase().into(),
                    ..LigneDroitAcces::default()
                });
                for ex in groupe.lignes {
                    let t = type_de(&e.noms, &ex.permission);
                    let phrase = if ex.state == Etat::Partiel {
                        let liste = |ids: &[String]| {
                            ids.iter()
                                .map(|id| nom_res(&e, t.as_deref(), id))
                                .collect::<Vec<_>>()
                                .join(", ")
                        };
                        let mut s = if ex.allow.iter().any(|a| a == "*") {
                            "autorisé partout".to_string()
                        } else {
                            format!("autorisé sur {}", liste(&ex.allow))
                        };
                        if !ex.deny.is_empty() {
                            s.push_str(&format!(", sauf sur {}", liste(&ex.deny)));
                        }
                        s
                    } else {
                        resume_effectif(ex)
                    };
                    droits.push(LigneDroitAcces {
                        en_tete: false,
                        titre: "".into(),
                        permission: ex.permission.clone().into(),
                        libelle: libelle_permission(&e.noms, &ex.permission).into(),
                        etat: ex.state.libelle().into(),
                        ton: ex.state.ton().into(),
                        icone: ex.state.icone().into(),
                        phrase: phrase.into(),
                        ouvert: x.ouverte.as_deref() == Some(ex.permission.as_str()),
                    });
                }
            }
            g.set_droits(modele_de(droits));
            g.set_vide_droits(if x.filtre == FiltreEffectifs::Distingue {
                "Rien ne distingue ce compte : il a exactement ce que donne « Tout le monde »."
                    .into()
            } else {
                "Aucune permission dans ce filtre.".into()
            });
            let heritees = compteurs[3] - compteurs[0];
            g.set_heritees(if x.filtre == FiltreEffectifs::Distingue && heritees > 0 {
                format!("Afficher les {heritees} autres permissions (« Tout le monde » ou non définies)").into()
            } else {
                "".into()
            });
            let ouverte = x
                .ouverte
                .as_ref()
                .and_then(|p| perms.iter().find(|ex| &ex.permission == p));
            let t = x.ouverte.as_ref().and_then(|p| type_de(&e.noms, p));
            g.set_chaine(modele_de(
                ouverte
                    .map(|ex| {
                        ex.rules
                            .iter()
                            .map(|r| {
                                let ou = if r.resource == "*" {
                                    "partout".to_string()
                                } else {
                                    format!("sur {}", nom_res(&e, t.as_deref(), &r.resource))
                                };
                                let jusqu = r
                                    .expires_at
                                    .as_deref()
                                    .map(|d| format!(" · jusqu'au {}", quand(Some(d), dec)))
                                    .unwrap_or_default();
                                MaillonAcces {
                                    refus: r.effect == Effet::Deny,
                                    texte: format!("{} · {ou}{jusqu}", origine(&r.source)).into(),
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            ));
            g.set_essai_type(t.is_some());
            g.set_essai_ressource(
                x.essai
                    .as_ref()
                    .map(|r| r.name.clone())
                    .unwrap_or_default()
                    .into(),
            );
            g.set_a_verdict(x.decision.is_some());
            if let Some(d) = &x.decision {
                g.set_verdict_autorise(d.allowed);
                let sur = match (&x.essai, &t) {
                    (Some(r), _) => format!(" sur {}", r.name),
                    (None, Some(_)) => " sans ressource précise".into(),
                    _ => String::new(),
                };
                g.set_verdict(
                    format!("{}{sur}", if d.allowed { "Autorisé" } else { "Refusé" }).into(),
                );
                g.set_verdict_raison(d.reason.clone().into());
            }
            g.set_copie_source(
                x.source
                    .as_ref()
                    .map(|s| nom_compte(Some(s), &s.sub))
                    .unwrap_or_default()
                    .into(),
            );
            g.set_copie_mode(
                match x.mode {
                    ModeCopie::Ajouter => "Ajouter à l'existant",
                    ModeCopie::Remplacer => "Remplacer l'existant",
                }
                .into(),
            );
        }

        g.set_liaison_role(
            e.liaison
                .role
                .as_ref()
                .map_or_else(|| "Choisir…".to_string(), |r| r.name.clone())
                .into(),
        );
        g.set_liaison_portee(
            e.liaison
                .portee
                .as_ref()
                .map_or_else(
                    || "Partout".to_string(),
                    |(_, n)| format!("Une ressource : {n}"),
                )
                .into(),
        );
        g.set_liaison_a_portee(e.liaison.portee.is_some());
        g.set_liaison_ressource(
            e.liaison
                .ressource
                .as_ref()
                .map(|r| r.name.clone())
                .unwrap_or_default()
                .into(),
        );
        g.set_liaison_valide(
            e.liaison.role.is_some()
                && (e.liaison.portee.is_none() || e.liaison.ressource.is_some()),
        );

        let choisie = e.regle.permission.as_ref().and_then(|p| e.noms.get(p));
        g.set_regle_permission(choisie.map(|p| p.label.clone()).unwrap_or_default().into());
        g.set_regle_description(
            choisie
                .map(|p| {
                    format!(
                        "{}{}",
                        p.description,
                        if p.resource_type.is_some() {
                            " · se restreint à une ressource"
                        } else {
                            ""
                        }
                    )
                })
                .unwrap_or_default()
                .into(),
        );
        g.set_permissions_trouvees(modele_de(
            Self::permissions_trouvees(&e)
                .iter()
                .map(|p| {
                    let libelle = if p.category.is_empty() {
                        p.label.clone()
                    } else {
                        format!("{} · {}", p.category, p.label)
                    };
                    choix(&p.name, &libelle, &p.name)
                })
                .collect(),
        ));
        g.set_regle_effet(if e.regle.refus { "Refuser" } else { "Accorder" }.into());
        g.set_regle_accorder(!e.regle.refus);
        g.set_regle_a_type(choisie.is_some_and(|p| p.resource_type.is_some()));
        g.set_regle_ressource(
            e.regle
                .ressource
                .as_ref()
                .map_or_else(|| "partout".to_string(), |r| r.name.clone())
                .into(),
        );
        g.set_groupes_libres_etat(e.groupes_libres.etat());
        g.set_groupes_libres(modele_de(
            Self::groupes_libres(&e)
                .iter()
                .map(|x| choix(&x.id, &x.name, ""))
                .collect(),
        ));
        g.set_groupe_libre(
            e.groupe_libre
                .as_ref()
                .map(|x| x.name.clone())
                .unwrap_or_default()
                .into(),
        );
        g.set_comptes_trouves(modele_de(
            e.comptes_trouves
                .iter()
                .flat_map(|p| &p.accounts)
                .map(|c| {
                    let note = match (&c.compte.display_name, &c.compte.email) {
                        (Some(n), Some(_)) => format!("{n} · {}", c.compte.sub),
                        _ => c.compte.sub.clone(),
                    };
                    choix(
                        &c.compte.sub,
                        &nom_compte(Some(&c.compte), &c.compte.sub),
                        &note,
                    )
                })
                .collect(),
        ));
        g.set_comptes_trouves_vide(
            e.comptes_trouves
                .as_ref()
                .is_some_and(|p| p.accounts.is_empty()),
        );
        g.set_ressources_trouvees(modele_de(
            e.ressources_trouvees
                .iter()
                .flatten()
                .map(|r| choix(&r.id, &r.name, &r.id))
                .collect(),
        ));
        g.set_ressources_chargees(e.ressources_trouvees.is_some());

        g.set_roles_etat(e.roles.etat());
        let compte_app = |cle: &str| -> usize {
            let Some(x) = &e.editeur else { return 0 };
            catalogue.apps.iter().find(|a| a.key == cle).map_or(0, |a| {
                a.permissions
                    .iter()
                    .filter(|p| {
                        p.obsolete_at.is_none()
                            && (x.joker.contains_key(&p.name)
                                || x.precises.iter().any(|r| r.permission == p.name))
                    })
                    .count()
            })
        };
        g.set_apps(modele_de(
            catalogue
                .apps
                .iter()
                .map(|a| a.name.clone().into())
                .collect(),
        ));
        g.set_role_apps(modele_de(
            catalogue
                .apps
                .iter()
                .map(|a| format!("{} · {}", a.name, compte_app(&a.key)).into())
                .collect(),
        ));
        let app_index = e.app.min(catalogue.apps.len().saturating_sub(1));
        g.set_app(app_index as i32);
        let roles = e.roles.donnees.clone().unwrap_or_default();
        g.set_colonnes(modele_de(
            roles
                .iter()
                .map(|r| ColonneRoleAcces {
                    id: r.id.clone().into(),
                    nom: r.name.clone().into(),
                    qui: qui_a_le_role(r).into(),
                })
                .collect(),
        ));
        let mut grille = Vec::new();
        for c in categories(catalogue.apps.get(app_index), |_| true) {
            grille.push(LigneGrilleAcces {
                categorie: true,
                titre: c.nom.to_uppercase().into(),
                ..LigneGrilleAcces::default()
            });
            for p in c.permissions {
                grille.push(LigneGrilleAcces {
                    categorie: false,
                    titre: "".into(),
                    permission: p.name.clone().into(),
                    libelle: p.label.clone().into(),
                    cases: modele_de(
                        roles
                            .iter()
                            .map(|r| {
                                let x = cellule(e.details.get(&r.id), &p.name);
                                let ex = if x.exceptions > 0 {
                                    exceptions(x.exceptions)
                                } else {
                                    String::new()
                                };
                                CaseRoleAcces {
                                    effet: x.effet.map_or("", Effet::cle).into(),
                                    exceptions: ex.clone().into(),
                                    description: format!(
                                        "{} {} {}{}",
                                        r.name,
                                        mot_effet(x.effet),
                                        p.label,
                                        if ex.is_empty() {
                                            String::new()
                                        } else {
                                            format!(", {ex}")
                                        }
                                    )
                                    .into(),
                                }
                            })
                            .collect(),
                    ),
                });
            }
        }
        g.set_grille(modele_de(grille));

        g.set_role_ouvert(e.editeur.is_some());
        if let Some(x) = &e.editeur {
            g.set_role_etat(x.role.etat());
            if let Some(r) = &x.role.donnees {
                let systeme = r.resume.system;
                g.set_role_titre(r.resume.name.clone().into());
                g.set_role_note(note_role(r).into());
                g.set_role_systeme(systeme.is_some());
                g.set_role_tout_le_monde(systeme == Some(Systeme::ToutLeMonde));
                g.set_role_alerte(
                    match systeme {
                        Some(Systeme::ToutLeMonde) => "Ce rôle s'applique à tous les comptes connectés. Retirer une permission ici la ferme pour tous, sauf pour qui la reçoit par un autre rôle, un groupe ou une règle directe.",
                        Some(Systeme::Admin) => "Ce rôle donne aegis:admin : l'administration de toutes les permissions.",
                        None => "",
                    }
                    .into(),
                );
                g.set_role_alerte_danger(systeme == Some(Systeme::Admin));
                g.set_role_tous(
                    format!(
                        "Tous les comptes ({} dans l'annuaire).",
                        r.resume.accounts_count
                    )
                    .into(),
                );
                let qui = |genre: Sujet| -> Vec<LigneQuiAcces> {
                    r.bindings
                        .iter()
                        .filter(|l| (l.subject_kind == Sujet::Group) == (genre == Sujet::Group))
                        .map(|l| {
                            let ou = match &l.scope_id {
                                Some(s) => {
                                    format!("sur {}", nom_res(&e, l.scope_type.as_deref(), s))
                                }
                                None => "partout".into(),
                            };
                            let jusqu = l
                                .expires_at
                                .as_deref()
                                .map(|d| format!(" · jusqu'au {}", quand(Some(d), dec)))
                                .unwrap_or_default();
                            LigneQuiAcces {
                                id: l.id.clone().into(),
                                nom: l
                                    .subject_label
                                    .clone()
                                    .unwrap_or_else(|| l.subject_id.clone())
                                    .into(),
                                details: format!("{ou}{jusqu}").into(),
                            }
                        })
                        .collect()
                };
                g.set_role_groupes(modele_de(qui(Sujet::Group)));
                g.set_role_comptes(modele_de(qui(Sujet::Account)));
                let regles = Self::regles_saisies(x);
                let touchees = permissions_modifiees(&x.depart, &regles);
                let nom = g.get_role_nom().to_string();
                let description = g.get_role_description().to_string();
                let autres = usize::from(nom != r.resume.name)
                    + usize::from(description != r.resume.description);
                let modifie = touchees.len() + autres > 0;
                g.set_role_modifie(modifie);
                let libelles: Vec<String> = touchees
                    .iter()
                    .map(|t| libelle_permission(&e.noms, t))
                    .collect();
                g.set_role_resume(
                    if modifie {
                        resume_modifications(&libelles, autres)
                    } else {
                        format!(
                            "{} règle{} · aucune modification",
                            regles.len(),
                            if regles.len() > 1 { "s" } else { "" }
                        )
                    }
                    .into(),
                );
                let seulement = g.get_role_seulement_defini();
                let mut lignes = Vec::new();
                for (cat, perms) in self.lignes_editeur(&e, &g.get_role_filtre(), seulement) {
                    lignes.push(LigneRegleRoleAcces {
                        categorie: true,
                        titre: cat.into(),
                        ..LigneRegleRoleAcces::default()
                    });
                    for p in perms {
                        let mut note = format!(
                            "{} · {}",
                            p.name,
                            if p.open_default {
                                "ouverte par défaut"
                            } else {
                                "fermée par défaut"
                            }
                        );
                        if !p.description.is_empty() {
                            note.push_str(&format!(" · {}", p.description));
                        }
                        let t = p.resource_type.as_ref().map(|t| format!("{}:{t}", p.app));
                        lignes.push(LigneRegleRoleAcces {
                            categorie: false,
                            titre: "".into(),
                            permission: p.name.clone().into(),
                            libelle: p.label.clone().into(),
                            note: note.into(),
                            choix: match x.joker.get(&p.name) {
                                Some(Effet::Allow) => 1,
                                Some(Effet::Deny) => 2,
                                None => 0,
                            },
                            modifie: touchees.contains(&p.name),
                            a_ressource: t.is_some(),
                            exceptions: modele_de(
                                x.precises
                                    .iter()
                                    .filter(|r| r.permission == p.name)
                                    .map(|r| ExceptionAcces {
                                        libelle: format!(
                                            "{} sur {}",
                                            if r.effect == Effet::Allow {
                                                "autorisé"
                                            } else {
                                                "refusé"
                                            },
                                            nom_res(&e, t.as_deref(), &r.resource)
                                        )
                                        .into(),
                                        refus: r.effect == Effet::Deny,
                                    })
                                    .collect(),
                            ),
                            ajout: x.ajout_pour.as_deref() == Some(p.name.as_str()),
                        });
                    }
                }
                g.set_role_cible(
                    x.cible
                        .as_ref()
                        .and_then(|c| lignes.iter().position(|l| l.permission.as_str() == c))
                        .map_or(-1, |i| i as i32),
                );
                g.set_role_regles(modele_de(lignes));
                g.set_role_vide(
                    if seulement {
                        "Ce rôle ne définit rien dans cette application."
                    } else {
                        "Aucune permission ne correspond."
                    }
                    .into(),
                );
                g.set_exception_effet(
                    if x.ajout_effet == Effet::Allow {
                        "Autoriser sur…"
                    } else {
                        "Refuser sur…"
                    }
                    .into(),
                );
            }
        }

        g.set_groupes_etat(e.groupes.etat());
        let groupes = e.groupes.donnees.clone().unwrap_or_default();
        g.set_groupes(modele_de(
            groupes
                .iter()
                .map(|x| choix(&x.id, &x.name, &resume_groupe(x)))
                .collect(),
        ));
        g.set_groupe_choisi(e.groupe_choisi.as_ref().map_or(-1, |id| {
            groupes
                .iter()
                .position(|x| &x.id == id)
                .map_or(-1, |i| i as i32)
        }));
        if e.groupe_choisi.is_some() && g.get_groupe_choisi() < 0 {
            g.set_groupe_choisi(i32::MAX);
        }
        g.set_groupe_etat(e.groupe.etat());
        if let Some(gr) = &e.groupe.donnees {
            g.set_groupe_nom(gr.resume.name.clone().into());
            g.set_membres(modele_de(
                gr.members
                    .iter()
                    .map(|m| LigneMembreAcces {
                        sub: m.account.sub.clone().into(),
                        nom: nom_compte(Some(&m.account), &m.account.sub).into(),
                        supprime: !m.account.in_keycloak,
                        expire: expire_ou_jamais(m.expires_at.as_deref(), dec).into(),
                        ajoute: quand(Some(&m.added_at), dec).into(),
                    })
                    .collect(),
            ));
            g.set_liaisons(modele_de(
                gr.bindings
                    .iter()
                    .map(|l| LigneLiaisonAcces {
                        id: l.id.clone().into(),
                        titre: l.role.clone().into(),
                        genre: l.subject_kind.libelle().into(),
                        portee: l
                            .scope_id
                            .as_deref()
                            .map_or_else(
                                || "partout".to_string(),
                                |s| nom_res(&e, l.scope_type.as_deref(), s),
                            )
                            .into(),
                        expire: expire_ou_jamais(l.expires_at.as_deref(), dec).into(),
                        depuis: quand(Some(&l.created_at), dec).into(),
                    })
                    .collect(),
            ));
            g.set_regles(modele_de(
                gr.rules
                    .iter()
                    .map(|r| LigneRegleAcces {
                        id: r.id.clone().into(),
                        libelle: libelle_permission(&e.noms, &r.permission).into(),
                        permission: r.permission.clone().into(),
                        accordee: r.effect == Effet::Allow,
                        portee: nom_res(
                            &e,
                            type_de(&e.noms, &r.permission).as_deref(),
                            &r.resource,
                        )
                        .into(),
                        expire: expire_ou_jamais(r.expires_at.as_deref(), dec).into(),
                        note: if r.note.is_empty() {
                            "—".into()
                        } else {
                            r.note.clone().into()
                        },
                        par: r.granted_by.clone().into(),
                        obsolete: r.obsolete,
                    })
                    .collect(),
            ));
        }

        let n_obsoletes = obsoletes(&catalogue);
        g.set_obsoletes(if n_obsoletes > 0 {
            format!("{n_obsoletes} permission(s) ne sont plus déclarées par leur application : leurs règles sont gardées mais ignorées.").into()
        } else {
            "".into()
        });
        let mut lignes = Vec::new();
        for a in &catalogue.apps {
            let mut note = format!("manifeste reçu {}", quand(Some(&a.seen_at), dec));
            if !a.resource_types.is_empty() {
                note.push_str(&format!(
                    " · ressources : {}",
                    a.resource_types
                        .iter()
                        .map(|t| t.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            lignes.push(LigneCatalogueAcces {
                en_tete: true,
                titre: a.name.clone().into(),
                cle: a.key.clone().into(),
                note: note.into(),
                ..LigneCatalogueAcces::default()
            });
            for p in &a.permissions {
                lignes.push(LigneCatalogueAcces {
                    en_tete: false,
                    permission: p.name.clone().into(),
                    libelle: p.label.clone().into(),
                    description: p.description.clone().into(),
                    categorie: if p.category.is_empty() {
                        "—".into()
                    } else {
                        p.category.clone().into()
                    },
                    ressource: p.resource_type.clone().unwrap_or_else(|| "—".into()).into(),
                    ouverte: p.open_default,
                    regles: p.rules_count.to_string().into(),
                    obsolete: p.obsolete_at.is_some(),
                    depliee: e.depliee.as_deref() == Some(p.name.as_str()),
                    ..LigneCatalogueAcces::default()
                });
            }
        }
        g.set_lignes_catalogue(modele_de(lignes));
        g.set_detenteurs_etat(e.detenteurs.etat());
        g.set_detenteurs(modele_de(
            e.detenteurs
                .donnees
                .iter()
                .flatten()
                .map(|d| {
                    let mut details = String::new();
                    if d.rule.resource != "*" {
                        details.push_str(&format!(
                            "· {}",
                            nom_res(
                                &e,
                                type_de(&e.noms, &d.rule.permission).as_deref(),
                                &d.rule.resource
                            )
                        ));
                    }
                    if let Some(x) = &d.rule.expires_at {
                        details.push_str(&format!(" · jusqu'au {}", quand(Some(x), dec)));
                    }
                    DetenteurAcces {
                        accorde: d.rule.effect == Effet::Allow,
                        sujet: d.rule.subject_kind.libelle().into(),
                        nom: d
                            .subject_label
                            .clone()
                            .unwrap_or_else(|| d.rule.subject_id.clone())
                            .into(),
                        details: details.trim().to_string().into(),
                    }
                })
                .collect(),
        ));

        g.set_action(
            if e.action.is_empty() {
                "Toutes".to_string()
            } else {
                libelle_action(&e.action).to_string()
            }
            .into(),
        );
        g.set_journal_etat(e.journal.etat());
        g.set_journal(modele_de(
            e.journal
                .donnees
                .iter()
                .flatten()
                .map(|l| self.ligne_journal(&e, l))
                .collect(),
        ));
    }
}

fn modele_de<T: Clone + 'static>(lignes: Vec<T>) -> ModelRc<T> {
    modele(lignes)
}

pub fn brancher(f: &Principale, source: impl Fn() -> Option<PontAcces> + Clone + 'static) {
    let g = f.global::<DonneesAcces>();
    macro_rules! rappel {
        ($methode:ident, |$p:ident, $f:ident $(, $arg:ident)*| $corps:expr) => {{
            let source = source.clone();
            let faible = f.as_weak();
            g.$methode(move |$($arg),*| {
                if let (Some($p), Some($f)) = (source(), faible.upgrade()) {
                    $corps;
                }
            });
        }};
    }
    let indice = |i: i32| i.max(0) as usize;
    rappel!(on_choisir_onglet, |p, f, id| p.choisir_onglet(&f, &id));
    rappel!(on_deplacer_onglet, |p, f, pas| p.deplacer_onglet(&f, pas));
    rappel!(on_recharger, |p, f, cle| p.recharger(&f, &cle));
    rappel!(on_ouvrir_choix, |p, f, quel, x, haut, bas| p
        .ouvrir_choix(&f, &quel, x, haut, bas));
    rappel!(on_chercher_comptes, |p, f, texte| p
        .chercher_comptes(&f, &texte));
    rappel!(on_comptes_defiles, |p, f, defilement, vue| p
        .comptes_defiles(&f, defilement, vue));
    rappel!(on_ouvrir_compte, |p, f, sub| p.ouvrir_compte(&f, &sub));
    rappel!(on_fermer_compte, |p, f| p.fermer_compte(&f));
    rappel!(on_basculer_historique, |p, f| p.basculer_historique(&f));
    rappel!(on_basculer_formulaire, |p, f, quel| p
        .basculer_formulaire(&f, &quel));
    rappel!(on_menu_compte, |p, f, x, haut, bas| p
        .menu_compte(&f, x, haut, bas));
    rappel!(on_retirer_puce, |p, f, i| p.retirer_puce(&f, indice(i)));
    rappel!(on_puces_mesurees, |p, f, largeur| p
        .puces_mesurees(&f, largeur));
    rappel!(on_choisir_filtre, |p, f, i| p.choisir_filtre(&f, indice(i)));
    rappel!(on_pourquoi, |p, f, permission| p.pourquoi(&f, &permission));
    rappel!(on_afficher_heritees, |p, f| p.afficher_heritees(&f));
    rappel!(on_essai_partout, |p, f| p.essai_partout(&f));
    rappel!(on_copier, |p, f| p.copier(&f));
    rappel!(on_changer_source, |p, f| p.changer_source(&f));
    rappel!(on_donner_liaison, |p, f| p.donner_liaison(&f));
    rappel!(on_changer_ressource_liaison, |p, f| p
        .changer_ressource_liaison(&f));
    rappel!(on_chercher_permissions, |p, f, texte| p
        .chercher_permissions(&f, &texte));
    rappel!(on_choisir_permission, |p, f, i| p.choisir_permission(&f, i));
    rappel!(on_regle_partout, |p, f| p.regle_partout(&f));
    rappel!(on_ajouter_regle, |p, f| p.ajouter_regle(&f));
    rappel!(on_rejoindre_groupe, |p, f| p.rejoindre_groupe(&f));
    rappel!(on_chercher_compte, |p, f, texte| p
        .chercher_compte(&f, &texte));
    rappel!(on_choisir_compte, |p, f, i| p.choisir_compte(&f, indice(i)));
    rappel!(on_chercher_ressource, |p, f, texte| p
        .chercher_ressource(&f, &texte));
    rappel!(on_choisir_ressource, |p, f, i| p
        .choisir_ressource(&f, indice(i)));
    rappel!(on_choisir_app, |p, f, i| p.choisir_app(&f, indice(i)));
    rappel!(on_ouvrir_role, |p, f, id, cible| p
        .ouvrir_role(&f, &id, &cible));
    rappel!(on_creer_role, |p, f| p.creer_role(&f));
    rappel!(on_fermer_role, |p, f| p.fermer_role(&f));
    rappel!(on_role_edite, |p, f| p.projeter(&f));
    rappel!(on_choisir_app_role, |p, f, i| p.choisir_app(&f, indice(i)));
    rappel!(on_regler, |p, f, permission, choix| p.regler(
        &f,
        &permission,
        choix
    ));
    rappel!(on_tout_regler, |p, f, categorie, choix| p
        .tout_regler(&f, &categorie, choix));
    rappel!(on_ouvrir_exception, |p, f, permission| p
        .ouvrir_exception(&f, &permission));
    rappel!(on_retirer_exception, |p, f, permission, i| p
        .retirer_exception(&f, &permission, indice(i)));
    rappel!(on_annuler_role, |p, f| p.annuler_role(&f));
    rappel!(on_enregistrer_role, |p, f| p.enregistrer_role(&f));
    rappel!(on_supprimer_role, |p, f| p.supprimer_role(&f));
    rappel!(on_retirer_qui, |p, f, id| p.retirer_qui(&f, &id));
    rappel!(on_choisir_groupe, |p, f, i| p.choisir_groupe(&f, indice(i)));
    rappel!(on_creer_groupe, |p, f| p.creer_groupe(&f));
    rappel!(on_supprimer_groupe, |p, f| p.supprimer_groupe(&f));
    rappel!(on_retirer_membre, |p, f, i| p.retirer_membre(&f, indice(i)));
    rappel!(on_retirer_liaison, |p, f, i| p
        .retirer_liaison(&f, indice(i)));
    rappel!(on_retirer_regle, |p, f, i| p.retirer_regle(&f, indice(i)));
    rappel!(on_purger, |p, f| p.purger(&f));
    rappel!(on_deplier, |p, f, permission| p.deplier(&f, &permission));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn puce(nom: &str) -> PuceAcces {
        PuceAcces {
            nom: nom.into(),
            ..PuceAcces::default()
        }
    }

    #[test]
    fn range_les_puces_selon_la_largeur() {
        let puces = vec![
            puce("Tout le monde"),
            puce("Modérateurs"),
            puce("Équipe"),
            puce("x"),
        ];
        let rangees = ranger_puces(puces.clone(), 10_000.0);
        assert_eq!(rangees.len(), 1);
        let etroit = ranger_puces(puces.clone(), 150.0);
        assert_eq!(etroit.len(), 3);
        assert_eq!(etroit.iter().map(Vec::len).sum::<usize>(), 4);
        assert!(ranger_puces(Vec::new(), 100.0).is_empty());
    }
}
