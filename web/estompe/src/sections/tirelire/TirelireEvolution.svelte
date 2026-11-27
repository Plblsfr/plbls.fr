<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { euros, pluriel } from './formats'
  import { annees, arrondi, colonne, compte, detailTendance, etendue, etiquetteTendance, FENETRES, FLECHES_DIRECTION, nomColonne, nomFenetre, type Direction, type Evolution, type Tendance } from './evolution'
  import { eurosRonds, MODES, pourcentSigne, unite, type Mode } from './stats'
  import TirelireAnnees from './TirelireAnnees.svelte'
  import TirelireEmpile from './TirelireEmpile.svelte'
  import TirelireEtincelle from './TirelireEtincelle.svelte'
  import TirelireSolde from './TirelireSolde.svelte'
  import { NATURES, teinte } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let { evolution }: { evolution: Evolution } = $props()

  let boite = $state<HTMLElement>()
  let debordant = $state(true)

  const h = $derived(evolution.historique)
  const mode = $derived<Mode>(h.mode)
  const n = $derived(h.periods.length)
  const aJour = $derived(h.mode === tirelire.mode && h.window === tirelire.fenetre)
  const ans = $derived(annees(h.periods, mode))
  const derniere = $derived(h.periods.at(-1))
  const sansPlafond = $derived(h.total.budget === null)
  const aClasser = $derived(h.unclassified.amounts.some((m) => m !== 0))
  const dessus = $derived(h.categories.reduce((s, c) => s + (c.over_cap ?? 0), 0))
  const tendances = $derived(h.categories.filter((c) => c.trend.values.some((v) => v !== 0)))
  const comptes = $derived(compte(tendances))
  const suffisant = $derived(h.total.trend.direction !== 'insufficient')
  const reference = $derived(h.trend_period_ids.length)
  const dernierComplet = $derived([...h.periods.keys()].reverse().find((k) => h.periods[k].complete))
  const ecartMoyenne = $derived(h.total.average && dernierComplet !== undefined && h.complete > 1 ? Math.round(((h.total.amounts[dernierComplet] - h.total.average) / h.total.average) * 10_000) : null)
  const lien = (id: number) => hrefs.tirelire('categorie', id, derniere ? `periode=${derniere.id}` : '')
  const classe = (d: Direction) => (d === 'up' ? 'tir-etq--danger' : d === 'down' ? 'tir-etq--ok' : '')

  $effect(() => {
    if (!boite || typeof ResizeObserver === 'undefined') return
    const mesurer = () => { if (boite) debordant = boite.scrollWidth > boite.clientWidth + 1 }
    const o = new ResizeObserver(mesurer)
    o.observe(boite)
    if (boite.firstElementChild) o.observe(boite.firstElementChild)
    return () => o.disconnect()
  })

  const titreTable = $derived(`Catégories × ${unite(mode, 2)}`)
</script>

{#snippet etiquette(t: Tendance)}
  <span class="tir-etq {classe(t.direction)}">{#if FLECHES_DIRECTION[t.direction]}<span aria-hidden="true">{FLECHES_DIRECTION[t.direction]}</span>{/if}{etiquetteTendance(t)}</span>
{/snippet}

{#snippet cellule(montant: number, plafond: number | null, k: number)}
  <td class:tir-gr-dessus={plafond !== null && montant > plafond} class:tir-gr-actuel={k === n - 1}>{#if plafond !== null && montant > plafond}<span class="tir-gr-marque" aria-hidden="true">▲</span><span class="tir-sr">au-dessus du plafond : </span>{/if}{arrondi(montant)}</td>
{/snippet}

<div class="tir-ev" style:opacity={aJour ? undefined : 0.6} aria-busy={!aJour}>
  <section class="tir-carte tir-carte--serre tir-ev-reglages" aria-label="Fenêtre et découpage">
    <div class="tir-seg" role="group" aria-label="Nombre de périodes affichées">
      {#each FENETRES as f (f)}<button type="button" aria-pressed={tirelire.fenetre === f} onclick={() => tirelire.changerFenetre(f)}>{nomFenetre(f, tirelire.mode)}</button>{/each}
    </div>
    <div class="tir-seg" role="group" aria-label="Découpage des périodes">
      {#each MODES as m (m.id)}<button type="button" aria-pressed={tirelire.mode === m.id} onclick={() => tirelire.changerMode(m.id)}>{m.label}</button>{/each}
    </div>
    <p class="tir-note">{n < h.available ? `${n} ${unite(mode, n)} sur ${h.available}` : pluriel(n, mode === 'calendar' ? 'mois disponible' : 'relevé disponible', mode === 'calendar' ? 'mois disponibles' : 'relevés disponibles')} · {etendue(h.periods, mode)}{h.complete < n ? ` · ${h.complete} ${h.complete > 1 ? 'complets' : 'complet'}` : ''}</p>
  </section>

  {#if n === 1}
    <p class="tir-invite tir-ev-large" role="note"><span class="tir-pas"><Icon name="tendance" size={18} /></span><span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">Une seule période pour l’instant</span><span class="tir-note">L’évolution se dessinera avec {mode === 'calendar' ? 'un deuxième mois' : 'un deuxième relevé'} ; les tendances demandent 3 {unite(mode, 3)} complets.</span></span></p>
  {/if}

  <TirelireEmpile {h} />

  <div class="tir-ev-tuiles">
    <section class="tir-carte tir-carte--serre" aria-label="Moyenne des dépenses">
      <span class="tir-note">Moyenne{h.complete ? ` sur ${h.complete} ${unite(mode, h.complete)} ${h.complete > 1 ? 'complets' : 'complet'}` : ''}</span>
      {#if h.total.average !== null}
        <span class="tir-val tir-fixe">{euros(h.total.average)}</span>
        <span class="tir-note">{#if ecartMoyenne !== null && dernierComplet !== undefined}{colonne(h.periods[dernierComplet], mode)} : <span class="tir-ec" class:tir-ec--mal={ecartMoyenne >= 50} class:tir-ec--bien={ecartMoyenne <= -50}><span aria-hidden="true">{ecartMoyenne >= 50 ? '▲' : ecartMoyenne <= -50 ? '▼' : '='}</span>{pourcentSigne(ecartMoyenne).replace(/^=\s*/, '')}</span>{:else}total {eurosRonds(h.total.total)}{/if}</span>
      {:else}
        <span class="tir-val tir-val--vide">—</span>
        <span class="tir-note">aucune période complète dans la fenêtre</span>
      {/if}
    </section>
    <section class="tir-carte tir-carte--serre" aria-label="Périodes au-dessus du budget">
      <span class="tir-note">{mode === 'calendar' ? 'Mois' : 'Relevés'} au-dessus du budget</span>
      {#if h.total.budget !== null && h.total.over_budget !== null}
        <span class="tir-val tir-fixe">{h.total.over_budget} sur {n}</span>
        <span class="tir-note">budget {eurosRonds(h.total.budget)} par mois</span>
      {:else}
        <span class="tir-val tir-val--vide">—</span>
        <span class="tir-note">aucun plafond fixé</span>
      {/if}
    </section>
    <section class="tir-carte tir-carte--serre tir-ev-tuile-tendance" aria-label="Tendance des dépenses">
      <span class="tir-note">Tendance des dépenses</span>
      <span>{@render etiquette(h.total.trend)}</span>
      <span class="tir-note">{detailTendance(h.total.trend, mode)}</span>
    </section>
  </div>

  <section class="tir-carte tir-ev-large" aria-label={titreTable}>
    <div class="tir-ligne tir-ligne--ecart tir-ligne--plis">
      <div class="tir-col tir-col--serre"><h2 class="tir-h2">{titreTable}</h2><span class="tir-note">dépenses en €, arrondies · moyenne des {unite(mode, 2)} complets</span></div>
      {#if !sansPlafond}<span class="tir-note tir-ligne" style:gap="6px"><span class="tir-etq tir-etq--danger"><span aria-hidden="true">▲</span>{dessus}</span>{dessus > 1 ? 'montants' : 'montant'} au-dessus du plafond</span>{/if}
    </div>
    {#if !h.categories.length && !aClasser}
      <p class="tir-note tir-note--texte">Aucune dépense sur cette fenêtre.</p>
    {:else}
      {#if !h.categories.length}<p class="tir-note tir-note--texte">Rien n’est classé : tout est dans « À classer ». Classe tes opérations pour voir les catégories.</p>{/if}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div class="tir-gr-defile" role="region" aria-label="{titreTable}, tableau à faire défiler" tabindex="0" bind:this={boite}>
        <table class="tir-gr">
          <caption class="tir-sr">Dépenses par catégorie et par {unite(mode)}, {etendue(h.periods, mode)}</caption>
          <thead>
            <tr>
              <th scope="col">Catégorie</th>
              {#each h.periods as p, k (p.id)}<th scope="col" class:tir-gr-actuel={k === n - 1} title={nomColonne(p, mode)}><span class="tir-gr-tete"><span>{colonne(p, mode)}{p.complete ? '' : '*'}</span><span class="tir-gr-an">{ans[k]}</span></span></th>{/each}
              <th scope="col" class="tir-gr-bord">Total</th><th scope="col">Moy.</th><th scope="col">Plafond</th><th scope="col" title="Nombre de périodes au-dessus du plafond">Dépass.<span class="tir-sr"> : nombre de périodes au-dessus du plafond</span></th>
            </tr>
          </thead>
          <tbody>
            {#each h.categories as c (c.id)}
              <tr>
                <th scope="row"><a class="tir-gr-nom" href={lien(c.id)}><span class="tir-pt tir-pt--{teinte(c)}" aria-hidden="true"></span><span class="truncate">{c.name}</span>{#if c.kind === 'fixe'}<span class="tir-gr-nature">{NATURES[c.kind]}</span>{/if}</a></th>
                {#each c.amounts as m, k (h.periods[k].id)}{@render cellule(m, c.monthly_cap, k)}{/each}
                <td class="tir-fort tir-gr-bord">{arrondi(c.total)}</td>
                <td>{c.average === null ? '—' : arrondi(c.average)}</td>
                <td>{#if c.monthly_cap === null}<span class="tir-sr">pas de plafond</span><span aria-hidden="true">—</span>{:else}{arrondi(c.monthly_cap)}{/if}</td>
                <td>{#if c.over_cap === null}<span class="tir-sr">pas de plafond</span><span aria-hidden="true">—</span>{:else}<span class:tir-gr-compte={c.over_cap > 0}>{c.over_cap}<span class="tir-sr"> {unite(mode, c.over_cap)}</span> / {n}</span>{/if}</td>
              </tr>
            {/each}
            {#if aClasser}
              <tr>
                <th scope="row"><a class="tir-gr-nom" href={hrefs.tirelire('a-classer')}><span class="tir-pt tir-pt--vide" aria-hidden="true"></span><span class="truncate">À classer</span></a></th>
                {#each h.unclassified.amounts as m, k (h.periods[k].id)}{@render cellule(m, null, k)}{/each}
                <td class="tir-fort tir-gr-bord">{arrondi(h.unclassified.total)}</td>
                <td>{h.unclassified.average === null ? '—' : arrondi(h.unclassified.average)}</td>
                <td><span aria-hidden="true">—</span></td><td><span aria-hidden="true">—</span></td>
              </tr>
            {/if}
          </tbody>
          <tfoot>
            <tr>
              <th scope="row"><span class="tir-gr-nom tir-fort">Total</span></th>
              {#each h.total.amounts as m, k (h.periods[k].id)}{@render cellule(m, h.total.budget, k)}{/each}
              <td class="tir-fort tir-gr-bord">{arrondi(h.total.total)}</td>
              <td class="tir-fort">{h.total.average === null ? '—' : arrondi(h.total.average)}</td>
              <td class="tir-fort">{#if h.total.budget === null}<span class="tir-sr">pas de budget</span><span aria-hidden="true">—</span>{:else}{arrondi(h.total.budget)}{/if}</td>
              <td>{#if h.total.over_budget === null}<span aria-hidden="true">—</span>{:else}<span class:tir-gr-compte={h.total.over_budget > 0}>{h.total.over_budget}<span class="tir-sr"> {unite(mode, h.total.over_budget)}</span> / {n}</span>{/if}</td>
            </tr>
          </tfoot>
        </table>
      </div>
      <p class="tir-note">{[sansPlafond ? '' : '▲ et fond coloré : montant au-dessus du plafond de la catégorie (du budget pour le total) ; « Dépass. » compte ces périodes.', h.complete < n ? '* période incomplète, hors moyenne.' : '', debordant ? 'Fais défiler le tableau vers la droite pour voir toutes les colonnes.' : '', `Les achats par carte comptent à leur date d’achat${mode === 'calendar' ? '' : ', dans le relevé qui les porte'}.`].filter(Boolean).join(' ')}</p>
      {#if sansPlafond}<a class="tir-invite" href={hrefs.tirelire('categories')}><span class="tir-pas"><Icon name="cible" size={18} /></span><span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">Aucun plafond fixé</span><span class="tir-note">Les colonnes « Plafond » et « Dépass. » restent vides tant qu’aucune catégorie n’a de plafond mensuel.</span></span><Icon name="droite" size={18} /></a>{/if}
    {/if}
  </section>

  <section class="tir-carte tir-ev-tendance" aria-label="Tendance par catégorie">
    <div class="tir-ligne tir-ligne--ecart tir-ligne--plis"><h2 class="tir-h2">Tendance</h2>{#if suffisant}<span class="tir-note">sur les {reference} derniers {unite(mode, reference)} complets</span>{/if}</div>
    {#if !suffisant}
      <p class="tir-note tir-note--texte">Pas assez de données : il faut au moins 3 {unite(mode, 3)} complets pour dire si une catégorie monte ou baisse. Tu en as {reference}.</p>
    {:else if !tendances.length}
      <p class="tir-note tir-note--texte">Aucune dépense classée sur ces {unite(mode, 2)}.</p>
    {:else}
      <ul class="tir-ligne tir-ligne--plis" aria-label="Résumé des tendances">
        <li><span class="tir-etq tir-etq--danger"><span aria-hidden="true">▲</span>{comptes.up} en hausse</span></li>
        <li><span class="tir-etq"><span aria-hidden="true">=</span>{pluriel(comptes.stable, 'stable', 'stables')}</span></li>
        <li><span class="tir-etq tir-etq--ok"><span aria-hidden="true">▼</span>{comptes.down} en baisse</span></li>
      </ul>
      <ul class="tir-liste">
        {#each tendances as c (c.id)}
          <li>
            <a class="tir-li tir-td" href={lien(c.id)}>
              <span class="tir-col tir-col--colle tir-grandit"><span class="tir-ligne" style:gap="8px"><span class="tir-pt tir-pt--{teinte(c)}" aria-hidden="true"></span><span class="tir-h3 truncate">{c.name}</span></span><span class="tir-note">{detailTendance(c.trend, mode)}</span></span>
              <TirelireEtincelle valeurs={c.trend.values} />
              {@render etiquette(c.trend)}
            </a>
          </li>
        {/each}
      </ul>
      <p class="tir-note">Variation entre le premier et le dernier de ces {unite(mode, 2)}, d’après la pente médiane : un mois isolé très haut ou très bas ne compte pas. Hausse ou baisse à partir de 10 € et 10 % de la moyenne.</p>
    {/if}
  </section>

  <TirelireSolde soldes={evolution.soldes} />

  <TirelireAnnees a={evolution.annees} {mode} />
</div>
