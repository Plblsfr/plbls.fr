use std::sync::OnceLock;

use regex::Regex;

pub const TAILLE_MAX: usize = 5 << 20;

fn motif(cellule: &'static OnceLock<Regex>, source: &str) -> &'static Regex {
    cellule.get_or_init(|| Regex::new(source).expect("motif de sous-titres"))
}

fn balise_autorisee(balise: &str) -> bool {
    let nom = balise
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim_start_matches('/')
        .to_ascii_lowercase();
    matches!(nom.as_str(), "b" | "i" | "u")
}

fn retirer_blocs(vtt: &str) -> String {
    let mut sortie = Vec::new();
    for bloc in vtt.replace("\r\n", "\n").split("\n\n") {
        let debut = bloc.trim_start();
        if debut.starts_with("STYLE") || debut.starts_with("NOTE") || debut.starts_with("REGION") {
            continue;
        }
        sortie.push(bloc.to_string());
    }
    sortie.join("\n\n")
}

fn nettoyer_texte(texte: &str) -> String {
    static SCRIPT: OnceLock<Regex> = OnceLock::new();
    static BALISE: OnceLock<Regex> = OnceLock::new();
    static LIEN: OnceLock<Regex> = OnceLock::new();
    let script = motif(&SCRIPT, r"(?is)<script\b.*?(</script\s*>|$)");
    let balise = motif(&BALISE, r"</?[a-zA-Z][^<>]*>");
    let lien = motif(
        &LIEN,
        r"(?i)\b(?:https?|ftp|file|javascript|data):\S*|\bwww\.\S+",
    );
    let mut courant = script.replace_all(texte, "").into_owned();
    loop {
        let suivant = balise
            .replace_all(&courant, |c: &regex::Captures<'_>| {
                if balise_autorisee(&c[0]) {
                    c[0].to_ascii_lowercase()
                } else {
                    String::new()
                }
            })
            .into_owned();
        let stable = suivant == courant;
        courant = suivant;
        if stable {
            break;
        }
    }
    let courant = lien.replace_all(&courant, "").into_owned();
    balise
        .replace_all(&courant, "\u{0}$0\u{1}")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace("\u{0}&lt;", "<")
        .replace("&gt;\u{1}", ">")
        .replace(['\u{0}', '\u{1}'], "")
}

pub fn nettoyer(vtt: &str) -> String {
    let sans_entete = vtt
        .trim_start_matches('\u{feff}')
        .trim_start()
        .strip_prefix("WEBVTT")
        .map(|reste| reste.split_once('\n').map_or("", |(_, r)| r))
        .unwrap_or(vtt);
    let mut sortie = String::from("WEBVTT\n\n");
    let blocs = retirer_blocs(sans_entete);
    let mut temps_vu = false;
    for ligne in blocs.lines() {
        if ligne.contains("-->") {
            temps_vu = true;
            sortie.push_str(ligne);
        } else if temps_vu {
            sortie.push_str(&nettoyer_texte(ligne));
        }
        sortie.push('\n');
    }
    sortie
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPERE: &str = "00:00:01.000 --> 00:00:02.000";

    fn cue(texte: &str) -> String {
        nettoyer(&format!("WEBVTT\n\n{REPERE}\n{texte}\n"))
    }

    #[test]
    fn garde_le_texte_et_les_balises_simples() {
        let s = cue("Bonjour <i>toi</i> <B>ici</B> <u>là</u>");
        assert!(s.starts_with("WEBVTT\n\n"), "{s}");
        assert!(s.contains(REPERE), "{s}");
        assert!(s.contains("Bonjour <i>toi</i> <b>ici</b> <u>là</u>"), "{s}");
    }

    #[test]
    fn retire_script_liens_et_autres_balises() {
        for (entree, interdit) in [
            ("<script>alert(1)</script>ok", "alert"),
            ("<SCRIPT src=x>alert(1)", "alert"),
            ("<img src=x onerror=alert(1)>ok", "onerror"),
            ("<a href='https://x.test'>ok</a>", "href"),
            ("voir https://x.test/a", "x.test"),
            ("voir www.x.test", "x.test"),
            ("javascript:alert(1)", "alert"),
            ("<c.yellow>ok</c>", "yellow"),
        ] {
            let s = cue(entree);
            assert!(!s.contains(interdit), "{entree} : {s}");
        }
    }

    #[test]
    fn balise_imbriquee_neutralisee() {
        let s = cue("<scr<b>ipt>alert(1)</scr</b>ipt>");
        assert!(!s.to_ascii_lowercase().contains("<script"), "{s}");
        assert!(!s.contains("<scr"), "{s}");
    }

    #[test]
    fn chevron_isole_echappe() {
        let s = cue("1 < 2 et 3 > 2");
        assert!(s.contains("1 &lt; 2 et 3 &gt; 2"), "{s}");
    }

    #[test]
    fn retire_style_et_notes() {
        let s = nettoyer(&format!(
            "WEBVTT\n\nSTYLE\n::cue {{ background: url(https://x.test) }}\n\nNOTE secret\n\n{REPERE}\nok\n"
        ));
        assert!(
            !s.contains("STYLE") && !s.contains("x.test") && !s.contains("secret"),
            "{s}"
        );
        assert!(s.contains("ok"), "{s}");
    }

    #[test]
    fn entete_ajoute_si_absent() {
        let s = nettoyer(&format!("{REPERE}\nok\n"));
        assert!(s.starts_with("WEBVTT\n\n"), "{s}");
        assert!(s.contains("ok"), "{s}");
    }
}
