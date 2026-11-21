use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use estompe_client::appels::EmojiPersonnalise;
use estompe_client::emoji::{lignes, pousser_recent, rechercher, LigneEmoji, GROUPES};
use slint::{ComponentHandle, Model, ModelNotify, ModelRc, ModelTracker, VecModel};

use super::emoji;
use super::medias::Image;
use crate::{CelluleEmoji, DonneesSelecteur, OngletEmoji, Principale, RangeeEmoji};

pub const COLONNES: usize = 8;
pub const HAUTEUR_RANGEE: f32 = 36.0;
pub const RESULTATS_MAX: usize = 200;
pub const ICONE_RECENTS: &str = "🕘";
pub const TITRE_RECENTS: &str = "Récents";
pub const TITRE_SERVEURS: &str = "Smileys des serveurs";
pub const AUCUN_RESULTAT: &str = "Aucun smiley trouvé.";
pub const AIDE: &str = "Maj + Entrée ou Maj + clic pour en insérer plusieurs";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cellule {
    Unicode(&'static LigneEmoji),
    Serveur(EmojiPersonnalise),
}

impl Cellule {
    pub fn nom(&self) -> String {
        match self {
            Cellule::Unicode(l) => l.nom.clone(),
            Cellule::Serveur(e) => format!(":{}:", e.name),
        }
    }

    pub fn code(&self) -> String {
        match self {
            Cellule::Unicode(l) => format!(":{}:", l.code),
            Cellule::Serveur(e) => format!(":{}:", e.name),
        }
    }

    pub fn insertion(&self) -> String {
        match self {
            Cellule::Unicode(l) => l.caractere.clone(),
            Cellule::Serveur(e) => format!(":{}:", e.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rangee {
    Titre(String),
    Cellules { debut: usize, nombre: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Onglet {
    pub icone: &'static str,
    pub libelle: &'static str,
    pub premiere: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Cible {
    #[default]
    Saisie,
    Reaction(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choix {
    pub cible: Cible,
    pub insertion: String,
    pub garder: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sens {
    Gauche,
    Droite,
    Haut,
    Bas,
}

#[derive(Debug, Default)]
pub struct EtatSelecteur {
    pub ouvert: bool,
    pub cible: Cible,
    pub recherche: String,
    pub recents: Vec<String>,
    pub emojis_serveur: Vec<EmojiPersonnalise>,
    pub actif: Option<usize>,
    pub survol: Option<usize>,
    pub cellules: Vec<Cellule>,
    pub rangees: Vec<Rangee>,
    pub onglets: Vec<Onglet>,
    pub onglet_actif: usize,
}

fn terme(recherche: &str) -> String {
    let t = recherche.trim();
    let t = t.strip_prefix(':').unwrap_or(t);
    t.strip_suffix(':').unwrap_or(t).to_lowercase()
}

impl EtatSelecteur {
    pub fn ouvrir(
        &mut self,
        cible: Cible,
        recents: Vec<String>,
        emojis_serveur: Vec<EmojiPersonnalise>,
    ) {
        self.ouvert = true;
        self.cible = cible;
        self.recents = recents;
        self.emojis_serveur = emojis_serveur;
        self.recherche.clear();
        self.recalculer();
    }

    pub fn fermer(&mut self) {
        self.ouvert = false;
        self.recherche.clear();
        self.actif = None;
        self.survol = None;
    }

    pub fn chercher(&mut self, recherche: &str) {
        if self.recherche == recherche {
            return;
        }
        self.recherche = recherche.to_string();
        self.recalculer();
    }

    pub fn definir_emojis_serveur(&mut self, emojis: Vec<EmojiPersonnalise>) {
        if self.emojis_serveur != emojis {
            self.emojis_serveur = emojis;
            self.recalculer();
        }
    }

    fn ajouter_section(&mut self, titre: Option<&str>, cellules: Vec<Cellule>) -> usize {
        let premiere = self.cellules.len();
        if let Some(titre) = titre {
            self.rangees.push(Rangee::Titre(titre.to_string()));
        }
        let mut debut = premiere;
        for morceau in cellules.chunks(COLONNES) {
            self.rangees.push(Rangee::Cellules {
                debut,
                nombre: morceau.len(),
            });
            debut += morceau.len();
        }
        self.cellules.extend(cellules);
        premiere
    }

    fn recalculer(&mut self) {
        self.cellules.clear();
        self.rangees.clear();
        self.onglets.clear();
        self.actif = None;
        self.survol = None;
        self.onglet_actif = 0;
        let t = terme(&self.recherche);
        let donnees = lignes();
        if !t.is_empty() {
            let mut trouves: Vec<Cellule> = rechercher(donnees, &t, RESULTATS_MAX)
                .into_iter()
                .map(Cellule::Unicode)
                .collect();
            trouves.extend(
                self.emojis_serveur
                    .iter()
                    .filter(|e| e.name.contains(&t))
                    .cloned()
                    .map(Cellule::Serveur),
            );
            self.ajouter_section(None, trouves);
            return;
        }
        let recents: Vec<Cellule> = self
            .recents
            .iter()
            .filter_map(|c| donnees.iter().find(|l| &l.caractere == c))
            .map(Cellule::Unicode)
            .collect();
        if !recents.is_empty() {
            let premiere = self.ajouter_section(Some(TITRE_RECENTS), recents);
            self.onglets.push(Onglet {
                icone: ICONE_RECENTS,
                libelle: TITRE_RECENTS,
                premiere,
            });
        }
        for (i, g) in GROUPES.iter().enumerate() {
            let cellules: Vec<Cellule> = donnees
                .iter()
                .filter(|l| usize::from(l.groupe) == i)
                .map(Cellule::Unicode)
                .collect();
            if cellules.is_empty() {
                continue;
            }
            let premiere = self.ajouter_section(Some(g.libelle), cellules);
            self.onglets.push(Onglet {
                icone: g.icone,
                libelle: g.libelle,
                premiere,
            });
        }
        if !self.emojis_serveur.is_empty() {
            let serveur = self
                .emojis_serveur
                .iter()
                .cloned()
                .map(Cellule::Serveur)
                .collect();
            self.ajouter_section(Some(TITRE_SERVEURS), serveur);
        }
    }

    pub fn message(&self) -> &'static str {
        if self.cellules.is_empty() && !terme(&self.recherche).is_empty() {
            AUCUN_RESULTAT
        } else {
            ""
        }
    }

    pub fn rangee_de(&self, index: usize) -> Option<usize> {
        self.rangees.iter().position(
            |r| matches!(r, Rangee::Cellules { debut, nombre } if (*debut..debut + nombre).contains(&index)),
        )
    }

    pub fn onglet_de(&self, index: usize) -> usize {
        self.onglets
            .iter()
            .rposition(|o| o.premiere <= index)
            .unwrap_or(0)
    }

    fn activer(&mut self, index: Option<usize>) {
        self.actif = index;
        if let Some(i) = index {
            self.onglet_actif = self.onglet_de(i);
        }
    }

    pub fn deplacer(&mut self, sens: Sens) {
        let n = self.cellules.len();
        if n == 0 {
            return;
        }
        let Some(i) = self.actif else {
            if matches!(sens, Sens::Bas | Sens::Droite) {
                self.activer(Some(0));
            }
            return;
        };
        let suivant = match sens {
            Sens::Gauche => Some(i.saturating_sub(1)),
            Sens::Droite => Some((i + 1).min(n - 1)),
            Sens::Haut | Sens::Bas => {
                let r = self.rangee_de(i).unwrap_or(0);
                let colonne = match self.rangees[r] {
                    Rangee::Cellules { debut, .. } => i - debut,
                    Rangee::Titre(_) => 0,
                };
                let voisine = if sens == Sens::Haut {
                    self.rangees[..r]
                        .iter()
                        .rev()
                        .find(|r| matches!(r, Rangee::Cellules { .. }))
                } else {
                    self.rangees[r + 1..]
                        .iter()
                        .find(|r| matches!(r, Rangee::Cellules { .. }))
                };
                match voisine {
                    Some(Rangee::Cellules { debut, nombre }) => {
                        Some(debut + colonne.min(nombre - 1))
                    }
                    _ if sens == Sens::Haut => None,
                    _ => Some(i),
                }
            }
        };
        self.activer(suivant);
    }

    pub fn aller_a(&mut self, onglet: usize) -> Option<usize> {
        let premiere = self.onglets.get(onglet)?.premiere;
        self.activer(Some(premiere));
        self.onglet_actif = onglet;
        self.rangee_de(premiere)
    }

    pub fn defile(&mut self, haut: f32) {
        let rangee = (haut / HAUTEUR_RANGEE + 0.5).max(0.0) as usize;
        let premiere = self.rangees[rangee.min(self.rangees.len().saturating_sub(1))..]
            .iter()
            .find_map(|r| match r {
                Rangee::Cellules { debut, .. } => Some(*debut),
                Rangee::Titre(_) => None,
            });
        if let Some(premiere) = premiere {
            self.onglet_actif = self.onglet_de(premiere);
        }
    }

    pub fn survoler(&mut self, index: Option<usize>) {
        self.survol = index.filter(|i| *i < self.cellules.len());
    }

    pub fn pied(&self) -> Option<&Cellule> {
        self.survol
            .or(self.actif)
            .and_then(|i| self.cellules.get(i))
    }

    pub fn choisir(&mut self, index: usize, garder: bool) -> Option<Choix> {
        let cellule = self.cellules.get(index)?.clone();
        if let Cellule::Unicode(l) = &cellule {
            self.recents = pousser_recent(&self.recents, &l.caractere);
        }
        let choix = Choix {
            cible: self.cible.clone(),
            insertion: cellule.insertion(),
            garder,
        };
        if !garder {
            self.fermer();
        }
        Some(choix)
    }

    pub fn choisir_actif(&mut self, garder: bool) -> Option<Choix> {
        self.choisir(self.actif.unwrap_or(0), garder)
    }
}

type Images = Arc<Mutex<HashMap<String, Image>>>;
type Retenir = Arc<dyn Fn(Vec<String>) + Send + Sync>;

fn verrou<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn image_de(cellule: &Cellule, images: &HashMap<String, Image>) -> slint::Image {
    match cellule {
        Cellule::Unicode(l) => emoji::image(&l.caractere).unwrap_or_default(),
        Cellule::Serveur(e) => images
            .get(&e.id)
            .cloned()
            .map(slint::Image::from_rgba8)
            .unwrap_or_default(),
    }
}

struct ModeleRangees {
    etat: Arc<Mutex<EtatSelecteur>>,
    images: Images,
    notify: ModelNotify,
}

impl Model for ModeleRangees {
    type Data = RangeeEmoji;

    fn row_count(&self) -> usize {
        verrou(&self.etat).rangees.len()
    }

    fn row_data(&self, row: usize) -> Option<RangeeEmoji> {
        let etat = verrou(&self.etat);
        Some(match etat.rangees.get(row)? {
            Rangee::Titre(titre) => RangeeEmoji {
                titre: titre.as_str().into(),
                cellules: ModelRc::default(),
            },
            Rangee::Cellules { debut, nombre } => {
                let images = verrou(&self.images);
                let cellules: Vec<CelluleEmoji> = (*debut..debut + nombre)
                    .map(|i| CelluleEmoji {
                        index: i as i32,
                        libelle: etat.cellules[i].nom().into(),
                        image: image_de(&etat.cellules[i], &images),
                    })
                    .collect();
                RangeeEmoji {
                    titre: Default::default(),
                    cellules: ModelRc::new(VecModel::from(cellules)),
                }
            }
        })
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Clone, Default)]
pub struct Selecteur {
    etat: Arc<Mutex<EtatSelecteur>>,
    images: Images,
    retenir: Arc<Mutex<Option<Retenir>>>,
    hauteur_visible: Arc<Mutex<f32>>,
}

impl Selecteur {
    pub fn definir_recents(&self, recents: Vec<String>, retenir: Retenir) {
        verrou(&self.etat).recents = recents;
        *verrou(&self.retenir) = Some(retenir);
    }

    pub fn ouvert(&self) -> bool {
        verrou(&self.etat).ouvert
    }

    pub fn cible(&self) -> Cible {
        verrou(&self.etat).cible.clone()
    }

    pub fn etat(&self) -> MutexGuard<'_, EtatSelecteur> {
        verrou(&self.etat)
    }

    pub fn ouvrir(&self, f: &Principale, cible: Cible, emojis: Vec<EmojiPersonnalise>) {
        {
            let mut e = verrou(&self.etat);
            let recents = e.recents.clone();
            e.ouvrir(cible, recents, emojis);
        }
        let g = f.global::<DonneesSelecteur>();
        g.set_recherche("".into());
        g.set_defilement(0.0);
        self.afficher(f, true);
        g.set_demande_focus(g.get_demande_focus().wrapping_add(1));
    }

    pub fn fermer(&self, f: &Principale) {
        verrou(&self.etat).fermer();
        self.afficher(f, false);
    }

    pub fn emojis_serveur(&self, f: &Principale, emojis: Vec<EmojiPersonnalise>) {
        verrou(&self.etat).definir_emojis_serveur(emojis);
        self.afficher(f, true);
    }

    pub fn installer_image(&self, f: &Principale, id: &str, image: Image) {
        verrou(&self.images).insert(id.to_string(), image);
        self.afficher(f, true);
    }

    pub fn a_l_image(&self, id: &str) -> bool {
        verrou(&self.images).contains_key(id)
    }

    pub fn chercher(&self, f: &Principale, recherche: &str) {
        verrou(&self.etat).chercher(recherche);
        f.global::<DonneesSelecteur>().set_defilement(0.0);
        self.afficher(f, true);
    }

    pub fn deplacer(&self, f: &Principale, dx: i32, dy: i32) {
        let sens = match (dx, dy) {
            (d, _) if d < 0 => Sens::Gauche,
            (d, _) if d > 0 => Sens::Droite,
            (_, d) if d < 0 => Sens::Haut,
            _ => Sens::Bas,
        };
        verrou(&self.etat).deplacer(sens);
        self.faire_voir(f);
        self.afficher(f, false);
    }

    pub fn aller_a(&self, f: &Principale, onglet: usize) {
        verrou(&self.etat).aller_a(onglet);
        self.faire_voir(f);
        self.afficher(f, false);
    }

    pub fn survoler(&self, f: &Principale, index: i32) {
        verrou(&self.etat).survoler(usize::try_from(index).ok());
        self.afficher(f, false);
    }

    pub fn defile(&self, f: &Principale, haut: f32, hauteur: f32) {
        *verrou(&self.hauteur_visible) = hauteur;
        let avant = verrou(&self.etat).onglet_actif;
        verrou(&self.etat).defile(haut);
        if verrou(&self.etat).onglet_actif != avant {
            self.afficher(f, false);
        }
    }

    pub fn choisir(&self, f: &Principale, index: Option<usize>, garder: bool) -> Option<Choix> {
        let (choix, recents) = {
            let mut e = verrou(&self.etat);
            let avant = e.recents.clone();
            let choix = match index {
                Some(i) => e.choisir(i, garder),
                None => e.choisir_actif(garder),
            }?;
            (choix, (e.recents != avant).then(|| e.recents.clone()))
        };
        if let (Some(recents), Some(retenir)) = (recents, verrou(&self.retenir).clone()) {
            retenir(recents);
        }
        self.afficher(f, false);
        Some(choix)
    }

    fn faire_voir(&self, f: &Principale) {
        let Some(rangee) = ({
            let e = verrou(&self.etat);
            e.actif.and_then(|i| e.rangee_de(i))
        }) else {
            return;
        };
        let g = f.global::<DonneesSelecteur>();
        let hauteur = *verrou(&self.hauteur_visible);
        let haut = -g.get_defilement();
        let y = rangee as f32 * HAUTEUR_RANGEE;
        let y = if rangee <= 1 { 0.0 } else { y };
        if y < haut {
            g.set_defilement(-y);
        } else if hauteur > 0.0 && y + HAUTEUR_RANGEE > haut + hauteur {
            g.set_defilement(-(y + HAUTEUR_RANGEE - hauteur));
        }
    }

    fn afficher(&self, f: &Principale, rangees: bool) {
        let g = f.global::<DonneesSelecteur>();
        let e = verrou(&self.etat);
        g.set_ouvert(e.ouvert);
        g.set_actif(e.actif.map_or(-1, |i| i as i32));
        g.set_onglet_actif(e.onglet_actif as i32);
        g.set_message(e.message().into());
        g.set_aide(AIDE.into());
        match e.pied() {
            Some(c) => {
                g.set_pied_nom(c.nom().into());
                g.set_pied_code(c.code().into());
                g.set_pied_image(image_de(c, &verrou(&self.images)));
            }
            None => {
                g.set_pied_nom("".into());
                g.set_pied_code("".into());
                g.set_pied_image(slint::Image::default());
            }
        }
        if !rangees {
            return;
        }
        let onglets: Vec<OngletEmoji> = e
            .onglets
            .iter()
            .map(|o| OngletEmoji {
                libelle: o.libelle.into(),
                image: emoji::image(o.icone).unwrap_or_default(),
            })
            .collect();
        drop(e);
        g.set_onglets(ModelRc::new(VecModel::from(onglets)));
        let modele = g.get_rangees();
        match modele.as_any().downcast_ref::<ModeleRangees>() {
            Some(m) if Arc::ptr_eq(&m.etat, &self.etat) => m.notify.reset(),
            _ => g.set_rangees(ModelRc::new(ModeleRangees {
                etat: self.etat.clone(),
                images: self.images.clone(),
                notify: ModelNotify::default(),
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ouvert() -> EtatSelecteur {
        let mut e = EtatSelecteur::default();
        e.ouvrir(Cible::Saisie, Vec::new(), Vec::new());
        e
    }

    fn perso(nom: &str) -> EmojiPersonnalise {
        EmojiPersonnalise {
            id: format!("e-{nom}"),
            server_id: "s1".into(),
            name: nom.into(),
            created_by: "me".into(),
        }
    }

    fn libelle_actif(e: &EtatSelecteur) -> String {
        e.pied().map(Cellule::nom).unwrap_or_default()
    }

    #[test]
    fn affiche_les_neuf_categories_sans_recents_a_l_ouverture() {
        let e = ouvert();
        assert_eq!(e.onglets.len(), 9);
        assert_eq!(e.onglets[0].libelle, "Smileys et émotions");
        assert_eq!(e.rangees[0], Rangee::Titre("Smileys et émotions".into()));
        assert_eq!(e.actif, None);
        assert_eq!(e.message(), "");
    }

    #[test]
    fn cherche_par_nom_code_ou_mot_cle_et_insere_avec_entree_ou_maj() {
        let mut e = ouvert();
        e.chercher(":joy:");
        assert_eq!(e.cellules[0].nom(), "visage riant aux larmes");
        assert!(e.onglets.is_empty());
        let garde = e.choisir(0, true).unwrap();
        assert_eq!((garde.insertion.as_str(), garde.garder), ("😂", true));
        assert!(e.ouvert);
        let choix = e.choisir_actif(false).unwrap();
        assert_eq!((choix.insertion.as_str(), choix.garder), ("😂", false));
        assert!(!e.ouvert);
        e.ouvrir(Cible::Saisie, Vec::new(), Vec::new());
        e.chercher("pouce vers le haut");
        assert_eq!(e.cellules[0].insertion(), "👍");
        e.chercher("fromage");
        assert!(e.cellules.iter().any(|c| c.insertion() == "🧀"));
    }

    #[test]
    fn range_le_smiley_choisi_dans_les_recents_et_montre_nom_et_code_au_survol() {
        let mut e = ouvert();
        e.chercher("joy");
        e.survoler(Some(0));
        assert_eq!(e.pied().map(Cellule::code).as_deref(), Some(":joy:"));
        e.choisir(0, true);
        e.chercher("");
        assert_eq!(e.onglets[0].libelle, "Récents");
        assert_eq!(e.onglets.len(), 10);
        assert_eq!(e.recents, ["😂"]);
    }

    #[test]
    fn recherche_sans_resultat_et_emoji_de_serveur() {
        let mut e = ouvert();
        e.definir_emojis_serveur(vec![perso("chat_zqx")]);
        assert_eq!(
            e.rangees.iter().rev().nth(1),
            Some(&Rangee::Titre("Smileys des serveurs".into()))
        );
        e.chercher("zzzzzzzzz");
        assert!(e.cellules.is_empty());
        assert_eq!(e.message(), "Aucun smiley trouvé.");
        e.chercher("zqx");
        assert_eq!(e.cellules, [Cellule::Serveur(perso("chat_zqx"))]);
        assert_eq!(e.choisir_actif(false).unwrap().insertion, ":chat_zqx:");
        assert!(e.recents.is_empty());
    }

    #[test]
    fn le_clavier_parcourt_la_grille_et_les_categories() {
        let mut e = ouvert();
        e.deplacer(Sens::Haut);
        assert_eq!(e.actif, None);
        e.deplacer(Sens::Bas);
        assert_eq!(e.actif, Some(0));
        e.deplacer(Sens::Droite);
        e.deplacer(Sens::Bas);
        assert_eq!(e.actif, Some(1 + COLONNES));
        e.deplacer(Sens::Gauche);
        assert_eq!(e.actif, Some(COLONNES));
        e.deplacer(Sens::Haut);
        e.deplacer(Sens::Haut);
        assert_eq!(
            e.actif, None,
            "flèche haut sur la première ligne : retour à la recherche"
        );
        let rangee = e.aller_a(1).unwrap();
        assert_eq!(e.rangees[rangee - 1], Rangee::Titre("Personnes".into()));
        assert_eq!(e.onglet_actif, 1);
        let premiere = e.actif.unwrap();
        e.deplacer(Sens::Haut);
        assert_eq!(
            e.onglet_actif, 0,
            "la ligne du dessus est dans la catégorie précédente"
        );
        assert!(e.actif.unwrap() < premiere);
        assert!(!libelle_actif(&e).is_empty());
        e.defile(rangee as f32 * HAUTEUR_RANGEE);
        assert_eq!(e.onglet_actif, 1);
        let derniere = e.cellules.len() - 1;
        e.actif = Some(derniere);
        e.deplacer(Sens::Bas);
        e.deplacer(Sens::Droite);
        assert_eq!(e.actif, Some(derniere));
    }

    #[test]
    fn la_cible_suit_le_choix() {
        let mut e = EtatSelecteur::default();
        e.ouvrir(Cible::Reaction("m1".into()), Vec::new(), Vec::new());
        e.chercher("joy");
        assert_eq!(
            e.choisir_actif(false).unwrap().cible,
            Cible::Reaction("m1".into())
        );
    }
}
