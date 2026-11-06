use quick_xml::{NsReader, XmlVersion, events::Event, name::ResolveResult};

const ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "path",
    "circle",
    "ellipse",
    "rect",
    "line",
    "polyline",
    "polygon",
    "defs",
    "linearGradient",
    "radialGradient",
    "stop",
    "clipPath",
    "mask",
    "use",
];

const ATTRIBUTS: &[&str] = &[
    "viewBox",
    "width",
    "height",
    "x",
    "y",
    "x1",
    "y1",
    "x2",
    "y2",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "d",
    "points",
    "transform",
    "id",
    "fill",
    "fill-opacity",
    "fill-rule",
    "stroke",
    "stroke-width",
    "stroke-opacity",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "opacity",
    "offset",
    "stop-color",
    "stop-opacity",
    "gradientUnits",
    "gradientTransform",
    "fx",
    "fy",
    "spreadMethod",
    "clip-path",
    "clip-rule",
    "mask",
    "clipPathUnits",
    "maskUnits",
    "preserveAspectRatio",
];

fn valeur_sure(v: &str) -> bool {
    let l = v.to_lowercase();
    if l.contains("javascript:") || l.contains("data:") || l.contains('<') {
        return false;
    }
    let mut reste = l.as_str();
    while let Some(i) = reste.find("url(") {
        reste = reste[i + 4..].trim_start_matches([' ', '\'', '"']);
        if !reste.starts_with('#') {
            return false;
        }
    }
    true
}

fn echapper(v: &str, sortie: &mut String) {
    for c in v.chars() {
        match c {
            '"' => sortie.push_str("&#34;"),
            '\'' => sortie.push_str("&#39;"),
            '&' => sortie.push_str("&amp;"),
            '<' => sortie.push_str("&lt;"),
            '>' => sortie.push_str("&gt;"),
            '\t' => sortie.push_str("&#x9;"),
            '\n' => sortie.push_str("&#xA;"),
            '\r' => sortie.push_str("&#xD;"),
            c => sortie.push(c),
        }
    }
}

pub fn nettoyer(entree: &[u8]) -> Result<Vec<u8>, &'static str> {
    const ILLISIBLE: &str = "SVG illisible";
    let texte = std::str::from_utf8(entree).map_err(|_| ILLISIBLE)?;
    let mut lecteur = NsReader::from_str(texte);
    lecteur.config_mut().expand_empty_elements = true;
    let mut sortie = String::new();
    let (mut ignore, mut profondeur) = (0usize, 0usize);
    loop {
        match lecteur.read_event().map_err(|_| ILLISIBLE)? {
            Event::Eof => break,
            Event::DocType(_) => return Err("SVG : DOCTYPE interdit"),
            Event::Start(e) => {
                let nom = e.local_name().as_ref().to_string();
                if ignore > 0
                    || !ELEMENTS.contains(&nom.as_str())
                    || (profondeur == 0 && nom != "svg")
                {
                    ignore += 1;
                    continue;
                }
                sortie.push('<');
                sortie.push_str(&nom);
                if profondeur == 0 {
                    sortie.push_str(" xmlns=\"http://www.w3.org/2000/svg\"");
                }
                let mut view_box = false;
                for a in e.attributes() {
                    let a = a.map_err(|_| ILLISIBLE)?;
                    if a.key.as_ref() == "xmlns" || a.key.as_ref().starts_with("xmlns:") {
                        continue;
                    }
                    let (espace, local) = lecteur.resolver().resolve_attribute(a.key);
                    let espace = match espace {
                        ResolveResult::Unbound => String::new(),
                        ResolveResult::Bound(ns) => ns.as_ref().to_string(),
                        ResolveResult::Unknown(prefixe) => prefixe,
                    };
                    let nom_attr = local.as_ref().to_string();
                    let valeur = a
                        .normalized_value(XmlVersion::Implicit1_0)
                        .map_err(|_| ILLISIBLE)?;
                    let lien =
                        nom_attr == "href" && (espace.is_empty() || espace.contains("xlink"));
                    if ((!ATTRIBUTS.contains(&nom_attr.as_str()) || !espace.is_empty()) && !lien)
                        || !valeur_sure(&valeur)
                        || (lien && !valeur.starts_with('#'))
                    {
                        continue;
                    }
                    view_box = view_box || nom_attr == "viewBox";
                    sortie.push(' ');
                    sortie.push_str(&nom_attr);
                    sortie.push_str("=\"");
                    echapper(&valeur, &mut sortie);
                    sortie.push('"');
                }
                if profondeur == 0 && !view_box {
                    return Err("SVG : attribut viewBox requis");
                }
                sortie.push('>');
                profondeur += 1;
            }
            Event::End(e) => {
                if ignore > 0 {
                    ignore -= 1;
                    continue;
                }
                profondeur = profondeur.saturating_sub(1);
                sortie.push_str("</");
                sortie.push_str(e.local_name().as_ref());
                sortie.push('>');
            }
            _ => {}
        }
    }
    if sortie.is_empty() || profondeur != 0 {
        return Err("SVG : élément <svg> attendu");
    }
    Ok(sortie.into_bytes())
}
