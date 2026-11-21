use std::io::Read;
use std::path::Path;

use base64::Engine;
use minisign_verify::{Error, PublicKey, Signature};

use super::ErreurMiseAJour;

const PREFIXE_COMMENTAIRE: &str = "estompe-natif ";
const TAMPON: usize = 64 * 1024;

fn commentaire_attendu(version: &str) -> String {
    format!("{PREFIXE_COMMENTAIRE}{version}")
}

fn raison(erreur: &Error) -> String {
    match erreur {
        Error::InvalidSignature => "contenu modifié ou signature fausse".into(),
        Error::UnexpectedKeyId => "signée par une autre clé".into(),
        Error::UnsupportedLegacyMode | Error::UnexpectedAlgorithm => {
            "signature d'un format refusé".into()
        }
        Error::InvalidEncoding | Error::UnsupportedAlgorithm => "signature illisible".into(),
        Error::IoError(e) => format!("lecture impossible : {e}"),
    }
}

fn decoder(signature_base64: &str, cle: &str) -> Result<(PublicKey, Signature), ErreurMiseAJour> {
    let cle = PublicKey::decode(cle)
        .map_err(|_| ErreurMiseAJour::Signature("clé publique illisible".into()))?;
    let texte = base64::engine::general_purpose::STANDARD
        .decode(signature_base64.trim())
        .ok()
        .and_then(|octets| String::from_utf8(octets).ok())
        .ok_or_else(|| ErreurMiseAJour::Signature("signature illisible".into()))?;
    let signature =
        Signature::decode(&texte).map_err(|e| ErreurMiseAJour::Signature(raison(&e)))?;
    Ok((cle, signature))
}

fn controler_la_version(signature: &Signature, version: &str) -> Result<(), ErreurMiseAJour> {
    let commentaire = signature.trusted_comment().trim();
    if commentaire == commentaire_attendu(version) {
        Ok(())
    } else {
        Err(ErreurMiseAJour::Signature(format!(
            "signée pour « {commentaire} » et non pour la version {version}"
        )))
    }
}

pub fn verifier(
    octets: &[u8],
    signature_base64: &str,
    version: &str,
    cle: &str,
) -> Result<(), ErreurMiseAJour> {
    let (cle, signature) = decoder(signature_base64, cle)?;
    let mut verificateur = cle
        .verify_stream(&signature)
        .map_err(|e| ErreurMiseAJour::Signature(raison(&e)))?;
    verificateur.update(octets);
    verificateur
        .finalize()
        .map_err(|e| ErreurMiseAJour::Signature(raison(&e)))?;
    controler_la_version(&signature, version)
}

pub fn verifier_fichier(
    chemin: &Path,
    signature_base64: &str,
    version: &str,
    cle: &str,
) -> Result<(), ErreurMiseAJour> {
    let (cle, signature) = decoder(signature_base64, cle)?;
    let mut verificateur = cle
        .verify_stream(&signature)
        .map_err(|e| ErreurMiseAJour::Signature(raison(&e)))?;
    let mut fichier = std::fs::File::open(chemin)
        .map_err(|e| ErreurMiseAJour::Signature(format!("installeur illisible : {e}")))?;
    let mut tampon = vec![0u8; TAMPON];
    loop {
        let lus = fichier
            .read(&mut tampon)
            .map_err(|e| ErreurMiseAJour::Signature(format!("installeur illisible : {e}")))?;
        if lus == 0 {
            break;
        }
        verificateur.update(&tampon[..lus]);
    }
    verificateur
        .finalize()
        .map_err(|e| ErreurMiseAJour::Signature(raison(&e)))?;
    controler_la_version(&signature, version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mise_a_jour::essais::{
        installeur, CLE_ESSAI, SIGNATURE_ANCIENNE, SIGNATURE_ANCIEN_FORMAT, SIGNATURE_AUTRE_CLE,
        SIGNATURE_BONNE,
    };
    use crate::mise_a_jour::CLE_PUBLIQUE;

    #[test]
    fn accepte_l_installeur_signe_pour_sa_version() {
        assert_eq!(
            verifier(&installeur(), SIGNATURE_BONNE, "0.2.0", CLE_ESSAI),
            Ok(())
        );
        let dossier = tempfile::tempdir().unwrap();
        let chemin = dossier.path().join("installeur.exe");
        std::fs::write(&chemin, installeur()).unwrap();
        assert_eq!(
            verifier_fichier(&chemin, SIGNATURE_BONNE, "0.2.0", CLE_ESSAI),
            Ok(())
        );
    }

    #[test]
    fn refuse_un_octet_modifie() {
        let mut modifie = installeur();
        modifie[1000] ^= 0x01;
        let erreur = verifier(&modifie, SIGNATURE_BONNE, "0.2.0", CLE_ESSAI).unwrap_err();
        assert_eq!(
            erreur,
            ErreurMiseAJour::Signature("contenu modifié ou signature fausse".into())
        );
        let tronque = &installeur()[..100_000];
        assert!(verifier(tronque, SIGNATURE_BONNE, "0.2.0", CLE_ESSAI).is_err());
    }

    #[test]
    fn refuse_une_signature_d_une_autre_cle() {
        assert_eq!(
            verifier(&installeur(), SIGNATURE_AUTRE_CLE, "0.2.0", CLE_ESSAI),
            Err(ErreurMiseAJour::Signature(
                "signée par une autre clé".into()
            ))
        );
        assert_eq!(
            verifier(&installeur(), SIGNATURE_BONNE, "0.2.0", CLE_PUBLIQUE),
            Err(ErreurMiseAJour::Signature(
                "signée par une autre clé".into()
            ))
        );
    }

    #[test]
    fn refuse_un_installeur_signe_pour_une_autre_version() {
        let erreur = verifier(&installeur(), SIGNATURE_ANCIENNE, "0.2.0", CLE_ESSAI).unwrap_err();
        assert_eq!(
            erreur,
            ErreurMiseAJour::Signature(
                "signée pour « estompe-natif 0.1.9 » et non pour la version 0.2.0".into()
            )
        );
        assert!(verifier(&installeur(), SIGNATURE_BONNE, "0.2.1", CLE_ESSAI).is_err());
    }

    #[test]
    fn refuse_une_signature_non_prehachee() {
        assert_eq!(
            verifier(&installeur(), SIGNATURE_ANCIEN_FORMAT, "0.2.0", CLE_ESSAI),
            Err(ErreurMiseAJour::Signature(
                "signature d'un format refusé".into()
            ))
        );
    }

    #[test]
    fn refuse_une_signature_illisible() {
        for illisible in ["", "pas du base64 !", "dW50cnVzdGVkIGNvbW1lbnQ6IHRlc3QK"] {
            assert!(
                matches!(
                    verifier(&installeur(), illisible, "0.2.0", CLE_ESSAI),
                    Err(ErreurMiseAJour::Signature(_))
                ),
                "{illisible}"
            );
        }
        assert!(matches!(
            verifier(&installeur(), SIGNATURE_BONNE, "0.2.0", "clé"),
            Err(ErreurMiseAJour::Signature(_))
        ));
    }
}
