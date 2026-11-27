<script lang="ts">
  import { untrack } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { centimes, dateCourte, euros, eurosSignes, montant, pluriel, saisieEuros } from './formats'
  import TirelireEcart from './TirelireEcart.svelte'
  import TirelireJauge from './TirelireJauge.svelte'
  import { eurosRonds, filtresPeriode, moyenneSur, nomPeriode, nomPrecedente, pasDePlafond, pourcent, pourcentSigne, titrePeriode, unite, type DetailCategorie } from './stats'
  import { estDepense, requeteDe, titreDe } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let { detail }: { detail: DetailCategorie } = $props()

  let saisie = $state('')
  let enregistre = $state('')

  const c = $derived(detail.category)
  const depense = $derived(estDepense(c))
  const calendaire = $derived(detail.mode === 'calendar')
  const avant = $derived(nomPrecedente(detail.category.previous ? detail.history.at(-2)?.id : null, detail.mode))
  const lu = $derived(saisie.trim() ? centimes(saisie) : null)
  const faux = $derived(!!saisie.trim() && lu === null)
  const change = $derived(!faux && lu !== c.monthly_cap)
  const plafond = $derived(faux ? c.monthly_cap : lu)
  const maximum = $derived(Math.max(...detail.history.map((h) => h.amount), plafond ?? 0, 100))
  const haut = (v: number) => (Math.max(0, v) / (maximum * 1.12)) * 100
  const curseurMax = $derived(Math.ceil((Math.max(maximum, c.avg12?.amount ?? 0) * 2) / 1000) * 1000)
  const reperes = $derived([c.avg12 ? { nom: `moy. ${c.avg12.periods} ${unite(detail.mode, c.avg12.periods)}`, valeur: c.avg12.amount } : null, c.avg3 && c.avg3.periods !== c.avg12?.periods ? { nom: `moy. ${c.avg3.periods} ${unite(detail.mode, c.avg3.periods)}`, valeur: c.avg3.amount } : null].filter((r) => r !== null && r.valeur > 0) as { nom: string; valeur: number }[])
  const maxCommercant = $derived(Math.max(...detail.merchants.map((m) => m.amount), 1))
  const operations = (f = {}) => hrefs.tirelire('operations', undefined, requeteDe(filtresPeriode(detail.period, detail.mode, { categorie: c.id, ...f })))
  const court = (id: string, to: string) => (calendaire ? new Intl.DateTimeFormat('fr-FR', { month: 'narrow', timeZone: 'UTC' }).format(new Date(`${id}-01T00:00:00Z`)).toUpperCase() : dateCourte(to).replace(/ \d{4}$/, '').replace(/^(\d+|1er) /, '$1\u00a0'))

  const connu = $derived(detail.category.monthly_cap)
  const cle = $derived(`${detail.category.id} ${detail.period.id}`)

  $effect(() => {
    const p = connu
    untrack(() => { saisie = saisieEuros(p) })
  })

  $effect(() => {
    const vu = cle
    untrack(() => { if (vu) enregistre = '' })
  })

  function ajuster(sens: number) {
    const base = lu ?? c.monthly_cap ?? Math.round((c.avg3?.amount ?? c.amount) / 500) * 500
    saisie = saisieEuros(Math.max(0, base + sens * pasDePlafond(base)))
  }

  async function enregistrer(valeur: number | null) {
    if (faux) return
    if (await tirelire.enregistrerCategorie(c.id, { monthly_cap: valeur })) enregistre = valeur === null ? 'Plafond retiré.' : `Plafond enregistré : ${euros(valeur)} par mois.`
  }
</script>

<div class="tir-vc">
  <div class="tir-col" style:gap="12px">
    <section class="tir-carte" class:tir-carte--alerte={depense && (c.cap_delta ?? 0) > 0} aria-label="Montant de la période">
      <div class="tir-ligne tir-ligne--ecart tir-ligne--haut">
        <div class="tir-col tir-col--serre"><span class="tir-note tir-note--clair">{titrePeriode(detail.period, detail.mode)} · {pluriel(c.operations, 'opération', 'opérations')}</span><span class="tir-gros">{euros(c.amount)}</span></div>
        {#if depense && c.cap_delta !== null}
          <div class="tir-col tir-col--serre" style:align-items="flex-end">
            {#if c.cap_delta > 0}
              <span class="tir-etq tir-etq--danger"><Icon name="alerte" size={14} />{eurosSignes(c.cap_delta)}</span>
              {#if c.cap_bp !== null}<span class="tir-note tir-note--clair tir-fort">{pourcentSigne(c.cap_bp - 10_000)} au-dessus du plafond</span>{/if}
            {:else}
              <span class="tir-etq tir-etq--ok"><Icon name="ok" size={14} />{euros(-c.cap_delta)} de marge</span>
              {#if c.cap_bp !== null}<span class="tir-note tir-note--clair">{pourcent(c.cap_bp)} du plafond</span>{/if}
            {/if}
          </div>
        {/if}
      </div>
      {#if depense && c.monthly_cap !== null}<TirelireJauge montant={c.amount} reference={c.monthly_cap} />{/if}
      {#if detail.periods > 1}
        <dl class="tir-g3">
          <div class="tir-col tir-col--serre"><dt class="tir-note tir-note--clair">vs {avant}</dt><dd><TirelireEcart ecart={c.previous} bon={depense ? 'baisse' : 'hausse'} /></dd></div>
          <div class="tir-col tir-col--serre"><dt class="tir-note tir-note--clair">{c.avg3 ? moyenneSur(c.avg3, detail.mode).replace('moyenne sur', 'moy.') : 'moy. 3'}</dt><dd class="tir-fort">{c.avg3 ? eurosRonds(c.avg3.amount) : '—'}</dd></div>
          <div class="tir-col tir-col--serre"><dt class="tir-note tir-note--clair">{c.avg12 ? moyenneSur(c.avg12, detail.mode).replace('moyenne sur', 'moy.') : 'moy. 12'}</dt><dd class="tir-fort">{c.avg12 ? eurosRonds(c.avg12.amount) : '—'}</dd></div>
        </dl>
      {:else}
        <p class="tir-note tir-note--clair">Une seule période disponible : pas encore d’écart ni de moyenne.</p>
      {/if}
    </section>

    <section class="tir-carte" aria-label="Historique">
      <div class="tir-ligne tir-ligne--ecart"><h2 class="tir-h2">Historique</h2><span class="tir-note">{pluriel(detail.history.length, calendaire ? 'mois' : 'relevé', calendaire ? 'mois' : 'relevés')}</span></div>
      <div class="tir-histo" role="img" aria-label="Montant par période : {detail.history.map((h) => `${nomPeriode(h, detail.mode)} ${euros(h.amount)}`).join(', ')}{plafond !== null && depense ? `. Plafond ${euros(plafond)}` : ''}.">
        {#if depense && plafond !== null && plafond > 0}<span class="tir-histo-plafond" style:bottom="{haut(plafond)}%"><span>Plafond {eurosRonds(plafond)}</span></span>{/if}
        {#each detail.history as h, i (h.id)}
          {@const dessus = depense && plafond !== null && h.amount > plafond}
          <span class="tir-histo-col" class:tir-histo-col--actuel={i === detail.history.length - 1}>
            <span class="tir-histo-valeur">{eurosRonds(h.amount)}</span>
            <span class="tir-histo-barre" class:tir-histo-barre--dessus={dessus} class:tir-histo-barre--actuel={i === detail.history.length - 1} class:tir-histo-barre--partiel={!h.complete} style:height="{haut(h.amount)}%"></span>
          </span>
        {/each}
      </div>
      <div class="tir-histo-axe" aria-hidden="true">{#each detail.history as h, i (h.id)}<span class:tir-fort={i === detail.history.length - 1}>{court(h.id, h.to)}</span>{/each}</div>
      {#if depense && c.monthly_cap !== null && detail.history.length > 1}
        <p class="tir-note">{detail.over_cap ? `Plafond dépassé ${detail.over_cap} fois sur ${detail.history.length} ${unite(detail.mode, detail.history.length)}.` : `Plafond jamais dépassé sur ${detail.history.length} ${unite(detail.mode, detail.history.length)}.`}{detail.history.some((h) => !h.complete) ? ' Les périodes hachurées sont incomplètes.' : ''}</p>
      {:else if detail.history.some((h) => !h.complete)}
        <p class="tir-note">Les périodes hachurées sont incomplètes.</p>
      {/if}
      <details class="tir-donnees">
        <summary>Tableau des données</summary>
        <table class="tir-tab tir-tab--simple">
          <caption class="tir-sr">Montant de {c.name} par période</caption>
          <thead><tr><th scope="col">Période</th><th scope="col">Montant</th>{#if depense && c.monthly_cap !== null}<th scope="col">vs plafond</th>{/if}</tr></thead>
          <tbody>{#each detail.history as h (h.id)}<tr><th scope="row">{nomPeriode(h, detail.mode)}{h.complete ? '' : ' (incomplet)'}</th><td>{euros(h.amount)}</td>{#if depense && c.monthly_cap !== null}<td>{eurosSignes(h.amount - c.monthly_cap)}</td>{/if}</tr>{/each}</tbody>
        </table>
      </details>
    </section>

    {#if depense}
      <form class="tir-carte" aria-label="Plafond mensuel" onsubmit={(e) => { e.preventDefault(); void enregistrer(lu) }}>
        <div class="tir-ligne tir-ligne--ecart"><h2 class="tir-h2">Plafond mensuel</h2><span class="tir-etq">{c.monthly_cap === null ? 'aucun' : 'modifiable'}</span></div>
        <div class="tir-ligne">
          <button type="button" class="tir-ib tir-ib--fond" aria-label="Baisser le plafond" onclick={() => ajuster(-1)}><Icon name="minus" size={20} /></button>
          <span class="tir-champ tir-champ--centre tir-grandit"><input type="text" inputmode="decimal" aria-label="Plafond mensuel en euros" placeholder="aucun" bind:value={saisie} /><span class="tir-note" aria-hidden="true">€ / mois</span></span>
          <button type="button" class="tir-ib tir-ib--fond" aria-label="Monter le plafond" onclick={() => ajuster(1)}><Icon name="plus" size={20} /></button>
        </div>
        <input class="tir-curseur" type="range" aria-label="Plafond mensuel, curseur" min="0" max={curseurMax} step="500" value={plafond ?? 0} oninput={(e) => (saisie = saisieEuros(Number(e.currentTarget.value)))} />
        {#if reperes.length}
          <div class="tir-ligne tir-ligne--plis"><span class="tir-note">Repères :</span>{#each reperes as r (r.nom)}<button type="button" class="tir-btn tir-btn--contour tir-btn--petit" onclick={() => (saisie = saisieEuros(Math.round(r.valeur / 100) * 100))}>{r.nom} · {eurosRonds(r.valeur)}</button>{/each}</div>
        {/if}
        {#if faux}<p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />Plafond : un montant en euros, par exemple 150 ou 150,50.</p>{/if}
        {#if tirelire.erreurClassement}<p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />{tirelire.erreurClassement}</p>{/if}
        {#if enregistre && !change}<p class="tir-fait" role="status"><Icon name="ok" size={18} /><span class="tir-note tir-note--texte">{enregistre}</span></p>{/if}
        <div class="tir-actions">
          {#if c.monthly_cap !== null}<button type="button" class="tir-btn tir-btn--contour" disabled={tirelire.occupe} onclick={() => void enregistrer(null)}>Retirer</button>{/if}
          <button class="tir-btn tir-btn--plein tir-btn--grandit" disabled={tirelire.occupe || faux || !change}>Enregistrer le plafond</button>
        </div>
      </form>
    {/if}
  </div>

  <div class="tir-col" style:gap="12px">
    <section class="tir-carte tir-carte--liste" aria-label="Commerçants">
      <div class="tir-ligne tir-ligne--ecart" style:padding="12px 0 4px"><h2 class="tir-h2">Commerçants</h2><span class="tir-note">{detail.merchants_total} sur la période</span></div>
      {#if detail.merchants.length}
        <ul class="tir-liste">
          {#each detail.merchants as m (m.merchant_id)}
            <li>
              <svelte:element this={m.merchant_id === null ? 'div' : 'a'} class="tir-li tir-li--haut" href={m.merchant_id === null ? undefined : operations({ commercant: m.merchant_id })}>
                <span class="tir-col tir-grandit">
                  <span class="tir-ligne tir-ligne--ecart"><span class="tir-h3">{m.name ?? 'Sans commerçant'}</span><span class="tir-fort tir-fixe">{euros(m.amount)}</span></span>
                  <span class="tir-jauge tir-jauge--rose" aria-hidden="true"><span style:width="{(Math.max(0, m.amount) / maxCommercant) * 100}%"></span></span>
                  <span class="tir-sous"><span class="tir-note">{pluriel(m.operations, 'opération', 'opérations')} · panier moyen {euros(m.average)}</span>{#if m.previous && m.previous.amount !== 0}<TirelireEcart ecart={m.previous} bon={depense ? 'baisse' : 'hausse'} />{:else if m.previous}<span class="tir-note">· absent {calendaire ? `en ${avant}` : 'du relevé précédent'}</span>{/if}</span>
                </span>
              </svelte:element>
            </li>
          {/each}
        </ul>
        {#if detail.merchants_total > detail.merchants.length}<p class="tir-note">Les {detail.merchants.length} premiers par montant.</p>{/if}
      {:else}
        <p class="tir-note tir-note--texte" style:padding="8px 0">Aucune opération dans cette catégorie sur la période.</p>
      {/if}
    </section>

    <section class="tir-carte tir-carte--liste" aria-label="Opérations">
      <div class="tir-ligne tir-ligne--ecart" style:padding="12px 0 4px"><h2 class="tir-h2">Opérations</h2><span class="tir-note">{detail.operations_total} sur la période</span></div>
      {#if detail.operations.length}
        <ul class="tir-liste">
          {#each detail.operations as o (o.id)}
            <li>
              <a class="tir-li" href={hrefs.tirelire('operations', o.id)}>
                <span class="tir-note tir-fixe" style:min-width="52px">{dateCourte(o.purchase_date ?? o.date).replace(/ \d{4}$/, '')}</span>
                <span class="tir-h3 tir-grandit">{titreDe(o)}</span>
                <span class="tir-fort tir-fixe" class:tir-credit={o.direction === 'credit'}>{montant(o.amount, o.direction)}</span>
                <Icon name="droite" size={18} />
              </a>
            </li>
          {/each}
        </ul>
        <a class="tir-btn tir-btn--large" style:margin-top="8px" href={operations()}>{detail.operations_total > detail.operations.length ? `Voir les ${detail.operations_total} opérations` : 'Voir dans Opérations'}</a>
      {:else}
        <p class="tir-note tir-note--texte" style:padding="8px 0">Aucune opération.</p>
      {/if}
    </section>
  </div>
</div>
