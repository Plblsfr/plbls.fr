use axum::{Router, http::header, routing::get};

use crate::AppState;

const SVG: [(header::HeaderName, &str); 2] = [
    (header::CONTENT_TYPE, "image/svg+xml"),
    (header::CACHE_CONTROL, "public, max-age=86400"),
];

macro_rules! svg {
    ($router:expr, $name:literal) => {
        $router.route(
            concat!("/", $name, ".svg"),
            get(|| async { (SVG, include_str!(concat!("../../assets/", $name, ".svg"))) }),
        )
    };
}

macro_rules! ico {
    ($router:expr, $path:literal, $file:literal) => {
        $router.route(
            $path,
            get(|| async {
                (
                    [
                        (header::CONTENT_TYPE, "image/x-icon"),
                        (header::CACHE_CONTROL, "public, max-age=86400"),
                    ],
                    include_bytes!(concat!("../../assets/", $file)).as_slice(),
                )
            }),
        )
    };
}

pub fn router() -> Router<AppState> {
    let r = Router::new();
    let r = svg!(r, "logo-light");
    let r = svg!(r, "logo-dark");
    let r = svg!(r, "favicon-light");
    let r = svg!(r, "favicon-dark");
    // /favicon.ico est demandé d'office par les navigateurs, sans notion de thème : version claire
    let r = ico!(r, "/favicon.ico", "favicon-light.ico");
    ico!(r, "/favicon-dark.ico", "favicon-dark.ico")
}

#[cfg(test)]
mod tests {
    #[test]
    fn svg_valides() {
        for s in [
            include_str!("../../assets/logo-light.svg"),
            include_str!("../../assets/logo-dark.svg"),
            include_str!("../../assets/favicon-light.svg"),
            include_str!("../../assets/favicon-dark.svg"),
        ] {
            assert!(s.starts_with("<svg") && s.trim_end().ends_with("</svg>"));
        }
        for ico in [
            &include_bytes!("../../assets/favicon-light.ico")[..],
            &include_bytes!("../../assets/favicon-dark.ico")[..],
        ] {
            assert_eq!(ico[..4], [0, 0, 1, 0]);
        }
    }
}
