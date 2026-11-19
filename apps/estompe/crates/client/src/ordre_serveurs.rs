use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sens {
    Precedent,
    Suivant,
}

pub fn ordonner_serveurs<T>(
    serveurs: Vec<T>,
    ordre: &[String],
    identifiant: impl Fn(&T) -> &str,
) -> Vec<T> {
    let rangs: HashMap<&str, usize> = ordre
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let mut cles: Vec<(usize, T)> = serveurs
        .into_iter()
        .enumerate()
        .map(|(i, serveur)| {
            let cle = rangs
                .get(identifiant(&serveur))
                .copied()
                .unwrap_or(ordre.len() + i);
            (cle, serveur)
        })
        .collect();
    cles.sort_by_key(|(cle, _)| *cle);
    cles.into_iter().map(|(_, serveur)| serveur).collect()
}

pub fn deplacer_serveur(ids: &[String], de: &str, vers: &str) -> Vec<String> {
    let (Some(i), Some(j)) = (
        ids.iter().position(|id| id == de),
        ids.iter().position(|id| id == vers),
    ) else {
        return ids.to_vec();
    };
    if i == j {
        return ids.to_vec();
    }
    let mut suivant = ids.to_vec();
    let element = suivant.remove(i);
    suivant.insert(j, element);
    suivant
}

pub fn voisin<'a>(ids: &'a [String], courant: &str, sens: Sens) -> Option<&'a str> {
    if ids.is_empty() {
        return None;
    }
    let Some(i) = ids.iter().position(|id| id == courant) else {
        return Some(&ids[0]);
    };
    let pas = match sens {
        Sens::Precedent => ids.len() - 1,
        Sens::Suivant => 1,
    };
    Some(&ids[(i + pas) % ids.len()])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(valeurs: &[&str]) -> Vec<String> {
        valeurs.iter().map(|v| v.to_string()).collect()
    }

    fn ordonner(serveurs: &[&str], ordre: &[&str]) -> Vec<String> {
        ordonner_serveurs(ids(serveurs), &ids(ordre), |s| s.as_str())
    }

    #[test]
    fn suit_l_ordre_choisi_et_garde_les_nouveaux_serveurs_a_la_fin() {
        assert_eq!(
            ordonner(&["a", "b", "c", "d"], &["c", "a", "zz"]),
            ids(&["c", "a", "b", "d"])
        );
        assert_eq!(ordonner(&["a", "b"], &[]), ids(&["a", "b"]));
    }

    #[test]
    fn deplace_un_serveur_a_la_place_d_un_autre() {
        assert_eq!(
            deplacer_serveur(&ids(&["a", "b", "c"]), "a", "c"),
            ids(&["b", "c", "a"])
        );
        assert_eq!(
            deplacer_serveur(&ids(&["a", "b", "c"]), "c", "a"),
            ids(&["c", "a", "b"])
        );
        assert_eq!(
            deplacer_serveur(&ids(&["a", "b"]), "a", "x"),
            ids(&["a", "b"])
        );
    }

    #[test]
    fn passe_au_voisin_en_bouclant() {
        assert_eq!(
            voisin(&ids(&["h", "a", "b"]), "b", Sens::Suivant),
            Some("h")
        );
        assert_eq!(
            voisin(&ids(&["h", "a", "b"]), "h", Sens::Precedent),
            Some("b")
        );
        assert_eq!(voisin(&ids(&["h", "a"]), "zz", Sens::Suivant), Some("h"));
        assert_eq!(voisin(&[], "a", Sens::Suivant), None);
    }
}
