use axum::{
    extract::{Path, State},
    response::Response,
};
use serde_json::{Value, json};

use crate::{
    arr::Service,
    etat::AppState,
    http::{indisponible, introuvable, ok},
    sante,
};

fn texte(v: &Value) -> Option<String> {
    v.as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(String::from)
}

fn positif(v: &Value) -> Option<f64> {
    v.as_f64().filter(|n| *n > 0.0)
}

fn nom_de_fichier(v: &Value) -> Option<String> {
    let chemin = texte(&v["relativePath"]).or_else(|| texte(&v["path"]))?;
    chemin
        .rsplit(['/', '\\'])
        .next()
        .filter(|n| !n.is_empty())
        .map(String::from)
}

fn langues(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| texte(&l["name"]))
        .collect()
}

fn fichier(f: &Value, groupe: Option<String>) -> Value {
    let m = &f["mediaInfo"];
    let video = texte(&m["videoCodec"]).map(|codec| {
        json!({
            "codec": codec,
            "resolution": texte(&m["resolution"]),
            "bit_depth": positif(&m["videoBitDepth"]),
            "dynamic_range": texte(&m["videoDynamicRangeType"]).or_else(|| texte(&m["videoDynamicRange"])),
            "fps": positif(&m["videoFps"]),
            "bitrate": positif(&m["videoBitrate"]),
            "scan": texte(&m["scanType"]),
        })
    });
    let codec_audio = texte(&m["audioCodec"]).or_else(|| texte(&m["audioFormat"]));
    let audio = codec_audio.map(|codec| {
        json!({
            "codec": codec,
            "channels": positif(&m["audioChannels"]),
            "languages": texte(&m["audioLanguages"]),
            "bitrate": positif(&m["audioBitrate"]).or_else(|| texte(&m["audioBitrate"]).and_then(|t| t.trim_end_matches("kbps").trim().parse::<f64>().ok()).map(|k| k * 1000.0)),
            "streams": positif(&m["audioStreamCount"]),
            "sample_rate": positif(&m["audioSampleRate"]).or_else(|| texte(&m["audioSampleRate"]).and_then(|t| t.trim_end_matches("kHz").trim().parse::<f64>().ok()).map(|k| k * 1000.0)),
            "bits": positif(&m["audioBits"]).or_else(|| texte(&m["audioBits"]).and_then(|t| t.trim_end_matches("bit").trim().parse::<f64>().ok())),
        })
    });
    json!({
        "name": nom_de_fichier(f),
        "group": groupe,
        "size": f["size"].as_i64(),
        "quality": texte(&f["quality"]["quality"]["name"]),
        "release_group": texte(&f["releaseGroup"]),
        "scene_name": texte(&f["sceneName"]),
        "edition": texte(&f["edition"]),
        "languages": langues(&f["languages"]),
        "video": video,
        "audio": audio,
        "subtitles": texte(&m["subtitles"]),
        "duration": texte(&m["runTime"]),
    })
}

fn genres(v: &Value) -> Vec<String> {
    v["genres"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(texte)
        .collect()
}

fn annee(v: &Value) -> Option<i64> {
    v["year"].as_i64().filter(|a| *a > 0).or_else(|| {
        v["releaseDate"]
            .as_str()
            .and_then(|d| d.get(..4))
            .and_then(|a| a.parse().ok())
    })
}

pub fn composer(service: Service, titre: &Value, fichiers: &[Value]) -> Value {
    let id = |cle: &str| titre[cle].as_i64().filter(|i| *i > 0);
    let mut fiches: Vec<Value> = match service {
        Service::Radarr => titre
            .get("movieFile")
            .filter(|f| f.is_object())
            .map(|f| fichier(f, None))
            .into_iter()
            .collect(),
        Service::Sonarr => fichiers
            .iter()
            .map(|f| {
                let saison = f["seasonNumber"].as_i64().map(|n| match n {
                    0 => "Épisodes spéciaux".to_string(),
                    n => format!("Saison {n}"),
                });
                fichier(f, saison)
            })
            .collect(),
        _ => fichiers.iter().map(|f| fichier(f, None)).collect(),
    };
    fiches.sort_by(|a, b| {
        let cle = |v: &Value| {
            (
                v["group"].as_str().unwrap_or_default().to_string(),
                v["name"].as_str().unwrap_or_default().to_string(),
            )
        };
        cle(a).cmp(&cle(b))
    });
    json!({
        "service": service.cle(),
        "id": titre["id"].as_i64(),
        "title": texte(&titre["title"]),
        "original_title": texte(&titre["originalTitle"]),
        "artist": texte(&titre["artist"]["artistName"]),
        "year": annee(titre),
        "runtime": id("runtime"),
        "genres": genres(titre),
        "ids": {
            "tmdb": id("tmdbId"),
            "imdb": texte(&titre["imdbId"]),
            "tvdb": id("tvdbId"),
            "musicbrainz": texte(&titre["foreignAlbumId"]),
        },
        "files": fiches,
    })
}

pub async fn lire(
    State(state): State<AppState>,
    Path((service, id)): Path<(String, i64)>,
) -> Response {
    let Some(service) = Service::bibliotheque(&service) else {
        return introuvable();
    };
    let Some(c) = sante::client(&state, service) else {
        return indisponible(
            "not_configured",
            &format!("{} n'est pas configuré", service.nom()),
        );
    };
    if id <= 0 {
        return introuvable();
    }
    match c.titre_et_fichiers(id).await {
        Ok(Some((titre, fichiers))) => ok(&composer(service, &titre, &fichiers)),
        Ok(None) => introuvable(),
        Err(e) => indisponible("upstream", &format!("{} : {e}", service.nom())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fiche_d_un_film() {
        let film = json!({"id": 4, "title": "Le Film", "originalTitle": "The Movie", "year": 2024, "tmdbId": 603, "imdbId": "tt0133093",
            "runtime": 136, "genres": ["Action", "Science-Fiction"],
            "movieFile": {"relativePath": "Le Film (2024)/Le.Film.2024.1080p.BluRay.x264-GRP.mkv", "path": "/films/Le Film (2024)/x.mkv",
                "size": 9000, "quality": {"quality": {"name": "Bluray-1080p"}}, "releaseGroup": "GRP", "sceneName": "Le.Film.2024.1080p.BluRay.x264-GRP",
                "languages": [{"name": "French"}, {"name": "English"}], "edition": "",
                "mediaInfo": {"videoCodec": "x264", "resolution": "1920x800", "videoBitDepth": 8, "videoDynamicRangeType": "", "videoFps": 23.976,
                    "videoBitrate": 9500000, "audioCodec": "DTS", "audioChannels": 5.1, "audioLanguages": "fre/eng", "audioBitrate": 1509000,
                    "audioStreamCount": 2, "subtitles": "fre/eng", "runTime": "2:16:17", "scanType": "Progressive"}}});
        let f = composer(Service::Radarr, &film, &[]);
        assert_eq!(
            (&f["title"], &f["original_title"], &f["year"], &f["runtime"]),
            (
                &json!("Le Film"),
                &json!("The Movie"),
                &json!(2024),
                &json!(136)
            )
        );
        assert_eq!(
            f["ids"],
            json!({"tmdb": 603, "imdb": "tt0133093", "tvdb": null, "musicbrainz": null})
        );
        let fichier = &f["files"][0];
        assert_eq!(
            fichier["name"], "Le.Film.2024.1080p.BluRay.x264-GRP.mkv",
            "nom seul, jamais le chemin"
        );
        assert!(!f.to_string().contains("/films"));
        assert_eq!(
            fichier["video"],
            json!({"codec": "x264", "resolution": "1920x800", "bit_depth": 8.0, "dynamic_range": null, "fps": 23.976, "bitrate": 9500000.0, "scan": "Progressive"})
        );
        assert_eq!(fichier["audio"]["channels"], 5.1);
        assert_eq!(
            (&fichier["languages"], &fichier["edition"]),
            (&json!(["French", "English"]), &Value::Null)
        );
        let sans = composer(
            Service::Radarr,
            &json!({"id": 9, "title": "Sans fichier"}),
            &[],
        );
        assert_eq!(sans["files"], json!([]));
    }

    #[test]
    fn fiche_d_une_serie_et_d_un_album() {
        let serie = json!({"id": 2, "title": "La Série", "year": 2020, "tvdbId": 77});
        let fichiers = [
            json!({"seasonNumber": 2, "relativePath": "Season 02/S02E01.mkv", "size": 5}),
            json!({"seasonNumber": 1, "relativePath": "Season 01/S01E02.mkv", "size": 5}),
            json!({"seasonNumber": 1, "relativePath": "Season 01/S01E01.mkv", "size": 5}),
            json!({"seasonNumber": 0, "relativePath": "Specials/S00E01.mkv", "size": 5}),
        ];
        let f = composer(Service::Sonarr, &serie, &fichiers);
        let ordre: Vec<(&str, &str)> = f["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| (x["group"].as_str().unwrap(), x["name"].as_str().unwrap()))
            .collect();
        assert_eq!(
            ordre,
            [
                ("Saison 1", "S01E01.mkv"),
                ("Saison 1", "S01E02.mkv"),
                ("Saison 2", "S02E01.mkv"),
                ("Épisodes spéciaux", "S00E01.mkv")
            ]
        );
        let album = json!({"id": 3, "title": "L'Album", "artist": {"artistName": "Le Groupe"}, "releaseDate": "1999-05-01T00:00:00Z",
            "foreignAlbumId": "mbid-1", "genres": ["Rock"]});
        let pistes = [
            json!({"path": "/musique/Le Groupe/L'Album/01 - Titre.flac", "size": 30,
            "quality": {"quality": {"name": "FLAC"}}, "mediaInfo": {"audioFormat": "FLAC", "audioBitrate": "1011kbps", "audioChannels": 2,
            "audioBits": "16bit", "audioSampleRate": "44.1kHz"}}),
        ];
        let f = composer(Service::Lidarr, &album, &pistes);
        assert_eq!(
            (&f["artist"], &f["year"], &f["ids"]["musicbrainz"]),
            (&json!("Le Groupe"), &json!(1999), &json!("mbid-1"))
        );
        assert_eq!(f["files"][0]["name"], "01 - Titre.flac");
        assert_eq!(
            f["files"][0]["audio"],
            json!({"codec": "FLAC", "channels": 2.0, "languages": null, "bitrate": 1011000.0, "streams": null, "sample_rate": 44100.0, "bits": 16.0})
        );
        assert_eq!(f["files"][0]["video"], Value::Null);
    }
}
