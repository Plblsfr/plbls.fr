use std::time::{Duration, Instant};

use super::*;
use crate::mention::{GenreMention, Mentionnables};

fn rendre(texte: &str, pseudos: &[&str]) -> Rendu {
    analyser(texte, &Mentionnables::personnes(pseudos.iter().copied()))
}

fn r(texte: &str) -> Vec<Bloc> {
    rendre(texte, &[]).blocs
}

fn tous_les_segments(blocs: &[Bloc]) -> Vec<Segment> {
    let mut sortie = Vec::new();
    for bloc in blocs {
        match bloc {
            Bloc::Paragraphe(s) | Bloc::Titre { segments: s, .. } => {
                sortie.extend(s.iter().cloned())
            }
            Bloc::Citation(b) => sortie.extend(tous_les_segments(b)),
            Bloc::Liste { elements, .. } => {
                for e in elements {
                    sortie.extend(tous_les_segments(e));
                }
            }
            Bloc::Code { texte, .. } => sortie.push(Segment {
                styles: Styles {
                    code: true,
                    ..Styles::default()
                },
                ..Segment::texte(texte)
            }),
        }
    }
    sortie
}

fn texte(t: &str, styles: Styles) -> Segment {
    Segment {
        styles,
        ..Segment::texte(t)
    }
}

fn gras() -> Styles {
    Styles {
        gras: true,
        ..Styles::default()
    }
}

fn italique() -> Styles {
    Styles {
        italique: true,
        ..Styles::default()
    }
}

fn barre() -> Styles {
    Styles {
        barre: true,
        ..Styles::default()
    }
}

fn code() -> Styles {
    Styles {
        code: true,
        ..Styles::default()
    }
}

fn saut() -> Segment {
    Segment {
        contenu: Contenu::SautDeLigne,
        ..Segment::texte("")
    }
}

fn mention(t: &str, genre: GenreMention) -> Segment {
    Segment {
        contenu: Contenu::Mention {
            texte: t.to_string(),
            genre,
        },
        ..Segment::texte("")
    }
}

fn lien(t: &str, url: &str) -> Segment {
    Segment {
        lien: Some(url.to_string()),
        ..Segment::texte(t)
    }
}

fn para(segments: Vec<Segment>) -> Vec<Bloc> {
    vec![Bloc::Paragraphe(segments)]
}

fn p1(t: &str) -> Bloc {
    Bloc::Paragraphe(vec![Segment::texte(t)])
}

#[test]
fn gere_gras_italique_barre_code_citation_et_listes() {
    assert_eq!(r("**gras**"), para(vec![texte("gras", gras())]));
    assert_eq!(r("*it*"), para(vec![texte("it", italique())]));
    assert_eq!(r("~~barré~~"), para(vec![texte("barré", barre())]));
    assert_eq!(r("`code`"), para(vec![texte("code", code())]));
    assert_eq!(
        r("```\nbloc\n```"),
        vec![Bloc::Code {
            langage: None,
            texte: "bloc".into()
        }]
    );
    assert_eq!(r("> citation"), vec![Bloc::Citation(vec![p1("citation")])]);
    assert_eq!(
        r("- a\n- b"),
        vec![Bloc::Liste {
            ordonnee: false,
            elements: vec![vec![p1("a")], vec![p1("b")]]
        }]
    );
    assert_eq!(
        r("1. a\n2. b"),
        vec![Bloc::Liste {
            ordonnee: true,
            elements: vec![vec![p1("a")], vec![p1("b")]]
        }]
    );
}

#[test]
fn conserve_les_retours_a_la_ligne() {
    assert_eq!(
        r("a\nb"),
        para(vec![Segment::texte("a"), saut(), Segment::texte("b")])
    );
}

#[test]
fn ne_met_pas_en_forme_a_l_interieur_du_code() {
    let blocs = r("`**x** ||y||`");
    assert_eq!(blocs, para(vec![texte("**x** ||y||", code())]));
    assert!(tous_les_segments(&blocs).iter().all(|s| s.masque.is_none()));
}

#[test]
fn affiche_les_spoilers() {
    assert_eq!(
        r("||secret||"),
        para(vec![Segment {
            masque: Some(1),
            ..Segment::texte("secret")
        }])
    );
    assert_eq!(
        r("||un **gras**||"),
        para(vec![
            Segment {
                masque: Some(1),
                ..Segment::texte("un ")
            },
            Segment {
                masque: Some(1),
                styles: gras(),
                ..Segment::texte("gras")
            },
        ])
    );
    assert!(tous_les_segments(&r("a || b"))
        .iter()
        .all(|s| s.masque.is_none()));
}

#[test]
fn met_en_evidence_les_mentions_de_role_accents_et_espaces_compris() {
    let c = |roles: &[&str]| {
        Mentionnables::nouveau(
            Vec::<String>::new(),
            roles.iter().copied(),
            Vec::<String>::new(),
            Vec::<String>::new(),
        )
    };
    assert_eq!(
        analyser("hé @Modérateur !", &c(&["Modérateur"])).blocs,
        para(vec![
            Segment::texte("hé "),
            mention("Modérateur", GenreMention::Role),
            Segment::texte(" !")
        ])
    );
    assert_eq!(
        analyser("@Chef de projet merci", &c(&["Chef de projet"])).blocs,
        para(vec![
            mention("Chef de projet", GenreMention::Role),
            Segment::texte(" merci")
        ])
    );
    assert_eq!(
        analyser("@Modérateurs", &c(&["Modérateur"])).blocs,
        para(vec![Segment::texte("@Modérateurs")])
    );
}

#[test]
fn met_en_evidence_les_mentions_des_pseudos_connus_seulement() {
    assert_eq!(
        rendre("salut @Alice !", &["alice"]).blocs,
        para(vec![
            Segment::texte("salut "),
            mention("Alice", GenreMention::Personne),
            Segment::texte(" !")
        ])
    );
    assert_eq!(
        rendre("salut @inconnu", &["alice"]).blocs,
        para(vec![Segment::texte("salut @inconnu")])
    );
    assert_eq!(
        rendre("mail@alice.fr", &["alice"]).blocs,
        para(vec![lien("mail@alice.fr", "mailto:mail@alice.fr")])
    );
    assert_eq!(
        rendre("salut @alice.", &["alice"]).blocs,
        para(vec![
            Segment::texte("salut "),
            mention("alice", GenreMention::Personne),
            Segment::texte(".")
        ])
    );
}

#[test]
fn mentions_de_groupe_et_emojis_connus() {
    let c = Mentionnables::nouveau(
        ["alice"],
        Vec::<String>::new(),
        ["everyone", "here"],
        ["smile"],
    );
    assert_eq!(
        analyser("@everyone :smile: :inconnu: :SMILE:", &c).blocs,
        para(vec![
            mention("everyone", GenreMention::Groupe),
            Segment::texte(" "),
            Segment {
                contenu: Contenu::Emoji("smile".into()),
                ..Segment::texte("")
            },
            Segment::texte(" :inconnu: "),
            Segment {
                contenu: Contenu::Emoji("smile".into()),
                ..Segment::texte("")
            },
        ])
    );
    assert_eq!(
        analyser("@everyone", &Mentionnables::default()).blocs,
        para(vec![Segment::texte("@everyone")])
    );
    assert_eq!(
        analyser("[@alice](https://x)", &c).blocs,
        para(vec![Segment {
            lien: Some("https://x".into()),
            ..mention("alice", GenreMention::Personne)
        }])
    );
}

#[test]
fn cree_des_liens_surs() {
    assert_eq!(
        r("voir https://example.com/x?a=1"),
        para(vec![
            Segment::texte("voir "),
            lien("https://example.com/x?a=1", "https://example.com/x?a=1")
        ])
    );
}

#[test]
fn remplace_les_titres_et_les_images() {
    assert_eq!(
        r("# Titre"),
        vec![Bloc::Titre {
            niveau: 1,
            segments: vec![texte("Titre", gras())]
        }]
    );
    assert_eq!(
        r("![x](https://evil.example/p.png)"),
        para(vec![lien("x", "https://evil.example/p.png")])
    );
    assert_eq!(
        r("![](https://e.example/p.png)"),
        para(vec![lien(
            "https://e.example/p.png",
            "https://e.example/p.png"
        )])
    );
}

const DANGEREUX: &[&str] = &[
    "<script>alert(1)</script>",
    "<img src=x onerror=alert(1)>",
    "<svg onload=alert(1)>",
    "<iframe src=\"javascript:alert(1)\"></iframe>",
    "<a href=\"javascript:alert(1)\">x</a>",
    "[x](javascript:alert(1))",
    "[x](data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==)",
    "[x](vbscript:alert(1))",
    "<div style=\"x\" onclick=\"alert(1)\">x</div>",
    "<span class=\"spoiler\" onmouseover=\"alert(1)\">x</span>",
    "<input type=\"text\" autofocus onfocus=\"alert(1)\">",
    "<style>*{display:none}</style>",
    "<math><mi//xlink:href=\"data:x,<script>alert(1)</script>\">",
    "**<img src=x onerror=alert(1)>**",
    "||<img src=x onerror=alert(1)>||",
    "> <script>alert(1)</script>",
    "`</code><script>alert(1)</script>`",
    "[a](https://x.example \"t\" onmouseover=\"alert(1)\")",
];

fn verifier_inerte(entree: &str) {
    let rendu = r(entree);
    for s in tous_les_segments(&rendu) {
        if let Some(url) = &s.lien {
            let bas = url.to_ascii_lowercase();
            assert!(
                bas.starts_with("http:") || bas.starts_with("https:") || bas.starts_with("mailto:"),
                "{entree} : lien {url}"
            );
            assert_eq!(
                lien_ouvrable(url).as_deref(),
                Some(url.as_str()),
                "{entree}"
            );
        }
        assert!(
            !matches!(s.contenu, Contenu::Mention { .. } | Contenu::Emoji(_)),
            "{entree}"
        );
    }
    let brut = texte_brut(&rendu);
    for balise in [
        "<script", "<img", "<svg", "<iframe", "<style", "<math", "<input", "<div", "<span", "<a ",
    ] {
        if entree.contains(balise) {
            assert!(
                brut.contains(balise),
                "{entree} : {balise} doit rester du texte, obtenu {brut:?}"
            );
        }
    }
}

#[test]
fn neutralise_les_contenus_dangereux() {
    for entree in DANGEREUX {
        verifier_inerte(entree);
    }
}

#[test]
fn neutralise_les_contenus_dangereux_dans_le_detail() {
    assert_eq!(
        r("<script>alert(1)</script>"),
        para(vec![Segment::texte("<script>alert(1)</script>")])
    );
    assert_eq!(
        r("<img src=x onerror=alert(1)>"),
        para(vec![Segment::texte("<img src=x onerror=alert(1)>")])
    );
    assert_eq!(
        r("[x](javascript:alert(1))"),
        para(vec![Segment::texte("x")])
    );
    assert_eq!(r("[x](vbscript:alert(1))"), para(vec![Segment::texte("x")]));
    assert_eq!(
        r("[x](data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==)"),
        para(vec![Segment::texte("x")])
    );
    assert_eq!(
        r("<a href=\"javascript:alert(1)\">x</a>"),
        para(vec![Segment::texte(
            "<a href=\"javascript:alert(1)\">x</a>"
        )])
    );
    assert_eq!(
        r("**<img src=x onerror=alert(1)>**"),
        para(vec![texte("<img src=x onerror=alert(1)>", gras())])
    );
    assert_eq!(
        r("||<img src=x onerror=alert(1)>||"),
        para(vec![Segment {
            masque: Some(1),
            ..Segment::texte("<img src=x onerror=alert(1)>")
        }])
    );
    assert_eq!(
        r("> <script>alert(1)</script>"),
        vec![Bloc::Citation(vec![p1("<script>alert(1)</script>")])]
    );
    assert_eq!(
        r("`</code><script>alert(1)</script>`"),
        para(vec![texte("</code><script>alert(1)</script>", code())])
    );
    assert_eq!(
        r("[a](https://x.example \"t\" onmouseover=\"alert(1)\")"),
        para(vec![
            Segment::texte("[a]("),
            lien("https://x.example", "https://x.example"),
            Segment::texte(" \"t\" onmouseover=\"alert(1)\")"),
        ])
    );
    assert_eq!(
        r("<style>*{display:none}</style>"),
        para(vec![Segment::texte("<style>*{display:none}</style>")])
    );
}

#[test]
fn affiche_le_html_brut_comme_du_texte() {
    let blocs = r("<b>gras ?</b>");
    assert_eq!(blocs, para(vec![Segment::texte("<b>gras ?</b>")]));
    assert!(texte_brut(&blocs).contains("<b>gras ?</b>"));
}

#[test]
fn n_autorise_aucune_classe_venant_de_l_utilisateur() {
    let blocs = r("<span class=\"plbls-modal__fond\">x</span>");
    assert_eq!(
        blocs,
        para(vec![Segment::texte(
            "<span class=\"plbls-modal__fond\">x</span>"
        )])
    );
    assert!(tous_les_segments(&blocs)
        .iter()
        .all(|s| s.masque.is_none() && s.styles == Styles::default()));
}

#[test]
fn trouve_le_premier_lien_hors_code() {
    assert_eq!(
        premier_lien("regarde https://a.example/x, puis https://b.example").as_deref(),
        Some("https://a.example/x")
    );
    assert_eq!(
        premier_lien("`https://code.example` https://vrai.example").as_deref(),
        Some("https://vrai.example")
    );
    assert_eq!(premier_lien("```\nhttps://bloc.example\n```"), None);
    assert_eq!(premier_lien("rien"), None);
    assert_eq!(premier_lien("ftp://x.example"), None);
}

#[test]
fn ecarts_de_marked_reproduits() {
    assert_eq!(r("---"), Vec::<Bloc>::new());
    assert_eq!(r("a\n\n* * *\n\nb"), vec![p1("a"), p1("b")]);
    assert_eq!(
        r("Titre\n==="),
        vec![Bloc::Titre {
            niveau: 1,
            segments: vec![texte("Titre", gras())]
        }]
    );
    assert_eq!(
        r("a  \nb"),
        para(vec![Segment::texte("a"), saut(), Segment::texte("b")])
    );
    assert_eq!(
        r("a\\\nb"),
        para(vec![Segment::texte("a"), saut(), Segment::texte("b")])
    );
    assert_eq!(
        r("a\n2. b"),
        para(vec![Segment::texte("a"), saut(), Segment::texte("2. b")])
    );
    assert_eq!(
        r("```rust\nfn x\n```"),
        vec![Bloc::Code {
            langage: Some("rust".into()),
            texte: "fn x".into()
        }]
    );
    assert_eq!(
        r("```\nnon fermé"),
        vec![Bloc::Code {
            langage: None,
            texte: "non fermé".into()
        }]
    );
    assert_eq!(
        r("    code indenté"),
        vec![Bloc::Code {
            langage: None,
            texte: "code indenté".into()
        }]
    );
    assert_eq!(
        r("&amp; &lt; &copy; &#65; &foo;"),
        para(vec![Segment::texte("& < © A &foo;")])
    );
    assert_eq!(r("\\*pas\\*"), para(vec![Segment::texte("*pas*")]));
    assert_eq!(
        r("snake_case_var et _it_"),
        para(vec![
            Segment::texte("snake_case_var et "),
            texte("it", italique())
        ])
    );
    assert_eq!(r("_a_b_"), para(vec![texte("a_b", italique())]));
    assert_eq!(
        r("***x***"),
        para(vec![texte(
            "x",
            Styles {
                gras: true,
                italique: true,
                ..Styles::default()
            }
        )])
    );
    assert_eq!(r("**a*b**"), para(vec![texte("a*b", gras())]));
    assert_eq!(
        r("~b~ ~~~c~~~"),
        para(vec![texte("b", barre()), Segment::texte(" ~~~c~~~")])
    );
    assert_eq!(
        r("||a **b|| c**"),
        para(vec![
            Segment {
                masque: Some(1),
                ..Segment::texte("a **b")
            },
            Segment::texte(" c**")
        ])
    );
    assert_eq!(r("||secret|| et || x||").len(), 1);
    assert_eq!(r("``a`b``"), para(vec![texte("a`b", code())]));
    assert_eq!(r("x ` y"), para(vec![Segment::texte("x ` y")]));
    assert_eq!(r("**a\n\nb**"), vec![p1("**a"), p1("b**")]);
}

#[test]
fn liens_et_autoliens_comme_marked() {
    assert_eq!(
        r("mail@alice.fr www.ex.com https://a.b/c). <https://x.y>"),
        para(vec![
            lien("mail@alice.fr", "mailto:mail@alice.fr"),
            Segment::texte(" "),
            lien("www.ex.com", "http://www.ex.com"),
            Segment::texte(" "),
            lien("https://a.b/c", "https://a.b/c"),
            Segment::texte("). "),
            lien("https://x.y", "https://x.y"),
        ])
    );
    assert_eq!(
        r("a@b.co."),
        para(vec![lien("a@b.co", "mailto:a@b.co"), Segment::texte(".")])
    );
    assert_eq!(
        r("[t](https://x \"titre\")"),
        para(vec![lien("t", "https://x")])
    );
    assert_eq!(
        r("[a](<https://x/a b>)"),
        para(vec![lien("a", "https://x/a%20b")])
    );
    assert_eq!(
        r("[a](https://x/(y))"),
        para(vec![lien("a", "https://x/(y)")])
    );
    assert_eq!(r("[t](ftp://x)"), para(vec![Segment::texte("t")]));
    assert_eq!(r("[t](/relatif)"), para(vec![Segment::texte("t")]));
    assert_eq!(
        r("[t](mailto:a@b.c)"),
        para(vec![lien("t", "mailto:a@b.c")])
    );
    assert_eq!(
        r("www. x http://"),
        para(vec![Segment::texte("www. x http://")])
    );
    assert_eq!(
        r("https://x.y/a_b_c **https://g.h**"),
        para(vec![
            lien("https://x.y/a_b_c", "https://x.y/a_b_c"),
            Segment::texte(" "),
            Segment {
                styles: gras(),
                ..lien("https://g.h", "https://g.h")
            },
        ])
    );
    assert_eq!(
        r("https://exemple.fr/é"),
        para(vec![lien(
            "https://exemple.fr/é",
            "https://exemple.fr/%C3%A9"
        )])
    );
}

#[test]
fn autoliens_seulement_en_debut_de_jeton_comme_marked() {
    assert_eq!(
        r("écris à mail@alice.fr"),
        para(vec![Segment::texte("écris à mail@alice.fr")])
    );
    assert_eq!(
        r("(mail@alice.fr)"),
        para(vec![Segment::texte("(mail@alice.fr)")])
    );
    assert_eq!(r("  m@d.fr"), para(vec![Segment::texte("m@d.fr")]));
    assert_eq!(
        r("a\nmail@alice.fr"),
        para(vec![
            Segment::texte("a"),
            saut(),
            lien("mail@alice.fr", "mailto:mail@alice.fr")
        ])
    );
    assert_eq!(
        r("**mail@alice.fr**"),
        para(vec![Segment {
            styles: gras(),
            ..lien("mail@alice.fr", "mailto:mail@alice.fr")
        }])
    );
    assert_eq!(r("*m@d.fr"), para(vec![Segment::texte("*m@d.fr")]));
    assert_eq!(
        r("m@d.fr.a__"),
        para(vec![
            lien("m@d.fr", "mailto:m@d.fr"),
            Segment::texte(".a__")
        ])
    );
    assert_eq!(r("xwww.ex.fr"), para(vec![Segment::texte("xwww.ex.fr")]));
    assert_eq!(
        r("(www.ex.fr)"),
        para(vec![
            Segment::texte("("),
            lien("www.ex.fr", "http://www.ex.fr"),
            Segment::texte(")")
        ])
    );
    assert_eq!(
        rendre("_@alice_", &["alice"]).blocs,
        para(vec![Segment {
            styles: italique(),
            ..mention("alice", GenreMention::Personne)
        }])
    );
    assert_eq!(
        rendre("@alice_ x", &["alice"]).blocs,
        para(vec![Segment::texte("@alice_ x")])
    );
}

#[test]
fn blocs_imbriques_comme_marked() {
    assert_eq!(
        r("- a\n  - b\n    - c\n- d"),
        vec![Bloc::Liste {
            ordonnee: false,
            elements: vec![
                vec![
                    p1("a"),
                    Bloc::Liste {
                        ordonnee: false,
                        elements: vec![vec![
                            p1("b"),
                            Bloc::Liste {
                                ordonnee: false,
                                elements: vec![vec![p1("c")]]
                            }
                        ]]
                    }
                ],
                vec![p1("d")],
            ]
        }]
    );
    assert_eq!(
        r("> a\n> > b\nlazy"),
        vec![Bloc::Citation(vec![
            p1("a"),
            Bloc::Citation(vec![Bloc::Paragraphe(vec![
                Segment::texte("b"),
                saut(),
                Segment::texte("lazy")
            ])])
        ])]
    );
    assert_eq!(r("> a\n\nb"), vec![Bloc::Citation(vec![p1("a")]), p1("b")]);
    assert_eq!(
        r("- a\n* b"),
        vec![
            Bloc::Liste {
                ordonnee: false,
                elements: vec![vec![p1("a")]]
            },
            Bloc::Liste {
                ordonnee: false,
                elements: vec![vec![p1("b")]]
            }
        ]
    );
    assert_eq!(
        r("- [ ] tache\n- [x] fait"),
        vec![Bloc::Liste {
            ordonnee: false,
            elements: vec![vec![p1("tache")], vec![p1("fait")]]
        }]
    );
    assert_eq!(
        r("1. a\n\n   suite\n2. b"),
        vec![Bloc::Liste {
            ordonnee: true,
            elements: vec![vec![p1("a"), p1("suite")], vec![p1("b")]]
        }]
    );
    assert_eq!(
        r("<div>**x**\nsuite</div>\n\npara"),
        vec![
            Bloc::Paragraphe(vec![
                Segment::texte("<div>**x**"),
                saut(),
                Segment::texte("suite</div>")
            ]),
            p1("para")
        ]
    );
}

#[test]
fn texte_brut_d_un_message() {
    let c = Mentionnables::nouveau(
        ["alice"],
        Vec::<String>::new(),
        Vec::<String>::new(),
        ["smile"],
    );
    let rendu = analyser(
        "Salut **@alice** :smile:\nligne 2\n\n> cité\n\n- un\n- deux\n\n```\ncode\n```",
        &c,
    );
    assert_eq!(
        texte_brut(&rendu.blocs),
        "Salut @alice :smile:\nligne 2\ncité\nun\ndeux\ncode"
    );
    assert_eq!(texte_brut(&[]), "");
}

#[test]
fn lien_ouvrable_ne_laisse_passer_que_http_https_et_mailto() {
    assert_eq!(
        lien_ouvrable("https://a.example/x?b=1#c").as_deref(),
        Some("https://a.example/x?b=1#c")
    );
    assert_eq!(
        lien_ouvrable("HTTP://A.EXAMPLE").as_deref(),
        Some("HTTP://A.EXAMPLE")
    );
    assert_eq!(
        lien_ouvrable("mailto:a@b.c").as_deref(),
        Some("mailto:a@b.c")
    );
    assert_eq!(
        lien_ouvrable("  https://a.example/é  ").as_deref(),
        Some("https://a.example/%C3%A9")
    );
    assert_eq!(
        lien_ouvrable("https://a.example/<x>\"").as_deref(),
        Some("https://a.example/%3Cx%3E%22")
    );
    for refuse in [
        "javascript:alert(1)",
        "JavaScript:alert(1)",
        " javascript:alert(1)",
        "java\tscript:alert(1)",
        "java\nscript:alert(1)",
        "\u{0}javascript:alert(1)",
        "file:///etc/passwd",
        "data:text/html;base64,PHNjcmlwdD4=",
        "vbscript:msgbox(1)",
        "ftp://x.example",
        "/relatif",
        "#ancre",
        "//hote.example/x",
        "httpx://a",
        "",
    ] {
        assert_eq!(lien_ouvrable(refuse), None, "{refuse:?}");
    }
    assert_eq!(
        lien_ouvrable(&format!(
            "https://a.example/{}",
            "x".repeat(LONGUEUR_URL_MAX)
        )),
        None
    );
}

#[test]
fn aucun_lien_du_modele_n_est_dangereux() {
    for entree in [
        "[a](javascript:alert(1)) [b](file:///etc/passwd) [c](data:x) [d](vbscript:x) [e](JAVASCRIPT:x)",
        "<javascript:alert(1)> <file:///x> <data:text/html,x>",
        "![i](javascript:alert(1)) ![j](data:image/png;base64,AAAA)",
        "[a](java&#x09;script:alert(1)) [b](\\javascript:x) [c]( javascript:x )",
    ] {
        let rendu = r(entree);
        assert!(tous_les_segments(&rendu).iter().all(|s| s.lien.is_none()), "{entree} : {rendu:?}");
    }
}

fn profondeur(blocs: &[Bloc]) -> usize {
    blocs
        .iter()
        .map(|b| match b {
            Bloc::Citation(i) => 1 + profondeur(i),
            Bloc::Liste { elements, .. } => {
                1 + elements.iter().map(|e| profondeur(e)).max().unwrap_or(0)
            }
            _ => 0,
        })
        .max()
        .unwrap_or(0)
}

fn compter(blocs: &[Bloc]) -> (usize, usize) {
    let mut total = (0, 0);
    for b in blocs {
        total.0 += 1;
        match b {
            Bloc::Paragraphe(s) | Bloc::Titre { segments: s, .. } => total.1 += s.len(),
            Bloc::Citation(i) => {
                let (b, s) = compter(i);
                total = (total.0 + b, total.1 + s);
            }
            Bloc::Liste { elements, .. } => {
                for e in elements {
                    let (b, s) = compter(e);
                    total = (total.0 + b, total.1 + s);
                }
            }
            Bloc::Code { .. } => {}
        }
    }
    total
}

fn entrees_pathologiques(taille: usize) -> Vec<(&'static str, String)> {
    let repeter = |motif: &str| motif.repeat(taille / motif.len() + 1)[..taille].to_string();
    vec![
        ("étoiles", "*".repeat(taille)),
        ("étoiles entre lettres", format!("a{}b", "*".repeat(taille))),
        ("emphases ouvertes", repeter("*a ")),
        ("soulignés", repeter("_a")),
        ("tildes", repeter("~a ")),
        ("spoilers sans fin", repeter("||a ")),
        ("crochets", repeter("[")),
        ("liens sans fin", repeter("[a](b")),
        ("accents graves", repeter("` `` ")),
        ("balises non fermées", repeter("<b ")),
        ("citations", format!("{}x", ">".repeat(taille))),
        ("citations espacées", format!("{}x", repeter("> "))),
        ("listes", format!("{}x", repeter("- "))),
        (
            "listes indentées",
            (0..taille / 40)
                .map(|i| format!("{}- x\n", " ".repeat(i % 30)))
                .collect(),
        ),
        ("paragraphes", repeter("a\n\n")),
        ("mentions", repeter("@alice ")),
        ("urls", repeter("https://a.b/(c ")),
        ("emojis", repeter(":smile:")),
        (
            "mélange",
            repeter("**gras** _it_ ~~b~~ `c` ||s|| [l](https://x) @alice :smile: > q\n- i\n"),
        ),
    ]
}

fn contexte_charge() -> Mentionnables {
    Mentionnables::nouveau(
        ["alice"],
        ["Chef de projet", "Modérateur"],
        ["everyone", "here"],
        ["smile"],
    )
}

#[test]
fn texte_malforme_borne_sans_panique() {
    let c = contexte_charge();
    for (nom, entree) in entrees_pathologiques(100_000) {
        let rendu = analyser(&entree, &c);
        let (blocs, segments) = compter(&rendu.blocs);
        assert!(blocs <= BLOCS_MAX, "{nom} : {blocs} blocs");
        assert!(segments <= SEGMENTS_MAX, "{nom} : {segments} segments");
        assert!(profondeur(&rendu.blocs) <= PROFONDEUR_BLOCS_MAX, "{nom}");
        let _ = texte_brut(&rendu.blocs);
    }
}

#[test]
fn citations_imbriquees_mille_fois() {
    let entree = format!("{}fin", "> ".repeat(1_000));
    let rendu = r(&entree);
    assert_eq!(profondeur(&rendu), PROFONDEUR_BLOCS_MAX);
    assert!(texte_brut(&rendu).ends_with("fin"));
    let lignes: String = (0..1_000)
        .map(|i| format!("{}x\n", ">".repeat(i + 1)))
        .collect();
    assert!(profondeur(&r(&lignes)) <= PROFONDEUR_BLOCS_MAX);
}

#[test]
fn dix_mille_etoiles_et_balises_non_fermees() {
    assert_eq!(r(&"*".repeat(10_000)), Vec::<Bloc>::new());
    let entree = format!("a{}b", "*".repeat(10_000));
    assert_eq!(texte_brut(&r(&entree)), entree);
    let entree = "**a ".repeat(2_500);
    assert_eq!(texte_brut(&r(&entree)), entree.trim_end());
    let entree = "<b>".repeat(3_000);
    assert_eq!(texte_brut(&r(&entree)), entree);
}

#[test]
fn troncature_marquee() {
    let rendu = rendre(&"a\n\n".repeat(BLOCS_MAX + 10), &[]);
    assert!(rendu.tronque);
    assert_eq!(rendu.blocs.len(), BLOCS_MAX);
    let rendu = rendre(&"a\n".repeat(SEGMENTS_MAX), &[]);
    assert!(rendu.tronque);
    assert_eq!(compter(&rendu.blocs).1, SEGMENTS_MAX);
    let rendu = rendre("court **message**", &[]);
    assert!(!rendu.tronque);
}

fn mesurer(entree: &str, c: &Mentionnables) -> Duration {
    let mut meilleur = Duration::MAX;
    for _ in 0..3 {
        let debut = Instant::now();
        let rendu = analyser(entree, c);
        meilleur = meilleur.min(debut.elapsed());
        std::hint::black_box(rendu);
    }
    meilleur
}

#[test]
fn temps_quasi_lineaire() {
    let c = contexte_charge();
    let petits = entrees_pathologiques(10_000);
    let grands = entrees_pathologiques(100_000);
    for ((nom, petit), (_, grand)) in petits.iter().zip(grands.iter()) {
        let t_petit = mesurer(petit, &c).max(Duration::from_micros(200));
        let t_grand = mesurer(grand, &c);
        let rapport = t_grand.as_secs_f64() / t_petit.as_secs_f64();
        eprintln!("{nom} : {t_petit:?} -> {t_grand:?} (rapport {rapport:.1})");
        assert!(
            rapport < 40.0,
            "{nom} : {t_petit:?} pour 10 000, {t_grand:?} pour 100 000 (rapport {rapport:.1})"
        );
    }
}
