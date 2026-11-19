#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Accueil,
    Salon {
        serveur: String,
        salon: Option<String>,
        message: Option<String>,
    },
    MessagePrive {
        id: String,
        message: Option<String>,
    },
    Invitation {
        code: String,
    },
    Admin {
        section: Option<String>,
    },
    Films {
        section: Option<String>,
        id: Option<String>,
    },
    Musique {
        section: Option<String>,
        id: Option<String>,
    },
    Regie {
        section: Option<String>,
    },
    Stats {
        section: Option<String>,
        id: Option<String>,
        detail: Option<String>,
    },
    Espace {
        espace: String,
        section: Option<String>,
    },
}

impl Route {
    pub fn nom(&self) -> &'static str {
        match self {
            Self::Accueil => "home",
            Self::Salon { .. } => "channel",
            Self::MessagePrive { .. } => "dm",
            Self::Invitation { .. } => "join",
            Self::Admin { .. } => "admin",
            Self::Films { .. } => "films",
            Self::Musique { .. } => "musique",
            Self::Regie { .. } => "regie",
            Self::Stats { .. } => "stats",
            Self::Espace { .. } => "space",
        }
    }
}

fn valeur_hexadecimale(octet: u8) -> Option<u8> {
    match octet {
        b'0'..=b'9' => Some(octet - b'0'),
        b'a'..=b'f' => Some(octet - b'a' + 10),
        b'A'..=b'F' => Some(octet - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn decoder_composant(texte: &str) -> String {
    let octets = texte.as_bytes();
    let mut sortie = Vec::with_capacity(octets.len());
    let mut i = 0;
    while i < octets.len() {
        if octets[i] == b'%' && i + 2 < octets.len() {
            let haut = valeur_hexadecimale(octets[i + 1]);
            let bas = valeur_hexadecimale(octets[i + 2]);
            if let (Some(haut), Some(bas)) = (haut, bas) {
                sortie.push(haut * 16 + bas);
                i += 3;
                continue;
            }
        }
        sortie.push(octets[i]);
        i += 1;
    }
    String::from_utf8_lossy(&sortie).into_owned()
}

pub(crate) fn encoder_composant(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    for octet in texte.bytes() {
        if octet.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&octet) {
            sortie.push(octet as char);
        } else {
            sortie.push_str(&format!("%{octet:02X}"));
        }
    }
    sortie
}

fn sans_diese(hash: &str) -> &str {
    let reste = hash.strip_prefix('#').unwrap_or(hash);
    reste.strip_prefix('/').unwrap_or(reste)
}

pub fn analyser_hash(hash: &str, espaces: &[&str]) -> Route {
    let parties: Vec<String> = sans_diese(hash)
        .split('/')
        .filter(|p| !p.is_empty())
        .map(decoder_composant)
        .collect();
    let partie = |i: usize| parties.get(i).cloned();
    let premiere = parties.first().map(String::as_str).unwrap_or_default();
    match premiere {
        "s" if parties.len() > 1 => Route::Salon {
            serveur: parties[1].clone(),
            salon: partie(2),
            message: partie(3),
        },
        "dm" if parties.len() > 1 => Route::MessagePrive {
            id: parties[1].clone(),
            message: partie(2),
        },
        "join" if parties.len() > 1 => Route::Invitation {
            code: parties[1].clone(),
        },
        "admin" => Route::Admin { section: partie(1) },
        "films" => Route::Films {
            section: partie(1),
            id: partie(2),
        },
        "musique" => Route::Musique {
            section: partie(1),
            id: partie(2),
        },
        "regie" => Route::Regie { section: partie(1) },
        "stats" => Route::Stats {
            section: partie(1),
            id: partie(2),
            detail: partie(3),
        },
        espace if !espace.is_empty() && espaces.contains(&espace) => Route::Espace {
            espace: espace.to_string(),
            section: partie(1),
        },
        _ => Route::Accueil,
    }
}

fn suffixe(valeur: Option<&str>) -> String {
    match valeur {
        Some(v) if !v.is_empty() => format!("/{v}"),
        _ => String::new(),
    }
}

fn suffixe_encode(valeur: Option<&str>) -> String {
    match valeur {
        Some(v) if !v.is_empty() => format!("/{}", encoder_composant(v)),
        _ => String::new(),
    }
}

pub fn lien_accueil() -> String {
    "#/".to_string()
}

pub fn lien_salon(serveur: &str, salon: Option<&str>) -> String {
    format!("#/s/{serveur}{}", suffixe(salon))
}

pub fn lien_message_prive(id: &str) -> String {
    format!("#/dm/{id}")
}

pub fn lien_admin(section: Option<&str>) -> String {
    format!("#/admin{}", suffixe(section))
}

pub fn lien_films(section: Option<&str>, id: Option<&str>) -> String {
    format!("#/films{}{}", suffixe(section), suffixe_encode(id))
}

pub fn lien_musique(section: Option<&str>, id: Option<&str>) -> String {
    format!("#/musique{}{}", suffixe(section), suffixe_encode(id))
}

pub fn lien_regie(section: Option<&str>) -> String {
    format!("#/regie{}", suffixe(section))
}

pub fn lien_stats(section: Option<&str>, id: Option<&str>, detail: Option<&str>) -> String {
    format!(
        "#/stats{}{}{}",
        suffixe(section),
        suffixe_encode(id),
        suffixe_encode(detail)
    )
}

pub fn lien_espace(espace: &str, section: Option<&str>) -> String {
    format!("#/{espace}{}", suffixe(section))
}

pub fn lien_invitation(code: &str) -> String {
    format!("#/join/{}", encoder_composant(code))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temps_reel::battement::{section_de, Section};

    fn s(texte: &str) -> Option<String> {
        Some(texte.to_string())
    }

    fn stats(section: Option<&str>, id: Option<&str>, detail: Option<&str>) -> Route {
        Route::Stats {
            section: section.map(String::from),
            id: id.map(String::from),
            detail: detail.map(String::from),
        }
    }

    #[test]
    fn lit_stats_sa_sous_section_et_son_identifiant() {
        assert_eq!(analyser_hash("#/stats", &[]), stats(None, None, None));
        assert_eq!(
            analyser_hash("#/stats/recaps", &[]),
            stats(Some("recaps"), None, None)
        );
        assert_eq!(
            analyser_hash("#/stats/recaps/12", &[]),
            stats(Some("recaps"), Some("12"), None)
        );
        assert_eq!(
            analyser_hash("#/stats/admin/personnes/kc%2Fa", &[]),
            stats(Some("admin"), Some("personnes"), Some("kc/a"))
        );
    }

    #[test]
    fn ecrit_les_liens_de_la_section() {
        assert_eq!(lien_stats(None, None, None), "#/stats");
        assert_eq!(lien_stats(Some("partage"), None, None), "#/stats/partage");
        assert_eq!(
            lien_stats(Some("recaps"), Some("12"), None),
            "#/stats/recaps/12"
        );
        assert_eq!(
            lien_stats(Some("admin"), Some("personnes"), Some("kc/a")),
            "#/stats/admin/personnes/kc%2Fa"
        );
    }

    #[test]
    fn le_battement_range_la_route_dans_la_section_sablier() {
        assert_eq!(
            section_de(analyser_hash("#/stats/recaps", &[]).nom()),
            Section::Sablier
        );
        for (adresse, section) in [
            ("#/", Section::Messagerie),
            ("#/s/a/b", Section::Messagerie),
            ("#/dm/x", Section::Messagerie),
            ("#/films", Section::Entracte),
            ("#/musique/albums", Section::Sillon),
            ("#/regie", Section::Regie),
            ("#/admin", Section::Admin),
            ("#/join/abc", Section::Autre),
        ] {
            assert_eq!(
                section_de(analyser_hash(adresse, &[]).nom()),
                section,
                "{adresse}"
            );
        }
    }

    #[test]
    fn lit_les_autres_routes() {
        assert_eq!(analyser_hash("", &[]), Route::Accueil);
        assert_eq!(analyser_hash("#/", &[]), Route::Accueil);
        assert_eq!(
            analyser_hash("#/s/a/b/c", &[]),
            Route::Salon {
                serveur: "a".into(),
                salon: s("b"),
                message: s("c")
            }
        );
        assert_eq!(analyser_hash("#/s", &[]), Route::Accueil);
        assert_eq!(
            analyser_hash("#/dm/x/9", &[]),
            Route::MessagePrive {
                id: "x".into(),
                message: s("9")
            }
        );
        assert_eq!(
            analyser_hash("#/join/ab%20c", &[]),
            Route::Invitation {
                code: "ab c".into()
            }
        );
        assert_eq!(
            analyser_hash("#/films/vus/f1", &[]),
            Route::Films {
                section: s("vus"),
                id: s("f1")
            }
        );
        assert_eq!(
            analyser_hash("#/forge/outils", &["forge"]),
            Route::Espace {
                espace: "forge".into(),
                section: s("outils")
            }
        );
        assert_eq!(analyser_hash("#/forge/outils", &[]), Route::Accueil);
    }

    #[test]
    fn ecrit_les_autres_liens() {
        assert_eq!(lien_accueil(), "#/");
        assert_eq!(lien_salon("a", Some("b")), "#/s/a/b");
        assert_eq!(lien_salon("a", None), "#/s/a");
        assert_eq!(lien_message_prive("x"), "#/dm/x");
        assert_eq!(lien_admin(Some("roles")), "#/admin/roles");
        assert_eq!(lien_films(Some("vus"), Some("a b")), "#/films/vus/a%20b");
        assert_eq!(lien_musique(None, None), "#/musique");
        assert_eq!(lien_regie(Some("x")), "#/regie/x");
        assert_eq!(lien_espace("forge", Some("o")), "#/forge/o");
        assert_eq!(lien_invitation("a/b"), "#/join/a%2Fb");
    }
}
