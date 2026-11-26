<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { hrefs, router } from '../../lib/route.svelte'
  import { dateCourte, euros, eurosSignes, pluriel } from './formats'
  import TirelireCourbe from './TirelireCourbe.svelte'
  import TirelireEcart from './TirelireEcart.svelte'
  import { etatPeriode, eurosRonds, filtresPeriode, MODES, moyenneSur, nomPrecedente, pourcent, titrePeriode, unite, type Ecart, type Mode, type Mois, type Moyenne } from './stats'
  import { NATURES, requeteDe, teinte } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let { mois }: { mois: Mois } = $props()

  const calendaire = $derived(mois.mode === 'calendar')
  const avant = $derived(nomPrecedente(mois.cumulative.previous?.id, mois.mode))
  const d = $derived(mois.expenses)
  const lien = (id: number) => hrefs.tirelire('categorie', id, `periode=${mois.period.id}`)
  const operations = (f = {}) => hrefs.tirelire('operations', undefined, requeteDe(filtresPeriode(mois.period, mois.mode, f)))
  const parts = $derived([
    { cle: 'fixe', nom: 'Fixe', montant: d.fixed, bp: d.fixed_bp },
    { cle: 'variable', nom: 'Variable', montant: d.variable, bp: d.variable_bp },
    { cle: 'classer', nom: 'À classer', montant: d.unclassified, bp: d.unclassified_bp },
  ].filter((p) => p.montant > 0))
  const colonnes = $derived([`vs ${calendaire && mois.cumulative.previous ? avant : 'précédent'}`, `vs moy. 3 ${unite(mois.mode, 3) === 'mois' ? 'm.' : 'rel.'}`, `vs moy. 12 ${unite(mois.mode, 12) === 'mois' ? 'm.' : 'rel.'}`, 'vs plafond'])
  const plafonds = $derived(mois.categories.reduce((n, c) => n + (c.monthly_cap ?? 0), 0))

  function choisirMode(m: Mode) {
    if (m === tirelire.mode) return
    tirelire.changerMode(m)
    router.go(hrefs.tirelire('mois'))
  }

  const moyennes = $derived<{ nom: string; m: Moyenne | null }[]>([{ nom: `vs moyenne 3 ${unite(mois.mode, 3)}`, m: d.avg3 }, { nom: `vs moyenne 12 ${unite(mois.mode, 12)}`, m: d.avg12 }])
</script>

{#snippet cellule(e: Ecart | null, titre: string)}
  <td data-titre={titre}><TirelireEcart ecart={e} /></td>
{/snippet}

<div class="tir-vm">
  <div class="tir-col tir-vm-tete" style:gap="12px">
    <section class="tir-carte tir-carte--serre" aria-label="Période">
      <div class="tir-ligne">
        {#if mois.previous_id}<a class="tir-ib" href={hrefs.tirelire('mois', mois.previous_id)} aria-label="Période précédente"><Icon name="gauche" size={20} /></a>{:else}<span class="tir-ib tir-ib--absent" aria-hidden="true"><Icon name="gauche" size={20} /></span>{/if}
        <div class="tir-col tir-col--colle tir-grandit" style:align-items="center" style:text-align="center">
          {#if tirelire.periodes && tirelire.periodes.length > 1}
            <span class="tir-champ tir-champ--titre"><select aria-label="Choisir la période" value={mois.period.id} onchange={(e) => router.go(hrefs.tirelire('mois', e.currentTarget.value))}>{#each [...tirelire.periodes].reverse() as p (p.id)}<option value={p.id}>{titrePeriode(p, mois.mode)}</option>{/each}</select><Icon name="bas" size={16} /></span>
          {:else}
            <span class="tir-h2">{titrePeriode(mois.period, mois.mode)}</span>
          {/if}
          <span class="tir-note">{etatPeriode(mois.period, mois.mode)}</span>
        </div>
        {#if mois.next_id}<a class="tir-ib" href={hrefs.tirelire('mois', mois.next_id)} aria-label="Période suivante"><Icon name="droite" size={20} /></a>{:else}<span class="tir-ib tir-ib--absent" aria-hidden="true"><Icon name="droite" size={20} /></span>{/if}
      </div>
      <div class="tir-seg" role="group" aria-label="Découpage des périodes">
        {#each MODES as m (m.id)}<button type="button" aria-pressed={mois.mode === m.id} onclick={() => choisirMode(m.id)}>{m.label}</button>{/each}
      </div>
    </section>

    <section class="tir-carte" aria-label="Dépenses de la période">
      <div class="tir-ligne tir-ligne--ecart tir-ligne--haut tir-ligne--plis">
        <div class="tir-col tir-col--serre"><span class="tir-note">{calendaire ? 'Dépenses du mois' : 'Dépenses du relevé'}</span><span class="tir-gros">{euros(d.total)}</span></div>
        {#if d.previous}
          <div class="tir-col tir-col--serre" style:align-items="flex-end"><span class="tir-etq" class:tir-etq--danger={d.previous.delta > 0} class:tir-etq--ok={d.previous.delta < 0}><span aria-hidden="true">{d.previous.delta > 0 ? '▲' : d.previous.delta < 0 ? '▼' : '='}</span>{eurosSignes(d.previous.delta)} vs {avant}</span><span class="tir-note">{avant} : {euros(d.previous.amount)}</span></div>
        {/if}
      </div>
      {#if d.previous_to_date}<p class="tir-note">Au même jour {calendaire ? `en ${avant}` : 'du relevé précédent'} : {euros(d.previous_to_date.amount)}, soit <TirelireEcart ecart={d.previous_to_date} texte={eurosSignes(d.previous_to_date.delta)} /> à date.</p>{/if}
      {#if mois.periods === 1}
        <p class="tir-note">Une seule période disponible : pas encore d’écart ni de moyenne. Ils apparaîtront avec {calendaire ? 'un deuxième mois' : 'un deuxième relevé'}.</p>
      {:else if !d.previous}
        <p class="tir-note">Pas de comparaison : {calendaire ? 'le mois précédent n’est couvert par aucun relevé' : 'aucun relevé précédent'}.</p>
      {:else if !mois.history.at(-2)?.complete}
        <p class="tir-note">Attention : {avant} n’est couvert qu’en partie par tes relevés, la comparaison est indicative.</p>
      {/if}
    </section>

    {#if mois.periods > 1}
      <section class="tir-g2" aria-label="Écart avec les moyennes">
        {#each moyennes as x (x.nom)}
          <div class="tir-carte tir-carte--serre">
            <span class="tir-note">{x.nom}</span>
            {#if x.m}
              <span class="tir-val"><TirelireEcart ecart={x.m} /></span>
              <span class="tir-note">{eurosRonds(x.m.amount)} · {moyenneSur(x.m, mois.mode)}</span>
            {:else}
              <span class="tir-val tir-val--vide">—</span>
              <span class="tir-note">aucune période complète avant celle-ci</span>
            {/if}
          </div>
        {/each}
      </section>
    {/if}

    <section class="tir-carte" aria-label="Fixe et variable">
      <div class="tir-ligne tir-ligne--ecart"><h2 class="tir-h2">Fixe et variable</h2><span class="tir-note">sur {euros(d.total)}</span></div>
      {#if parts.length}
        <span class="tir-parts" role="img" aria-label={parts.map((p) => `${p.nom} : ${euros(p.montant)}${p.bp === null ? '' : `, ${pourcent(p.bp)}`}`).join(' ; ')}>
          {#each parts as p (p.cle)}<span class="tir-part tir-part--{p.cle}" style:flex-grow={p.montant}></span>{/each}
        </span>
        <dl class="tir-parts-legende">
          {#each parts as p (p.cle)}
            <div class="tir-col tir-col--serre"><dt class="tir-ligne" style:gap="6px"><span class="tir-carre tir-part--{p.cle}" aria-hidden="true"></span><span class="tir-note">{p.nom}{p.bp === null ? '' : ` · ${pourcent(p.bp)}`}</span></dt><dd class="tir-val tir-fixe">{euros(p.montant)}</dd></div>
          {/each}
        </dl>
      {:else}
        <p class="tir-note tir-note--texte">Aucune dépense sur cette période.</p>
      {/if}
    </section>
  </div>

  <div class="tir-vm-courbe"><TirelireCourbe {mois} /></div>

  <section class="tir-carte tir-vm-table" aria-label="Par catégorie">
    <div class="tir-ligne tir-ligne--ecart"><h2 class="tir-h2">Par catégorie</h2><a class="tir-btn tir-btn--contour" href={operations()}><Icon name="list" size={16} />Opérations</a></div>
    {#if !mois.categories.length && mois.unclassified.operations > 0}
      <p class="tir-note tir-note--texte">Aucune opération de cette période n’est classée : tout est dans « À classer ». Classe-les pour voir la répartition.</p>
    {:else if !mois.categories.length}
      <p class="tir-note tir-note--texte">Aucune dépense sur cette période.</p>
    {/if}
    {#if mois.categories.length || mois.unclassified.operations > 0}
      <table class="tir-tab">
        <caption class="tir-sr">Dépenses par catégorie, {titrePeriode(mois.period, mois.mode)}</caption>
        <thead>
          <tr><th scope="col" class="tir-tab-nom">Catégorie</th><th scope="col" class="tir-tab-montant">Montant</th><th scope="col" class="tir-tab-part">Part</th>{#each colonnes as c (c)}<th scope="col">{c}</th>{/each}</tr>
        </thead>
        <tbody>
          {#each mois.categories as c (c.id)}
            <tr>
              <th scope="row" class="tir-tab-nom"><a class="tir-tab-lien" href={lien(c.id)}><span class="tir-pt tir-pt--{teinte(c)}" aria-hidden="true"></span><span class="tir-h3">{c.name}</span>{#if c.kind === 'fixe'}<span class="tir-etq tir-etq--info">{NATURES[c.kind]}</span>{/if}</a></th>
              <td class="tir-tab-montant tir-fort">{euros(c.amount)}</td>
              <td class="tir-tab-part tir-note">{c.share_bp === null ? '—' : pourcent(c.share_bp, 1)}</td>
              {@render cellule(c.previous, colonnes[0])}
              {@render cellule(c.avg3, colonnes[1])}
              {@render cellule(c.avg12, colonnes[2])}
              <td data-titre={colonnes[3]}>{#if c.cap_delta === null}<span class="tir-ec tir-ec--vide"><span class="tir-sr">pas de plafond</span><span aria-hidden="true">—</span></span>{:else}<span class="tir-ec" class:tir-ec--mal={c.cap_delta > 0} class:tir-ec--bien={c.cap_delta < 0}><span aria-hidden="true">{c.cap_delta > 0 ? '▲' : c.cap_delta < 0 ? '▼' : '='}</span>{c.cap_delta === 0 ? euros(0) : eurosSignes(c.cap_delta)}</span>{/if}</td>
            </tr>
          {/each}
          {#if mois.unclassified.operations > 0}
            <tr>
              <th scope="row" class="tir-tab-nom"><a class="tir-tab-lien" href={hrefs.tirelire('a-classer')}><span class="tir-pt tir-pt--vide" aria-hidden="true"></span><span class="tir-h3">À classer</span><span class="tir-etq tir-etq--rose">{pluriel(mois.unclassified.operations, 'opération', 'opérations')}</span></a></th>
              <td class="tir-tab-montant tir-fort">{euros(mois.unclassified.amount)}</td>
              <td class="tir-tab-part tir-note">{mois.unclassified.share_bp === null ? '—' : pourcent(mois.unclassified.share_bp, 1)}</td>
              {@render cellule(mois.unclassified.previous, colonnes[0])}
              <td data-titre={colonnes[1]}></td><td data-titre={colonnes[2]}></td><td data-titre={colonnes[3]}></td>
            </tr>
          {/if}
        </tbody>
        <tfoot>
          <tr>
            <th scope="row" class="tir-tab-nom"><span class="tir-h3">Total</span></th>
            <td class="tir-tab-montant tir-fort">{euros(d.total)}</td>
            <td class="tir-tab-part tir-note">{pourcent(10_000)}</td>
            {@render cellule(d.previous, colonnes[0])}
            {@render cellule(d.avg3, colonnes[1])}
            {@render cellule(d.avg12, colonnes[2])}
            <td data-titre={colonnes[3]}>{#if mois.budget}<span class="tir-ec" class:tir-ec--mal={mois.budget.delta > 0} class:tir-ec--bien={mois.budget.delta < 0}><span aria-hidden="true">{mois.budget.delta > 0 ? '▲' : mois.budget.delta < 0 ? '▼' : '='}</span>{eurosSignes(mois.budget.delta)}</span>{:else}<span class="tir-ec tir-ec--vide"><span class="tir-sr">pas de budget</span><span aria-hidden="true">—</span></span>{/if}</td>
          </tr>
        </tfoot>
      </table>
      {#if !plafonds && !mois.budget}<a class="tir-invite" href={hrefs.tirelire('categories')}><span class="tir-pas"><Icon name="cible" size={18} /></span><span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">Aucun plafond fixé</span><span class="tir-note">La colonne « vs plafond » reste vide tant qu’aucune catégorie n’a de plafond mensuel.</span></span><Icon name="droite" size={18} /></a>{/if}
      <p class="tir-note">▲ au-dessus de la référence, ▼ en dessous. Les achats par carte comptent à leur date d’achat{calendaire ? '' : ', dans le relevé qui les porte'}. Données connues jusqu’au {dateCourte(mois.period.known_through)}.</p>
    {/if}
  </section>
</div>
