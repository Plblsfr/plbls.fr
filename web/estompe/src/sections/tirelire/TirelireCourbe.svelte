<script lang="ts">
  import { dateCourte, euros } from './formats'
  import { abscisse, eurosRonds, graduations, joursGradues, moisDe, ordonnee, trace, unite, type Cadre, type Mois } from './stats'

  let { mois, titre = 'Dépenses cumulées' }: { mois: Mois; titre?: string } = $props()

  let largeur = $state(340)
  let survol = $state<number | null>(null)
  let boite = $state<HTMLElement>()

  $effect(() => {
    if (!boite || typeof ResizeObserver === 'undefined') return
    const o = new ResizeObserver(([e]) => { largeur = e.contentRect.width })
    o.observe(boite)
    return () => o.disconnect()
  })

  const HAUTEUR = 210
  const jours = $derived(mois.period.days)
  const actuel = $derived(mois.cumulative.current)
  const avant = $derived(mois.cumulative.previous)
  const moyenne = $derived(mois.cumulative.average)
  const budget = $derived(mois.budget?.total ?? null)
  const projection = $derived(mois.projection?.total ?? null)
  const calendaire = $derived(mois.mode === 'calendar')
  const nom = $derived(calendaire ? moisDe(mois.period.id) : 'ce relevé')
  const nomAvant = $derived(avant ? (calendaire ? moisDe(avant.id) : 'relevé précédent') : '')
  const cadre = $derived<Cadre>({ largeur: Math.max(260, largeur), hauteur: HAUTEUR, gauche: 6, droite: 52, haut: 26, bas: 26 })
  const echelle = $derived(graduations(Math.max(...actuel, ...(avant?.values ?? []), ...(moyenne?.values ?? []), budget ?? 0, projection ?? 0)))
  const x = $derived(abscisse(cadre, jours))
  const y = $derived(ordonnee(cadre, echelle.max))
  const dernier = $derived(actuel.length)
  const total = $derived(actuel.at(-1) ?? 0)
  const jour = (n: number) => { const d = new Date(`${mois.period.from}T00:00:00Z`); d.setUTCDate(d.getUTCDate() + n - (calendaire ? 1 : 0)); return d.toISOString().slice(0, 10) }
  const etiquette = (n: number) => (calendaire ? String(n) : `J${n}`)
  const resume = $derived(`${titre} jour par jour, ${nom} : ${euros(total)} au jour ${dernier} sur ${jours}${avant ? ` ; ${nomAvant} : ${euros(avant.values.at(-1) ?? 0)}` : ''}${moyenne ? ` ; moyenne de ${moyenne.periods} ${unite(mois.mode, moyenne.periods)} : ${euros(moyenne.values.at(-1) ?? 0)}` : ''}${budget !== null ? ` ; budget : ${euros(budget)}` : ''}${projection !== null ? ` ; projection : ${euros(projection)}` : ''}.`)
  const aDroite = $derived(dernier > jours * 0.6)
  const lecture = (n: number) => [`${dateCourte(jour(n))}`, n <= dernier ? `${nom} ${euros(actuel[n - 1])}` : '', avant ? `${nomAvant} ${euros(avant.values[n - 1])}` : '', moyenne ? `moyenne ${euros(moyenne.values[n - 1])}` : ''].filter(Boolean).join(', ')

  function viser(e: PointerEvent) {
    const boite = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const part = (e.clientX - boite.left - cadre.gauche) / Math.max(1, boite.width - cadre.gauche - cadre.droite)
    survol = Math.min(jours, Math.max(1, Math.round(part * (jours - 1)) + 1))
  }

  function clavier(e: KeyboardEvent) {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight' && e.key !== 'Escape') return
    e.preventDefault()
    survol = e.key === 'Escape' ? null : Math.min(jours, Math.max(1, (survol ?? dernier) + (e.key === 'ArrowLeft' ? -1 : 1)))
  }
</script>

<section class="tir-carte" aria-label={titre}>
  <div class="tir-ligne tir-ligne--ecart"><h2 class="tir-h2">{titre}</h2><span class="tir-note">jour par jour</span></div>
  <ul class="tir-legende" aria-label="Légende">
    <li><span class="tir-cle tir-cle--actuel" aria-hidden="true"></span>{nom.charAt(0).toUpperCase() + nom.slice(1)} · {eurosRonds(total)}</li>
    {#if avant}<li><span class="tir-cle tir-cle--avant" aria-hidden="true"></span>{nomAvant.charAt(0).toUpperCase() + nomAvant.slice(1)} · {eurosRonds(avant.values.at(-1) ?? 0)}</li>{/if}
    {#if moyenne}<li><span class="tir-cle tir-cle--moyenne" aria-hidden="true"></span>Moyenne {moyenne.periods} {unite(mois.mode, moyenne.periods)} · {eurosRonds(moyenne.values.at(-1) ?? 0)}</li>{/if}
    {#if budget !== null}<li><span class="tir-cle tir-cle--budget" aria-hidden="true"></span>Budget · {eurosRonds(budget)}</li>{/if}
    {#if projection !== null}<li><span class="tir-cle tir-cle--projection" aria-hidden="true"></span>Projection · {eurosRonds(projection)}</li>{/if}
  </ul>
  <p class="tir-sr">{resume}</p>
  <div class="tir-courbe" bind:this={boite} role="slider" tabindex="0" aria-label="Parcourir les jours de la courbe" aria-valuemin={1} aria-valuemax={jours} aria-valuenow={survol ?? dernier} aria-valuetext={lecture(survol ?? dernier)} onpointermove={viser} onpointerdown={viser} onpointerleave={() => (survol = null)} onkeydown={clavier} onblur={() => (survol = null)}>
    <svg width={cadre.largeur} height={HAUTEUR} viewBox="0 0 {cadre.largeur} {HAUTEUR}" aria-hidden="true">
      {#each echelle.valeurs as v (v)}
        <line class="tir-grille" x1={cadre.gauche} x2={cadre.largeur - cadre.droite} y1={y(v)} y2={y(v)} />
        <text class="tir-axe" x={cadre.largeur} y={y(v) + 4} text-anchor="end">{eurosRonds(v)}</text>
      {/each}
      <line class="tir-grille tir-grille--base" x1={cadre.gauche} x2={cadre.largeur - cadre.droite} y1={y(0)} y2={y(0)} />
      {#each joursGradues(jours, dernier < jours ? dernier : undefined) as j (j)}
        <text class="tir-axe" class:tir-axe--fort={j === dernier && dernier < jours} x={x(j)} y={HAUTEUR - 8} text-anchor={j === 1 ? 'start' : j === jours ? 'end' : 'middle'}>{etiquette(j)}</text>
      {/each}
      {#if budget !== null}
        <line class="tir-trait-budget" x1={cadre.gauche} x2={cadre.largeur - cadre.droite} y1={y(budget)} y2={y(budget)} />
      {/if}
      {#if moyenne}<path class="tir-serie tir-serie--moyenne" d={trace(moyenne.values, x, y)} />{/if}
      {#if avant}<path class="tir-serie tir-serie--avant" d={trace(avant.values, x, y)} />{/if}
      {#if projection !== null}
        <line class="tir-serie tir-serie--projection" x1={x(dernier)} y1={y(total)} x2={x(jours)} y2={y(projection)} />
        <circle class="tir-point tir-point--creux" cx={x(jours)} cy={y(projection)} r="4" />
        <text class="tir-axe" x={x(jours)} y={y(projection) - 10} text-anchor="end">≈ {eurosRonds(projection)}</text>
      {/if}
      <path class="tir-serie tir-serie--actuel" d={trace(actuel, x, y)} />
      <circle class="tir-point" cx={x(dernier)} cy={y(total)} r="4" />
      <text class="tir-axe tir-axe--fort" x={x(dernier) + (aDroite ? -8 : 8)} y={Math.max(12, y(total) - 9)} text-anchor={aDroite ? 'end' : 'start'}>{eurosRonds(total)}</text>
      {#if survol !== null}<line class="tir-viseur" x1={x(survol)} x2={x(survol)} y1={cadre.haut - 8} y2={y(0)} />{/if}
    </svg>
    {#if survol !== null}
      <div class="tir-bulle" style:left="{Math.min(Math.max(x(survol), 86), cadre.largeur - 86)}px">
        <span class="tir-note">{calendaire ? dateCourte(jour(survol)) : `Jour ${survol} · ${dateCourte(jour(survol))}`}</span>
        {#if survol <= dernier}<span><span class="tir-cle tir-cle--actuel" aria-hidden="true"></span><b>{euros(actuel[survol - 1])}</b> {nom}</span>{/if}
        {#if avant}<span><span class="tir-cle tir-cle--avant" aria-hidden="true"></span><b>{euros(avant.values[survol - 1])}</b> {nomAvant}</span>{/if}
        {#if moyenne}<span><span class="tir-cle tir-cle--moyenne" aria-hidden="true"></span><b>{euros(moyenne.values[survol - 1])}</b> moyenne</span>{/if}
      </div>
    {/if}
  </div>
  <details class="tir-donnees">
    <summary>Tableau des données</summary>
    <div class="tir-donnees-defile">
      <table class="tir-tab tir-tab--simple">
        <caption class="tir-sr">{titre}, en euros, jour par jour</caption>
        <thead><tr><th scope="col">Jour</th><th scope="col">{nom}</th>{#if avant}<th scope="col">{nomAvant}</th>{/if}{#if moyenne}<th scope="col">moyenne</th>{/if}</tr></thead>
        <tbody>
          {#each { length: jours } as _, i (i)}
            <tr><th scope="row">{dateCourte(jour(i + 1))}</th><td>{i < dernier ? euros(actuel[i]) : '—'}</td>{#if avant}<td>{euros(avant.values[i])}</td>{/if}{#if moyenne}<td>{euros(moyenne.values[i])}</td>{/if}</tr>
          {/each}
        </tbody>
      </table>
    </div>
  </details>
</section>
