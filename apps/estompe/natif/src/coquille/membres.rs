use std::cmp::Ordering;

use chrono::{DateTime, Datelike, Local};
use estompe_client::roles::{
    couleur_du_pseudo, grouper_membres, membre_exclu, nom_du_membre, roles_du_membre, Membre,
};
use estompe_client::store::{Etat, Presence};
pub use estompe_client::virtualisation::Fenetre;
use estompe_client::virtualisation::fenetre_de_hauteurs;

use super::avatars::CleAvatar;
use super::projection::{initiales, teinte, MOIS};

pub const HAUTEUR_LIGNE: f32 = 44.0;
pub const MARGE_FENETRE: f32 = 5.0 * HAUTEUR_LIGNE;
pub const ID_BOT: &str = "00000000-0000-0000-0000-000000000001";
pub const SEUIL_FILTRE: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct DonneesMembre {
    pub cle: String,
    pub en_tete: bool,
    pub titre: String,
    pub couleur_titre: Option<String>,
    pub utilisateur: String,
    pub nom: String,
    pub pseudo: String,
    pub initiales: String,
    pub teinte: i32,
    pub couleur: Option<String>,
    pub presence: Presence,
    pub activite: String,
    pub createur: bool,
    pub bot: bool,
    pub exclu: bool,
    pub avatar: Option<CleAvatar>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DonneesMarge {
    pub titre: String,
    pub total: usize,
    pub en_ligne: usize,
    pub filtrable: bool,
    pub vide: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoleCarte {
    pub nom: String,
    pub couleur: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DonneesCarte {
    pub utilisateur: String,
    pub nom: String,
    pub pseudo_origine: String,
    pub initiales: String,
    pub teinte: i32,
    pub couleur: Option<String>,
    pub banniere: Option<String>,
    pub presence: Presence,
    pub statut: String,
    pub exclu_jusqu_au: String,
    pub rang: &'static str,
    pub roles: Vec<RoleCarte>,
    pub serveur: String,
    pub arrivee: String,
    pub creation: String,
    pub bot: bool,
    pub moi: bool,
    pub avatar: Option<CleAvatar>,
}

pub fn replier(texte: &str) -> String {
    texte
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ý' | 'ÿ' => 'y',
            autre => autre,
        })
        .collect()
}

pub fn libelle_presence(presence: Presence) -> &'static str {
    match presence {
        Presence::EnLigne => "En ligne",
        Presence::Absent => "Absent",
        Presence::NePasDeranger => "Ne pas déranger",
        Presence::HorsLigne => "Hors ligne",
    }
}

fn texte(valeur: &Option<String>) -> &str {
    valeur.as_deref().unwrap_or_default()
}

fn couleur_valide(couleur: Option<&str>) -> Option<String> {
    couleur
        .filter(|c| analyser_couleur(c).is_some())
        .map(str::to_string)
}

pub fn analyser_couleur(couleur: &str) -> Option<(u8, u8, u8)> {
    let hexa = couleur.strip_prefix('#')?;
    if hexa.len() != 6 || !hexa.bytes().all(|o| o.is_ascii_hexdigit()) {
        return None;
    }
    let canal = |i: usize| u8::from_str_radix(&hexa[i..i + 2], 16).ok();
    Some((canal(0)?, canal(2)?, canal(4)?))
}

pub fn cle_avatar(etat: &Etat, serveur: &str, utilisateur: &str) -> Option<CleAvatar> {
    let propre = etat.version_avatar_du_serveur(utilisateur, Some(serveur));
    if propre > 0 {
        return Some(CleAvatar {
            utilisateur: utilisateur.to_string(),
            version: propre,
            serveur: Some(serveur.to_string()),
        });
    }
    let globale = etat.version_avatar(utilisateur);
    (globale > 0).then(|| CleAvatar {
        utilisateur: utilisateur.to_string(),
        version: globale,
        serveur: None,
    })
}

fn ligne_de_membre(
    etat: &Etat,
    serveur: &str,
    membre: &Membre,
    maintenant_ms: i64,
) -> DonneesMembre {
    let detail = etat.details.get(serveur);
    let utilisateur = texte(&membre.id).to_string();
    let nom = nom_du_membre(membre).to_string();
    let proprietaire = detail
        .and_then(|d| d.server.as_ref())
        .and_then(|s| s.owner_id.as_deref());
    DonneesMembre {
        cle: format!("m:{utilisateur}"),
        en_tete: false,
        titre: String::new(),
        couleur_titre: None,
        initiales: initiales(&nom),
        teinte: teinte(if utilisateur.is_empty() {
            &nom
        } else {
            &utilisateur
        }),
        pseudo: texte(&membre.pseudo).to_string(),
        nom,
        couleur: couleur_valide(couleur_du_pseudo(detail, Some(membre))),
        presence: etat.presence_de(&utilisateur),
        activite: etat.activite_de(&utilisateur).to_string(),
        createur: proprietaire == Some(utilisateur.as_str()),
        bot: utilisateur == ID_BOT,
        exclu: membre_exclu(Some(membre), maintenant_ms),
        avatar: cle_avatar(etat, serveur, &utilisateur),
        utilisateur,
    }
}

fn en_tete(cle: String, titre: String, couleur: Option<String>) -> DonneesMembre {
    DonneesMembre {
        cle,
        en_tete: true,
        titre,
        couleur_titre: couleur,
        utilisateur: String::new(),
        nom: String::new(),
        pseudo: String::new(),
        initiales: String::new(),
        teinte: 1,
        couleur: None,
        presence: Presence::HorsLigne,
        activite: String::new(),
        createur: false,
        bot: false,
        exclu: false,
        avatar: None,
    }
}

fn comparer(etat: &Etat, a: &(&Membre, String), b: &(&Membre, String)) -> Ordering {
    let hors_ligne = |m: &Membre| etat.presence_de(texte(&m.id)) == Presence::HorsLigne;
    hors_ligne(a.0)
        .cmp(&hors_ligne(b.0))
        .then_with(|| a.1.cmp(&b.1))
        .then_with(|| texte(&a.0.pseudo).cmp(texte(&b.0.pseudo)))
        .then_with(|| texte(&a.0.id).cmp(texte(&b.0.id)))
}

pub fn lignes_membres(
    etat: &Etat,
    serveur: &str,
    filtre: &str,
    maintenant_ms: i64,
) -> Vec<DonneesMembre> {
    let Some(detail) = etat.details.get(serveur) else {
        return Vec::new();
    };
    let cherche = replier(filtre.trim());
    let mut tries: Vec<(&Membre, String)> = etat
        .membres(serveur)
        .iter()
        .filter(|m| {
            cherche.is_empty()
                || replier(&format!("{} {}", texte(&m.pseudo), texte(&m.nickname)))
                    .contains(&cherche)
        })
        .map(|m| (m, replier(texte(&m.pseudo))))
        .collect();
    tries.sort_by(|a, b| comparer(etat, a, b));
    let groupes = grouper_membres(detail, tries.iter().map(|(m, _)| *m));
    let mut lignes = Vec::with_capacity(tries.len() + groupes.len() + 2);
    for groupe in groupes {
        match groupe.role {
            Some(role) => {
                let nom = format!("{} {}", texte(&role.icon), texte(&role.name))
                    .trim()
                    .to_string();
                lignes.push(en_tete(
                    format!("g:r:{}", texte(&role.id)),
                    format!("{} · {}", nom.to_uppercase(), groupe.membres.len()),
                    couleur_valide(role.color.as_deref()),
                ));
                lignes.extend(
                    groupe
                        .membres
                        .iter()
                        .map(|m| ligne_de_membre(etat, serveur, m, maintenant_ms)),
                );
            }
            None => {
                let (presents, absents): (Vec<&Membre>, Vec<&Membre>) = groupe
                    .membres
                    .iter()
                    .partition(|m| etat.presence_de(texte(&m.id)) != Presence::HorsLigne);
                for (cle, titre, liste) in [
                    ("g:en-ligne", "EN LIGNE", presents),
                    ("g:hors-ligne", "HORS LIGNE", absents),
                ] {
                    if liste.is_empty() {
                        continue;
                    }
                    lignes.push(en_tete(
                        cle.to_string(),
                        format!("{titre} · {}", liste.len()),
                        None,
                    ));
                    lignes.extend(
                        liste
                            .iter()
                            .map(|m| ligne_de_membre(etat, serveur, m, maintenant_ms)),
                    );
                }
            }
        }
    }
    lignes
}

pub fn donnees_marge(etat: &Etat, serveur: &str, lignes: &[DonneesMembre]) -> DonneesMarge {
    let membres = etat.membres(serveur);
    let en_ligne = membres
        .iter()
        .filter(|m| etat.presence_de(texte(&m.id)) != Presence::HorsLigne)
        .count();
    DonneesMarge {
        titre: format!("MEMBRES · {en_ligne}/{}", membres.len()),
        total: membres.len(),
        en_ligne,
        filtrable: membres.len() > SEUIL_FILTRE,
        vide: lignes.is_empty(),
    }
}

pub fn date_du_jour(iso: &str) -> String {
    let Ok(date) = DateTime::parse_from_rfc3339(iso) else {
        return String::new();
    };
    let locale = date.with_timezone(&Local);
    format!(
        "{} {} {}",
        locale.day(),
        MOIS[locale.month0() as usize],
        locale.year()
    )
}

fn date_et_heure(iso: &str) -> String {
    let Ok(date) = DateTime::parse_from_rfc3339(iso) else {
        return String::new();
    };
    let locale = date.with_timezone(&Local);
    format!("{} à {}", date_du_jour(iso), locale.format("%H:%M"))
}

pub fn donnees_carte(
    etat: &Etat,
    serveur: &str,
    utilisateur: &str,
    maintenant_ms: i64,
) -> Option<DonneesCarte> {
    let detail = etat.details.get(serveur)?;
    let membre = etat.membre(serveur, utilisateur);
    let moi = utilisateur == etat.moi();
    if membre.is_none() && !moi {
        return None;
    }
    let nom = match membre {
        Some(m) => nom_du_membre(m).to_string(),
        None => etat.pseudo().to_string(),
    };
    let pseudo_origine = membre
        .filter(|m| !texte(&m.nickname).is_empty())
        .map(|m| texte(&m.pseudo).to_string())
        .unwrap_or_default();
    let presence = etat.presence_de(utilisateur);
    let activite = etat.activite_de(utilisateur);
    let statut = if activite.is_empty() {
        libelle_presence(presence).to_string()
    } else {
        format!("{} · En jeu sur {activite}", libelle_presence(presence))
    };
    let couleur = couleur_valide(couleur_du_pseudo(Some(detail), membre));
    let banniere =
        couleur_valide(membre.and_then(|m| m.banner_color.as_deref())).or_else(|| couleur.clone());
    let proprietaire = detail.server.as_ref().and_then(|s| s.owner_id.as_deref());
    let exclu_jusqu_au = match membre {
        Some(m) if membre_exclu(Some(m), maintenant_ms) => date_et_heure(texte(&m.timeout_until)),
        _ => String::new(),
    };
    Some(DonneesCarte {
        utilisateur: utilisateur.to_string(),
        initiales: initiales(&nom),
        teinte: teinte(utilisateur),
        nom,
        pseudo_origine,
        couleur,
        banniere,
        presence,
        statut,
        exclu_jusqu_au,
        rang: if proprietaire == Some(utilisateur) {
            "Créateur"
        } else {
            "Membre"
        },
        roles: roles_du_membre(Some(detail), membre)
            .into_iter()
            .map(|r| RoleCarte {
                nom: format!("{} {}", texte(&r.icon), texte(&r.name))
                    .trim()
                    .to_string(),
                couleur: couleur_valide(r.color.as_deref()),
            })
            .collect(),
        serveur: detail
            .server
            .as_ref()
            .map(|s| texte(&s.name).to_string())
            .unwrap_or_default(),
        arrivee: membre
            .map(|m| date_du_jour(texte(&m.joined_at)))
            .unwrap_or_default(),
        creation: membre
            .map(|m| date_du_jour(texte(&m.created_at)))
            .unwrap_or_default(),
        bot: utilisateur == ID_BOT,
        moi,
        avatar: cle_avatar(etat, serveur, utilisateur),
    })
}

pub fn fenetre_des_membres(nombre: usize, defilement: f32, vue: f32) -> Fenetre {
    fenetre_de_hauteurs(
        nombre,
        |_| f64::from(HAUTEUR_LIGNE),
        f64::from(defilement.max(0.0)),
        f64::from(vue.max(0.0)),
        f64::from(MARGE_FENETRE),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coquille::essais::etat;
    use serde_json::json;

    fn depuis<T: serde::de::DeserializeOwned>(valeur: serde_json::Value) -> T {
        serde_json::from_value(valeur).unwrap()
    }

    fn avec_membres() -> Etat {
        let mut e = etat();
        let detail = e.details.get_mut("s1").unwrap();
        detail.roles = Some(depuis(json!([
            {"id": "r0", "name": "everyone", "is_default": true, "position": 0, "hoist": true, "color": "", "icon": ""},
            {"id": "rm", "name": "Modération", "position": 5, "hoist": true, "color": "#2f5d8a", "icon": ""},
            {"id": "ra", "name": "Artistes", "position": 2, "hoist": false, "color": "#c0392b", "icon": "🎨"},
            {"id": "rv", "name": "Vide", "position": 9, "hoist": true, "color": "#000000", "icon": ""}
        ])));
        detail.members = Some(depuis(json!([
            {"id": "me", "pseudo": "bob", "roles": ["rm"]},
            {"id": "carol", "pseudo": "carol", "roles": ["ra"], "nickname": "Caro", "joined_at": "2026-03-04T12:00:00Z", "created_at": "2025-01-02T12:00:00Z", "banner_color": "#123456"},
            {"id": "dave", "pseudo": "dave", "roles": ["rm"]},
            {"id": "élise", "pseudo": "Élise", "roles": []},
            {"id": "fred", "pseudo": "fred", "roles": ["ra"], "timeout_until": "2099-01-01T00:00:00Z"},
            {"id": "gus", "pseudo": "Gus"},
            {"id": ID_BOT, "pseudo": "Estompe"}
        ])));
        e.en_ligne = ["carol", "élise", ID_BOT]
            .into_iter()
            .map(String::from)
            .collect();
        e.statuts.insert("carol".into(), depuis(json!("dnd")));
        e.statuts.insert("élise".into(), depuis(json!("idle")));
        e.activites.insert("carol".into(), "Tetris".into());
        e.activites
            .insert("gus".into(), "Hors ligne donc caché".into());
        e.avatars.insert("carol".into(), 3);
        e.avatars_serveur
            .entry("s1".into())
            .or_default()
            .insert("dave".into(), 2);
        e.avatars.insert("dave".into(), 7);
        e
    }

    fn resume(lignes: &[DonneesMembre]) -> Vec<String> {
        lignes
            .iter()
            .map(|l| {
                if l.en_tete {
                    format!("# {}", l.titre)
                } else {
                    l.nom.clone()
                }
            })
            .collect()
    }

    #[test]
    fn les_membres_sont_groupes_par_role_affiche_puis_par_presence() {
        let e = avec_membres();
        let lignes = lignes_membres(&e, "s1", "", 0);
        assert_eq!(
            resume(&lignes),
            [
                "# MODÉRATION · 2",
                "bob",
                "dave",
                "# EN LIGNE · 3",
                "Caro",
                "Élise",
                "Estompe",
                "# HORS LIGNE · 2",
                "fred",
                "Gus",
            ]
        );
        assert_eq!(lignes[0].couleur_titre.as_deref(), Some("#2f5d8a"));
        assert_eq!(lignes[0].cle, "g:r:rm");
        assert_eq!(lignes[3].cle, "g:en-ligne");
        let carol = lignes.iter().find(|l| l.utilisateur == "carol").unwrap();
        assert_eq!(
            (
                carol.cle.as_str(),
                carol.pseudo.as_str(),
                carol.presence,
                carol.activite.as_str(),
                carol.couleur.as_deref()
            ),
            (
                "m:carol",
                "carol",
                Presence::NePasDeranger,
                "Tetris",
                Some("#c0392b")
            )
        );
        let par_id = |id: &str| lignes.iter().find(|l| l.utilisateur == id).unwrap();
        assert!(par_id("me").createur && par_id("me").presence == Presence::EnLigne);
        assert!(par_id(ID_BOT).bot);
        assert!(par_id("fred").exclu);
        assert_eq!(par_id("gus").activite, "");
        assert_eq!(par_id("élise").presence, Presence::Absent);
        assert_eq!(
            par_id("dave").avatar,
            Some(CleAvatar {
                utilisateur: "dave".into(),
                version: 2,
                serveur: Some("s1".into())
            })
        );
        assert_eq!(par_id("carol").avatar.as_ref().unwrap().serveur, None);
        assert_eq!(par_id("gus").avatar, None);
        assert!(lignes_membres(&e, "inconnu", "", 0).is_empty());
    }

    #[test]
    fn le_filtre_ignore_casse_et_accents_et_les_compteurs_suivent() {
        let e = avec_membres();
        let lignes = lignes_membres(&e, "s1", "  ELI ", 0);
        assert_eq!(resume(&lignes), ["# EN LIGNE · 1", "Élise"]);
        let surnom = lignes_membres(&e, "s1", "caro", 0);
        assert_eq!(resume(&surnom), ["# EN LIGNE · 1", "Caro"]);
        assert!(lignes_membres(&e, "s1", "zz", 0).is_empty());
        let marge = donnees_marge(&e, "s1", &lignes);
        assert_eq!(
            marge,
            DonneesMarge {
                titre: "MEMBRES · 4/7".into(),
                total: 7,
                en_ligne: 4,
                filtrable: false,
                vide: false,
            }
        );
        assert!(donnees_marge(&e, "s1", &[]).vide);
    }

    #[test]
    fn un_changement_de_presence_deplace_le_membre_et_les_compteurs() {
        let mut e = avec_membres();
        let avant = lignes_membres(&e, "s1", "", 0);
        e.en_ligne.insert("gus".into());
        let apres = lignes_membres(&e, "s1", "", 0);
        assert_eq!(
            resume(&apres),
            [
                "# MODÉRATION · 2",
                "bob",
                "dave",
                "# EN LIGNE · 4",
                "Caro",
                "Élise",
                "Estompe",
                "Gus",
                "# HORS LIGNE · 1",
                "fred",
            ]
        );
        assert_eq!(avant.len(), apres.len());
        e.statuts.insert("gus".into(), depuis(json!("idle")));
        let absent = lignes_membres(&e, "s1", "", 0);
        let differences: Vec<usize> = (0..apres.len())
            .filter(|&i| apres[i] != absent[i])
            .collect();
        assert_eq!(differences, [7]);
    }

    #[test]
    fn la_carte_reprend_ce_que_montre_le_profil_du_front() {
        let e = avec_membres();
        let carte = donnees_carte(&e, "s1", "carol", 0).unwrap();
        assert_eq!(carte.nom, "Caro");
        assert_eq!(carte.pseudo_origine, "carol");
        assert_eq!(carte.statut, "Ne pas déranger · En jeu sur Tetris");
        assert_eq!(carte.rang, "Membre");
        assert_eq!(
            carte.roles,
            [RoleCarte {
                nom: "🎨 Artistes".into(),
                couleur: Some("#c0392b".into())
            }]
        );
        assert_eq!(carte.banniere.as_deref(), Some("#123456"));
        assert_eq!(carte.arrivee, "4 mars 2026");
        assert_eq!(carte.creation, "2 janvier 2025");
        assert_eq!(carte.serveur, "Les amis");
        assert!(!carte.moi && !carte.bot && carte.exclu_jusqu_au.is_empty());
        let moi = donnees_carte(&e, "s1", "me", 0).unwrap();
        assert_eq!(
            (moi.rang, moi.moi, moi.pseudo_origine.as_str()),
            ("Créateur", true, "")
        );
        assert_eq!(moi.banniere.as_deref(), Some("#2f5d8a"));
        let fred = donnees_carte(&e, "s1", "fred", 0).unwrap();
        assert!(
            fred.exclu_jusqu_au.starts_with("1 janvier 2099")
                || fred.exclu_jusqu_au.starts_with("31 décembre 2098")
        );
        assert_eq!(fred.statut, "Hors ligne");
        assert!(donnees_carte(&e, "s1", "inconnu", 0).is_none());
        assert!(donnees_carte(&e, "zz", "carol", 0).is_none());
    }

    #[test]
    fn couleurs_et_repli_des_textes() {
        assert_eq!(analyser_couleur("#2F5d8a"), Some((47, 93, 138)));
        for invalide in ["", "2f5d8a", "#2f5d8", "#zzzzzz", "#2f5d8a0"] {
            assert_eq!(analyser_couleur(invalide), None, "{invalide}");
        }
        assert_eq!(replier("Élise À Ça"), "elise a ca");
        assert_eq!(libelle_presence(Presence::NePasDeranger), "Ne pas déranger");
    }

    #[test]
    fn la_fenetre_des_membres_tient_cinq_mille_lignes_en_quelques_dizaines() {
        let w = fenetre_des_membres(5000, 0.0, 700.0);
        assert_eq!((w.debut, w.fin), (0, 21));
        let loin = fenetre_des_membres(5000, 100_000.0, 700.0);
        assert_eq!(loin.fin - loin.debut, 27);
        assert_eq!(loin.debut, (100_000.0 - MARGE_FENETRE) as usize / 44);
    }
}
