<script lang="ts">
  import { dateCourte, euros, eurosSignes, periode, pluriel } from './formats'
  import { debutsDeMois, ecartJours, echelleSolde, etiquetteMois, paliers, plusJours, soldeLe, type Soldes } from './evolution'
  import { eurosRonds } from './stats'

  let { soldes }: { soldes: Soldes } = $props()

  let largeur = $state(340)
  let survol = $state<number | null>(null)
  let boite = $state<HTMLElement>()
  let choisi = $state<number | null>(null)

  $effect(() => {
    if (!boite || typeof ResizeObserver === 'undefined') return
    const o = new ResizeObserver(([e]) => { largeur = e.contentRect.width })
    o.observe(boite)
    return () => o.disconnect()
  })

  const HAUTEUR = 220
  const c = $derived(soldes.accounts.find((x) => x.account_id === choisi) ?? soldes.accounts[0] ?? null)
  const du = $derived(soldes.from ?? '')
  const au = $derived(soldes.to ?? '')
  const jours = $derived(du && au ? ecartJours(du, au) + 1 : 1)
  const cadre = $derived({ largeur: Math.max(260, largeur), gauche: 6, droite: 56, haut: 30, bas: 26 })
  const echelle = $derived(echelleSolde(c?.min?.amount ?? 0, c?.max?.amount ?? 0))
  const x = $derived((i: number) => cadre.gauche + (i / Math.max(1, jours - 1)) * (cadre.largeur - cadre.gauche - cadre.droite))
  const y = $derived((v: number) => HAUTEUR - cadre.bas - ((v - echelle.min) / Math.max(1, echelle.max - echelle.min)) * (HAUTEUR - cadre.haut - cadre.bas))
  const rang = (jour: string) => ecartJours(du, jour)
  const traces = $derived((c?.segments ?? []).map((s) => ({ cle: s.from, seul: s.values.length === 1, x: x(rang(s.from)), y: y(s.values[0]), d: s.values.map((v, i) => (i ? `H${x(rang(s.from) + i).toFixed(1)}V${y(v).toFixed(1)}` : `M${x(rang(s.from)).toFixed(1)} ${y(v).toFixed(1)}`)).join('') })))
  const trous = $derived((c?.gaps ?? []).map((t) => { const a = x(Math.max(0, rang(t.from) - 0.5)); const b = x(Math.min(jours - 1, rang(t.to) + 0.5)); return { cle: t.from, x: a, largeur: Math.max(2, b - a) } }))
  const graduations = $derived(du && au ? debutsDeMois(du, au, Math.floor((cadre.largeur - cadre.gauche - cadre.droite) / 68)) : [])
  const plusieursAnnees = $derived(du.slice(0, 4) !== au.slice(0, 4))
  const jourDe = (i: number) => plusJours(du, i)
  const sansReleve = $derived((c?.gaps ?? []).reduce((n, t) => n + t.days, 0))
  const variation = $derived(c?.first && c.last && c.first.date !== c.last.date ? c.last.amount - c.first.amount : null)
  const ancre = (px: number) => (px < cadre.largeur * 0.3 ? 'start' : px > cadre.largeur * 0.62 ? 'end' : 'middle')
  const lecture = (i: number) => { const v = c ? soldeLe(c, jourDe(i)) : null; return `${dateCourte(jourDe(i))} : ${v === null ? 'aucun relevé ne couvre ce jour' : euros(v)}` }
  const defaut = $derived(c?.last ? rang(c.last.date) : jours - 1)
  const resume = $derived(c?.last && c.min && c.max ? `Solde du compte jour par jour, ${periode(du, au).toLowerCase()} : ${euros(c.last.amount)} le ${dateCourte(c.last.date)} ; plus bas ${euros(c.min.amount)} le ${dateCourte(c.min.date)} ; plus haut ${euros(c.max.amount)} le ${dateCourte(c.max.date)}${sansReleve ? ` ; ${pluriel(sansReleve, 'jour', 'jours')} sans relevé, non estimés` : ''}.` : '')

  function viser(e: PointerEvent) {
    const b = (e.currentTarget as HTMLElement).getBoundingClientRect()
    const part = (e.clientX - b.left - cadre.gauche) / Math.max(1, b.width - cadre.gauche - cadre.droite)
    survol = Math.min(jours - 1, Math.max(0, Math.round(part * (jours - 1))))
  }

  function clavier(e: KeyboardEvent) {
    const pas: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1, PageDown: -30, PageUp: 30 }
    if (!(e.key in pas) && e.key !== 'Escape' && e.key !== 'Home' && e.key !== 'End') return
    e.preventDefault()
    survol = e.key === 'Escape' ? null : e.key === 'Home' ? 0 : e.key === 'End' ? jours - 1 : Math.min(jours - 1, Math.max(0, (survol ?? defaut) + pas[e.key]))
  }
</script>

<section class="tir-carte tir-ev-solde" aria-label="Solde du compte">
  <div class="tir-ligne tir-ligne--ecart tir-ligne--haut">
    <div class="tir-col tir-col--serre"><h2 class="tir-h2">Solde du compte</h2><span class="tir-note">jour par jour, d’après les soldes de tes relevés</span></div>
    {#if c?.last}<div class="tir-col tir-col--colle" style:align-items="flex-end"><span class="tir-val tir-fixe">{euros(c.last.amount)}</span><span class="tir-note tir-fixe">au {dateCourte(c.last.date)}</span></div>{/if}
  </div>
  {#if soldes.accounts.length > 1}
    <label class="tir-champ"><span class="tir-sr">Compte affiché</span><select value={c?.account_id} onchange={(e) => { choisi = Number(e.currentTarget.value); survol = null }}>{#each soldes.accounts as a, i (a.account_id)}<option value={a.account_id}>Compte {i + 1}</option>{/each}</select></label>
  {/if}
  {#if !c || !c.last || !c.min || !c.max}
    <p class="tir-note tir-note--texte">Aucun relevé ne couvre cette période : pas de solde à montrer.</p>
  {:else}
    <p class="tir-sr">{resume}</p>
    <div class="tir-courbe" bind:this={boite} role="slider" tabindex="0" aria-label="Parcourir les jours du solde" aria-valuemin={0} aria-valuemax={jours - 1} aria-valuenow={survol ?? defaut} aria-valuetext={lecture(survol ?? defaut)} onpointermove={viser} onpointerdown={viser} onpointerleave={() => (survol = null)} onkeydown={clavier} onblur={() => (survol = null)}>
      <svg width={cadre.largeur} height={HAUTEUR} viewBox="0 0 {cadre.largeur} {HAUTEUR}" aria-hidden="true">
        <defs><pattern id="tir-hachures" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><line class="tir-hachure" x1="0" y1="0" x2="0" y2="6" /></pattern></defs>
        {#each echelle.valeurs as v (v)}
          <line class="tir-grille" class:tir-grille--base={v === 0} x1={cadre.gauche} x2={cadre.largeur - cadre.droite} y1={y(v)} y2={y(v)} />
          <text class="tir-axe" x={cadre.largeur} y={y(v) + 4} text-anchor="end">{eurosRonds(v)}</text>
        {/each}
        {#each trous as t (t.cle)}
          <rect class="tir-trou" x={t.x} y={cadre.haut - 8} width={t.largeur} height={HAUTEUR - cadre.bas - cadre.haut + 8} />
          {#if t.largeur > 84}<text class="tir-axe" x={t.x + t.largeur / 2} y={cadre.haut + 6} text-anchor="middle">pas de relevé</text>{/if}
        {/each}
        {#each graduations as g (g)}
          <text class="tir-axe" x={x(rang(g))} y={HAUTEUR - 8} text-anchor={rang(g) === 0 ? 'start' : 'middle'}>{etiquetteMois(g, plusieursAnnees && (g.slice(5, 7) === '01' || g === graduations[0]))}</text>
        {/each}
        {#each traces as t (t.cle)}
          {#if t.seul}<circle class="tir-point" cx={t.x} cy={t.y} r="3" />{:else}<path class="tir-serie tir-serie--actuel" d={t.d} />{/if}
        {/each}
        {#if c.min.amount !== c.max.amount}
          <circle class="tir-point tir-point--creux" cx={x(rang(c.min.date))} cy={y(c.min.amount)} r="4" />
          <text class="tir-axe tir-axe--fort" x={x(rang(c.min.date))} y={Math.min(HAUTEUR - cadre.bas - 4, y(c.min.amount) + 17)} text-anchor={ancre(x(rang(c.min.date)))}>bas {eurosRonds(c.min.amount)}</text>
          <circle class="tir-point tir-point--creux" cx={x(rang(c.max.date))} cy={y(c.max.amount)} r="4" />
          <text class="tir-axe tir-axe--fort" x={x(rang(c.max.date))} y={y(c.max.amount) - 9} text-anchor={ancre(x(rang(c.max.date)))}>haut {eurosRonds(c.max.amount)}</text>
        {/if}
        <circle class="tir-point" cx={x(rang(c.last.date))} cy={y(c.last.amount)} r="4" />
        {#if survol !== null}<line class="tir-viseur" x1={x(survol)} x2={x(survol)} y1={cadre.haut - 8} y2={HAUTEUR - cadre.bas} />{/if}
      </svg>
      {#if survol !== null}
        {@const v = soldeLe(c, jourDe(survol))}
        <div class="tir-bulle" style:left="{Math.min(Math.max(x(survol), 86), cadre.largeur - 86)}px">
          <span class="tir-note">{dateCourte(jourDe(survol))}</span>
          {#if v === null}<span>pas de relevé ce jour</span>{:else}<span><span class="tir-cle tir-cle--actuel" aria-hidden="true"></span><b>{euros(v)}</b></span>{/if}
        </div>
      {/if}
    </div>
    <dl class="tir-g2">
      <div class="tir-carte tir-carte--serre tir-carte--creux"><dt class="tir-note">Solde le plus bas</dt><dd class="tir-val tir-fixe" class:tir-val--mal={c.min.amount < 0}>{euros(c.min.amount)}</dd><dd class="tir-note">le {dateCourte(c.min.date)}</dd></div>
      <div class="tir-carte tir-carte--serre tir-carte--creux"><dt class="tir-note">Solde le plus haut</dt><dd class="tir-val tir-fixe">{euros(c.max.amount)}</dd><dd class="tir-note">le {dateCourte(c.max.date)}</dd></div>
    </dl>
    {#if variation !== null && c.first}<p class="tir-note">{variation === 0 ? 'Solde inchangé' : `${eurosSignes(variation)}`} entre le {dateCourte(c.first.date)} et le {dateCourte(c.last.date)}.</p>{/if}
    {#if sansReleve}
      <p class="tir-note"><span class="tir-carre tir-carre--trou" aria-hidden="true"></span> {pluriel(sansReleve, 'jour', 'jours')} sans relevé ({c.gaps.slice(0, 3).map((t) => periode(t.from, t.to).toLowerCase()).join(' ; ')}{c.gaps.length > 3 ? ` et ${c.gaps.length - 3} autres` : ''}) : la courbe s’interrompt, rien n’est estimé.</p>
    {/if}
    {#if !c.reconciled}<p class="tir-note" role="note"><span class="tir-etq tir-etq--attente">à vérifier</span> Le solde recalculé ne retombe pas sur le nouveau solde d’au moins un relevé.</p>{/if}
    <details class="tir-donnees">
      <summary>Tableau des données</summary>
      <div class="tir-donnees-defile">
        <table class="tir-tab tir-tab--simple">
          <caption class="tir-sr">Solde du compte à chaque jour où il change</caption>
          <thead><tr><th scope="col">Jour</th><th scope="col">Solde</th></tr></thead>
          <tbody>
            {#each paliers(c) as p (p.date)}
              <tr><th scope="row">{p.fin && p.fin !== p.date ? periode(p.date, p.fin) : dateCourte(p.date)}</th><td>{p.amount === null ? 'pas de relevé' : euros(p.amount)}</td></tr>
            {/each}
          </tbody>
        </table>
      </div>
    </details>
  {/if}
</section>
