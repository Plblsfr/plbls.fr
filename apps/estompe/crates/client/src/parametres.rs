use estompe_plateforme::libelle_touche;

use crate::emoji::replier;
use crate::preferences::Preferences;
use crate::types::{NotifySettings, NotifySettingsTypes};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Onglet {
    Compte,
    Notifications,
    Voix,
    Apparence,
    Application,
    Debogage,
    Signalements,
    Raccourcis,
    Replay,
}

pub const ONGLETS_DU_FRONT: [Onglet; 7] = [
    Onglet::Compte,
    Onglet::Notifications,
    Onglet::Voix,
    Onglet::Apparence,
    Onglet::Application,
    Onglet::Debogage,
    Onglet::Signalements,
];

pub const ONGLETS: [Onglet; 9] = [
    Onglet::Compte,
    Onglet::Notifications,
    Onglet::Voix,
    Onglet::Apparence,
    Onglet::Application,
    Onglet::Debogage,
    Onglet::Signalements,
    Onglet::Raccourcis,
    Onglet::Replay,
];

pub const ONGLET_PAR_DEFAUT: Onglet = Onglet::Compte;

impl Onglet {
    pub fn id(self) -> &'static str {
        match self {
            Self::Compte => "compte",
            Self::Notifications => "notifications",
            Self::Voix => "voix",
            Self::Apparence => "apparence",
            Self::Application => "application",
            Self::Debogage => "debogage",
            Self::Signalements => "signalements",
            Self::Raccourcis => "raccourcis",
            Self::Replay => "replay",
        }
    }

    pub fn depuis_id(id: &str) -> Option<Self> {
        ONGLETS.into_iter().find(|o| o.id() == id)
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::Compte => "Compte",
            Self::Notifications => "Notifications",
            Self::Voix => "Voix et vidéo",
            Self::Apparence => "Apparence",
            Self::Application => "Application",
            Self::Debogage => "Débogage",
            Self::Signalements => "Signalements",
            Self::Raccourcis => "Raccourcis",
            Self::Replay => "Instant replay",
        }
    }

    pub fn icone(self) -> &'static str {
        match self {
            Self::Compte => "perso",
            Self::Notifications => "cloche",
            Self::Voix => "mic",
            Self::Apparence => "soleil",
            Self::Application => "telecharge",
            Self::Debogage => "bug",
            Self::Signalements => "drapeau",
            Self::Raccourcis => "commande",
            Self::Replay => "replay",
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Self::Replay => "bêta",
            _ => "",
        }
    }

    pub fn index(self) -> &'static [&'static str] {
        match self {
            Self::Compte => &[
                "Avatar",
                "Photo de profil par serveur",
                "Se déconnecter",
                "Profil",
                "Session",
            ],
            Self::Notifications => &[
                "Messages privés et mentions",
                "Notification de test",
                "Ne pas déranger",
                "Types de notifications",
                "Messages privés",
                "Mentions",
                "Messages de salon",
                "Activité vocale",
                "Contenu et son",
                "Aperçu des notifications",
                "Son à la réception",
                "Par serveur et par salon",
                "Ignorer @everyone et @here",
                "Notifications du bureau",
                "Réglage général",
                "Tous les messages",
                "Mentions seulement",
                "Sourdine",
                "Régler un autre salon",
            ],
            Self::Voix => &[
                "Micro",
                "Sortie audio",
                "Caméra",
                "Volume du micro",
                "Réduction de bruit",
                "Activation du micro",
                "Détection de voix",
                "Push-to-talk",
                "Appuyer pour parler",
                "Tester mon micro",
                "Seuil de voix",
                "Regarder automatiquement les streams",
                "Sons d'arrivée et de départ",
                "Volume des sons",
                "Son de l'écran",
                "Qualité de la caméra",
                "Codec vidéo",
                "Arrière-plan de la caméra",
                "Flou",
                "Incrustation caméra",
                "Afficher ce que j'écoute",
                "Résolution du partage d'écran",
                "Images par seconde",
                "Jeux et activité",
                "Overlay",
            ],
            Self::Apparence => &[
                "Thème",
                "Clair",
                "Sombre",
                "Densité des messages",
                "Aperçu des liens",
                "Transcrire automatiquement",
                "Réduire les animations",
            ],
            Self::Application => &[
                "Mise à jour",
                "Version installée",
                "Télécharger",
                "Lancer à l'ouverture de la session",
                "À propos",
                "Licences",
            ],
            Self::Debogage => &[
                "Mode debug",
                "Journal",
                "Copier",
                "Exporter",
                "Vider",
                "Mesures de performance",
            ],
            Self::Signalements => &["Signaler un bug", "Mes signalements"],
            Self::Raccourcis => &[
                "Rechercher ou aller à",
                "Marge",
                "Serveur précédent",
                "Serveur suivant",
                "Couper le micro",
                "Couper le son",
                "Partage d'écran",
                "Touches globales",
                "Menu contextuel",
            ],
            Self::Replay => &[
                "Clips",
                "Tampon",
                "Résolution",
                "Images par seconde",
                "Durée gardée",
                "Qualité",
                "Codec",
                "Enregistrer mon micro",
                "Afficher le curseur",
                "Raccourci de sauvegarde",
                "Jeux ajoutés",
                "Jeux exclus",
                "Dossier des clips",
                "Quota",
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trouve {
    pub onglet: Onglet,
    pub trouves: Vec<&'static str>,
}

fn plat(texte: &str) -> String {
    replier(texte.trim())
}

pub fn chercher(requete: &str, onglets: &[Onglet]) -> Vec<Trouve> {
    let q = plat(requete);
    onglets
        .iter()
        .filter_map(|&onglet| {
            if q.is_empty() {
                return Some(Trouve {
                    onglet,
                    trouves: Vec::new(),
                });
            }
            let trouves: Vec<&'static str> = onglet
                .index()
                .iter()
                .copied()
                .filter(|s| plat(s).contains(&q))
                .collect();
            (plat(onglet.libelle()).contains(&q) || !trouves.is_empty())
                .then_some(Trouve { onglet, trouves })
        })
        .collect()
}

pub fn onglet_a_ouvrir(demande: &str, retenu: &str, onglets: &[Onglet]) -> Onglet {
    [demande, retenu]
        .into_iter()
        .filter_map(Onglet::depuis_id)
        .find(|o| onglets.contains(o))
        .unwrap_or(ONGLET_PAR_DEFAUT)
}

pub fn voisin(onglets: &[Onglet], courant: Onglet, pas: i32) -> Onglet {
    let Some(i) = onglets.iter().position(|o| *o == courant) else {
        return onglets.first().copied().unwrap_or(ONGLET_PAR_DEFAUT);
    };
    let n = onglets.len() as i32;
    onglets[(i as i32 + pas).rem_euclid(n) as usize]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raccourci {
    pub action: &'static str,
    pub touches: String,
    pub reglage: &'static str,
}

const RACCOURCIS_FIXES: [(&str, &str); 10] = [
    ("Rechercher ou aller à…", "Ctrl+K"),
    ("Afficher ou masquer la marge", "Ctrl+."),
    ("Serveur précédent ou suivant", "Ctrl+Alt+↑ / ↓"),
    ("Couper ou rétablir le micro", "Alt+Maj+M"),
    ("Couper ou rétablir le son", "Alt+Maj+D"),
    ("Activer ou couper la caméra (en vocal)", "Alt+Maj+V"),
    ("Partager l'écran ou arrêter (en vocal)", "Alt+Maj+S"),
    ("Ouvrir le menu contextuel", "Maj+F10 ou Menu"),
    ("Envoyer ou passer à la ligne", "Entrée / Maj+Entrée"),
    ("Fermer un menu, une fenêtre ou les paramètres", "Échap"),
];

pub fn raccourcis_fixes() -> Vec<Raccourci> {
    RACCOURCIS_FIXES
        .iter()
        .map(|(action, touches)| Raccourci {
            action,
            touches: (*touches).to_string(),
            reglage: "",
        })
        .collect()
}

fn touche_ou_desactive(actif: bool, code: &str) -> String {
    if actif {
        libelle_touche(code)
    } else {
        "désactivé".to_string()
    }
}

pub fn raccourcis_globaux(p: &Preferences, overlay: bool, replay: bool) -> Vec<Raccourci> {
    let mut liste = vec![Raccourci {
        action: "Appuyer pour parler",
        touches: touche_ou_desactive(p.ptt.actif, &p.ptt.touche),
        reglage: "Réglages audio",
    }];
    if overlay {
        let actif = p.overlay_messages.actif();
        liste.push(Raccourci {
            action: "Répondre au message de l'overlay",
            touches: touche_ou_desactive(actif, &p.overlay_messages.touche_reponse),
            reglage: "Jeux et activité",
        });
        liste.push(Raccourci {
            action: "Ouvrir le message de l'overlay",
            touches: touche_ou_desactive(actif, &p.overlay_messages.touche_ouvrir),
            reglage: "Jeux et activité",
        });
    }
    if replay {
        liste.push(Raccourci {
            action: "Enregistrer un clip",
            touches: touche_ou_desactive(p.replay.actif, &p.replay.touche),
            reglage: "Instant replay",
        });
    }
    liste
}

pub const TYPES_DE_NOTIFICATION: [&str; 4] = ["dm", "mention", "channel", "voice"];

pub fn types_de_notification(r: &NotifySettings) -> [bool; 4] {
    let t = r.types.clone().unwrap_or_default();
    [t.dm, t.mention, t.channel, t.voice].map(|v| v.unwrap_or(true))
}

pub fn avec_type_de_notification(
    r: &NotifySettings,
    genre: &str,
    actif: bool,
) -> Option<NotifySettings> {
    let [dm, mention, channel, voice] = types_de_notification(r);
    let mut t = NotifySettingsTypes {
        dm: Some(dm),
        mention: Some(mention),
        channel: Some(channel),
        voice: Some(voice),
    };
    match genre {
        "dm" => t.dm = Some(actif),
        "mention" => t.mention = Some(actif),
        "channel" => t.channel = Some(actif),
        "voice" => t.voice = Some(actif),
        _ => return None,
    }
    Some(NotifySettings {
        types: Some(t),
        ..r.clone()
    })
}

pub const CLICS_ALPHA: u32 = 7;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeblocageAlpha {
    clics: u32,
}

impl DeblocageAlpha {
    pub fn cliquer(&mut self, actuel: bool) -> Option<bool> {
        self.clics += 1;
        if self.clics < CLICS_ALPHA {
            return None;
        }
        self.clics = 0;
        Some(!actuel)
    }
}

pub fn annonce_alpha(actif: bool) -> &'static str {
    if actif {
        "Fonctions alpha débloquées."
    } else {
        "Fonctions alpha verrouillées."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(trouves: &[Trouve]) -> Vec<&'static str> {
        trouves.iter().map(|t| t.onglet.id()).collect()
    }

    #[test]
    fn montre_tous_les_onglets_sans_recherche() {
        assert_eq!(
            ids(&chercher("  ", &ONGLETS_DU_FRONT)),
            ONGLETS_DU_FRONT.map(Onglet::id)
        );
        assert_eq!(
            ONGLETS_DU_FRONT.map(Onglet::libelle),
            [
                "Compte",
                "Notifications",
                "Voix et vidéo",
                "Apparence",
                "Application",
                "Débogage",
                "Signalements"
            ]
        );
        assert!(chercher("", &ONGLETS).iter().all(|t| t.trouves.is_empty()));
    }

    #[test]
    fn trouve_un_onglet_par_son_nom_ou_par_un_reglage_sans_tenir_compte_des_accents_ni_de_la_casse()
    {
        assert_eq!(ids(&chercher("debogage", &ONGLETS_DU_FRONT)), ["debogage"]);
        let r = chercher("THEME", &ONGLETS_DU_FRONT);
        assert_eq!(ids(&r), ["apparence"]);
        assert_eq!(r[0].trouves, ["Thème"]);
        assert_eq!(
            ids(&chercher("ne pas déranger", &ONGLETS_DU_FRONT)),
            ["notifications"]
        );
        assert_eq!(
            chercher("sourdine", &ONGLETS_DU_FRONT)[0].trouves,
            ["Sourdine"]
        );
    }

    #[test]
    fn liste_les_onglets_de_chaque_reglage_trouve() {
        assert_eq!(ids(&chercher("camera", &ONGLETS)), ["voix"]);
        let micro = chercher("micro", &ONGLETS_DU_FRONT);
        assert!(micro
            .iter()
            .find(|t| t.onglet == Onglet::Voix)
            .unwrap()
            .trouves
            .contains(&"Volume du micro"));
        assert_eq!(ids(&chercher("clips", &ONGLETS)), ["replay"]);
        assert_eq!(
            ids(&chercher("raccourci", &ONGLETS)),
            ["raccourcis", "replay"]
        );
    }

    #[test]
    fn ne_renvoie_rien_quand_rien_ne_correspond() {
        assert!(chercher("zzz", &ONGLETS).is_empty());
    }

    #[test]
    fn l_onglet_ouvert_est_le_demande_sinon_le_dernier_sinon_le_compte() {
        assert_eq!(
            onglet_a_ouvrir("debogage", "voix", &ONGLETS),
            Onglet::Debogage
        );
        assert_eq!(onglet_a_ouvrir("", "voix", &ONGLETS), Onglet::Voix);
        assert_eq!(
            onglet_a_ouvrir("inconnu", "replay", &ONGLETS_DU_FRONT),
            Onglet::Compte
        );
        assert_eq!(onglet_a_ouvrir("", "", &ONGLETS), Onglet::Compte);
    }

    #[test]
    fn les_fleches_passent_d_un_onglet_a_l_autre_en_boucle() {
        assert_eq!(
            voisin(&ONGLETS_DU_FRONT, Onglet::Compte, 1),
            Onglet::Notifications
        );
        assert_eq!(
            voisin(&ONGLETS_DU_FRONT, Onglet::Compte, -1),
            Onglet::Signalements
        );
        assert_eq!(
            voisin(&ONGLETS_DU_FRONT, Onglet::Signalements, 1),
            Onglet::Compte
        );
        assert_eq!(voisin(&[Onglet::Voix], Onglet::Replay, 1), Onglet::Voix);
        assert_eq!(voisin(&[], Onglet::Replay, 1), Onglet::Compte);
    }

    #[test]
    fn sept_clics_sur_la_version_basculent_les_fonctions_alpha() {
        let mut d = DeblocageAlpha::default();
        for _ in 0..6 {
            assert_eq!(d.cliquer(false), None);
        }
        assert_eq!(d.cliquer(false), Some(true));
        for _ in 0..6 {
            assert_eq!(d.cliquer(true), None);
        }
        assert_eq!(d.cliquer(true), Some(false));
        assert_eq!(annonce_alpha(true), "Fonctions alpha débloquées.");
        assert_eq!(annonce_alpha(false), "Fonctions alpha verrouillées.");
    }

    #[test]
    fn un_type_de_notification_change_garde_les_autres_reglages_du_compte() {
        let mut r = crate::store::reglages_par_defaut();
        r.types = Some(NotifySettingsTypes {
            dm: Some(false),
            ..NotifySettingsTypes::default()
        });
        r.preview = Some(false);
        assert_eq!(types_de_notification(&r), [false, true, true, true]);
        let modifie = avec_type_de_notification(&r, "channel", false).unwrap();
        assert_eq!(types_de_notification(&modifie), [false, true, false, true]);
        assert_eq!(
            (modifie.preview, modifie.levels.clone()),
            (Some(false), r.levels.clone())
        );
        let t = modifie.types.unwrap();
        assert_eq!(
            (t.dm, t.mention, t.voice),
            (Some(false), Some(true), Some(true))
        );
        assert!(avec_type_de_notification(&r, "inconnu", true).is_none());
        assert_eq!(
            TYPES_DE_NOTIFICATION.len(),
            types_de_notification(&NotifySettings::default()).len()
        );
    }

    #[test]
    fn les_touches_globales_suivent_les_preferences() {
        let mut p = Preferences::default();
        let liste = raccourcis_globaux(&p, true, true);
        let touches: Vec<&str> = liste.iter().map(|r| r.touches.as_str()).collect();
        assert_eq!(touches, ["désactivé", "F8", "F7", "désactivé"]);
        p.ptt.actif = true;
        p.ptt.touche = "KeyV".into();
        p.replay.actif = true;
        p.replay.touche = "F10".into();
        let liste = raccourcis_globaux(&p, false, true);
        assert_eq!(
            liste.iter().map(|r| r.touches.as_str()).collect::<Vec<_>>(),
            ["V", "F10"]
        );
        assert_eq!(liste[1].reglage, "Instant replay");
        assert_eq!(raccourcis_fixes()[0].touches, "Ctrl+K");
    }
}
