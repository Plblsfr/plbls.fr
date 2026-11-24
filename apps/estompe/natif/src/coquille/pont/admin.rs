use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::admin::{
    activite_du_compte, attribuer_a, choix_de_detenteurs, configure, couleur_valide,
    destinataire_du_bot, detail_livekit, detenteurs, filtrer_badges, giga_octets,
    jour_d_inscription, mega_octets, message_de_suppression, nom_de_badge_valide, nombre,
    ordre_saisi, probleme_d_icone, section_admin, titre_de, type_d_icone, usage_stockage,
    ActionJournal, BadgeAdmin, Bot, CompteAdmin, Direct, ErreurServeur, SectionAdmin, Statistiques,
    TriBadges, VueEnsemble, EXTENSIONS_ICONE, TRIS_BADGES,
};
use estompe_client::creation::message_d_erreur;
use estompe_client::format::date_heure_complete;
use estompe_client::http::{Client, ErreurApi};
use estompe_client::profil::Recadrage;
use estompe_client::types::PostAdminBadgesRequete;
use estompe_plateforme::FiltreFichiers;
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};

use super::super::avatars::decoder_icone_badge;
use super::super::profil::{avatar_recadre, ouvrir_image};
use super::super::projection::{initiales, teinte};
use super::acces::PontAcces;
use super::couleur_slint;
use super::signalements::PontSignalements;
use crate::confirmation::{self, Demande, Saisie};
use crate::messagerie::pont::SystemeFichiers;
use crate::{
    ActionAdmin, AppareilAdmin, BadgeAdminCarte, CleValeurAdmin, CompteAdminLigne, DetenteurAdmin,
    DonneesAdmin, DonneesCoquille, ErreurAdmin, OngletAdmin, Principale, ServeurAdminLigne,
    TuileAdmin, VocalAdmin,
};

pub const PERIODE_DIRECT: Duration = Duration::from_secs(5);
pub const ATTENTE_RECHERCHE: Duration = Duration::from_millis(200);
pub const COULEUR_PAR_DEFAUT: &str = "#3b82f6";
pub const FILTRE_ICONES: FiltreFichiers = FiltreFichiers {
    nom: "Images SVG, WebP, PNG ou JPEG",
    extensions: EXTENSIONS_ICONE,
};
const IMAGE_ILLISIBLE: &str = "Image illisible.";

pub type Decalage = Arc<dyn Fn() -> i32 + Send + Sync>;

#[derive(Default, Clone)]
struct Formulaire {
    id: Option<String>,
    icone: Option<(&'static str, Vec<u8>)>,
    retirer: bool,
    message: String,
    trouves: Vec<CompteAdmin>,
    choisis: Vec<CompteAdmin>,
    recherche: u64,
}

struct EtatAdmin {
    sections: Vec<SectionAdmin>,
    seule: bool,
    onglet: &'static str,
    generation: u64,
    erreur: String,
    info: String,
    en_cours: bool,
    recherche: String,
    comptes: Vec<CompteAdmin>,
    vue: Option<VueEnsemble>,
    badges: Vec<BadgeAdmin>,
    recherche_badges: String,
    tri: TriBadges,
    formulaire: Option<Formulaire>,
    bot: Bot,
    stats: Option<Statistiques>,
    direct: Option<Direct>,
    actions: Vec<ActionJournal>,
    erreurs: Vec<ErreurServeur>,
    icones: HashMap<(String, i64), Vec<u8>>,
    demandes: HashSet<(String, i64)>,
}

impl Default for EtatAdmin {
    fn default() -> Self {
        Self {
            sections: Vec::new(),
            seule: false,
            onglet: "",
            generation: 0,
            erreur: String::new(),
            info: String::new(),
            en_cours: false,
            recherche: String::new(),
            comptes: Vec::new(),
            vue: None,
            badges: Vec::new(),
            recherche_badges: String::new(),
            tri: TriBadges::Ordre,
            formulaire: None,
            bot: Bot::default(),
            stats: None,
            direct: None,
            actions: Vec::new(),
            erreurs: Vec::new(),
            icones: HashMap::new(),
            demandes: HashSet::new(),
        }
    }
}

#[derive(Default)]
pub struct DonneesAdminChargees {
    pub comptes: Vec<CompteAdmin>,
    pub vue: Option<VueEnsemble>,
    pub badges: Vec<BadgeAdmin>,
    pub bot: Bot,
    pub stats: Option<Statistiques>,
    pub direct: Option<Direct>,
    pub actions: Vec<ActionJournal>,
    pub erreurs: Vec<ErreurServeur>,
}

#[derive(Clone)]
pub struct PontAdmin {
    interne: Arc<Mutex<EtatAdmin>>,
    client: Client,
    fichiers: Arc<Mutex<Option<SystemeFichiers>>>,
    fenetre: slint::Weak<Principale>,
    runtime: tokio::runtime::Handle,
    decalage: Decalage,
    acces: PontAcces,
    signalements: PontSignalements,
}

fn modele<T: Clone + 'static>(lignes: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(lignes))
}

fn detenteur(id: &str, pseudo: &str) -> DetenteurAdmin {
    DetenteurAdmin {
        id: id.into(),
        pseudo: pseudo.into(),
        initiales: initiales(pseudo).into(),
        teinte: teinte(id),
    }
}

fn cle(cle: &str, valeur: String, etiquette: bool, ok: bool) -> CleValeurAdmin {
    CleValeurAdmin {
        cle: cle.into(),
        valeur: valeur.into(),
        etiquette,
        ok,
    }
}

fn accent(couleur: &str) -> slint::Color {
    couleur_slint(Some(couleur)).unwrap_or_else(|| slint::Color::from_rgb_u8(0x3b, 0x82, 0xf6))
}

impl PontAdmin {
    pub fn nouveau(
        client: Client,
        fenetre: slint::Weak<Principale>,
        runtime: tokio::runtime::Handle,
        decalage: Decalage,
    ) -> Self {
        let signalements = PontSignalements::nouveau(
            client.clone(),
            fenetre.clone(),
            runtime.clone(),
            decalage.clone(),
        );
        Self {
            interne: Arc::new(Mutex::new(EtatAdmin::default())),
            acces: PontAcces::nouveau(
                client.clone(),
                fenetre.clone(),
                runtime.clone(),
                decalage.clone(),
            ),
            client,
            fichiers: Arc::new(Mutex::new(None)),
            fenetre,
            runtime,
            decalage,
            signalements,
        }
    }

    pub fn signalements(&self) -> &PontSignalements {
        &self.signalements
    }

    pub fn definir_fichiers(&self, fichiers: SystemeFichiers) {
        self.signalements.definir_fichiers(fichiers.clone());
        *self.fichiers.lock().unwrap_or_else(|e| e.into_inner()) = Some(fichiers);
    }

    fn verrou(&self) -> MutexGuard<'_, EtatAdmin> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn onglet(&self) -> &'static str {
        self.verrou().onglet
    }

    pub fn acces(&self) -> &PontAcces {
        &self.acces
    }

    pub fn definir_sections(&self, f: &Principale, sections: Vec<SectionAdmin>) {
        let g = f.global::<DonneesAdmin>();
        let (retenu, change) = {
            let mut e = self.verrou();
            let change = e.sections != sections;
            e.sections = sections;
            (section_admin(Some(e.onglet), &e.sections), change)
        };
        g.set_disponible(retenu.is_some() && !self.verrou().seule);
        if !change {
            return;
        }
        self.projeter_onglets(f);
        if !g.get_ouverte() {
            return;
        }
        match retenu {
            None => self.fermer(f),
            Some(id) if id != self.onglet() => self.choisir(f, id),
            Some(_) => {}
        }
    }

    pub fn ouvrir(&self, f: &Principale) {
        let Some(id) = ({
            let e = self.verrou();
            section_admin(Some(e.onglet), &e.sections)
        }) else {
            return;
        };
        let g = f.global::<DonneesAdmin>();
        g.set_focus_initial(true);
        g.set_ouverte(true);
        self.choisir(f, id);
    }

    pub fn ouvrir_seule(&self, f: &Principale, sections: Vec<SectionAdmin>) {
        self.verrou().seule = true;
        f.global::<DonneesAdmin>().set_seule(true);
        self.definir_sections(f, sections);
        self.ouvrir(f);
    }

    pub fn fermer(&self, f: &Principale) {
        self.verrou().generation += 1;
        let g = f.global::<DonneesAdmin>();
        if !g.get_ouverte() {
            return;
        }
        g.set_ouverte(false);
        self.fermer_badge(f);
        let c = f.global::<DonneesCoquille>();
        c.set_demande_focus(c.get_demande_focus().wrapping_add(1));
    }

    pub fn oublier(&self, f: &Principale) {
        self.verrou().generation += 1;
        let g = f.global::<DonneesAdmin>();
        g.set_ouverte(false);
        g.set_seule(false);
        g.set_disponible(false);
        g.set_badge_ouvert(false);
        self.signalements.oublier(f);
    }

    pub fn choisir(&self, f: &Principale, id: &str) {
        let generation = {
            let mut e = self.verrou();
            let Some(retenu) = section_admin(Some(id), &e.sections) else {
                return;
            };
            e.onglet = retenu;
            e.generation += 1;
            e.erreur.clear();
            e.info.clear();
            e.generation
        };
        self.projeter_onglets(f);
        self.projeter(f);
        match self.onglet() {
            "utilisateurs" => self.charger_comptes(f),
            "serveurs" => self.charger_vue(),
            "badges" => self.charger_badges(),
            "bot" => self.charger_bot(),
            "journaux" => self.charger_journaux(),
            "signalements" => self.signalements.charger_admin(f),
            "statistiques" => self.charger_statistiques(generation),
            "acces" => self.acces.ouvrir(f),
            _ => {}
        }
    }

    pub fn deplacer(&self, f: &Principale, pas: i32) {
        let (ids, actuel) = {
            let e = self.verrou();
            (
                e.sections.iter().map(|s| s.id).collect::<Vec<_>>(),
                e.onglet,
            )
        };
        let Some(i) = ids.iter().position(|id| *id == actuel) else {
            return;
        };
        let n = ids.len() as i32;
        let cible = (i as i32 + pas).clamp(0, n - 1) as usize;
        self.choisir(f, ids[cible]);
        let g = f.global::<DonneesAdmin>();
        g.set_retour_focus(g.get_retour_focus().wrapping_add(1));
    }

    fn lancer<T: Send + 'static>(
        &self,
        tache: impl Future<Output = Result<T, ErreurApi>> + Send + 'static,
        suite: impl FnOnce(&PontAdmin, &Principale, T) + Send + 'static,
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
                match resultat {
                    Ok(valeur) => {
                        p.verrou().erreur.clear();
                        suite(&p, &f, valeur);
                    }
                    Err(erreur) => p.verrou().erreur = message_d_erreur(&erreur),
                }
                p.projeter(&f);
            });
        });
    }

    fn agir<T: Send + 'static>(
        &self,
        f: &Principale,
        tache: impl Future<Output = Result<T, ErreurApi>> + Send + 'static,
        suite: impl FnOnce(&PontAdmin, &Principale, T) + Send + 'static,
    ) {
        {
            let mut e = self.verrou();
            if e.en_cours {
                return;
            }
            e.en_cours = true;
            e.erreur.clear();
            e.info.clear();
        }
        self.projeter(f);
        let pont = self.clone();
        self.runtime.spawn(async move {
            let resultat = tache.await;
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                p.verrou().en_cours = false;
                match resultat {
                    Ok(valeur) => suite(&p, &f, valeur),
                    Err(erreur) => {
                        let message = message_d_erreur(&erreur);
                        let mut e = p.verrou();
                        match e.formulaire.as_mut() {
                            Some(formulaire) => formulaire.message = message,
                            None => e.erreur = message,
                        }
                    }
                }
                p.projeter(&f);
            });
        });
    }

    pub fn chercher_comptes(&self, f: &Principale) {
        self.verrou().recherche = f.global::<DonneesAdmin>().get_recherche().to_string();
        self.charger_comptes(f);
    }

    fn charger_comptes(&self, _f: &Principale) {
        let client = self.client.clone();
        let recherche = self.verrou().recherche.trim().to_string();
        self.lancer(
            async move { client.comptes(&recherche).await },
            |p, _, l| p.verrou().comptes = l,
        );
    }

    fn charger_vue(&self) {
        let client = self.client.clone();
        self.lancer(
            async move { client.vue_d_ensemble_admin().await },
            |p, _, v| p.verrou().vue = Some(v),
        );
    }

    fn charger_badges(&self) {
        let client = self.client.clone();
        self.lancer(async move { client.badges_admin().await }, |p, f, l| {
            p.verrou().badges = l;
            p.demander_icones(f);
        });
    }

    fn charger_bot(&self) {
        let client = self.client.clone();
        self.lancer(async move { client.bot_admin().await }, |p, _, b| {
            p.verrou().bot = b
        });
    }

    fn charger_journaux(&self) {
        let client = self.client.clone();
        self.lancer(
            async move {
                let actions = client.journal_admin().await?;
                Ok((actions, client.erreurs_admin().await?))
            },
            |p, _, (actions, erreurs)| {
                let mut e = p.verrou();
                e.actions = actions;
                e.erreurs = erreurs;
            },
        );
    }

    fn charger_statistiques(&self, generation: u64) {
        let client = self.client.clone();
        self.lancer(
            async move {
                let stats = client.statistiques_admin().await?;
                Ok((stats, client.vue_d_ensemble_admin().await?))
            },
            |p, _, (stats, vue)| {
                let mut e = p.verrou();
                e.stats = Some(stats);
                e.vue = Some(vue);
            },
        );
        let pont = self.clone();
        self.runtime.spawn(async move {
            loop {
                let resultat = pont.client.direct_admin().await;
                let p = pont.clone();
                let (envoi, reception) = tokio::sync::oneshot::channel();
                let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                    let actif = p.verrou().generation == generation
                        && f.global::<DonneesAdmin>().get_ouverte();
                    if actif {
                        match resultat {
                            Ok(direct) => p.verrou().direct = Some(direct),
                            Err(erreur) => p.verrou().erreur = message_d_erreur(&erreur),
                        }
                        p.projeter(&f);
                    }
                    let _ = envoi.send(actif);
                });
                if reception.await != Ok(true) {
                    return;
                }
                tokio::time::sleep(PERIODE_DIRECT).await;
            }
        });
    }

    pub fn basculer_suspension(&self, f: &Principale, i: usize) {
        let Some(u) = self.verrou().comptes.get(i).cloned() else {
            return;
        };
        let pont = self.clone();
        let suite = move |f: &Principale| {
            let client = pont.client.clone();
            let (id, suspendre) = (u.id.clone(), !u.suspended);
            pont.agir(
                f,
                async move { client.suspendre_compte(&id, suspendre).await },
                |p, f, ()| p.charger_comptes(f),
            );
        };
        if u.suspended {
            suite(f);
            return;
        }
        confirmation::confirmer(
            f,
            Demande {
                titre: "Suspendre le compte".into(),
                message: format!(
                    "Suspendre {} ? Il ne pourra plus utiliser Estompe.",
                    u.pseudo
                ),
                action: "Suspendre".into(),
                destructif: true,
                ..Demande::default()
            },
            move |f, ok| {
                if ok {
                    suite(f);
                }
            },
        );
    }

    pub fn supprimer_serveur(&self, f: &Principale, i: usize) {
        let Some(s) = self
            .verrou()
            .vue
            .as_ref()
            .and_then(|v| v.servers.get(i).cloned())
        else {
            return;
        };
        let pont = self.clone();
        confirmation::confirmer(
            f,
            Demande {
                titre: "Supprimer le serveur".into(),
                message: format!(
                    "Supprimer définitivement le serveur « {} » et tous ses messages ?",
                    s.name
                ),
                action: "Supprimer le serveur".into(),
                destructif: true,
                saisie: Some(Saisie {
                    libelle: "Saisissez le nom du serveur pour confirmer".into(),
                    valeur: String::new(),
                    exiger: Some(s.name.clone()),
                }),
                ..Demande::default()
            },
            move |f, ok| {
                if !ok {
                    return;
                }
                let client = pont.client.clone();
                pont.agir(
                    f,
                    async move { client.supprimer_serveur_en_administrateur(&s.id).await },
                    |p, _, ()| p.charger_vue(),
                );
            },
        );
    }

    pub fn chercher_badges(&self, f: &Principale, texte: &str) {
        self.verrou().recherche_badges = texte.to_string();
        self.projeter(f);
    }

    pub fn choisir_tri(&self, f: &Principale, i: usize) {
        if let Some((tri, _)) = TRIS_BADGES.get(i) {
            self.verrou().tri = *tri;
        }
        self.projeter(f);
    }

    fn badges_affiches(&self) -> Vec<BadgeAdmin> {
        let e = self.verrou();
        filtrer_badges(&e.badges, &e.recherche_badges, e.tri)
    }

    fn demander_icones(&self, _f: &Principale) {
        let manquantes: Vec<(String, i64)> = {
            let mut e = self.verrou();
            let manquantes: Vec<(String, i64)> = e
                .badges
                .iter()
                .filter(|b| b.icon_version > 0)
                .map(|b| (b.id.clone(), b.icon_version))
                .filter(|cle| !e.icones.contains_key(cle) && !e.demandes.contains(cle))
                .collect();
            e.demandes.extend(manquantes.iter().cloned());
            manquantes
        };
        for (id, version) in manquantes {
            let client = self.client.clone();
            let pont = self.clone();
            self.runtime.spawn(async move {
                let Ok(octets) = client.icone_de_badge(&id).await else {
                    return;
                };
                let p = pont.clone();
                let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                    p.verrou().icones.insert((id, version), octets.to_vec());
                    p.projeter(&f);
                });
            });
        }
    }

    fn image_de(&self, badge: &BadgeAdmin) -> Option<Image> {
        if badge.icon_version > 0 {
            let octets = self
                .verrou()
                .icones
                .get(&(badge.id.clone(), badge.icon_version))
                .cloned();
            if let Some(image) = octets.as_deref().and_then(decoder_icone_badge) {
                return Some(image);
            }
        }
        let emoji = badge.icon.trim();
        (!emoji.is_empty())
            .then(|| crate::messagerie::emoji::image(emoji))
            .flatten()
    }

    pub fn nouveau_badge(&self, f: &Principale) {
        self.ouvrir_formulaire(f, None);
    }

    pub fn ouvrir_badge(&self, f: &Principale, i: usize) {
        let badge = self.badges_affiches().get(i).cloned();
        if badge.is_some() {
            self.ouvrir_formulaire(f, badge);
        }
    }

    fn ouvrir_formulaire(&self, f: &Principale, badge: Option<BadgeAdmin>) {
        let g = f.global::<DonneesAdmin>();
        let b = badge.clone().unwrap_or_else(|| BadgeAdmin {
            color: COULEUR_PAR_DEFAUT.into(),
            ..BadgeAdmin::default()
        });
        g.set_badge_nom(b.name.as_str().into());
        g.set_badge_description(b.description.as_str().into());
        g.set_badge_couleur(b.color.as_str().into());
        g.set_badge_ordre(b.sort_order.to_string().into());
        g.set_recherche_detenteurs(SharedString::new());
        self.verrou().formulaire = Some(Formulaire {
            id: badge.map(|b| b.id),
            ..Formulaire::default()
        });
        self.projeter(f);
        g.set_badge_ouvert(true);
    }

    pub fn fermer_badge(&self, f: &Principale) {
        self.verrou().formulaire = None;
        let g = f.global::<DonneesAdmin>();
        g.set_badge_ouvert(false);
        g.set_badge_apercu(Image::default());
    }

    fn badge_ouvert(&self) -> Option<BadgeAdmin> {
        let e = self.verrou();
        let id = e.formulaire.as_ref()?.id.clone()?;
        e.badges.iter().find(|b| b.id == id).cloned()
    }

    pub fn choisir_image_badge(&self, _f: &Principale) {
        let Some(fichiers) = self
            .fichiers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        else {
            return;
        };
        if !fichiers.disponible() {
            return;
        }
        let pont = self.clone();
        let _ = std::thread::Builder::new()
            .name("estompe-icone-badge".into())
            .spawn(move || {
                let Ok(Some(chemin)) =
                    fichiers.choisir_fichier("Choisir une image", &FILTRE_ICONES, None)
                else {
                    return;
                };
                let nom = chemin.to_string_lossy().to_string();
                let taille = std::fs::metadata(&chemin).map(|m| m.len()).unwrap_or(0);
                let octets = std::fs::read(&chemin).unwrap_or_default();
                let p = pont.clone();
                let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                    p.image_badge_choisie(&f, &nom, taille, octets)
                });
            });
    }

    pub fn image_badge_choisie(&self, f: &Principale, nom: &str, taille: u64, octets: Vec<u8>) {
        let type_mime = type_d_icone(nom).unwrap_or("");
        let probleme = probleme_d_icone(type_mime, taille);
        let icone = if !probleme.is_empty() {
            Err(probleme)
        } else if type_mime == "image/svg+xml" {
            Ok((type_mime, octets))
        } else {
            ouvrir_image(&octets)
                .and_then(|image| avatar_recadre(&image, Recadrage::default()).ok())
                .ok_or(IMAGE_ILLISIBLE)
        };
        if let Some(formulaire) = self.verrou().formulaire.as_mut() {
            match icone {
                Ok(icone) => {
                    formulaire.icone = Some(icone);
                    formulaire.retirer = false;
                    formulaire.message.clear();
                }
                Err(message) => formulaire.message = message.to_string(),
            }
        }
        self.projeter(f);
    }

    pub fn retirer_image_badge(&self, f: &Principale) {
        let version = self.badge_ouvert().map_or(0, |b| b.icon_version);
        if let Some(formulaire) = self.verrou().formulaire.as_mut() {
            let avait = formulaire.icone.take().is_some();
            formulaire.retirer = !avait || version > 0;
        }
        self.projeter(f);
    }

    pub fn enregistrer_badge(&self, f: &Principale) {
        let g = f.global::<DonneesAdmin>();
        let valeurs = PostAdminBadgesRequete {
            name: g.get_badge_nom().trim().to_string(),
            color: g.get_badge_couleur().trim().to_string(),
            description: Some(g.get_badge_description().trim().to_string()),
            sort_order: Some(ordre_saisi(&g.get_badge_ordre())),
            ..PostAdminBadgesRequete::default()
        };
        let Some(formulaire) = self.verrou().formulaire.clone() else {
            return;
        };
        if !nom_de_badge_valide(&valeurs.name) || !couleur_valide(&valeurs.color) {
            return;
        }
        let client = self.client.clone();
        self.agir(
            f,
            async move {
                let id = match &formulaire.id {
                    Some(id) => {
                        client.modifier_badge(id, &valeurs).await?;
                        id.clone()
                    }
                    None => client.creer_badge(&valeurs).await?.id,
                };
                if let Some((type_mime, octets)) = formulaire.icone {
                    client
                        .definir_icone_de_badge(&id, type_mime, octets)
                        .await?;
                } else if formulaire.retirer {
                    client.supprimer_icone_de_badge(&id).await?;
                }
                Ok(())
            },
            |p, f, ()| {
                p.fermer_badge(f);
                p.charger_badges();
            },
        );
    }

    pub fn supprimer_badge(&self, f: &Principale) {
        let Some(badge) = self.badge_ouvert() else {
            return;
        };
        let pont = self.clone();
        confirmation::confirmer(
            f,
            Demande {
                titre: "Supprimer le badge".into(),
                message: message_de_suppression(&badge),
                action: "Supprimer".into(),
                destructif: true,
                ..Demande::default()
            },
            move |f, ok| {
                if !ok {
                    return;
                }
                let client = pont.client.clone();
                pont.agir(
                    f,
                    async move { client.supprimer_badge(&badge.id).await },
                    |p, f, ()| {
                        p.fermer_badge(f);
                        p.charger_badges();
                    },
                );
            },
        );
    }

    pub fn retirer_detenteur(&self, f: &Principale, i: usize) {
        let Some((badge, pseudo)) = self
            .badge_ouvert()
            .and_then(|b| b.holders.get(i).map(|h| (b.id.clone(), h.pseudo.clone())))
        else {
            return;
        };
        let client = self.client.clone();
        self.agir(
            f,
            async move { client.retirer_badge(&badge, &pseudo).await },
            |p, _, ()| p.charger_badges(),
        );
    }

    pub fn chercher_detenteurs(&self, f: &Principale, texte: &str) {
        let texte = texte.trim().to_string();
        let demande = {
            let mut e = self.verrou();
            let Some(formulaire) = e.formulaire.as_mut() else {
                return;
            };
            formulaire.recherche += 1;
            if texte.is_empty() {
                formulaire.trouves.clear();
            }
            formulaire.recherche
        };
        if texte.is_empty() {
            self.projeter(f);
            return;
        }
        let pont = self.clone();
        self.runtime.spawn(async move {
            tokio::time::sleep(ATTENTE_RECHERCHE).await;
            if pont
                .verrou()
                .formulaire
                .as_ref()
                .is_none_or(|fo| fo.recherche != demande)
            {
                return;
            }
            let trouves = pont.client.comptes(&texte).await.unwrap_or_default();
            let p = pont.clone();
            let _ = pont.fenetre.upgrade_in_event_loop(move |f| {
                if let Some(formulaire) = p
                    .verrou()
                    .formulaire
                    .as_mut()
                    .filter(|fo| fo.recherche == demande)
                {
                    formulaire.trouves = trouves;
                }
                p.projeter(&f);
            });
        });
    }

    fn propositions(&self) -> Vec<CompteAdmin> {
        let badge = self.badge_ouvert().unwrap_or_default();
        let e = self.verrou();
        e.formulaire
            .as_ref()
            .map(|fo| choix_de_detenteurs(&fo.trouves, &fo.choisis, &badge))
            .unwrap_or_default()
    }

    pub fn choisir_detenteur(&self, f: &Principale, i: usize) {
        let Some(compte) = self.propositions().get(i).cloned() else {
            return;
        };
        if let Some(formulaire) = self.verrou().formulaire.as_mut() {
            formulaire.choisis.push(compte);
            formulaire.trouves.clear();
            formulaire.recherche += 1;
        }
        f.global::<DonneesAdmin>()
            .set_recherche_detenteurs(SharedString::new());
        self.projeter(f);
    }

    pub fn ne_pas_ajouter(&self, f: &Principale, i: usize) {
        if let Some(formulaire) = self.verrou().formulaire.as_mut() {
            if i < formulaire.choisis.len() {
                formulaire.choisis.remove(i);
            }
        }
        self.projeter(f);
    }

    pub fn attribuer_badge(&self, f: &Principale) {
        let Some(badge) = self.badge_ouvert() else {
            return;
        };
        let pseudos: Vec<String> = self
            .verrou()
            .formulaire
            .as_ref()
            .map(|fo| fo.choisis.iter().map(|c| c.pseudo.clone()).collect())
            .unwrap_or_default();
        if pseudos.is_empty() {
            return;
        }
        let client = self.client.clone();
        self.agir(
            f,
            async move { client.accorder_badges(&badge.id, &pseudos).await },
            |p, _, attribution| {
                if let Some(formulaire) = p.verrou().formulaire.as_mut() {
                    formulaire.choisis.clear();
                    formulaire.message = if attribution.missing.is_empty() {
                        String::new()
                    } else {
                        format!("Comptes introuvables : {}", attribution.missing.join(", "))
                    };
                }
                p.charger_badges();
            },
        );
    }

    pub fn envoyer_bot(&self, f: &Principale) {
        let g = f.global::<DonneesAdmin>();
        let contenu = g.get_bot_message().trim().to_string();
        let a = g.get_bot_destinataire().trim().to_string();
        if contenu.is_empty() {
            return;
        }
        let pont = self.clone();
        confirmation::confirmer(
            f,
            Demande {
                titre: "Envoyer le message".into(),
                message: format!(
                    "Envoyer ce message en privé à {} ?",
                    destinataire_du_bot(&a)
                ),
                action: "Envoyer".into(),
                ..Demande::default()
            },
            move |f, ok| {
                if !ok {
                    return;
                }
                let client = pont.client.clone();
                pont.agir(
                    f,
                    async move { client.message_du_bot(&contenu, &a).await },
                    |p, f, envoyes| {
                        p.verrou().info = format!("{envoyes} message(s) envoyé(s)");
                        f.global::<DonneesAdmin>()
                            .set_bot_message(SharedString::new());
                        p.charger_bot();
                    },
                );
            },
        );
    }

    fn projeter_onglets(&self, f: &Principale) {
        let e = self.verrou();
        let g = f.global::<DonneesAdmin>();
        g.set_onglets(modele(
            e.sections
                .iter()
                .map(|s| OngletAdmin {
                    id: s.id.into(),
                    libelle: s.libelle.into(),
                    icone: s.icone.into(),
                })
                .collect(),
        ));
        g.set_onglet(e.onglet.into());
        g.set_titre(
            e.sections
                .iter()
                .find(|s| s.id == e.onglet)
                .map(titre_de)
                .unwrap_or_default()
                .into(),
        );
    }

    pub fn projeter(&self, f: &Principale) {
        let decalage = (self.decalage)();
        let quand = |iso: &str| date_heure_complete(iso, decalage).unwrap_or_default();
        let badges = self.badges_affiches();
        let cartes: Vec<BadgeAdminCarte> = badges
            .iter()
            .map(|b| {
                let image = self.image_de(b);
                BadgeAdminCarte {
                    id: b.id.as_str().into(),
                    nom: b.name.as_str().into(),
                    description: b.description.as_str().into(),
                    detenteurs: detenteurs(b.holders.len()).into(),
                    couleur: accent(&b.color),
                    a_image: image.is_some(),
                    image: image.unwrap_or_default(),
                }
            })
            .collect();
        let ouvert = self.badge_ouvert();
        let propositions = self.propositions();
        let (icone, retirer) = {
            let e = self.verrou();
            e.formulaire.as_ref().map_or((None, false), |fo| {
                (fo.icone.as_ref().map(|(_, o)| o.clone()), fo.retirer)
            })
        };
        let apercu = match (icone, &ouvert) {
            (Some(octets), _) => decoder_icone_badge(&octets),
            (None, Some(b)) if !retirer => self.image_de(b),
            _ => None,
        };
        let e = self.verrou();
        let g = f.global::<DonneesAdmin>();
        g.set_erreur(e.erreur.as_str().into());
        g.set_info(e.info.as_str().into());
        g.set_en_cours(e.en_cours);
        g.set_comptes(modele(
            e.comptes
                .iter()
                .map(|u| CompteAdminLigne {
                    id: u.id.as_str().into(),
                    pseudo: u.pseudo.as_str().into(),
                    inscrit: jour_d_inscription(&u.created_at, decalage).into(),
                    activite: activite_du_compte(u).into(),
                    suspendu: u.suspended,
                })
                .collect(),
        ));
        let vue = e.vue.clone().unwrap_or_default();
        g.set_serveurs(modele(
            vue.servers
                .iter()
                .map(|s| ServeurAdminLigne {
                    id: s.id.as_str().into(),
                    nom: s.name.as_str().into(),
                    initiales: initiales(&s.name).into(),
                    membres: format!("{} membres · {} salons", s.members, s.channels).into(),
                    messages: format!("{} messages", nombre(s.messages)).into(),
                    dernier: s
                        .last_message_at
                        .as_deref()
                        .map_or_else(|| "—".to_string(), |d| format!("dernier {}", quand(d)))
                        .into(),
                })
                .collect(),
        ));
        g.set_tris(modele(
            TRIS_BADGES
                .iter()
                .map(|(_, l)| SharedString::from(*l))
                .collect(),
        ));
        g.set_tri(
            TRIS_BADGES
                .iter()
                .position(|(t, _)| *t == e.tri)
                .unwrap_or(0) as i32,
        );
        g.set_badges(modele(cartes));
        g.set_vide_badges(
            if e.badges.is_empty() {
                "Aucun badge"
            } else {
                "Aucun badge ne correspond"
            }
            .into(),
        );
        if let Some(fo) = &e.formulaire {
            let couleur = g.get_badge_couleur();
            g.set_badge_existant(fo.id.is_some());
            g.set_badge_accent(accent(&couleur));
            g.set_badge_valide(
                nom_de_badge_valide(&g.get_badge_nom()) && couleur_valide(couleur.trim()),
            );
            g.set_badge_message(fo.message.as_str().into());
            g.set_badge_a_apercu(apercu.is_some());
            g.set_badge_apercu(apercu.unwrap_or_default());
            g.set_badge_peut_retirer_image(
                fo.icone.is_some()
                    || (ouvert.as_ref().is_some_and(|b| b.icon_version > 0) && !fo.retirer),
            );
            let b = ouvert.clone().unwrap_or_default();
            g.set_detenteurs(modele(
                b.holders
                    .iter()
                    .map(|h| detenteur(&h.id, &h.pseudo))
                    .collect(),
            ));
            g.set_compte_detenteurs(detenteurs(b.holders.len()).into());
            g.set_propositions(modele(
                propositions
                    .iter()
                    .map(|u| detenteur(&u.id, &u.pseudo))
                    .collect(),
            ));
            g.set_choisis(modele(
                fo.choisis
                    .iter()
                    .map(|u| detenteur(&u.id, &u.pseudo))
                    .collect(),
            ));
            g.set_attribuer(attribuer_a(fo.choisis.len()).into());
        }
        g.set_versions_envoyees(modele(
            e.bot
                .releases
                .iter()
                .map(|v| SharedString::from(v.as_str()))
                .collect(),
        ));
        let config = &vue.config;
        let reglages = &config.settings;
        g.set_tuiles(modele(match &e.stats {
            None => Vec::new(),
            Some(s) => {
                let usage = usage_stockage(s.storage_bytes, reglages.max_storage)
                    .filter(|_| e.vue.is_some());
                vec![
                    TuileAdmin {
                        titre: "COMPTES".into(),
                        valeur: nombre(s.users).into(),
                        note: format!(
                            "{} cette semaine · {} suspendu(s)",
                            s.new_users_7d, s.suspended
                        )
                        .into(),
                        ..TuileAdmin::default()
                    },
                    TuileAdmin {
                        titre: "SERVEURS · MESSAGES".into(),
                        valeur: format!("{} · {}", nombre(s.servers), nombre(s.messages)).into(),
                        note: format!("+{} ces dernières 24 h", nombre(s.messages_24h)).into(),
                        ..TuileAdmin::default()
                    },
                    TuileAdmin {
                        titre: "STOCKAGE".into(),
                        valeur: mega_octets(s.storage_bytes).into(),
                        note: if e.vue.is_some() {
                            format!(
                                "sur {} Go · {} fichier(s)",
                                giga_octets(reglages.max_storage),
                                nombre(s.files)
                            )
                        } else {
                            format!("{} fichier(s)", nombre(s.files))
                        }
                        .into(),
                        a_jauge: usage.is_some(),
                        jauge: usage.unwrap_or(0.0),
                        ..TuileAdmin::default()
                    },
                    TuileAdmin {
                        titre: "ERREURS 24 H".into(),
                        valeur: s.errors_24h.to_string().into(),
                        note: "Voir les logs".into(),
                        danger: s.errors_24h > 0,
                        lien: true,
                        ..TuileAdmin::default()
                    },
                ]
            }
        }));
        let direct = e.direct.clone().unwrap_or_default();
        g.set_a_direct(e.direct.is_some());
        g.set_connexions(format!("{} connexion(s) temps réel", direct.connections).into());
        g.set_appareils(format!("{} appareil(s) connecté(s)", direct.online.len()).into());
        g.set_en_ligne(modele(
            direct
                .online
                .iter()
                .map(|o| AppareilAdmin {
                    pseudo: o.pseudo.as_str().into(),
                    plateforme: if o.platform.is_empty() {
                        "?"
                    } else {
                        o.platform.as_str()
                    }
                    .into(),
                    version: o.version.as_str().into(),
                })
                .collect(),
        ));
        g.set_vocaux(modele(
            direct
                .voice
                .iter()
                .map(|v| VocalAdmin {
                    titre: format!("{} › {}", v.server, v.channel).into(),
                    compte: v.participants.len().to_string().into(),
                    participants: modele(
                        v.participants
                            .iter()
                            .map(|p| {
                                let mut texte = p.pseudo.clone();
                                if p.screen {
                                    texte.push_str(" · écran");
                                }
                                if p.camera {
                                    texte.push_str(" · caméra");
                                }
                                SharedString::from(texte)
                            })
                            .collect(),
                    ),
                })
                .collect(),
        ));
        g.set_a_vue(e.vue.is_some());
        let origines = reglages.origins.clone().unwrap_or_default().join(", ");
        g.set_configuration(modele(vec![
            cle(
                "Notifications push (web)",
                configure(reglages.push_web).into(),
                true,
                reglages.push_web,
            ),
            cle(
                "Notifications push (Android)",
                configure(reglages.push_android).into(),
                true,
                reglages.push_android,
            ),
            cle(
                "GIPHY",
                configure(reglages.giphy).into(),
                true,
                reglages.giphy,
            ),
            cle(
                "Métriques",
                if reglages.metrics {
                    "activées"
                } else {
                    "désactivées"
                }
                .into(),
                true,
                reglages.metrics,
            ),
            cle("LiveKit", detail_livekit(config), false, false),
            cle(
                "Stockage",
                format!(
                    "{} Go · fichiers {} max",
                    giga_octets(reglages.max_storage),
                    mega_octets(config.max_file_size)
                ),
                false,
                false,
            ),
            cle(
                "Origines autorisées",
                if origines.is_empty() {
                    "—".into()
                } else {
                    origines
                },
                false,
                false,
            ),
        ]));
        g.set_versions(modele(vec![
            cle("Backend", vue.version.clone(), false, false),
            cle("App native", crate::VERSION.to_string(), false, false),
        ]));
        g.set_actions(modele(
            e.actions
                .iter()
                .map(|l| ActionAdmin {
                    date: quand(&l.at).into(),
                    acteur: l.actor.as_str().into(),
                    action: l.action.as_str().into(),
                    cible: if l.target.is_empty() {
                        "—"
                    } else {
                        l.target.as_str()
                    }
                    .into(),
                })
                .collect(),
        ));
        g.set_erreurs(modele(
            e.erreurs
                .iter()
                .map(|x| ErreurAdmin {
                    date: quand(&x.at).into(),
                    statut: x.status.to_string().into(),
                    requete: format!("{} {}", x.method, x.path).into(),
                })
                .collect(),
        ));
    }

    pub fn installer(&self, f: &Principale, d: DonneesAdminChargees) {
        {
            let mut e = self.verrou();
            e.generation += 1;
            e.erreur.clear();
            e.comptes = d.comptes;
            e.vue = d.vue;
            e.badges = d.badges;
            e.bot = d.bot;
            e.stats = d.stats;
            e.direct = d.direct;
            e.actions = d.actions;
            e.erreurs = d.erreurs;
        }
        self.projeter(f);
    }

    pub fn badge_modifie(&self, f: &Principale) {
        self.projeter(f);
    }
}
