<script lang="ts">
  import { euros, eurosSignes } from './formats'
  import { initiale, moisCourt, moisEntier, suites, type Annees, type Cumul } from './evolution'
  import { eurosRonds, graduations, pourcentSigne, sensDe, FLECHES, type Ecart, type Mode } from './stats'
  import TirelireEcart from './TirelireEcart.svelte'
  import { teinte } from './tirelire'

  let { a, mode }: { a: Annees | null; mode: Mode } = $props()

  let largeur = $state(340)
  let survol = $state<number | null>(null)
  let boite = $state<HTMLElement>()

  $effect(() => {
    if (!boite || typeof ResizeObserver === 'undefined') return
    const o = new ResizeObserver(([e]) => { largeur = e.contentRect.width })
    o.observe(boite)
    return () => o.disconnect()
  })

  const HAUTEUR = 190
  const cadre = $derived({ largeur: Math.max(260, largeur), gauche: 10, droite: 56, haut: 22, bas: 26 })
  const actuels = $derived(a?.months.map((m) => m.current) ?? [])
  const anciens = $derived(a?.months.map((m) => m.previous) ?? [])
  const echelle = $derived(graduations(Math.max(100, ...actuels.map((v) => v ?? 0), ...anciens.map((v) => v ?? 0)), 3))
  const x = $derived((i: number) => cadre.gauche + (i / 11) * (cadre.largeur - cadre.gauche - cadre.droite))
  const y = $derived((v: number) => HAUTEUR - cadre.bas - (Math.max(0, v) / echelle.max) * (HAUTEUR - cadre.haut - cadre.bas))
  const chemin = (s: { debut: number; valeurs: number[] }) => s.valeurs.map((v, i) => `${i ? 'L' : 'M'}${x(s.debut + i).toFixed(1)} ${y(v).toFixed(1)}`).join('')
  const cumul = (c: Cumul): Ecart => ({ amount: c.previous, delta: c.delta, delta_bp: c.delta_bp })
  const ceMois = $derived(a ? moisEntier(a.month) : '')
  const tous = $derived(a ? a.compared_months.length === a.month : false)
  const etendueCumul = $derived(a ? (a.month === 1 ? 'janvier' : `janv. → ${moisCourt(a.month)}`) : '')
  const compares = $derived(a ? a.compared_months.map((m) => moisCourt(m)).join(', ') : '')
  const lignes = $derived(a ? [...a.categories.map((c) => ({ cle: String(c.id), nom: c.name, teinte: String(teinte(c)), c })), ...(a.unclassified.ytd !== 0 || a.unclassified.month !== 0 || a.unclassified.month_previous?.amount ? [{ cle: 'classer', nom: 'À classer', teinte: 'vide', c: a.unclassified }] : [])] : [])
  const restreint = $derived(!!a && a.has_previous && !tous && a.compared_months.length > 0)
  const cumulDe = (c: { ytd: number; ytd_compared: Cumul | null }) => (restreint ? (c.ytd_compared?.current ?? c.ytd) : c.ytd)
  const lecture = (i: number) => (a ? `${moisEntier(i + 1)} : ${a.year} ${actuels[i] === null ? 'aucune donnée' : euros(actuels[i]!)}${a.has_previous ? `, ${a.previous_year} ${anciens[i] === null ? 'aucune donnée' : euros(anciens[i]!)}` : ''}` : '')
  const resume = $derived(a ? `Dépenses mensuelles ${a.year}${a.has_previous ? ` et ${a.previous_year}` : ''} : ${a.months.filter((m) => m.current !== null).map((m) => `${moisCourt(m.month)} ${euros(m.current!)}`).join(', ')}.` : '')

  function viser(e: PointerEvent) {
    const b = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const part = (e.clientX - b.left - cadre.gauche) / Math.max(1, b.width - cadre.gauche - cadre.droite)
    survol = Math.min(11, Math.max(0, Math.round(part * 11)))
  }

  function clavier(e: KeyboardEvent) {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight' && e.key !== 'Escape') return
    e.preventDefault()
    survol = e.key === 'Escape' ? null : Math.min(11, Math.max(0, (survol ?? (a ? a.month - 1 : 0)) + (e.key === 'ArrowLeft' ? -1 : 1)))
  }
</script>

{#snippet ecart(e: Ecart | null, detail = false)}
  {#if e}<TirelireEcart ecart={e} />{#if detail && e.delta_bp !== null && e.delta !== 0}<span class="tir-note tir-fixe tir-apres">({eurosSignes(e.delta)})</span>{/if}{:else}<span class="tir-ec tir-ec--vide"><span class="tir-sr">pas de comparaison</span><span aria-hidden="true">—</span></span>{/if}
{/snippet}

<section class="tir-carte tir-ev-annees" aria-label="D’une année sur l’autre">
  {#if !a}
    <h2 class="tir-h2">D’une année sur l’autre</h2>
    <p class="tir-note tir-note--texte">Aucune période à comparer pour l’instant.</p>
  {:else}
    <div class="tir-ligne tir-ligne--ecart tir-ligne--haut tir-ligne--plis">
      <div class="tir-col tir-col--serre"><h2 class="tir-h2">{a.year} vs {a.previous_year}</h2><span class="tir-note">dépenses {mode === 'calendar' ? 'par mois' : 'par relevé, au mois de la date d’arrêté'}</span></div>
      {#if a.total.ytd_compared}
        {@const c = a.total.ytd_compared}
        <span class="tir-etq" class:tir-etq--danger={c.delta > 0} class:tir-etq--ok={c.delta < 0}><span aria-hidden="true">{FLECHES[sensDe(c.delta)]}</span>{c.delta_bp === null ? eurosSignes(c.delta) : pourcentSigne(c.delta_bp).replace(/^=\s*/, '')} sur {tous ? 'l’année' : `${a.compared_months.length} mois`}</span>
      {/if}
    </div>
    {#if !a.has_previous}
      <div class="tir-invite" role="note">
        <span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">Pas encore de {a.previous_year} à comparer</span><span class="tir-note">Aucun relevé importé ne couvre {a.previous_year}. La comparaison apparaîtra dès qu’un même mois existera sur les deux années ; en attendant, voici {a.year} seule.</span></span>
      </div>
    {:else if !a.total.month_previous && !a.total.ytd_compared}
      <p class="tir-note" role="note">Tes relevés de {a.previous_year} ne couvrent aucun des mois de {etendueCumul} : pas de comparaison possible pour l’instant.</p>
    {/if}
    <ul class="tir-legende" aria-label="Légende">
      <li><span class="tir-cle tir-cle--actuel" aria-hidden="true"></span>{a.year} · {eurosRonds(a.total.ytd)} ({etendueCumul})</li>
      {#if a.has_previous}<li><span class="tir-cle tir-cle--pointille" aria-hidden="true"></span>{a.previous_year}{#if a.total.ytd_compared} · {eurosRonds(a.total.ytd_compared.previous)} ({tous ? etendueCumul : `${a.compared_months.length} mois comparés`}){/if}</li>{/if}
    </ul>
    <p class="tir-sr">{resume}</p>
    <div class="tir-courbe" bind:this={boite} role="slider" tabindex="0" aria-label="Parcourir les mois de l’année" aria-valuemin={1} aria-valuemax={12} aria-valuenow={(survol ?? a.month - 1) + 1} aria-valuetext={lecture(survol ?? a.month - 1)} onpointermove={viser} onpointerdown={viser} onpointerleave={() => (survol = null)} onkeydown={clavier} onblur={() => (survol = null)}>
      <svg width={cadre.largeur} height={HAUTEUR} viewBox="0 0 {cadre.largeur} {HAUTEUR}" aria-hidden="true">
        {#each echelle.valeurs as v (v)}
          <line class="tir-grille" x1={cadre.gauche} x2={cadre.largeur - cadre.droite} y1={y(v)} y2={y(v)} />
          <text class="tir-axe" x={cadre.largeur} y={y(v) + 4} text-anchor="end">{eurosRonds(v)}</text>
        {/each}
        <line class="tir-grille tir-grille--base" x1={cadre.gauche} x2={cadre.largeur - cadre.droite} y1={y(0)} y2={y(0)} />
        {#each a.months as m, i (m.month)}<text class="tir-axe" class:tir-axe--fort={m.month === a.month} x={x(i)} y={HAUTEUR - 8} text-anchor="middle">{initiale(m.month)}</text>{/each}
        {#each suites(anciens) as s (s.debut)}
          {#if s.valeurs.length > 1}<path class="tir-serie tir-serie--annee-avant" d={chemin(s)} />{/if}
          {#each s.valeurs as v, i (i)}<circle class="tir-point tir-point--avant" cx={x(s.debut + i)} cy={y(v)} r="3" />{/each}
        {/each}
        {#each suites(actuels) as s (s.debut)}
          {#if s.valeurs.length > 1}<path class="tir-serie tir-serie--actuel" d={chemin(s)} />{/if}
          {#each s.valeurs as v, i (i)}<circle class="tir-point" cx={x(s.debut + i)} cy={y(v)} r={s.debut + i === a.month - 1 ? 4.5 : 3} />{/each}
        {/each}
        {#if actuels[a.month - 1] !== null}<text class="tir-axe tir-axe--fort" x={x(a.month - 1)} y={Math.max(12, y(actuels[a.month - 1]!) - 10)} text-anchor={a.month > 9 ? 'end' : a.month < 3 ? 'start' : 'middle'}>{eurosRonds(actuels[a.month - 1]!)}</text>{/if}
        {#if survol !== null}<line class="tir-viseur" x1={x(survol)} x2={x(survol)} y1={cadre.haut - 8} y2={y(0)} />{/if}
      </svg>
      {#if survol !== null}
        <div class="tir-bulle" style:left="{Math.min(Math.max(x(survol), 86), cadre.largeur - 86)}px">
          <span class="tir-note">{moisEntier(survol + 1)}</span>
          <span><span class="tir-cle tir-cle--actuel" aria-hidden="true"></span>{#if actuels[survol] === null}{a.year} : aucune donnée{:else}<b>{euros(actuels[survol]!)}</b> {a.year}{/if}</span>
          {#if a.has_previous}<span><span class="tir-cle tir-cle--pointille" aria-hidden="true"></span>{#if anciens[survol] === null}{a.previous_year} : aucune donnée{:else}<b>{euros(anciens[survol]!)}</b> {a.previous_year}{/if}</span>{/if}
        </div>
      {/if}
    </div>
    <dl class="tir-g2">
      <div class="tir-carte tir-carte--serre tir-carte--creux">
        <dt class="tir-note">{ceMois.replace(/^./, (c) => c.toUpperCase())} {a.year} vs {ceMois} {a.previous_year}</dt>
        <dd class="tir-val tir-fixe">{euros(a.total.month)}</dd>
        <dd>{#if a.total.month_previous}{@render ecart(a.total.month_previous, true)}<span class="tir-note tir-apres">· {eurosRonds(a.total.month_previous.amount)} en {a.previous_year}</span>{:else}<span class="tir-note">pas de {ceMois} {a.previous_year} dans tes relevés</span>{/if}</dd>
      </div>
      <div class="tir-carte tir-carte--serre tir-carte--creux">
        <dt class="tir-note">Cumul {etendueCumul} {a.year}</dt>
        <dd class="tir-val tir-fixe">{euros(a.total.ytd)}</dd>
        <dd>{#if a.total.ytd_compared}{@render ecart(cumul(a.total.ytd_compared), true)}<span class="tir-note tir-apres">· {tous ? `${eurosRonds(a.total.ytd_compared.previous)} en ${a.previous_year}` : `sur ${compares} : ${eurosRonds(a.total.ytd_compared.current)} contre ${eurosRonds(a.total.ytd_compared.previous)}`}</span>{:else}<span class="tir-note">pas de mois commun avec {a.previous_year}</span>{/if}</dd>
      </div>
    </dl>
    {#if a.has_previous && a.total.ytd_compared && !tous}<p class="tir-note" role="note">Le cumul n’est comparé que sur les mois présents dans les deux années ({compares}).</p>{/if}
    {#if a.partial}<p class="tir-note" role="note">Au moins un des mois comparés n’est couvert qu’en partie par tes relevés : la comparaison est indicative.</p>{/if}
    {#if lignes.length}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div class="tir-gr-defile" role="region" aria-label="Comparaison par catégorie, tableau à faire défiler" tabindex="0">
        <table class="tir-gr tir-gr--annees">
          <caption class="tir-sr">Dépenses par catégorie, {ceMois} et cumul {etendueCumul}, {a.year}{a.has_previous ? ` comparé à ${a.previous_year}` : ''}</caption>
          <thead>
            <tr><th scope="col">Catégorie</th><th scope="col">{moisCourt(a.month)} {a.year}</th>{#if a.has_previous}<th scope="col">{moisCourt(a.month)} {a.previous_year}</th><th scope="col">Écart</th>{/if}<th scope="col" class="tir-gr-bord">Cumul {a.year}{restreint ? ' (mois comparés)' : ''}</th>{#if a.has_previous}<th scope="col">Cumul {a.previous_year}</th><th scope="col">Écart</th>{/if}</tr>
          </thead>
          <tbody>
            {#each lignes as l (l.cle)}
              <tr>
                <th scope="row"><span class="tir-gr-nom"><span class="tir-pt tir-pt--{l.teinte}" aria-hidden="true"></span><span class="truncate">{l.nom}</span></span></th>
                <td class="tir-fort">{euros(l.c.month)}</td>
                {#if a.has_previous}<td>{l.c.month_previous ? euros(l.c.month_previous.amount) : '—'}</td><td>{@render ecart(l.c.month_previous)}</td>{/if}
                <td class="tir-fort tir-gr-bord">{euros(cumulDe(l.c))}</td>
                {#if a.has_previous}<td>{l.c.ytd_compared ? euros(l.c.ytd_compared.previous) : '—'}</td><td>{@render ecart(l.c.ytd_compared ? cumul(l.c.ytd_compared) : null)}</td>{/if}
              </tr>
            {/each}
          </tbody>
          <tfoot>
            <tr>
              <th scope="row"><span class="tir-gr-nom tir-fort">Total</span></th>
              <td class="tir-fort">{euros(a.total.month)}</td>
              {#if a.has_previous}<td>{a.total.month_previous ? euros(a.total.month_previous.amount) : '—'}</td><td>{@render ecart(a.total.month_previous)}</td>{/if}
              <td class="tir-fort tir-gr-bord">{euros(cumulDe(a.total))}</td>
              {#if a.has_previous}<td>{a.total.ytd_compared ? euros(a.total.ytd_compared.previous) : '—'}</td><td>{@render ecart(a.total.ytd_compared ? cumul(a.total.ytd_compared) : null)}</td>{/if}
            </tr>
          </tfoot>
        </table>
      </div>
    {/if}
  {/if}
</section>
