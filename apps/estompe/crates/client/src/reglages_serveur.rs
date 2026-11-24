use crate::appels::EmojiPersonnalise;
use crate::emoji::replier;
use crate::format::{date_heure_complete, date_locale};
use crate::roles::{couleur_du_pseudo, nom_du_membre, roles_du_membre};
use crate::store::id_de;
use crate::store::Etat;
use crate::types::{GetServersIdBansReponseElement, ServerInvite};

pub const MAX_EMOJIS: usize = 50;
pub const OCTETS_EMOJI_MAX: usize = 128 << 10;
pub const COTE_EMOJI_MAX: u32 = 256;
pub const MEMBRES_AFFICHES: usize = 200;
pub const TYPE_ATTENDU: &str = "Image PNG, JPEG ou GIF attendue";
pub const TROP_LOURDE: &str = "Image trop lourde : 128 Ko au plus";
pub const ILLISIBLE: &str = "Image illisible";
pub const TROP_GRANDE: &str = "Image trop grande : 256 px au plus";
pub const NOM_INVALIDE: &str = "2 à 32 caractères : a-z, 0-9 et _";
pub const EXPIRATIONS: [(&str, i64); 5] = [
    ("Sans expiration", 0),
    ("30 minutes", 1800),
    ("1 heure", 3600),
    ("24 heures", 86400),
    ("7 jours", 604800),
];
pub const UTILISATIONS: [(&str, i64); 5] = [
    ("Illimitées", 0),
    ("1 utilisation", 1),
    ("5 utilisations", 5),
    ("10 utilisations", 10),
    ("100 utilisations", 100),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Onglet {
    VueEnsemble,
    Invitations,
    Emojis,
    Membres,
    Bannis,
    Salons,
    Roles,
    Journal,
}

pub const ONGLETS: [Onglet; 8] = [
    Onglet::VueEnsemble,
    Onglet::Invitations,
    Onglet::Emojis,
    Onglet::Membres,
    Onglet::Bannis,
    Onglet::Salons,
    Onglet::Roles,
    Onglet::Journal,
];

impl Onglet {
    pub fn id(self) -> &'static str {
        match self {
            Self::VueEnsemble => "vue",
            Self::Invitations => "invitations",
            Self::Emojis => "emojis",
            Self::Membres => "membres",
            Self::Bannis => "bannis",
            Self::Salons => "salons",
            Self::Roles => "roles",
            Self::Journal => "journal",
        }
    }

    pub fn libelle(self) -> &'static str {
        match self {
            Self::VueEnsemble => "Vue d'ensemble",
            Self::Invitations => "Invitations",
            Self::Emojis => "Emojis",
            Self::Membres => "Membres",
            Self::Bannis => "Bannis",
            Self::Salons => "Salons",
            Self::Roles => "Rôles",
            Self::Journal => "Journal",
        }
    }

    pub fn icone(self) -> &'static str {
        match self {
            Self::VueEnsemble => "info",
            Self::Invitations => "lien",
            Self::Emojis => "sourire",
            Self::Membres => "gens",
            Self::Bannis => "interdit",
            Self::Salons => "hash",
            Self::Roles => "couronne",
            Self::Journal => "file",
        }
    }

    pub fn depuis_id(id: &str) -> Option<Self> {
        ONGLETS.into_iter().find(|o| o.id() == id)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Droits {
    pub proprietaire: bool,
    pub serveur: bool,
    pub icone: bool,
    pub invitations: bool,
    pub salons: bool,
    pub roles: bool,
    pub journal: bool,
    pub bannir: bool,
}

impl Droits {
    pub fn gestionnaire(&self) -> bool {
        self.proprietaire
            || self.serveur
            || self.salons
            || self.invitations
            || self.roles
            || self.journal
            || self.bannir
    }

    pub fn permet(&self, onglet: Onglet) -> bool {
        match onglet {
            Onglet::VueEnsemble | Onglet::Emojis | Onglet::Membres => true,
            Onglet::Invitations => self.invitations,
            Onglet::Bannis => self.bannir,
            Onglet::Salons => self.salons,
            Onglet::Roles => self.roles,
            Onglet::Journal => self.journal,
        }
    }

    pub fn role(&self) -> &'static str {
        if self.proprietaire {
            "Tu es propriétaire"
        } else if self.gestionnaire() {
            "Tu gères ce serveur"
        } else {
            "Tu es membre"
        }
    }
}

pub fn proprietaire_de(etat: &Etat, serveur: &str) -> Option<String> {
    etat.serveur(serveur)
        .and_then(|s| s.owner_id.clone())
        .or_else(|| {
            etat.details
                .get(serveur)
                .and_then(|d| d.server.as_ref())
                .and_then(|s| s.owner_id.clone())
        })
}

pub fn droits(etat: &Etat, serveur: &str) -> Droits {
    if !etat.details.contains_key(serveur) {
        return Droits::default();
    }
    let peut = |p: &str| etat.peut(Some(serveur), p, None);
    let gerer = peut("manage_server");
    Droits {
        proprietaire: proprietaire_de(etat, serveur).as_deref() == Some(etat.moi()),
        serveur: gerer,
        icone: gerer || etat.compte.admin == Some(true),
        invitations: peut("create_invites"),
        salons: peut("manage_channels"),
        roles: peut("manage_roles"),
        journal: peut("view_audit_log"),
        bannir: peut("ban_members"),
    }
}

pub fn onglets_visibles(droits: &Droits) -> Vec<Onglet> {
    ONGLETS.into_iter().filter(|o| droits.permet(*o)).collect()
}

pub fn onglet_retenu(voulu: Option<Onglet>, visibles: &[Onglet]) -> Onglet {
    voulu
        .filter(|o| visibles.contains(o))
        .unwrap_or(Onglet::VueEnsemble)
}

pub fn type_d_image(octets: &[u8]) -> Option<&'static str> {
    if octets.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if octets.starts_with(b"GIF87a") || octets.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if octets.starts_with(b"\xFF\xD8\xFF") {
        Some("image/jpeg")
    } else {
        None
    }
}

fn u16_be(octets: &[u8], i: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes([
        *octets.get(i)?,
        *octets.get(i + 1)?,
    ])))
}

pub fn dimensions_d_image(octets: &[u8], type_mime: &str) -> Option<(u32, u32)> {
    let (l, h) = match type_mime {
        "image/png" => {
            if octets.get(12..16)? != b"IHDR" {
                return None;
            }
            let u32_be = |i: usize| -> Option<u32> {
                Some(u32::from_be_bytes(octets.get(i..i + 4)?.try_into().ok()?))
            };
            (u32_be(16)?, u32_be(20)?)
        }
        "image/gif" => {
            let u16_le = |i: usize| -> Option<u32> {
                Some(u32::from(u16::from_le_bytes([
                    *octets.get(i)?,
                    *octets.get(i + 1)?,
                ])))
            };
            (u16_le(6)?, u16_le(8)?)
        }
        "image/jpeg" => {
            let mut i = 2;
            loop {
                while *octets.get(i)? != 0xFF {
                    i += 1;
                }
                while *octets.get(i)? == 0xFF {
                    i += 1;
                }
                let marqueur = *octets.get(i)?;
                i += 1;
                match marqueur {
                    0x00 | 0x01 | 0xD0..=0xD8 => {}
                    0xC0..=0xC2 => break (u16_be(octets, i + 5)?, u16_be(octets, i + 3)?),
                    0xD9 | 0xDA => return None,
                    _ => i += usize::try_from(u16_be(octets, i)?).ok()?,
                }
            }
        }
        _ => return None,
    };
    (l > 0 && h > 0).then_some((l, h))
}

pub fn verifier_emoji(octets: &[u8]) -> Result<&'static str, &'static str> {
    let type_mime = type_d_image(octets).ok_or(TYPE_ATTENDU)?;
    if octets.len() > OCTETS_EMOJI_MAX {
        return Err(TROP_LOURDE);
    }
    let (l, h) = dimensions_d_image(octets, type_mime).ok_or(ILLISIBLE)?;
    if l > COTE_EMOJI_MAX || h > COTE_EMOJI_MAX {
        return Err(TROP_GRANDE);
    }
    Ok(type_mime)
}

pub fn nom_d_emoji_valide(nom: &str) -> bool {
    (2..=32).contains(&nom.len())
        && nom
            .bytes()
            .all(|o| o.is_ascii_lowercase() || o.is_ascii_digit() || o == b'_')
}

pub fn nom_propose(fichier: &str) -> String {
    let base = fichier.rsplit(['/', '\\']).next().unwrap_or(fichier);
    let sans_extension = match base.rfind('.') {
        Some(p) => &base[..p],
        None => base,
    };
    let nom: String = sans_extension
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(32)
        .collect();
    if nom.len() >= 2 {
        nom
    } else {
        String::new()
    }
}

pub fn emojis_du_serveur<'a>(
    emojis: &'a [EmojiPersonnalise],
    serveur: &str,
) -> Vec<&'a EmojiPersonnalise> {
    emojis.iter().filter(|e| e.server_id == serveur).collect()
}

pub fn peut_modifier_emoji(etat: &Etat, serveur: &str, emoji: &EmojiPersonnalise) -> bool {
    emoji.created_by == etat.moi() || proprietaire_de(etat, serveur).as_deref() == Some(etat.moi())
}

pub fn auteur_d_emoji(etat: &Etat, serveur: &str, emoji: &EmojiPersonnalise) -> String {
    etat.details
        .get(serveur)
        .and_then(|d| d.members.as_deref())
        .unwrap_or_default()
        .iter()
        .find(|m| id_de(&m.id) == emoji.created_by)
        .and_then(|m| m.pseudo.clone())
        .unwrap_or_else(|| "ancien membre".to_string())
}

pub fn lien_d_invitation(adresse: &str, code: &str) -> String {
    crate::route::adresse_web(adresse, &crate::route::lien_invitation(code))
}

pub fn detail_invitation(invitation: &ServerInvite, decalage_minutes: i32) -> String {
    let utilisations = match invitation.max_uses.filter(|m| *m > 0) {
        Some(max) => format!("{} / {max}", invitation.uses.unwrap_or(0)),
        None => invitation.uses.unwrap_or(0).to_string(),
    };
    let expiration = invitation
        .expires_at
        .as_deref()
        .and_then(|d| date_heure_complete(d, decalage_minutes))
        .map_or_else(
            || "sans expiration".to_string(),
            |d| format!("expire le {d}"),
        );
    format!(
        "Par {} · {utilisations} utilisation(s) · {expiration}",
        invitation.created_by.as_deref().unwrap_or_default()
    )
}

fn jour_court(iso: &str, decalage_minutes: i32) -> Option<String> {
    let d = date_locale(iso, decalage_minutes)?;
    Some(format!("{:02}/{:02}/{}", d.jour, d.mois, d.annee))
}

pub fn detail_ban(ban: &GetServersIdBansReponseElement, decalage_minutes: i32) -> String {
    let jour = ban
        .at
        .as_deref()
        .and_then(|d| jour_court(d, decalage_minutes))
        .unwrap_or_default();
    let duree = ban
        .expires_at
        .as_deref()
        .and_then(|d| date_heure_complete(d, decalage_minutes))
        .map_or_else(|| "définitif".to_string(), |d| format!("jusqu'au {d}"));
    format!(
        "par {} · {jour} · {duree}",
        ban.by.as_deref().unwrap_or_default()
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembreReglage {
    pub id: String,
    pub nom: String,
    pub pseudo: String,
    pub role: String,
    pub couleur: Option<String>,
    pub createur: bool,
    pub arrivee: String,
}

pub fn membres_trouves(
    etat: &Etat,
    serveur: &str,
    recherche: &str,
    decalage_minutes: i32,
) -> (Vec<MembreReglage>, usize) {
    let Some(detail) = etat.details.get(serveur) else {
        return (Vec::new(), 0);
    };
    let proprietaire = proprietaire_de(etat, serveur);
    let cherche = replier(recherche.trim());
    let mut trouves: Vec<MembreReglage> = detail
        .members
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|m| {
            cherche.is_empty()
                || replier(nom_du_membre(m)).contains(&cherche)
                || replier(m.pseudo.as_deref().unwrap_or_default()).contains(&cherche)
        })
        .map(|m| {
            let id = id_de(&m.id).to_string();
            let pseudo = m.pseudo.clone().unwrap_or_default();
            let nom = nom_du_membre(m).to_string();
            MembreReglage {
                createur: proprietaire.as_deref() == Some(id.as_str()),
                role: roles_du_membre(Some(detail), Some(m))
                    .first()
                    .and_then(|r| r.name.clone())
                    .unwrap_or_default(),
                couleur: couleur_du_pseudo(Some(detail), Some(m)).map(str::to_string),
                arrivee: m
                    .joined_at
                    .as_deref()
                    .and_then(|d| jour_court(d, decalage_minutes))
                    .map(|d| format!("arrivé le {d}"))
                    .unwrap_or_default(),
                pseudo: if pseudo == nom { String::new() } else { pseudo },
                nom,
                id,
            }
        })
        .collect();
    trouves.sort_by_key(|m| (!m.createur, replier(&m.nom)));
    let total = trouves.len();
    trouves.truncate(MEMBRES_AFFICHES);
    (trouves, total)
}

pub fn compte_membres(affiches: usize, total: usize, recherche: &str) -> String {
    let base = if recherche.trim().is_empty() {
        format!("{total} membre{}", if total > 1 { "s" } else { "" })
    } else {
        format!("{total} trouvé{}", if total > 1 { "s" } else { "" })
    };
    if affiches < total {
        format!("{base}, {affiches} affichés : affine la recherche")
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn png(l: u32, h: u32, taille: usize) -> Vec<u8> {
        let mut o = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        o.extend(l.to_be_bytes());
        o.extend(h.to_be_bytes());
        o.resize(taille.max(o.len()), 0);
        o
    }

    fn gif(l: u16, h: u16) -> Vec<u8> {
        let mut o = b"GIF89a".to_vec();
        o.extend(l.to_le_bytes());
        o.extend(h.to_le_bytes());
        o.extend([0u8; 8]);
        o
    }

    fn jpeg(l: u16, h: u16) -> Vec<u8> {
        let mut o = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00];
        o.extend([0xFF, 0xC0, 0x00, 0x11, 0x08]);
        o.extend(h.to_be_bytes());
        o.extend(l.to_be_bytes());
        o.extend([0u8; 12]);
        o
    }

    #[test]
    fn un_emoji_hors_limites_est_refuse_avant_l_envoi() {
        assert_eq!(verifier_emoji(&png(256, 256, 2000)), Ok("image/png"));
        assert_eq!(verifier_emoji(&gif(32, 32)), Ok("image/gif"));
        assert_eq!(verifier_emoji(&jpeg(128, 64)), Ok("image/jpeg"));
        assert_eq!(verifier_emoji(&png(257, 10, 100)), Err(TROP_GRANDE));
        assert_eq!(verifier_emoji(&jpeg(10, 300)), Err(TROP_GRANDE));
        assert_eq!(verifier_emoji(&gif(400, 1)), Err(TROP_GRANDE));
        assert_eq!(
            verifier_emoji(&png(64, 64, OCTETS_EMOJI_MAX + 1)),
            Err(TROP_LOURDE)
        );
        assert_eq!(
            verifier_emoji(&png(64, 64, OCTETS_EMOJI_MAX)),
            Ok("image/png")
        );
        assert_eq!(verifier_emoji(b"RIFF....WEBPVP8 "), Err(TYPE_ATTENDU));
        assert_eq!(verifier_emoji(&png(0, 4, 100)), Err(ILLISIBLE));
        assert_eq!(verifier_emoji(&jpeg(4, 4)[..10]), Err(ILLISIBLE));
    }

    #[test]
    fn le_nom_propose_suit_le_front() {
        assert_eq!(nom_propose("/tmp/Chat Content.PNG"), "chat_content");
        assert_eq!(nom_propose("C:\\images\\été.gif"), "_t_");
        assert_eq!(nom_propose("a.png"), "");
        assert_eq!(nom_propose(&format!("{}.png", "x".repeat(40))).len(), 32);
        assert!(nom_d_emoji_valide("chat_ok"));
        assert!(!nom_d_emoji_valide("a"));
        assert!(!nom_d_emoji_valide("Chat"));
        assert!(!nom_d_emoji_valide(&"x".repeat(33)));
    }

    fn etat(permissions: serde_json::Value, proprietaire: &str) -> Etat {
        let mut etat = Etat {
            compte: serde_json::from_value(json!({"id": "u1", "pseudo": "moi"})).unwrap(),
            serveurs: serde_json::from_value(
                json!([{"id": "s1", "name": "S", "owner_id": proprietaire}]),
            )
            .unwrap(),
            ..Etat::default()
        };
        etat.details.insert(
            "s1".into(),
            serde_json::from_value(json!({
                "server": {"id": "s1", "name": "S", "owner_id": proprietaire},
                "members": [
                    {"id": "u1", "pseudo": "moi", "roles": ["r1"], "joined_at": "2026-03-04T12:00:00Z"},
                    {"id": "u2", "pseudo": "Élodie", "nickname": "Lolo", "roles": []},
                    {"id": "u3", "pseudo": "bob", "roles": []}
                ],
                "roles": [{"id": "r1", "name": "Modération", "position": 3, "color": "#2f5d8a"}, {"id": "r0", "name": "everyone", "is_default": true, "position": 0}],
                "permissions": permissions
            }))
            .unwrap(),
        );
        etat
    }

    fn ids(onglets: &[Onglet]) -> Vec<&'static str> {
        onglets.iter().map(|o| o.id()).collect()
    }

    #[test]
    fn chaque_onglet_suit_le_role() {
        let membre = droits(&etat(json!(["view_channel"]), "u2"), "s1");
        assert_eq!(
            ids(&onglets_visibles(&membre)),
            ["vue", "emojis", "membres"]
        );
        assert_eq!(membre.role(), "Tu es membre");
        assert!(!membre.icone && !membre.serveur);
        let invitations = droits(&etat(json!(["create_invites"]), "u2"), "s1");
        assert_eq!(
            ids(&onglets_visibles(&invitations)),
            ["vue", "invitations", "emojis", "membres"]
        );
        assert_eq!(invitations.role(), "Tu gères ce serveur");
        let moderation = droits(&etat(json!(["ban_members", "view_audit_log"]), "u2"), "s1");
        assert_eq!(
            ids(&onglets_visibles(&moderation)),
            ["vue", "emojis", "membres", "bannis", "journal"]
        );
        let tout = droits(
            &etat(
                json!([
                    "manage_server",
                    "create_invites",
                    "manage_channels",
                    "manage_roles",
                    "view_audit_log",
                    "ban_members"
                ]),
                "u1",
            ),
            "s1",
        );
        assert_eq!(ids(&onglets_visibles(&tout)).len(), 8);
        assert_eq!(tout.role(), "Tu es propriétaire");
        assert!(tout.icone);
        let mut admin = etat(json!(["view_channel"]), "u2");
        admin.compte.admin = Some(true);
        let d = droits(&admin, "s1");
        assert!(d.icone && !d.serveur);
        assert_eq!(droits(&admin, "inconnu"), Droits::default());
        assert_eq!(
            onglet_retenu(Some(Onglet::Bannis), &onglets_visibles(&membre)),
            Onglet::VueEnsemble
        );
        assert_eq!(
            onglet_retenu(Some(Onglet::Bannis), &onglets_visibles(&moderation)),
            Onglet::Bannis
        );
    }

    #[test]
    fn membres_cherches_sans_accents_createur_en_tete() {
        let e = etat(json!([]), "u3");
        let (tous, total) = membres_trouves(&e, "s1", "", 0);
        assert_eq!(total, 3);
        assert_eq!(
            tous.iter().map(|m| m.nom.as_str()).collect::<Vec<_>>(),
            ["bob", "Lolo", "moi"]
        );
        assert!(tous[0].createur);
        assert_eq!(tous[1].pseudo, "Élodie");
        assert_eq!(tous[2].role, "Modération");
        assert_eq!(tous[2].couleur.as_deref(), Some("#2f5d8a"));
        assert_eq!(tous[2].arrivee, "arrivé le 04/03/2026");
        let (trouves, _) = membres_trouves(&e, "s1", "elo", 0);
        assert_eq!(trouves.len(), 1);
        assert_eq!(compte_membres(3, 3, ""), "3 membres");
        assert_eq!(compte_membres(1, 1, "elo"), "1 trouvé");
        assert_eq!(
            compte_membres(200, 250, ""),
            "250 membres, 200 affichés : affine la recherche"
        );
    }

    #[test]
    fn details_des_invitations_et_des_bans() {
        let invitation: ServerInvite = serde_json::from_value(json!({"code": "abc", "created_by": "moi", "uses": 2, "max_uses": 10, "expires_at": "2026-11-22T09:30:00Z"})).unwrap();
        assert_eq!(
            detail_invitation(&invitation, 60),
            "Par moi · 2 / 10 utilisation(s) · expire le 22/11/2026 10:30:00"
        );
        let libre: ServerInvite =
            serde_json::from_value(json!({"code": "abc", "created_by": "moi"})).unwrap();
        assert_eq!(
            detail_invitation(&libre, 0),
            "Par moi · 0 utilisation(s) · sans expiration"
        );
        assert_eq!(
            lien_d_invitation("https://chat.plbls.fr/", "ab c"),
            "https://chat.plbls.fr/#/join/ab%20c"
        );
        let ban: GetServersIdBansReponseElement = serde_json::from_value(
            json!({"user_id": "u9", "pseudo": "zed", "by": "moi", "at": "2026-11-20T08:00:00Z"}),
        )
        .unwrap();
        assert_eq!(detail_ban(&ban, 0), "par moi · 20/11/2026 · définitif");
        let temporaire: GetServersIdBansReponseElement = serde_json::from_value(json!({"by": "moi", "at": "2026-11-20T08:00:00Z", "expires_at": "2026-11-27T08:00:00Z"})).unwrap();
        assert_eq!(
            detail_ban(&temporaire, 0),
            "par moi · 20/11/2026 · jusqu'au 27/11/2026 08:00:00"
        );
    }

    #[test]
    fn un_emoji_se_modifie_par_son_auteur_ou_le_createur() {
        let emoji = |auteur: &str| EmojiPersonnalise {
            id: "e1".into(),
            server_id: "s1".into(),
            name: "chat".into(),
            created_by: auteur.into(),
        };
        let membre = etat(json!([]), "u2");
        assert!(peut_modifier_emoji(&membre, "s1", &emoji("u1")));
        assert!(!peut_modifier_emoji(&membre, "s1", &emoji("u3")));
        assert!(peut_modifier_emoji(
            &etat(json!([]), "u1"),
            "s1",
            &emoji("u3")
        ));
        assert_eq!(auteur_d_emoji(&membre, "s1", &emoji("u3")), "bob");
        assert_eq!(auteur_d_emoji(&membre, "s1", &emoji("zz")), "ancien membre");
        let liste = [
            emoji("u1"),
            EmojiPersonnalise {
                server_id: "s2".into(),
                ..emoji("u1")
            },
        ];
        assert_eq!(emojis_du_serveur(&liste, "s1").len(), 1);
    }
}
