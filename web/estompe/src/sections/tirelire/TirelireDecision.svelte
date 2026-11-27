<script lang="ts">
  import { untrack } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import { confirmer } from '../../lib/confirm.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { centimes, euros, pluriel, saisieEuros } from './formats'
  import { actionLevier, detailLevier, lienLevier, messagePlafonds, phraseBase, phraseBudget, phraseEffet, titreLevier, type Resume } from './decision'
import { baisse, cleReglages, PAS, plafondsDe, prefillDe, reglagesDe, reglagesVides, SANS_REGLAGE, simuler, type LigneSimulee, type Reglages, type Simulation } from './simulation'
  import { CADENCES } from './habitudes'
  import { eurosRonds } from './stats'
  import TirelireBref from './TirelireBref.svelte'
  import { NATURES, teinte } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let { resume, simulation, requete = '' }: { resume: Resume | null; simulation: Simulation | null; requete?: string } = $props()

  const VISIBLES = 5
  let reglages = $state.raw<Reglages>(SANS_REGLAGE)
  let ajoutees = $state.raw<number[]>([])
  let serveur = $state.raw<{ cle: string; simulation: Simulation } | null>(null)
  let info = $state('')
  let avis = $state('')
  let pret = false
  let bloc = $state<HTMLElement>()

  $effect(() => {
    if (!simulation) return
    untrack(() => {
      serveur = null
      if (pret) return void valider()
      pret = true
      const p = prefillDe(requete)
      if (!p.cibles.length && !p.arrets.length) return
      const { reglages: r, dejaSous } = reglagesDe(simulation, p)
      reglages = r
      ajoutees = p.cibles.map((c) => c.id)
      if (dejaSous.length) avis = dejaSous.map((c) => `${c.name} : ta dépense habituelle (${euros(c.base)}) est déjà sous cette cible.`).join(' ')
      bloc?.scrollIntoView?.({ block: 'start' })
      void valider()
    })
  })

  const locale = $derived(simulation ? simuler(simulation, reglages) : null)
  const vue = $derived(simulation && serveur && serveur.cle === cleReglages(simulation.mode, reglages) ? serveur.simulation : locale)
  const lignes = $derived.by(() => {
    if (!vue) return []
    const variables = vue.categories.filter((c) => c.kind === 'variable' && c.base > 0).slice(0, VISIBLES).map((c) => c.id)
    return vue.categories.filter((c) => variables.includes(c.id) || ajoutees.includes(c.id) || reglages.categories[c.id])
  })
  const autres = $derived(vue ? vue.categories.filter((c) => !lignes.includes(c)) : [])
  const plafonds = $derived(vue ? plafondsDe(vue, reglages) : [])
  const echelle = $derived(vue ? Math.max(vue.reference.expenses, vue.reference.budget ?? 0, 1) * 1.04 : 1)
  const part = (v: number) => `${(Math.max(0, v) / echelle) * 100}%`

  async function valider() {
    if (!simulation || reglagesVides(reglages)) return
    const cle = cleReglages(simulation.mode, reglages)
    const s = await tirelire.simuler(reglages)
    if (s) serveur = { cle, simulation: s }
  }

  function regler(id: number, r: Reglages['categories'][number] | null) {
    const categories = { ...reglages.categories }
    if (r) categories[id] = r
    else delete categories[id]
    reglages = { ...reglages, categories }
    info = ''
    avis = ''
  }

  const glisser = (c: LigneSimulee, valeur: string) => regler(c.id, Number(valeur) > 0 ? { percent: Number(valeur) } : null)

  function cibler(c: LigneSimulee, champ: HTMLInputElement) {
    const ajustable = Math.max(0, c.base - c.stopped)
    const v = centimes(champ.value)
    if (v === null) avis = `Montant cible de ${c.name} : un montant en euros, par exemple 120 ou 120,50.`
    else if (v >= ajustable) { regler(c.id, null); if (v > ajustable) avis = `${c.name} : la cible ne peut pas dépasser ta dépense habituelle (${euros(ajustable)}).` }
    else regler(c.id, { target: v })
    champ.value = saisieEuros((vue?.categories.find((x) => x.id === c.id) ?? c).amount)
    void valider()
  }

  function cocher(id: string, coche: boolean) {
    reglages = { ...reglages, arrets: coche ? [...reglages.arrets.filter((a) => a !== id), id] : reglages.arrets.filter((a) => a !== id) }
    info = ''
    void valider()
  }

  function ajouter(champ: HTMLSelectElement) {
    if (champ.value) ajoutees = [...ajoutees, Number(champ.value)]
    champ.value = ''
  }

  function reinitialiser() {
    reglages = SANS_REGLAGE
    serveur = null
    info = ''
    avis = ''
  }

  async function fixer() {
    if (!plafonds.length) return
    const liste = plafonds
    const ok = await confirmer({ ...messagePlafonds(liste), action: 'Fixer' })
    if (!ok) return
    if (await tirelire.fixerPlafonds(liste)) info = `${pluriel(liste.length, 'plafond fixé', 'plafonds fixés')} : ${liste.map((p) => `${p.name} ${euros(p.cap)}`).join(', ')}.`
  }
</script>

<div class="tir-dec">
  <div class="tir-col tir-dec-resume" style:gap="12px">
    {#if resume}
      <TirelireBref {resume} />
      <section class="tir-carte" aria-label="Leviers les plus rentables">
        <div class="tir-col tir-col--serre"><h2 class="tir-h2">{resume.levers.length > 1 ? `Les ${resume.levers.length} leviers les plus rentables` : resume.levers.length ? 'Le levier le plus rentable' : 'Leviers'}</h2>{#if resume.levers.length > 1}<span class="tir-note">classés par gain estimé sur un an</span>{/if}</div>
        {#if resume.levers.length}
          <ol class="tir-liste">
            {#each resume.levers as l (l.rank)}
              {@const lien = lienLevier(l, resume)}
              <li>
                <a class="tir-li tir-li--haut" href={hrefs.tirelire(lien.section, lien.id, lien.requete)}>
                  <span class="tir-pas tir-pas--rose" aria-hidden="true">{l.rank}</span>
                  <span class="tir-col tir-col--serre tir-grandit"><span class="tir-h3"><span class="tir-sr">{l.rank}. </span>{titreLevier(l)}</span><span class="tir-note">{detailLevier(l, resume)}</span><span class="tir-note tir-dec-action">{actionLevier(l)}<Icon name="droite" size={14} /></span></span>
                  <span class="tir-col tir-col--colle" style:align-items="flex-end"><span class="tir-fort tir-fixe tir-dec-gain">{euros(l.yearly)}</span><span class="tir-note tir-fixe">par an</span><span class="tir-note tir-fixe">{euros(l.monthly)} par mois</span></span>
                </a>
              </li>
            {/each}
          </ol>
          <p class="tir-note">Gain estimé si tu reviens à ton plafond, à ta moyenne, à l’ancien tarif ou au niveau précédent, jamais en supprimant la dépense. {#if resume.levers_total > resume.levers.length}{pluriel(resume.levers_total - resume.levers.length, 'autre piste moins rentable n’est pas affichée', 'autres pistes moins rentables ne sont pas affichées')}.{/if}</p>
        {:else}
          <p class="tir-note tir-note--texte">Aucun levier à proposer sur cette période : aucun dépassement de plafond ou de moyenne, pas de petits achats répétés à réduire, pas d’abonnement nouveau ou plus cher, pas de commerçant en forte hausse.</p>
        {/if}
      </section>
    {:else if tirelire.resumeAbsent}
      <section class="tir-carte tir-carte--creux" aria-label="Résumé de la période"><p class="tir-note tir-note--texte">Aucun relevé importé ne couvre cette période.</p><a class="tir-btn tir-btn--large" href={hrefs.tirelire('decision')}>Voir la période la plus récente</a></section>
    {:else if tirelire.erreurResume}
      <p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />{tirelire.erreurResume}</p>
    {/if}
  </div>

  <div class="tir-col tir-dec-simulateur" style:gap="12px" bind:this={bloc}>
    {#if vue}
      <section class="tir-carte" aria-label="Simulateur">
        <div class="tir-ligne tir-ligne--ecart tir-ligne--haut">
          <div class="tir-col tir-col--serre"><h2 class="tir-h2">Simulateur</h2><span class="tir-note">si je baisse une catégorie de X %</span></div>
          <button type="button" class="tir-btn tir-btn--contour" disabled={reglagesVides(reglages)} onclick={reinitialiser}>Réinitialiser</button>
        </div>
        <p class="tir-note">{#if vue.basis.fragile}<span class="tir-etq tir-etq--attente">base fragile</span> {/if}{phraseBase(vue)} Curseur par pas de {PAS} %, ou saisis le montant visé.</p>
        {#if avis}<p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />{avis}</p>{/if}
        {#each lignes as c (c.id)}
          {@const taux = baisse(c)}
          <div class="tir-sim" role="group" aria-label={c.name}>
            <div class="tir-ligne tir-ligne--ecart">
              <span class="tir-ligne tir-grandit" style:gap="6px"><span class="tir-pt tir-pt--{teinte(c)}" aria-hidden="true"></span><span class="tir-h3">{c.name}</span>{#if c.kind === 'fixe'}<span class="tir-etq tir-etq--info">{NATURES[c.kind]}</span>{/if}</span>
              <span class="tir-etq" class:tir-etq--fort={c.saving > 0}>{c.saving > 0 ? `−${taux} %` : '0 %'}</span>
            </div>
            <input type="range" class="tir-curseur" min="0" max="100" step={PAS} value={taux} style:--part="{taux}%" aria-label="Baisse de {c.name} en pourcentage" aria-valuetext="{taux} %, {euros(c.saving)} économisés par mois" oninput={(e) => glisser(c, e.currentTarget.value)} onchange={() => void valider()} />
            <div class="tir-ligne tir-ligne--ecart tir-ligne--plis">
              <span class="tir-ligne" style:gap="6px">
                <span class="tir-note tir-fixe"><span class="tir-sr">Dépense habituelle : </span>{euros(c.base)}<span aria-hidden="true"> →</span></span>
                <label class="tir-champ tir-sim-cible"><span class="tir-sr">Montant cible de {c.name} en euros par mois</span><input type="text" inputmode="decimal" value={saisieEuros(c.amount)} onchange={(e) => cibler(c, e.currentTarget)} /><span class="tir-note" aria-hidden="true">€</span></label>
              </span>
              <span class="tir-fixe" class:tir-fort={c.saving > 0} class:tir-dec-gain={c.saving > 0} class:tir-note={c.saving === 0}>{c.saving > 0 ? `−${euros(c.saving)}` : euros(0)} par mois</span>
            </div>
            {#if c.stopped > 0}<p class="tir-note">{euros(c.stopped)} d’abonnements arrêtés sont retirés avant la baisse.</p>{/if}
          </div>
        {/each}
        {#if !lignes.length}<p class="tir-note tir-note--texte tir-separe">Aucune dépense variable sur la période de base. Ajoute une catégorie ci-dessous.</p>{/if}
        {#if autres.length}
          <label class="tir-champ tir-sim-ajout"><Icon name="plus" size={18} /><select aria-label="Ajouter une catégorie au simulateur" onchange={(e) => ajouter(e.currentTarget)}><option value="">Ajouter une catégorie</option>{#each autres as c (c.id)}<option value={c.id} disabled={c.base <= 0}>{c.name}{c.kind === 'fixe' ? ' (fixe)' : ''} · {c.base > 0 ? euros(c.base) : 'sans historique'}</option>{/each}</select><Icon name="bas" size={16} /></label>
        {/if}

        <div class="tir-col tir-separe" style:gap="4px" role="group" aria-label="Arrêter un abonnement">
          <h3 class="tir-h3">Arrêter un abonnement</h3>
          {#each vue.subscriptions as a (a.id)}
            <label class="tir-case tir-sim-abo"><input type="checkbox" checked={a.stopped} onchange={(e) => cocher(a.id, e.currentTarget.checked)} /><span class="tir-col tir-col--colle tir-grandit"><span class="tir-fort">{a.name ?? 'Commerçant sans nom'}</span><span class="tir-note">{euros(a.amount)} · {CADENCES[a.cadence]}</span></span><span class="tir-col tir-col--colle" style:align-items="flex-end"><span class="tir-fort tir-fixe">{euros(a.yearly)}</span><span class="tir-note">par an</span></span></label>
          {:else}
            <p class="tir-note tir-note--texte">Aucun abonnement actif repéré dans tes relevés.</p>
          {/each}
          <a class="tir-note tir-dec-lien" href={hrefs.tirelire('habitudes', 'abonnements')}>Voir tous les paiements récurrents<Icon name="droite" size={14} /></a>
        </div>
      </section>

      <section class="tir-carte tir-carte--rose tir-dec-effet" aria-label="Résultat de la simulation">
        <span class="tir-note">Avec ces réglages, tu économises</span>
        <div class="tir-g2" aria-live="polite" aria-atomic="true">
          <div class="tir-col tir-col--colle"><span class="tir-gros tir-dec-total">{euros(vue.result.monthly)}</span><span class="tir-note">par mois</span></div>
          <div class="tir-col tir-col--colle"><span class="tir-gros tir-dec-total">{euros(vue.result.yearly)}</span><span class="tir-note">par an</span></div>
        </div>
        <div class="tir-col" style:gap="8px" role="img" aria-label="Dépenses par mois : {euros(vue.reference.expenses)} d’habitude, {euros(vue.result.expenses)} simulé{vue.reference.budget === null ? '' : `, pour un budget de ${euros(vue.reference.budget)}`}">
          {#each [{ nom: 'Habituel', v: vue.reference.expenses, simule: false }, { nom: 'Simulé', v: vue.result.expenses, simule: true }] as b (b.nom)}
            {@const dans = vue.reference.budget === null ? b.v : Math.min(b.v, vue.reference.budget)}
            <div class="tir-dec-barre" title="{b.nom} : {euros(b.v)} par mois">
              <span class="tir-note">{b.nom}</span>
              <span class="tir-jg tir-grandit">
                <span class="tir-jg-plein" class:tir-dec-simule={b.simule} style:width={part(dans)}></span>
                {#if b.v > dans}<span class="tir-jg-exces" style:width={part(b.v - dans)}></span>{/if}
                {#if vue.reference.budget !== null}<span class="tir-jg-repere" style:left={part(vue.reference.budget)}></span>{/if}
              </span>
              <span class="tir-fort tir-fixe">{eurosRonds(b.v)}</span>
            </div>
          {/each}
        </div>
        <p class="tir-note tir-note--texte">{phraseBudget(vue)} {phraseEffet(vue)}</p>
        {#if tirelire.erreurSimulation}<p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />{tirelire.erreurSimulation}</p>{/if}
        {#if tirelire.erreurClassement}<p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />{tirelire.erreurClassement}</p>{/if}
        {#if info}<p class="tir-fait" role="status"><Icon name="ok" size={18} /><span class="tir-note tir-note--texte">{info}</span></p>{/if}
        <button type="button" class="tir-btn tir-btn--plein tir-btn--large" disabled={!plafonds.length || tirelire.occupe} onclick={() => void fixer()}><Icon name="cible" size={16} />{tirelire.occupe ? 'Enregistrement…' : 'Fixer comme plafond'}</button>
        <p class="tir-note">{plafonds.length ? `${pluriel(plafonds.length, 'catégorie réglée recevra son', 'catégories réglées recevront leur')} montant simulé comme plafond mensuel.` : 'Règle au moins une catégorie pour en faire un plafond. Un abonnement arrêté ne crée pas de plafond.'}</p>
      </section>
    {:else if tirelire.erreurSimulation}
      <section class="tir-carte tir-etat" role="alert">
        <span class="tir-pas tir-pas--grand tir-pas--danger"><Icon name="alerte" size={22} /></span>
        <h2 class="tir-h2">Simulateur indisponible</h2>
        <p class="tir-note tir-note--texte">{tirelire.erreurSimulation}</p>
        <button class="tir-btn tir-btn--plein" onclick={() => void tirelire.chargerSimulation()}><Icon name="actualise" size={16} />Réessayer</button>
      </section>
    {/if}
  </div>
</div>
