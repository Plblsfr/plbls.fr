pub mod essai;
pub mod factice;
pub mod format;
pub mod micro;
pub mod peripheriques;
pub mod tonalite;

pub use essai::EssaiMicro;
pub use estompe_plateforme::{FormatAudio, PeripheriqueAudio, SensAudio, SystemeAudio};
pub use format::{
    ConversionEntree, ConversionSortie, Reechantillonneur, SourceLecture, TamponLecture,
    ECHANTILLONS_PAR_TRAME, FREQUENCE,
};
pub use micro::{
    niveau_db, porte_ouverte, Apm, ChaineMicro, DetectionVoix, MesureMicro, ModeBruit, MoteurApm,
    NiveauMicro, ReglagesMicro,
};
pub use peripheriques::{resoudre, Bascule, Capture, Lecture, RenduTrame};
pub use tonalite::{SonVocal, Tonalite};

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::factice::SystemeFactice;
    use super::*;

    fn format(frequence: u32, canaux: u16) -> FormatAudio {
        FormatAudio { frequence, canaux }
    }

    fn systeme() -> SystemeFactice {
        let s = SystemeFactice::nouveau();
        s.brancher(
            SensAudio::Entree,
            "micro-integre",
            "Micro intégré",
            format(44_100, 2),
        );
        s.brancher(
            SensAudio::Entree,
            "casque-bt",
            "Casque Bluetooth",
            format(16_000, 1),
        );
        s.brancher(
            SensAudio::Sortie,
            "haut-parleurs",
            "Haut-parleurs",
            format(48_000, 2),
        );
        s.brancher(
            SensAudio::Sortie,
            "casque-bt-sortie",
            "Casque Bluetooth",
            format(44_100, 2),
        );
        s
    }

    #[test]
    fn choisit_le_peripherique_retenu_sinon_celui_par_defaut() {
        let s = systeme();
        let liste = s.peripheriques(SensAudio::Entree).unwrap();
        assert_eq!(resoudre(&liste, Some("casque-bt")).unwrap().id, "casque-bt");
        assert_eq!(
            resoudre(&liste, Some("absent")).unwrap().id,
            "micro-integre"
        );
        assert_eq!(resoudre(&liste, None).unwrap().id, "micro-integre");
        assert!(resoudre(&[], None).is_none());
    }

    #[test]
    fn capture_puis_lecture_dix_minutes_sans_perte_ni_decalage_croissant() {
        let s = systeme();
        let tampon = Arc::new(Mutex::new(TamponLecture::default()));
        let vers_tampon = tampon.clone();
        let mut trames = 0u64;
        let compte = Arc::new(Mutex::new(0u64));
        let compte_rendu = compte.clone();
        let _capture = Capture::ouvrir(
            Arc::new(s.clone()),
            None,
            Box::new(move |trame| {
                assert_eq!(trame.len(), ECHANTILLONS_PAR_TRAME);
                *compte_rendu.lock().unwrap() += 1;
                vers_tampon.lock().unwrap().pousser(trame);
            }),
        )
        .unwrap();
        let _lecture = Lecture::ouvrir(Arc::new(s.clone()), None, tampon.clone()).unwrap();
        let mut phase = 0.0f32;
        let mut latence_max = 0;
        let mut energie = 0.0f64;
        let mut silences_au_debut = 0;
        let pas = 10 * 60 * 100;
        for i in 0..pas {
            let mut entree = Vec::with_capacity(882);
            for _ in 0..441 {
                let e = 0.5 * phase.sin();
                phase = (phase + 440.0 * std::f32::consts::TAU / 44_100.0) % std::f32::consts::TAU;
                entree.extend([e, e]);
            }
            assert!(s.pousser("micro-integre", &entree));
            let sortie = s.tirer("haut-parleurs", 960).unwrap();
            if i == 10 {
                silences_au_debut = tampon.lock().unwrap().compteurs().silences;
            }
            if i > 10 {
                latence_max = latence_max.max(tampon.lock().unwrap().longueur());
                energie += sortie.iter().map(|e| f64::from(e * e)).sum::<f64>();
            }
            trames = *compte.lock().unwrap();
        }
        let compteurs = tampon.lock().unwrap().compteurs().clone();
        assert!(
            trames.abs_diff(pas as u64) <= 1,
            "{trames} trames pour {pas} pas"
        );
        assert_eq!(compteurs.perdus, 0);
        assert_eq!(compteurs.silences, silences_au_debut, "{compteurs:?}");
        assert!(compteurs.recus - compteurs.lus <= 3 * ECHANTILLONS_PAR_TRAME as u64);
        assert!(
            latence_max <= 3 * ECHANTILLONS_PAR_TRAME,
            "latence {latence_max}"
        );
        let puissance = energie / ((pas - 11) as f64 * 960.0);
        assert!((puissance - 0.125).abs() < 0.01, "puissance {puissance}");
    }

    #[test]
    fn un_micro_debranche_bascule_sur_celui_par_defaut_puis_revient() {
        let s = systeme();
        let recues = Arc::new(Mutex::new(0usize));
        let compte = recues.clone();
        let mut capture = Capture::ouvrir(
            Arc::new(s.clone()),
            Some("casque-bt".into()),
            Box::new(move |_| *compte.lock().unwrap() += 1),
        )
        .unwrap();
        assert_eq!(capture.peripherique(), Some("casque-bt"));
        assert!(!capture.en_repli());
        assert_eq!(capture.surveiller(), None);
        s.debrancher("casque-bt");
        assert!(!s.pousser("casque-bt", &[0.0; 160]));
        assert_eq!(
            capture.surveiller(),
            Some(Bascule::Repli("micro-integre".into()))
        );
        assert!(capture.en_repli());
        assert!(s.pousser("micro-integre", &[0.1; 1_764]));
        assert_eq!(*recues.lock().unwrap(), 1);
        assert_eq!(capture.surveiller(), None);
        s.brancher(
            SensAudio::Entree,
            "casque-bt",
            "Casque Bluetooth",
            format(16_000, 1),
        );
        assert_eq!(
            capture.surveiller(),
            Some(Bascule::Retour("casque-bt".into()))
        );
        assert_eq!(s.flux_ouverts(), vec!["casque-bt".to_string()]);
        assert!(s.pousser("casque-bt", &[0.1; 320]));
    }

    #[test]
    fn une_sortie_debranchee_bascule_sans_plantage_et_le_changement_a_chaud_suit_le_choix() {
        let s = systeme();
        let tampon = Arc::new(Mutex::new(TamponLecture::default()));
        let mut lecture =
            Lecture::ouvrir(Arc::new(s.clone()), Some("casque-bt-sortie".into()), tampon).unwrap();
        assert_eq!(lecture.peripherique(), Some("casque-bt-sortie"));
        s.debrancher("casque-bt-sortie");
        assert!(s.tirer("casque-bt-sortie", 882).is_none());
        assert_eq!(
            lecture.surveiller(),
            Some(Bascule::Repli("haut-parleurs".into()))
        );
        assert_eq!(s.tirer("haut-parleurs", 960).unwrap().len(), 960);
        assert_eq!(
            lecture.choisir(None).unwrap(),
            Bascule::Choix("haut-parleurs".into())
        );
        s.debrancher("haut-parleurs");
        assert!(matches!(lecture.surveiller(), Some(Bascule::Perdu(_))));
        s.brancher(
            SensAudio::Sortie,
            "haut-parleurs",
            "Haut-parleurs",
            format(48_000, 2),
        );
        assert_eq!(
            lecture.surveiller(),
            Some(Bascule::Retour("haut-parleurs".into()))
        );
    }

    #[test]
    fn un_systeme_sans_micro_refuse_la_capture() {
        let s = SystemeFactice::nouveau();
        assert!(Capture::ouvrir(Arc::new(s), None, Box::new(|_| {})).is_err());
    }
}
