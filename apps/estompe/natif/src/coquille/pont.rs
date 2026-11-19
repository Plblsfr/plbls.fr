use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard};

use chrono::NaiveDateTime;
use estompe_client::ordre_serveurs::Sens;
use estompe_client::route::Route;
use estompe_client::sections::Section;
use estompe_client::store::{Changement, CleConversation, Store};
use estompe_client::temps_reel::EtatReseau;
use slint::{ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel};

use super::modeles::{cibles, remplacer_ligne, synchroniser, Cibles, Ensemble};
use super::navigation::Navigation;
use super::projection::*;
use crate::session::Memoire;
use crate::{
    Accueil, DonneesCoquille, EnTetePanneau, LignePrive, LigneRattrapage, LigneSalon, LigneSection,
    LigneServeur, LigneVocal, Principale, VueContenu,
};

pub type Horloge = Arc<dyn Fn() -> NaiveDateTime + Send + Sync>;
pub type Icone = SharedPixelBuffer<Rgba8Pixel>;

const COTE_ICONE: u32 = 128;

pub fn horloge_locale() -> Horloge {
    Arc::new(|| chrono::Local::now().naive_local())
}

pub fn decoder_icone(octets: &[u8]) -> Option<Icone> {
    let image = image::load_from_memory(octets).ok()?;
    let image = if image.width() > COTE_ICONE || image.height() > COTE_ICONE {
        image.thumbnail(COTE_ICONE, COTE_ICONE)
    } else {
        image
    };
    let rgba = image.into_rgba8();
    Some(SharedPixelBuffer::clone_from_slice(
        rgba.as_raw(),
        rgba.width(),
        rgba.height(),
    ))
}

#[derive(Default)]
struct Ombres {
    serveurs: Vec<DonneesServeur>,
    salons: Vec<DonneesSalon>,
    prives: Vec<DonneesPrive>,
    sections: Vec<DonneesSection>,
    rattrapage: Vec<DonneesRattrapage>,
    vocaux: Vec<DonneesVocal>,
    panneau: Option<DonneesPanneau>,
    vue: Option<DonneesVue>,
    accueil: Option<DonneesAccueil>,
    pseudo: Option<String>,
}

struct Interne {
    navigation: Navigation,
    ombres: Ombres,
    sections: Vec<Section>,
    reseau: EtatReseau,
    deja_connecte: bool,
    icones: BTreeMap<String, (i64, Icone)>,
    demandes: BTreeSet<(String, i64)>,
}

#[derive(Clone)]
pub struct Pont {
    interne: Arc<Mutex<Interne>>,
    store: Store,
    fenetre: slint::Weak<Principale>,
    runtime: tokio::runtime::Handle,
    memoire: Option<Memoire>,
    horloge: Horloge,
}

fn vec_de<S: Clone + 'static>(modele: &ModelRc<S>) -> &VecModel<S> {
    modele
        .as_any()
        .downcast_ref::<VecModel<S>>()
        .expect("modèle installé par installer_modeles")
}

pub fn installer_modeles(f: &Principale) {
    let g = f.global::<DonneesCoquille>();
    g.set_serveurs(ModelRc::new(VecModel::<LigneServeur>::default()));
    g.set_salons(ModelRc::new(VecModel::<LigneSalon>::default()));
    g.set_prives(ModelRc::new(VecModel::<LignePrive>::default()));
    g.set_sections(ModelRc::new(VecModel::<LigneSection>::default()));
    g.set_rattrapage(ModelRc::new(VecModel::<LigneRattrapage>::default()));
    g.set_vocaux(ModelRc::new(VecModel::<LigneVocal>::default()));
}

pub fn vider_modeles(f: &Principale) {
    let g = f.global::<DonneesCoquille>();
    vec_de(&g.get_serveurs()).set_vec(Vec::new());
    vec_de(&g.get_salons()).set_vec(Vec::new());
    vec_de(&g.get_prives()).set_vec(Vec::new());
    vec_de(&g.get_sections()).set_vec(Vec::new());
    vec_de(&g.get_rattrapage()).set_vec(Vec::new());
    vec_de(&g.get_vocaux()).set_vec(Vec::new());
    g.set_panneau(EnTetePanneau::default());
    g.set_vue(VueContenu::default());
    g.set_accueil(Accueil::default());
    g.set_bandeau(crate::EtatBandeau::Aucun);
    g.set_pret(false);
    g.set_pseudo("".into());
    g.set_initiales("".into());
}

fn image_de(icones: &BTreeMap<String, (i64, Icone)>, serveur: &str, version: i64) -> Option<Image> {
    icones
        .get(serveur)
        .filter(|(v, _)| *v == version && version > 0)
        .map(|(_, tampon)| Image::from_rgba8(tampon.clone()))
}

fn ligne_serveur_slint(
    d: &DonneesServeur,
    icones: &BTreeMap<String, (i64, Icone)>,
) -> LigneServeur {
    let image = image_de(icones, &d.id, d.version_icone);
    LigneServeur {
        id: d.id.as_str().into(),
        nom: d.nom.as_str().into(),
        initiales: d.initiales.as_str().into(),
        a_image: image.is_some(),
        image: image.unwrap_or_default(),
        actif: d.actif,
        non_lus: d.non_lus.min(i64::from(i32::MAX)) as i32,
        mentions: d.mentions.min(i32::MAX as usize) as i32,
    }
}

fn ligne_salon_slint(d: &DonneesSalon) -> LigneSalon {
    LigneSalon {
        id: d.id.as_str().into(),
        nom: d.nom.as_str().into(),
        icone: d.icone.into(),
        titre_groupe: d.titre_groupe.into(),
        actif: d.actif,
        non_lus: d.non_lus.min(i64::from(i32::MAX)) as i32,
        mentions: d.mentions.min(i32::MAX as usize) as i32,
        bientot: d.bientot,
        occupants: d.occupants.min(i32::MAX as usize) as i32,
    }
}

fn ligne_prive_slint(d: &DonneesPrive) -> LignePrive {
    LignePrive {
        id: d.id.as_str().into(),
        pseudo: d.pseudo.as_str().into(),
        initiales: d.initiales.as_str().into(),
        teinte: d.teinte,
        actif: d.actif,
        non_lus: d.non_lus.min(i64::from(i32::MAX)) as i32,
        mentions: d.mentions.min(i32::MAX as usize) as i32,
        en_ligne: d.en_ligne,
    }
}

fn ligne_section_slint(d: &DonneesSection) -> LigneSection {
    LigneSection {
        id: identifiant_de_section(d.section).into(),
        nom: nom_de_section(d.section).into(),
        icone: icone_de_section(d.section).into(),
        actif: d.actif,
    }
}

fn ligne_rattrapage_slint(d: &DonneesRattrapage) -> LigneRattrapage {
    LigneRattrapage {
        cle: d.cle.as_str().into(),
        titre: d.titre.as_str().into(),
        detail: d.detail.as_str().into(),
        icone: d.icone.into(),
        nombre: d.nombre.min(i64::from(i32::MAX)) as i32,
        mention: d.mention,
    }
}

fn ligne_vocal_slint(d: &DonneesVocal) -> LigneVocal {
    LigneVocal {
        cle: d.cle.as_str().into(),
        nom: d.nom.as_str().into(),
        lieu: d.lieu.as_str().into(),
        icone: d.icone.into(),
        occupants: d.occupants.min(i32::MAX as usize) as i32,
    }
}

fn panneau_slint(d: &DonneesPanneau, icones: &BTreeMap<String, (i64, Icone)>) -> EnTetePanneau {
    let image = d
        .serveur
        .as_deref()
        .and_then(|s| image_de(icones, s, d.version_icone));
    EnTetePanneau {
        genre: d.genre,
        titre: d.titre.as_str().into(),
        initiales: d.initiales.as_str().into(),
        a_image: image.is_some(),
        image: image.unwrap_or_default(),
    }
}

fn vue_slint(d: &DonneesVue) -> VueContenu {
    VueContenu {
        genre: d.genre,
        titre: d.titre.as_str().into(),
        sujet: d.sujet.as_str().into(),
        icone: d.icone.into(),
        message: d.message.as_str().into(),
        detail: d.detail.as_str().into(),
    }
}

fn accueil_slint(d: &DonneesAccueil) -> Accueil {
    Accueil {
        actif: d.actif,
        non_lus: d.non_lus.min(i64::from(i32::MAX)) as i32,
        mentions: d.mentions.min(i32::MAX as usize) as i32,
        date: d.date.as_str().into(),
        salut: d.salut.as_str().into(),
        resume: d.resume.as_str().into(),
        sans_serveur: d.sans_serveur,
    }
}

fn tout() -> Cibles {
    Cibles {
        serveurs: Ensemble::Tous,
        salons: Ensemble::Tous,
        prives: Ensemble::Tous,
        accueil: true,
        panneau: true,
        vue: true,
        compte: true,
        corriger: true,
    }
}

impl Pont {
    pub fn nouveau(
        store: Store,
        fenetre: slint::Weak<Principale>,
        runtime: tokio::runtime::Handle,
        memoire: Option<Memoire>,
        horloge: Horloge,
        navigation: Navigation,
    ) -> Self {
        Self {
            interne: Arc::new(Mutex::new(Interne {
                navigation,
                ombres: Ombres::default(),
                sections: Vec::new(),
                reseau: EtatReseau::Connecte,
                deja_connecte: false,
                icones: BTreeMap::new(),
                demandes: BTreeSet::new(),
            })),
            store,
            fenetre,
            runtime,
            memoire,
            horloge,
        }
    }

    fn verrou(&self) -> MutexGuard<'_, Interne> {
        self.interne.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn route(&self) -> Route {
        self.verrou().navigation.route().clone()
    }

    pub fn demarrer(&self, f: &Principale) {
        let route = self.route();
        self.naviguer(f, route);
    }

    pub fn tout_projeter(&self, f: &Principale) {
        self.projeter(f, &tout(), true);
        f.global::<DonneesCoquille>().set_pret(true);
    }

    pub fn appliquer(&self, f: &Principale, changements: &[Changement]) {
        let mut c = {
            let interne = self.verrou();
            cibles(changements, interne.navigation.route())
        };
        if c.corriger {
            let corrigee = self
                .store
                .lire(|etat| self.verrou().navigation.corriger(etat));
            if corrigee {
                c = tout();
                self.ouvrir_conversation_courante();
            }
        }
        self.projeter(f, &c, false);
    }

    fn projeter(&self, f: &Principale, c: &Cibles, sections: bool) {
        let g = f.global::<DonneesCoquille>();
        let maintenant = (self.horloge)();
        let mut guard = self.verrou();
        let interne = &mut *guard;
        let route = interne.navigation.route().clone();
        let Interne {
            ombres,
            icones,
            sections: liste_sections,
            ..
        } = interne;
        self.store.lire(|etat| {
            let serveurs = g.get_serveurs();
            let modele = vec_de(&serveurs);
            let convertir = |d: &DonneesServeur| ligne_serveur_slint(d, icones);
            let mut complet = c.serveurs == Ensemble::Tous;
            if let Ensemble::Certains(ids) = &c.serveurs {
                for id in ids {
                    match (
                        ombres.serveurs.iter().position(|d| d.id == *id),
                        ligne_serveur(etat, id, &route),
                    ) {
                        (Some(i), Some(ligne)) => {
                            remplacer_ligne(modele, &mut ombres.serveurs, i, ligne, convertir);
                        }
                        _ => complet = true,
                    }
                }
            }
            if complet {
                synchroniser(
                    modele,
                    &mut ombres.serveurs,
                    lignes_serveurs(etat, &route),
                    |d| d.id.clone(),
                    convertir,
                );
            }
            if !c.salons.est_vide() {
                synchroniser(
                    vec_de(&g.get_salons()),
                    &mut ombres.salons,
                    lignes_salons(etat, &route),
                    |d| d.id.clone(),
                    ligne_salon_slint,
                );
            }
            if !c.prives.est_vide() {
                synchroniser(
                    vec_de(&g.get_prives()),
                    &mut ombres.prives,
                    lignes_prives(etat, &route),
                    |d| d.id.clone(),
                    ligne_prive_slint,
                );
            }
            if sections {
                synchroniser(
                    vec_de(&g.get_sections()),
                    &mut ombres.sections,
                    lignes_sections(liste_sections, &route),
                    |d| d.section,
                    ligne_section_slint,
                );
            }
            if c.accueil {
                let accueil = donnees_accueil(etat, &route, maintenant);
                if ombres.accueil.as_ref() != Some(&accueil) {
                    g.set_accueil(accueil_slint(&accueil));
                    ombres.accueil = Some(accueil);
                }
                synchroniser(
                    vec_de(&g.get_rattrapage()),
                    &mut ombres.rattrapage,
                    lignes_rattrapage(etat),
                    |d| d.cle.clone(),
                    ligne_rattrapage_slint,
                );
                synchroniser(
                    vec_de(&g.get_vocaux()),
                    &mut ombres.vocaux,
                    lignes_vocaux(etat),
                    |d| d.cle.clone(),
                    ligne_vocal_slint,
                );
            }
            if c.panneau {
                let panneau = donnees_panneau(etat, &route);
                if ombres.panneau.as_ref() != Some(&panneau) {
                    g.set_panneau(panneau_slint(&panneau, icones));
                    ombres.panneau = Some(panneau);
                }
            }
            if c.vue {
                let vue = donnees_vue(etat, &route);
                if ombres.vue.as_ref() != Some(&vue) {
                    g.set_vue(vue_slint(&vue));
                    ombres.vue = Some(vue);
                }
            }
            if c.compte {
                let pseudo = etat.pseudo().to_string();
                if ombres.pseudo.as_ref() != Some(&pseudo) {
                    g.set_pseudo(pseudo.as_str().into());
                    g.set_initiales(initiales(&pseudo).into());
                    ombres.pseudo = Some(pseudo);
                }
            }
        });
        let a_demander: Vec<(String, i64)> = interne
            .ombres
            .serveurs
            .iter()
            .filter(|d| d.version_icone > 0)
            .filter(|d| {
                interne
                    .icones
                    .get(&d.id)
                    .is_none_or(|(v, _)| *v != d.version_icone)
            })
            .map(|d| (d.id.clone(), d.version_icone))
            .filter(|cle| interne.demandes.insert(cle.clone()))
            .collect();
        drop(guard);
        for (serveur, version) in a_demander {
            self.telecharger_icone(serveur, version);
        }
    }

    fn telecharger_icone(&self, serveur: String, version: i64) {
        let pont = self.clone();
        self.runtime.spawn(async move {
            let Ok(octets) = pont.store.client().icone_de_serveur(&serveur).await else {
                return;
            };
            if let Some(tampon) = decoder_icone(&octets) {
                pont.installer_icone_recue(serveur, version, tampon);
            }
        });
    }

    fn installer_icone_recue(&self, serveur: String, version: i64, tampon: Icone) {
        self.verrou()
            .icones
            .insert(serveur.clone(), (version, tampon));
        let pont = self.clone();
        let _ = self
            .fenetre
            .upgrade_in_event_loop(move |f| pont.icone_recue(&f, &serveur));
    }

    pub fn installer_icone(&self, f: &Principale, serveur: &str, version: i64, tampon: Icone) {
        self.verrou()
            .icones
            .insert(serveur.to_string(), (version, tampon));
        self.icone_recue(f, serveur);
    }

    fn icone_recue(&self, f: &Principale, serveur: &str) {
        let g = f.global::<DonneesCoquille>();
        let interne = self.verrou();
        if let Some(i) = interne.ombres.serveurs.iter().position(|d| d.id == serveur) {
            vec_de(&g.get_serveurs()).set_row_data(
                i,
                ligne_serveur_slint(&interne.ombres.serveurs[i], &interne.icones),
            );
        }
        if let Some(panneau) = interne
            .ombres
            .panneau
            .as_ref()
            .filter(|p| p.serveur.as_deref() == Some(serveur))
        {
            g.set_panneau(panneau_slint(panneau, &interne.icones));
        }
    }

    pub fn naviguer(&self, f: &Principale, route: Route) {
        let adresse = self.store.lire(|etat| {
            let mut interne = self.verrou();
            interne.navigation.aller(route, etat);
            interne.navigation.corriger(etat);
            interne.navigation.adresse_a_retenir(etat)
        });
        self.tout_projeter(f);
        if let (Some(adresse), Some(memoire)) = (adresse, &self.memoire) {
            let vue = self.store.memoriser_vue(&adresse);
            memoire.modifier(|p| p.derniere_vue = Some(vue));
        }
        self.ouvrir_conversation_courante();
    }

    fn ouvrir_conversation_courante(&self) {
        let route = self.route();
        let cle = self
            .store
            .lire(|etat| conversation_de_la_route(etat, &route));
        let store = self.store.clone();
        if store.lire(|etat| etat.courante == cle) {
            return;
        }
        self.runtime.spawn(async move { store.ouvrir(cle).await });
    }

    pub fn ouvrir_accueil(&self, f: &Principale) {
        self.naviguer(f, Route::Accueil);
    }

    pub fn ouvrir_serveur(&self, f: &Principale, serveur: &str) {
        let route = self
            .store
            .lire(|etat| self.verrou().navigation.route_du_serveur(serveur, etat));
        self.naviguer(f, route);
    }

    pub fn ouvrir_salon(&self, f: &Principale, salon: &str) {
        let Some(serveur) = serveur_courant(&self.route()).map(str::to_string) else {
            return;
        };
        self.naviguer(
            f,
            Route::Salon {
                serveur,
                salon: Some(salon.to_string()),
                message: None,
            },
        );
    }

    pub fn ouvrir_prive(&self, f: &Principale, prive: &str) {
        self.naviguer(
            f,
            Route::MessagePrive {
                id: prive.to_string(),
                message: None,
            },
        );
    }

    pub fn ouvrir_section(&self, f: &Principale, identifiant: &str) {
        if let Some(section) = section_de_l_identifiant(identifiant) {
            self.naviguer(f, route_de_section(section));
        }
    }

    pub fn ouvrir_conversation(&self, f: &Principale, cle: &str) {
        let cle = cle.split('#').next().unwrap_or_default();
        let route = CleConversation::analyser(cle).and_then(|cle| {
            self.store
                .lire(|etat| Navigation::route_de_la_conversation(etat, &cle))
        });
        if let Some(route) = route {
            self.naviguer(f, route);
        }
    }

    pub fn deplacer_serveur(&self, de: &str, vers: &str) {
        let ordre = self.store.deplacer_serveur(de, vers);
        if let Some(memoire) = &self.memoire {
            memoire.modifier(|p| p.ordre_serveurs = ordre);
        }
    }

    pub fn serveur_voisin(&self, f: &Principale, suivant: bool) {
        let route = self.route();
        let sens = if suivant {
            Sens::Suivant
        } else {
            Sens::Precedent
        };
        let courant = match &route {
            Route::Salon { serveur, .. } => Some(serveur.as_str()),
            Route::Accueil | Route::MessagePrive { .. } => Some(""),
            _ => None,
        };
        match self.store.serveur_voisin(courant, sens).as_deref() {
            Some("") => self.ouvrir_accueil(f),
            Some(serveur) => self.ouvrir_serveur(f, serveur),
            None => {}
        }
    }

    pub fn definir_reseau(&self, f: &Principale, reseau: EtatReseau) {
        let etat = {
            let mut interne = self.verrou();
            interne.reseau = reseau;
            interne.deja_connecte |= reseau == EtatReseau::Connecte;
            bandeau(interne.reseau, interne.deja_connecte)
        };
        f.global::<DonneesCoquille>().set_bandeau(etat);
    }

    pub fn definir_sections(&self, f: &Principale, sections: Vec<Section>) {
        self.verrou().sections = sections;
        self.projeter(f, &Cibles::default(), true);
    }

    pub fn rafraichir_accueil(&self, f: &Principale) {
        self.projeter(
            f,
            &Cibles {
                accueil: true,
                ..Cibles::default()
            },
            false,
        );
    }

    pub fn nombre_de_lignes(f: &Principale) -> (usize, usize, usize) {
        let g = f.global::<DonneesCoquille>();
        (
            g.get_serveurs().row_count(),
            g.get_salons().row_count(),
            g.get_prives().row_count(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(largeur: u32, hauteur: u32) -> Vec<u8> {
        let image =
            image::RgbaImage::from_pixel(largeur, hauteur, image::Rgba([208, 104, 138, 255]));
        let mut octets = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut octets, image::ImageFormat::Png)
            .unwrap();
        octets.into_inner()
    }

    #[test]
    fn decode_l_icone_du_serveur_et_reduit_les_grandes_images() {
        let petite = decoder_icone(&png(64, 32)).unwrap();
        assert_eq!((petite.width(), petite.height()), (64, 32));
        assert_eq!(petite.as_slice()[0], Rgba8Pixel::new(208, 104, 138, 255));
        let grande = decoder_icone(&png(512, 512)).unwrap();
        assert_eq!((grande.width(), grande.height()), (COTE_ICONE, COTE_ICONE));
        assert!(decoder_icone(b"pas une image").is_none());
    }
}
