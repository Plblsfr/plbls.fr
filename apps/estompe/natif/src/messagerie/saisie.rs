use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use estompe_client::autocompletion::{Autocompletion, GenreProposition, Proposition};
use estompe_client::brouillons::Brouillons;
use estompe_client::markdown::{analyser, texte_brut};
use estompe_client::mention::Mentionnables;
use estompe_client::store::messages::{compteur, id_message, Brouillon};
use estompe_client::store::{CleConversation, Store};
use estompe_client::types::Message;
use slint::ComponentHandle;

use crate::{BandeauSaisie, DonneesSaisie, GenreBandeau, Principale, PropositionSaisie};

pub const DELAI_ENREGISTREMENT: Duration = Duration::from_millis(300);
pub const LONGUEUR_APERCU: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Reponse {
        id: String,
        auteur: String,
        texte: String,
    },
    Modification {
        id: String,
        texte: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Affichage {
    pub texte: String,
    pub indice: String,
    pub mode: Mode,
    pub compteur: String,
    pub alerte: bool,
    pub erreur: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Envoyer(CleConversation, Brouillon),
    Modifier { id: String, contenu: String },
    Supprimer { cle: CleConversation, id: String },
}

pub fn apercu(contenu: &str) -> String {
    let texte = texte_brut(&analyser(contenu, &Mentionnables::default()).blocs);
    let une_ligne = texte.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut apercu: String = une_ligne.chars().take(LONGUEUR_APERCU).collect();
    if une_ligne.chars().count() > LONGUEUR_APERCU {
        apercu.push('…');
    }
    apercu
}

pub fn indice_de(nom: &str, prive: bool) -> String {
    if prive {
        format!("Écrire à {nom}")
    } else if nom.is_empty() {
        "Écrire un message".to_string()
    } else {
        format!("Écrire dans #{nom}")
    }
}

#[derive(Debug, Default)]
pub struct EtatSaisie {
    pub cle: Option<CleConversation>,
    pub texte: String,
    pub mode: Mode,
    pub indice: String,
    pub erreur: String,
    pub curseur: usize,
    pub auto: Option<Autocompletion>,
    pub choisie: usize,
}

impl EtatSaisie {
    pub fn affichage(&self) -> Affichage {
        let (compteur, alerte) = match compteur(&self.texte) {
            Some((n, alerte)) => (
                format!("{n} / {}", estompe_client::store::messages::LONGUEUR_MAX),
                alerte,
            ),
            None => (String::new(), false),
        };
        Affichage {
            texte: self.texte.clone(),
            indice: self.indice.clone(),
            mode: self.mode.clone(),
            compteur,
            alerte,
            erreur: self.erreur.clone(),
        }
    }

    pub fn ouvrir(
        &mut self,
        brouillons: &mut Brouillons,
        cle: Option<CleConversation>,
        indice: String,
    ) {
        if self.cle == cle {
            self.indice = indice;
            return;
        }
        self.quitter(brouillons);
        self.texte = cle
            .as_ref()
            .map(|c| brouillons.texte(&c.to_string()).to_string())
            .unwrap_or_default();
        self.cle = cle;
        self.mode = Mode::Normal;
        self.indice = indice;
        self.erreur.clear();
    }

    fn quitter(&mut self, brouillons: &mut Brouillons) {
        if let (Some(cle), Mode::Normal | Mode::Reponse { .. }) = (&self.cle, &self.mode) {
            brouillons.definir(&cle.to_string(), &self.texte);
        }
    }

    pub fn modifie(&mut self, brouillons: &mut Brouillons, texte: &str) -> bool {
        self.texte = texte.to_string();
        self.erreur.clear();
        match (&self.cle, &self.mode) {
            (Some(cle), Mode::Normal | Mode::Reponse { .. }) => {
                brouillons.definir(&cle.to_string(), texte)
            }
            _ => false,
        }
    }

    pub fn envoyer(&mut self, brouillons: &mut Brouillons, texte: &str) -> Option<Action> {
        let cle = self.cle.clone()?;
        self.texte = texte.to_string();
        let contenu = texte.trim().to_string();
        let action = match std::mem::take(&mut self.mode) {
            Mode::Modification { id, texte: avant } => {
                if contenu.is_empty() {
                    self.mode = Mode::Modification { id, texte: avant };
                    return None;
                }
                self.texte = brouillons.texte(&cle.to_string()).to_string();
                if contenu == avant.trim() {
                    return None;
                }
                Action::Modifier { id, contenu }
            }
            mode => {
                if contenu.is_empty() {
                    self.mode = mode;
                    return None;
                }
                let reponse_a = match mode {
                    Mode::Reponse { id, .. } => Some(id),
                    _ => None,
                };
                self.texte.clear();
                brouillons.effacer(&cle.to_string());
                Action::Envoyer(
                    cle,
                    Brouillon {
                        contenu,
                        reponse_a,
                        ..Brouillon::default()
                    },
                )
            }
        };
        self.erreur.clear();
        Some(action)
    }

    pub fn repondre(&mut self, message: &Message) {
        let auteur = message
            .author
            .as_ref()
            .and_then(|p| p.pseudo.clone())
            .unwrap_or_default();
        if matches!(self.mode, Mode::Modification { .. }) {
            self.texte.clear();
        }
        self.mode = Mode::Reponse {
            id: id_message(message).to_string(),
            auteur,
            texte: apercu(message.content.as_deref().unwrap_or_default()),
        };
    }

    pub fn modifier(&mut self, brouillons: &mut Brouillons, message: &Message) {
        self.quitter(brouillons);
        let texte = message.content.clone().unwrap_or_default();
        self.mode = Mode::Modification {
            id: id_message(message).to_string(),
            texte: texte.clone(),
        };
        self.texte = texte;
        self.erreur.clear();
    }

    pub fn proposer(&mut self, auto: Option<Autocompletion>) -> bool {
        let change = self.auto != auto;
        if change {
            self.choisie = 0;
        }
        self.auto = auto;
        change
    }

    pub fn naviguer(&mut self, pas: i32) {
        let n = self.auto.as_ref().map_or(0, |a| a.propositions.len()) as i32;
        if n > 0 {
            self.choisie = (self.choisie as i32 + pas).rem_euclid(n) as usize;
        }
    }

    pub fn choisir(&mut self, index: usize) -> Option<(String, usize)> {
        let auto = self.auto.as_ref()?;
        auto.appliquer(&self.texte, self.curseur.min(self.texte.len()), index)
    }

    pub fn inserer(&mut self, insere: &str, ancre: usize) -> Option<(String, usize)> {
        let (a, b) = (ancre.min(self.curseur), ancre.max(self.curseur));
        estompe_client::mention::remplacer(&self.texte, a, b.min(self.texte.len()), insere)
    }

    pub fn annuler(&mut self, brouillons: &Brouillons) -> bool {
        match std::mem::take(&mut self.mode) {
            Mode::Normal => false,
            Mode::Reponse { .. } => true,
            Mode::Modification { .. } => {
                self.texte = self
                    .cle
                    .as_ref()
                    .map(|c| brouillons.texte(&c.to_string()).to_string())
                    .unwrap_or_default();
                true
            }
        }
    }
}

#[derive(Clone)]
pub struct Saisie {
    etat: Arc<Mutex<EtatSaisie>>,
    brouillons: Arc<Mutex<Brouillons>>,
    enregistrement: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl Default for Saisie {
    fn default() -> Self {
        Self::nouvelle(Brouillons::en_memoire())
    }
}

fn genre_slint(genre: GenreProposition) -> crate::GenreProposition {
    match genre {
        GenreProposition::Personne => crate::GenreProposition::Personne,
        GenreProposition::Groupe => crate::GenreProposition::Groupe,
        GenreProposition::Role => crate::GenreProposition::Role,
        GenreProposition::Salon => crate::GenreProposition::Salon,
        GenreProposition::Emoji | GenreProposition::EmojiServeur => crate::GenreProposition::Emoji,
    }
}

fn bandeau_slint(mode: &Mode) -> BandeauSaisie {
    match mode {
        Mode::Normal => BandeauSaisie::default(),
        Mode::Reponse { auteur, texte, .. } => BandeauSaisie {
            genre: GenreBandeau::Reponse,
            auteur: auteur.as_str().into(),
            texte: texte.as_str().into(),
        },
        Mode::Modification { texte, .. } => BandeauSaisie {
            genre: GenreBandeau::Modification,
            auteur: Default::default(),
            texte: apercu(texte).into(),
        },
    }
}

impl Saisie {
    pub fn nouvelle(brouillons: Brouillons) -> Self {
        Self {
            etat: Arc::new(Mutex::new(EtatSaisie::default())),
            brouillons: Arc::new(Mutex::new(brouillons)),
            enregistrement: Arc::new(Mutex::new(None)),
        }
    }

    pub fn definir_brouillons(&self, brouillons: Brouillons) {
        *self.brouillons() = brouillons;
    }

    fn etat(&self) -> MutexGuard<'_, EtatSaisie> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn brouillons(&self) -> MutexGuard<'_, Brouillons> {
        self.brouillons.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn conversations_en_brouillon(&self) -> HashSet<String> {
        self.brouillons().cles().map(str::to_string).collect()
    }

    pub fn affichage(&self) -> Affichage {
        self.etat().affichage()
    }

    pub fn mode(&self) -> Mode {
        self.etat().mode.clone()
    }

    pub fn cle(&self) -> Option<CleConversation> {
        self.etat().cle.clone()
    }

    pub fn afficher(&self, f: &Principale) {
        let a = self.affichage();
        let g = f.global::<DonneesSaisie>();
        if g.get_texte() != a.texte.as_str() {
            g.set_texte(a.texte.as_str().into());
        }
        g.set_indice(a.indice.as_str().into());
        g.set_bandeau(bandeau_slint(&a.mode));
        g.set_compteur(a.compteur.as_str().into());
        g.set_compteur_alerte(a.alerte);
        g.set_erreur(a.erreur.as_str().into());
    }

    pub fn demander_focus(&self, f: &Principale) {
        let g = f.global::<DonneesSaisie>();
        g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
    }

    pub fn ouvrir(&self, f: &Principale, cle: Option<CleConversation>, indice: String) -> bool {
        let (change, autre) = {
            let mut brouillons = self.brouillons();
            let mut etat = self.etat();
            let avant = brouillons.cles().count();
            let ancien = etat.cle.clone();
            etat.ouvrir(&mut brouillons, cle.clone(), indice);
            let autre = ancien != cle;
            if autre {
                etat.auto = None;
            }
            (autre && brouillons.cles().count() != avant, autre)
        };
        let g = f.global::<DonneesSaisie>();
        g.set_visible(cle.is_some());
        if autre {
            g.set_propositions(slint::ModelRc::default());
        }
        self.afficher(f);
        change
    }

    pub fn modifie(&self, f: &Principale, texte: &str, runtime: &tokio::runtime::Handle) -> bool {
        let presence_change = {
            let mut brouillons = self.brouillons();
            let avant = self
                .etat()
                .cle
                .as_ref()
                .is_some_and(|c| brouillons.a_un_brouillon(&c.to_string()));
            let mut etat = self.etat();
            etat.modifie(&mut brouillons, texte);
            let apres = etat
                .cle
                .as_ref()
                .is_some_and(|c| brouillons.a_un_brouillon(&c.to_string()));
            avant != apres
        };
        self.planifier_enregistrement(runtime);
        let a = self.affichage();
        let g = f.global::<DonneesSaisie>();
        g.set_compteur(a.compteur.as_str().into());
        g.set_compteur_alerte(a.alerte);
        if g.get_erreur() != "" {
            g.set_erreur("".into());
        }
        presence_change
    }

    pub fn envoyer(&self, f: &Principale, texte: &str) -> Option<Action> {
        let action = {
            let mut brouillons = self.brouillons();
            self.etat().envoyer(&mut brouillons, texte)
        };
        self.afficher(f);
        action
    }

    pub fn repondre(&self, f: &Principale, message: &Message) {
        self.etat().repondre(message);
        self.afficher(f);
        self.demander_focus(f);
    }

    pub fn modifier(&self, f: &Principale, message: &Message) {
        {
            let mut brouillons = self.brouillons();
            self.etat().modifier(&mut brouillons, message);
        }
        self.afficher(f);
        self.demander_focus(f);
    }

    pub fn annuler(&self, f: &Principale) -> bool {
        let annule = {
            let brouillons = self.brouillons();
            self.etat().annuler(&brouillons)
        };
        if annule {
            self.afficher(f);
        }
        annule
    }

    pub fn texte(&self) -> String {
        self.etat().texte.clone()
    }

    pub fn definir_curseur(&self, curseur: usize) {
        self.etat().curseur = curseur;
    }

    pub fn placer(&self, f: &Principale, texte: &str, curseur: usize) {
        let g = f.global::<DonneesSaisie>();
        g.set_texte(texte.into());
        g.set_curseur(curseur as i32);
        g.set_ancre(curseur as i32);
        g.set_curseur_demande(curseur as i32);
        g.set_demande_curseur(g.get_demande_curseur().wrapping_add(1));
        self.etat().curseur = curseur;
    }

    pub fn proposer(
        &self,
        f: &Principale,
        auto: Option<Autocompletion>,
        image: impl Fn(&Proposition) -> slint::Image,
    ) {
        if !self.etat().proposer(auto) {
            return;
        }
        let (titre, lignes) = match &self.etat().auto {
            Some(a) => (
                a.declencheur.titre(),
                a.propositions
                    .iter()
                    .map(|p| PropositionSaisie {
                        genre: genre_slint(p.genre),
                        libelle: p.libelle.as_str().into(),
                        detail: p.detail.as_str().into(),
                        image: image(p),
                    })
                    .collect::<Vec<_>>(),
            ),
            None => ("", Vec::new()),
        };
        let g = f.global::<DonneesSaisie>();
        g.set_titre_propositions(titre.into());
        g.set_propositions(slint::ModelRc::new(slint::VecModel::from(lignes)));
        g.set_proposition_choisie(0);
    }

    pub fn propositions(&self) -> Option<Autocompletion> {
        self.etat().auto.clone()
    }

    pub fn naviguer(&self, f: &Principale, pas: i32) {
        let choisie = {
            let mut e = self.etat();
            e.naviguer(pas);
            e.choisie
        };
        f.global::<DonneesSaisie>()
            .set_proposition_choisie(choisie as i32);
    }

    pub fn choisir(&self, index: usize) -> Option<(String, usize)> {
        self.etat().choisir(index)
    }

    pub fn inserer(&self, insere: &str, ancre: usize) -> Option<(String, usize)> {
        self.etat().inserer(insere, ancre)
    }

    pub fn signaler_erreur(&self, f: &Principale, erreur: &str) {
        self.etat().erreur = erreur.to_string();
        f.global::<DonneesSaisie>().set_erreur(erreur.into());
    }

    pub fn enregistrer_maintenant(&self) {
        let _ = self.brouillons().enregistrer();
    }

    fn planifier_enregistrement(&self, runtime: &tokio::runtime::Handle) {
        if !self.brouillons().a_enregistrer() {
            return;
        }
        let brouillons = self.brouillons.clone();
        let tache = runtime.spawn(async move {
            tokio::time::sleep(DELAI_ENREGISTREMENT).await;
            let _ = brouillons
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .enregistrer();
        });
        if let Some(ancienne) = self
            .enregistrement
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .replace(tache)
        {
            ancienne.abort();
        }
    }
}

pub fn executer(
    store: &Store,
    action: Action,
) -> impl std::future::Future<Output = Result<(), String>> + Send + 'static {
    let store = store.clone();
    async move {
        match action {
            Action::Envoyer(cle, brouillon) => {
                store.envoyer(&cle, brouillon).await;
                Ok(())
            }
            Action::Modifier { id, contenu } => store
                .modifier_message(&id, &contenu, None)
                .await
                .map_err(|e| format!("Modification impossible : {}", e.message())),
            Action::Supprimer { cle, id } => store
                .supprimer_message(&cle, &id)
                .await
                .map_err(|e| format!("Suppression impossible : {}", e.message())),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn message(id: &str, auteur: &str, contenu: &str) -> Message {
        serde_json::from_value(json!({"id": id, "channel_id": "c1", "author": {"id": auteur, "pseudo": auteur}, "content": contenu}))
            .unwrap()
    }

    fn c(id: &str) -> CleConversation {
        CleConversation::Salon(id.into())
    }

    #[test]
    fn le_brouillon_suit_la_conversation() {
        let mut b = Brouillons::en_memoire();
        let mut e = EtatSaisie::default();
        e.ouvrir(&mut b, Some(c("1")), "Écrire dans #a".into());
        e.modifie(&mut b, "bonjour");
        e.ouvrir(&mut b, Some(c("2")), "Écrire dans #b".into());
        assert_eq!(e.texte, "");
        e.ouvrir(&mut b, Some(c("1")), "Écrire dans #a".into());
        assert_eq!(e.texte, "bonjour");
        assert_eq!(b.texte("c:1"), "bonjour");
    }

    #[test]
    fn envoyer_vide_le_champ_et_le_brouillon() {
        let mut b = Brouillons::en_memoire();
        let mut e = EtatSaisie::default();
        e.ouvrir(&mut b, Some(c("1")), String::new());
        e.modifie(&mut b, "  salut\n");
        let action = e.envoyer(&mut b, "  salut\n").unwrap();
        assert_eq!(action, Action::Envoyer(c("1"), Brouillon::texte("salut")));
        assert_eq!(e.texte, "");
        assert!(!b.a_un_brouillon("c:1"));
        assert_eq!(e.envoyer(&mut b, "   "), None);
    }

    #[test]
    fn la_reponse_part_avec_son_message_et_echap_l_annule() {
        let mut b = Brouillons::en_memoire();
        let mut e = EtatSaisie::default();
        e.ouvrir(&mut b, Some(c("1")), String::new());
        e.repondre(&message("7", "bob", "une **longue** question"));
        assert_eq!(
            e.mode,
            Mode::Reponse {
                id: "7".into(),
                auteur: "bob".into(),
                texte: "une longue question".into()
            }
        );
        let Some(Action::Envoyer(_, brouillon)) = e.envoyer(&mut b, "oui") else {
            panic!("envoi attendu");
        };
        assert_eq!(brouillon.reponse_a.as_deref(), Some("7"));
        assert_eq!(e.mode, Mode::Normal);
        e.repondre(&message("8", "bob", "x"));
        assert!(e.annuler(&b));
        assert_eq!(e.mode, Mode::Normal);
        assert!(!e.annuler(&b));
    }

    #[test]
    fn modifier_reprend_le_texte_puis_rend_le_brouillon() {
        let mut b = Brouillons::en_memoire();
        let mut e = EtatSaisie::default();
        e.ouvrir(&mut b, Some(c("1")), String::new());
        e.modifie(&mut b, "brouillon en cours");
        e.modifier(&mut b, &message("3", "me", "ancien"));
        assert_eq!(e.texte, "ancien");
        e.modifie(&mut b, "nouveau");
        assert_eq!(b.texte("c:1"), "brouillon en cours");
        let action = e.envoyer(&mut b, "nouveau").unwrap();
        assert_eq!(
            action,
            Action::Modifier {
                id: "3".into(),
                contenu: "nouveau".into()
            }
        );
        assert_eq!(e.texte, "brouillon en cours");
        e.modifier(&mut b, &message("3", "me", "ancien"));
        assert_eq!(
            e.envoyer(&mut b, "ancien"),
            None,
            "texte inchangé : rien n'est envoyé"
        );
        e.modifier(&mut b, &message("3", "me", "ancien"));
        assert!(e.annuler(&b));
        assert_eq!(e.texte, "brouillon en cours");
    }

    #[test]
    fn le_compteur_apparait_pres_de_la_limite() {
        let mut e = EtatSaisie {
            texte: "a".repeat(3600),
            ..EtatSaisie::default()
        };
        assert_eq!(e.affichage().compteur, "3600 / 4000");
        assert!(!e.affichage().alerte);
        e.texte = "a".repeat(3950);
        assert!(e.affichage().alerte);
        e.texte = "a".into();
        assert_eq!(e.affichage().compteur, "");
    }

    #[test]
    fn les_propositions_se_parcourent_en_boucle_et_s_appliquent_au_curseur() {
        let mut e = EtatSaisie {
            texte: "salut @al ok".into(),
            curseur: 9,
            ..EtatSaisie::default()
        };
        let proposition = |nom: &str| Proposition {
            genre: GenreProposition::Personne,
            libelle: format!("@{nom}"),
            detail: String::new(),
            emoji: String::new(),
            insertion: format!("@{nom} "),
        };
        assert!(e.proposer(Some(Autocompletion {
            declencheur: estompe_client::autocompletion::Declencheur::Mention,
            debut: 6,
            propositions: vec![proposition("alice"), proposition("malik")],
        })));
        e.naviguer(-1);
        assert_eq!(e.choisie, 1);
        e.naviguer(1);
        assert_eq!(e.choisie, 0);
        assert_eq!(e.choisir(1), Some(("salut @malik  ok".to_string(), 13)));
        assert!(e.proposer(None));
        assert_eq!(e.choisir(0), None);
        e.curseur = 5;
        assert_eq!(e.inserer("😂", 0), Some(("😂 @al ok".to_string(), 4)));
    }

    #[test]
    fn l_indice_nomme_la_conversation() {
        assert_eq!(indice_de("général", false), "Écrire dans #général");
        assert_eq!(indice_de("bob", true), "Écrire à bob");
        assert_eq!(apercu(&"mot ".repeat(40)).chars().count(), 81);
    }
}
