export type Demande = {
  titre: string
  message?: string
  action?: string
  destructif?: boolean
  saisie?: { label: string; valeur?: string; exiger?: string }
  case?: string
}

type Reponse = { ok: boolean; valeur: string }

export const confirmation = $state<{ en: (Demande & { resoudre: (r: Reponse) => void }) | null }>({ en: null })

export function peutValider(d: Demande, valeur: string, coche: boolean): boolean {
  if (d.saisie?.exiger !== undefined && valeur.trim() !== d.saisie.exiger) return false
  return !d.case || coche
}

let courante: object | null = null

function demander(d: Demande): Promise<Reponse> {
  confirmation.en?.resoudre({ ok: false, valeur: '' })
  return new Promise((resoudre) => {
    const en = {
      ...d,
      resoudre: (r: Reponse) => {
        if (courante === en) confirmation.en = null
        resoudre(r)
      },
    }
    courante = en
    confirmation.en = en
  })
}

export const confirmer = async (d: Demande) => (await demander(d)).ok

export async function saisir(d: Demande & { saisie: NonNullable<Demande['saisie']> }): Promise<string | null> {
  const r = await demander(d)
  return r.ok ? r.valeur : null
}
