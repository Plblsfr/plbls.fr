use crate::fichiers::{refus_du_nombre, Limites};
use crate::types::Attachment;

pub const ATTENDRE_LES_FICHIERS: &str = "Attends la fin de l'envoi des fichiers.";
pub const FICHIERS_EN_ECHEC: &str = "Retire ou réessaie les fichiers en échec.";

#[derive(Debug, Clone, PartialEq)]
pub enum EtatPiece {
    Preparation,
    Envoi { envoyes: u64, total: u64 },
    Envoyee(Attachment),
    Echec(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub cle: u64,
    pub nom: String,
    pub taille: u64,
    pub etat: EtatPiece,
}

impl Piece {
    pub fn piece_jointe(&self) -> Option<&Attachment> {
        match &self.etat {
            EtatPiece::Envoyee(a) => Some(a),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListePieces {
    pub pieces: Vec<Piece>,
}

impl ListePieces {
    pub fn depuis(pieces: &[Attachment], mut cle: impl FnMut() -> u64) -> Self {
        Self {
            pieces: pieces
                .iter()
                .map(|a| Piece {
                    cle: cle(),
                    nom: a.name.clone().unwrap_or_default(),
                    taille: a.size.and_then(|t| u64::try_from(t).ok()).unwrap_or(0),
                    etat: EtatPiece::Envoyee(a.clone()),
                })
                .collect(),
        }
    }

    pub fn est_vide(&self) -> bool {
        self.pieces.is_empty()
    }

    pub fn ajouter(&mut self, cle: u64, nom: &str, limites: &Limites) -> Result<(), String> {
        if let Some(refus) = refus_du_nombre(self.pieces.len(), limites) {
            return Err(refus);
        }
        self.pieces.push(Piece {
            cle,
            nom: nom.to_string(),
            taille: 0,
            etat: EtatPiece::Preparation,
        });
        Ok(())
    }

    pub fn piece_mut(&mut self, cle: u64) -> Option<&mut Piece> {
        self.pieces.iter_mut().find(|p| p.cle == cle)
    }

    pub fn retirer(&mut self, cle: u64) -> Option<Piece> {
        let i = self.pieces.iter().position(|p| p.cle == cle)?;
        Some(self.pieces.remove(i))
    }

    pub fn progression(&mut self, cle: u64, envoyes: u64, total: u64) -> bool {
        match self.piece_mut(cle) {
            Some(p) if !matches!(p.etat, EtatPiece::Envoyee(_)) => {
                p.taille = total;
                p.etat = EtatPiece::Envoi { envoyes, total };
                true
            }
            _ => false,
        }
    }

    pub fn terminer(&mut self, cle: u64, resultat: Result<Attachment, String>) -> bool {
        let Some(p) = self.piece_mut(cle) else {
            return false;
        };
        p.etat = match resultat {
            Ok(a) => {
                if let Some(nom) = a.name.clone() {
                    p.nom = nom;
                }
                if let Some(taille) = a.size.and_then(|t| u64::try_from(t).ok()) {
                    p.taille = taille;
                }
                EtatPiece::Envoyee(a)
            }
            Err(erreur) => EtatPiece::Echec(erreur),
        };
        true
    }

    pub fn reessayer(&mut self, cle: u64) -> bool {
        match self.piece_mut(cle) {
            Some(p) if matches!(p.etat, EtatPiece::Echec(_)) => {
                p.etat = EtatPiece::Preparation;
                true
            }
            _ => false,
        }
    }

    pub fn bloquant(&self) -> Option<&'static str> {
        if self
            .pieces
            .iter()
            .any(|p| matches!(p.etat, EtatPiece::Echec(_)))
        {
            Some(FICHIERS_EN_ECHEC)
        } else if self.pieces.iter().any(|p| p.piece_jointe().is_none()) {
            Some(ATTENDRE_LES_FICHIERS)
        } else {
            None
        }
    }

    pub fn pieces_jointes(&self) -> Vec<Attachment> {
        self.pieces
            .iter()
            .filter_map(|p| p.piece_jointe().cloned())
            .collect()
    }

    pub fn identifiants(&self) -> Vec<String> {
        self.pieces
            .iter()
            .filter_map(|p| p.piece_jointe().and_then(|a| a.id.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jointe(id: &str) -> Attachment {
        Attachment {
            id: Some(id.into()),
            name: Some(format!("{id}.png")),
            size: Some(12),
            ..Attachment::default()
        }
    }

    #[test]
    fn une_piece_passe_de_la_preparation_a_l_envoi_puis_jointe() {
        let mut l = ListePieces::default();
        let limites = Limites::default();
        l.ajouter(1, "a.txt", &limites).unwrap();
        assert_eq!(l.bloquant(), Some(ATTENDRE_LES_FICHIERS));
        assert!(l.progression(1, 5, 10));
        assert_eq!(
            l.pieces[0].etat,
            EtatPiece::Envoi {
                envoyes: 5,
                total: 10
            }
        );
        assert!(l.terminer(1, Ok(jointe("f1"))));
        assert_eq!(l.pieces[0].nom, "f1.png");
        assert!(!l.progression(1, 10, 10), "une pièce jointe ne recule pas");
        assert_eq!(l.bloquant(), None);
        assert_eq!(l.identifiants(), ["f1"]);
    }

    #[test]
    fn un_echec_bloque_l_envoi_jusqu_a_la_reprise_ou_au_retrait() {
        let mut l = ListePieces::default();
        l.ajouter(1, "a", &Limites::default()).unwrap();
        l.terminer(1, Err("413 trop gros".into()));
        assert_eq!(l.bloquant(), Some(FICHIERS_EN_ECHEC));
        assert!(l.reessayer(1));
        assert_eq!(l.pieces[0].etat, EtatPiece::Preparation);
        assert!(!l.reessayer(1));
        assert!(l.retirer(1).is_some());
        assert!(l.est_vide() && l.bloquant().is_none());
        assert!(
            !l.terminer(1, Ok(jointe("x"))),
            "une pièce retirée est oubliée"
        );
    }

    #[test]
    fn le_nombre_de_pieces_est_borne() {
        let mut l = ListePieces::default();
        let limites = Limites::default();
        for cle in 0..10 {
            l.ajouter(cle, "f", &limites).unwrap();
        }
        assert_eq!(
            l.ajouter(10, "f", &limites),
            Err("10 fichiers maximum par message.".into())
        );
    }

    #[test]
    fn la_liste_d_un_message_modifie_reprend_ses_pieces_jointes() {
        let mut n = 0;
        let l = ListePieces::depuis(&[jointe("f1"), jointe("f2")], || {
            n += 1;
            n
        });
        assert_eq!(l.identifiants(), ["f1", "f2"]);
        assert_eq!(l.pieces[1].cle, 2);
        assert_eq!(l.pieces_jointes().len(), 2);
    }
}
