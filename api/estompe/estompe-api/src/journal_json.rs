use std::fmt;

use serde_json::{Map, Value};
use tracing::{Event, Subscriber, field::Visit};
use tracing_subscriber::{
    fmt::{
        FmtContext, FormatEvent, FormatFields,
        format::Writer,
        time::{FormatTime, SystemTime},
    },
    registry::LookupSpan,
};

pub struct FormatJson;

#[derive(Default)]
struct Champs(Map<String, Value>);

impl Champs {
    fn inserer(&mut self, nom: &str, valeur: &str) {
        if nom == "donnees"
            && let Ok(Value::Object(m)) = serde_json::from_str::<Value>(valeur)
        {
            self.0.extend(m);
            return;
        }
        self.0.insert(nom.into(), valeur.into());
    }
}

impl Visit for Champs {
    fn record_str(&mut self, champ: &tracing::field::Field, valeur: &str) {
        self.inserer(champ.name(), valeur);
    }

    fn record_i64(&mut self, champ: &tracing::field::Field, valeur: i64) {
        self.0.insert(champ.name().into(), valeur.into());
    }

    fn record_u64(&mut self, champ: &tracing::field::Field, valeur: u64) {
        self.0.insert(champ.name().into(), valeur.into());
    }

    fn record_bool(&mut self, champ: &tracing::field::Field, valeur: bool) {
        self.0.insert(champ.name().into(), valeur.into());
    }

    fn record_debug(&mut self, champ: &tracing::field::Field, valeur: &dyn fmt::Debug) {
        self.inserer(champ.name(), &format!("{valeur:?}"));
    }
}

pub fn ligne(horodatage: &str, evenement: &Event<'_>) -> String {
    let mut champs = Champs::default();
    evenement.record(&mut champs);
    let mut sortie = Map::new();
    sortie.insert("timestamp".into(), horodatage.into());
    sortie.insert("level".into(), evenement.metadata().level().as_str().into());
    sortie.insert("target".into(), evenement.metadata().target().into());
    sortie.extend(champs.0);
    Value::Object(sortie).to_string()
}

impl<S, N> FormatEvent<S, N> for FormatJson
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _: &FmtContext<'_, S, N>,
        mut ecrivain: Writer<'_>,
        evenement: &Event<'_>,
    ) -> fmt::Result {
        let mut horodatage = String::new();
        SystemTime.format_time(&mut Writer::new(&mut horodatage))?;
        writeln!(ecrivain, "{}", ligne(&horodatage, evenement))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tracing_subscriber::fmt::MakeWriter;

    use super::*;

    #[derive(Clone, Default)]
    struct Tampon(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Tampon {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Tampon {
        type Writer = Tampon;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn echange_journalise_a_plat_en_json() {
        let tampon = Tampon::default();
        let abonne = tracing_subscriber::fmt()
            .event_format(FormatJson)
            .with_writer(tampon.clone())
            .finish();
        tracing::subscriber::with_default(abonne, || {
            estompe_core::journal::Echange {
                request_type: "output",
                service: Some("entracte"),
                methode: "GET",
                chemin: "http://entracte/internal/titles".into(),
                requete: Value::Null,
                request_data: Value::Null,
                header_data: serde_json::json!({"x-service-token": estompe_core::journal::MASQUE}),
                statut: Some(401),
                response_header_data: Value::Null,
                response_data: "Unauthorized".into(),
                duree: std::time::Duration::from_millis(5),
                erreur: Some("HTTP 401".into()),
            }
            .emettre();
            tracing::info!("autre message");
        });
        let sortie = String::from_utf8(tampon.0.lock().unwrap().clone()).unwrap();
        let lignes: Vec<Value> = sortie
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lignes.len(), 2);
        assert_eq!(lignes[0]["request_type"], "output");
        assert_eq!(lignes[0]["response_status"], 401);
        assert_eq!(lignes[0]["error"], "HTTP 401");
        assert_eq!(
            lignes[0]["header_data"]["request"]["x-service-token"],
            "**MASKED**"
        );
        assert_eq!(lignes[0]["level"], "WARN");
        assert_eq!(lignes[1]["message"], "autre message");
    }
}
