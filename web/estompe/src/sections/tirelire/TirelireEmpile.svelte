<script lang="ts">
  import { hrefs } from '../../lib/route.svelte'
  import { euros } from './formats'
  import { arrondi, colonne, etendue, initiale, nomColonne, tranches, type Historique } from './evolution'
  import { eurosRonds, graduations, unite } from './stats'
  import { teinte } from './tirelire'

  let { h }: { h: Historique } = $props()

  let vise = $state<number | null>(null)

  const n = $derived(h.periods.length)
  const k = $derived(vise !== null && vise < n ? vise : n - 1)
  const budget = $derived(h.total.budget)
  const echelle = $derived(graduations(Math.max(...h.total.amounts, budget ?? 0, 100), 3))
  const haut = (v: number) => (Math.max(0, v) / echelle.max) * 100
  const legende = $derived(tranches(h, 0, teinte))
  const piles = $derived(h.periods.map((p, i) => ({ p, total: h.total.amounts[i], parts: tranches(h, i, teinte).filter((t) => t.montant > 0) })))
  const partiel = $derived(h.periods.some((p) => !p.complete))
  const lecture = (i: number) => `${nomColonne(h.periods[i], h.mode)}${h.periods[i].complete ? '' : ' (incomplet)'} : ${euros(h.total.amounts[i])}${piles[i].parts.length ? ` ; ${piles[i].parts.map((t) => `${t.nom} ${euros(t.montant)}`).join(', ')}` : ''}`
  const resume = $derived(n ? `Dépenses par catégorie sur ${n} ${unite(h.mode, n)}, de ${euros(Math.min(...h.total.amounts))} à ${euros(Math.max(...h.total.amounts))}${budget !== null ? ` ; budget ${euros(budget)}` : ''}. Le tableau « Catégories × ${unite(h.mode, 2)} » donne tous les montants.` : '')

  function viser(e: PointerEvent) {
    const b = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const style = getComputedStyle(e.currentTarget as HTMLElement)
    const gauche = b.left + parseFloat(style.paddingLeft || '0')
    vise = Math.min(n - 1, Math.max(0, Math.floor(((e.clientX - gauche) / Math.max(1, b.right - gauche)) * n)))
  }

  function clavier(e: KeyboardEvent) {
    const pas: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1 }
    if (!(e.key in pas) && e.key !== 'Home' && e.key !== 'End') return
    e.preventDefault()
    vise = e.key === 'Home' ? 0 : e.key === 'End' ? n - 1 : Math.min(n - 1, Math.max(0, k + pas[e.key]))
  }

  const mois = (i: number) => Number((h.mode === 'calendar' ? h.periods[i].from : h.periods[i].to).slice(5, 7))
</script>

<section class="tir-carte tir-ev-graphe" aria-label="Dépenses par catégorie">
  <div class="tir-ligne tir-ligne--ecart tir-ligne--plis"><h2 class="tir-h2">Dépenses par catégorie</h2><span class="tir-note">{etendue(h.periods, h.mode)}</span></div>
  <ul class="tir-legende" aria-label="Légende">
    {#each legende as t (t.cle)}<li><span class="tir-carre tir-emp--{t.teinte}" aria-hidden="true"></span>{t.nom}</li>{/each}
    {#if budget !== null}<li><span class="tir-cle tir-cle--budget" aria-hidden="true"></span>Budget · {eurosRonds(budget)}</li>{/if}
    {#if partiel}<li><span class="tir-carre tir-emp--partiel" aria-hidden="true"></span>{h.mode === 'calendar' ? 'Mois incomplet' : 'Période incomplète'}</li>{/if}
  </ul>
  <p class="tir-sr">{resume}</p>
  <div class="tir-emp" class:tir-emp--dense={n > 6} class:tir-emp--serre={n > 12}>
    <div class="tir-emp-piles" role="slider" tabindex="0" aria-label="Parcourir les {unite(h.mode, 2)} du graphique" aria-valuemin={1} aria-valuemax={n} aria-valuenow={k + 1} aria-valuetext={n ? lecture(k) : ''} onpointermove={viser} onpointerdown={viser} onkeydown={clavier}>
      {#each echelle.valeurs as v (v)}<span class="tir-emp-grille" style:bottom="{haut(v)}%"><span>{eurosRonds(v)}</span></span>{/each}
      {#if budget !== null}<span class="tir-emp-budget" style:bottom="{haut(budget)}%"></span>{/if}
      {#each piles as pile, i (pile.p.id)}
        <span class="tir-emp-col" class:tir-emp-col--vise={i === k}>
          <span class="tir-emp-valeur">{arrondi(pile.total)}</span>
          <span class="tir-emp-pile" class:tir-emp-pile--partiel={!pile.p.complete} style:height="{haut(pile.total)}%">
            {#each pile.parts as t (t.cle)}<span class="tir-emp-part tir-emp--{t.teinte}" style:flex-grow={t.montant}></span>{/each}
          </span>
        </span>
      {/each}
    </div>
  </div>
  <div class="tir-histo-axe tir-emp-axe" class:tir-emp--dense={n > 6} class:tir-emp--serre={n > 12} aria-hidden="true">
    {#each h.periods as p, i (p.id)}<span class:tir-fort={i === k}><span class="tir-emp-long">{colonne(p, h.mode)}</span><span class="tir-emp-court">{initiale(mois(i))}</span></span>{/each}
  </div>
  {#if n}
    <div class="tir-emp-lecture" role="status">
      <div class="tir-ligne tir-ligne--ecart tir-ligne--plis">
        <span class="tir-h3">{nomColonne(h.periods[k], h.mode).replace(/^./, (c) => c.toUpperCase())}{h.periods[k].complete ? '' : h.periods[k].open ? ' · en cours' : ' · incomplet'}</span>
        <span class="tir-ligne" style:gap="12px"><span class="tir-val tir-fixe">{euros(h.total.amounts[k])}</span><a class="tir-btn tir-btn--contour" href={hrefs.tirelire('mois', h.periods[k].id)}>{h.mode === 'calendar' ? 'Voir le mois' : 'Voir le relevé'}</a></span>
      </div>
      {#if piles[k].parts.length}
        <ul class="tir-emp-detail">
          {#each piles[k].parts as t (t.cle)}<li><span class="tir-carre tir-emp--{t.teinte}" aria-hidden="true"></span><span class="tir-grandit truncate">{t.nom}</span><span class="tir-fort tir-fixe">{euros(t.montant)}</span></li>{/each}
        </ul>
      {:else}
        <p class="tir-note">Aucune dépense sur cette période.</p>
      {/if}
    </div>
  {/if}
</section>
