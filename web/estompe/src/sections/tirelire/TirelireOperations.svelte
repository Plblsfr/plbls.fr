<script lang="ts">
  import { onDestroy, untrack } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs, router } from '../../lib/route.svelte'
  import { anneeDe, dateCourte, euros, eurosSignes, jourLong, montant, nombre, periode, pluriel, saisieEuros, type Sens } from './formats'
  import TirelirePastille from './TirelirePastille.svelte'
  import { actives, DELAI_RECHERCHE, filtresDe, memesFiltres, montantSaisi, nomReleve, parJour, pastilles, recherche, RECHERCHE_MAX, requeteDe, SANS_FILTRE, titreDe, totauxConnus, typeDe, TYPES, type Filtres, type TypeOperation } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let { annee = new Date().getFullYear(), requete = '' }: { annee?: number; requete?: string } = $props()

  const SENS: [Sens | null, string][] = [[null, 'Tout'], ['debit', 'Débits'], ['credit', 'Crédits']]

  let ouverts = $state(false)
  let saisie = $state('')
  let du = $state('')
  let au = $state('')
  let min = $state('')
  let max = $state('')
  let minuterie: ReturnType<typeof setTimeout> | undefined
  let pret = false
  const inverse = $derived(!!du && !!au && du > au)
  const bornes = $derived.by(() => {
    const a = montantSaisi(min)
    const b = montantSaisi(max)
    if (!a.ok || !b.ok) return { erreur: 'Écris un montant en euros, par exemple 12,50.' }
    if (a.valeur !== null && b.valeur !== null && a.valeur > b.valeur) return { erreur: 'Le montant minimum dépasse le montant maximum.' }
    return { erreur: '', min: a.valeur, max: b.valeur }
  })
  const jours = $derived(parJour(tirelire.operations ?? []))
  const connus = $derived(tirelire.operations ? (tirelire.totaux ?? totauxConnus(tirelire.filtres, tirelire.releves ?? [], tirelire.operations, tirelire.total)) : null)
  const sansReleve = $derived(tirelire.releves?.length === 0)
  const puces = $derived(pastilles(tirelire.filtres, { releves: tirelire.releves ?? [], categories: tirelire.categories ?? [], commercants: tirelire.commercants ?? [] }))

  function refleter() {
    const f = tirelire.filtres
    saisie = f.q
    du = f.du
    au = f.au
    min = saisieEuros(f.min)
    max = saisieEuros(f.max)
  }

  function appliquer(change: Partial<Filtres>) {
    clearTimeout(minuterie)
    const voulu = { ...tirelire.filtres, ...change }
    if (memesFiltres(voulu, tirelire.filtres)) return
    void tirelire.filtrer(voulu)
    router.remplacer(hrefs.tirelire('operations', undefined, requeteDe(voulu)))
  }

  $effect(() => {
    const voulu = filtresDe(requete)
    const vide = !requete
    untrack(() => {
      if (!pret) {
        pret = true
        if (vide && tirelire.filtre) router.remplacer(hrefs.tirelire('operations', undefined, requeteDe(tirelire.filtres)))
        if (!memesFiltres(voulu, tirelire.filtres) && !(vide && tirelire.filtre)) void tirelire.filtrer(voulu)
        else if (!tirelire.operations) void tirelire.chargerOperations()
        if (!tirelire.commercants) void tirelire.chargerCommercants()
        ouverts = !memesFiltres(tirelire.filtres, { ...SANS_FILTRE, q: tirelire.filtres.q })
        refleter()
      } else if (!memesFiltres(voulu, tirelire.filtres)) {
        clearTimeout(minuterie)
        void tirelire.filtrer(voulu)
        refleter()
      }
    })
  })

  onDestroy(() => clearTimeout(minuterie))

  function chercher() {
    clearTimeout(minuterie)
    minuterie = setTimeout(() => appliquer({ q: recherche(saisie) }), DELAI_RECHERCHE)
  }

  function viderRecherche() {
    saisie = ''
    appliquer({ q: '' })
  }

  function dates() {
    if (!inverse) appliquer({ du, au })
  }

  function montants() {
    if (!bornes.erreur) appliquer({ min: bornes.min, max: bornes.max })
  }

  function retirer(sans: Partial<Filtres>) {
    appliquer(sans)
    refleter()
  }

  const effacer = () => retirer(SANS_FILTRE)
</script>

{#if sansReleve}
  <section class="tir-carte tir-etat" aria-label="Aucune opération">
    <span class="tir-pas tir-pas--grand"><Icon name="list" size={22} /></span>
    <h2 class="tir-h2">Aucune opération</h2>
    <p class="tir-note tir-note--texte">Les opérations apparaissent ici dès qu’un relevé est importé.</p>
    <a class="tir-btn tir-btn--rose" href={hrefs.tirelire('import')}><Icon name="envoie" size={16} />Importer un relevé PDF</a>
  </section>
{:else}
  <div class="tir-ligne" style:gap="8px">
    <div class="tir-champ tir-grandit" role="search">
      <Icon name="cherche" size={18} />
      <input type="search" aria-label="Rechercher une opération" placeholder="Libellé, détails, commerçant" maxlength={RECHERCHE_MAX} autocomplete="off" enterkeyhint="search" bind:value={saisie} oninput={chercher} onkeydown={(e) => { if (e.key === 'Enter') appliquer({ q: recherche(saisie) }) }} />
      {#if saisie}<button class="tir-ib tir-ib--champ" aria-label="Effacer la recherche" onclick={viderRecherche}><Icon name="x" size={18} /></button>{/if}
    </div>
    <button class="tir-ib tir-ib--fond" aria-expanded={ouverts} aria-controls="tir-filtres" aria-label={ouverts ? 'Masquer les filtres' : 'Afficher les filtres'} onclick={() => (ouverts = !ouverts)}><Icon name="filtre" size={20} /></button>
  </div>

  {#if puces.length}
    <ul class="tir-puces" aria-label="Filtres actifs">
      {#each puces as p (p.cle)}
        <li><button class="tir-puce" aria-label={p.label} title={p.label} onclick={() => retirer(p.sans)}><span><span>{p.texte}</span><Icon name="x" size={14} /></span></button></li>
      {/each}
      {#if puces.length > 1}<li><button class="tir-puce tir-puce--texte" onclick={effacer}><span><span>Tout effacer</span></span></button></li>{/if}
    </ul>
  {/if}

  {#if ouverts}
    <section class="tir-carte" id="tir-filtres" aria-label="Filtres" style:gap="10px">
      <div class="tir-g2">
        <label class="tir-fl"><span>Du</span><span class="tir-champ"><input type="date" bind:value={du} max={au || undefined} onchange={dates} /></span></label>
        <label class="tir-fl"><span>Au</span><span class="tir-champ"><input type="date" bind:value={au} min={du || undefined} onchange={dates} /></span></label>
      </div>
      {#if inverse}<p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />La date de début est après la date de fin.</p>{/if}
      <div class="tir-g2">
        <label class="tir-fl"><span>Catégorie</span><span class="tir-champ">
          <select value={tirelire.filtres.aClasser ? 'a-classer' : tirelire.filtres.categorie === null ? '' : String(tirelire.filtres.categorie)} onchange={(e) => { const v = e.currentTarget.value; appliquer({ aClasser: v === 'a-classer', categorie: v && v !== 'a-classer' ? Number(v) : null }) }}>
            <option value="">Toutes</option>
            <option value="a-classer">À classer</option>
            {#each actives(tirelire.categories ?? []) as c (c.id)}<option value={String(c.id)}>{c.name}</option>{/each}
          </select><Icon name="bas" size={16} /></span></label>
        <label class="tir-fl"><span>Commerçant</span><span class="tir-champ">
          <select value={tirelire.filtres.commercant === null ? '' : String(tirelire.filtres.commercant)} onchange={(e) => appliquer({ commercant: e.currentTarget.value ? Number(e.currentTarget.value) : null })}>
            <option value="">Tous</option>
            {#each tirelire.commercants ?? [] as c (c.id)}<option value={String(c.id)}>{c.name}</option>{/each}
          </select><Icon name="bas" size={16} /></span></label>
        <label class="tir-fl"><span>Montant minimum</span><span class="tir-champ"><input type="text" inputmode="decimal" autocomplete="off" placeholder="0,00" bind:value={min} onchange={montants} /><span class="tir-note" aria-hidden="true">€</span></span></label>
        <label class="tir-fl"><span>Montant maximum</span><span class="tir-champ"><input type="text" inputmode="decimal" autocomplete="off" placeholder="sans limite" bind:value={max} onchange={montants} /><span class="tir-note" aria-hidden="true">€</span></span></label>
        <label class="tir-fl"><span>Type</span><span class="tir-champ">
          <select value={tirelire.filtres.type ?? ''} onchange={(e) => appliquer({ type: (e.currentTarget.value || null) as TypeOperation | null })}>
            <option value="">Tous</option>
            {#each Object.entries(TYPES) as [cle, nom] (cle)}<option value={cle}>{nom}</option>{/each}
          </select><Icon name="bas" size={16} /></span></label>
        <label class="tir-fl"><span>Relevé</span><span class="tir-champ">
          <select value={tirelire.filtres.releve === null ? '' : String(tirelire.filtres.releve)} onchange={(e) => appliquer({ releve: e.currentTarget.value ? Number(e.currentTarget.value) : null })}>
            <option value="">Tous les relevés</option>
            {#each tirelire.releves ?? [] as r (r.id)}<option value={String(r.id)}>{nomReleve(r)} · {dateCourte(r.to)}</option>{/each}
          </select><Icon name="bas" size={16} /></span></label>
      </div>
      {#if bornes.erreur}<p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />{bornes.erreur}</p>{/if}
      <div class="tir-seg" role="group" aria-label="Sens de l’opération">
        {#each SENS as [sens, nom] (nom)}<button aria-pressed={tirelire.filtres.sens === sens} onclick={() => appliquer({ sens })}>{nom}</button>{/each}
      </div>
      <div class="tir-ligne tir-ligne--plis">
        <button class="tir-btn tir-btn--contour" disabled={!tirelire.filtre && !saisie && !du && !au && !min && !max} onclick={effacer}>Réinitialiser</button>
        <button class="tir-btn tir-btn--plein tir-btn--grandit" disabled={tirelire.exportEnCours || !tirelire.operations || !tirelire.total} onclick={() => void tirelire.exporter()}><Icon name="telecharge" size={16} />{tirelire.exportEnCours ? 'Export en cours…' : 'Exporter en CSV'}</button>
      </div>
      {#if tirelire.erreurExport}<p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />L’export a échoué. {tirelire.erreurExport}</p>{/if}
      {#if tirelire.exporte}<p class="tir-note tir-note--texte" role="status">{pluriel(tirelire.total, 'opération exportée', 'opérations exportées')} dans {tirelire.exporte}.</p>{/if}
    </section>
  {/if}

  {#if tirelire.erreurOperations && !tirelire.operations}
    <section class="tir-carte tir-etat" role="alert">
      <span class="tir-pas tir-pas--grand tir-pas--danger"><Icon name="alerte" size={22} /></span>
      <h2 class="tir-h2">Les opérations ne se chargent pas</h2>
      <p class="tir-note tir-note--texte">{tirelire.erreurOperations}</p>
      <button class="tir-btn tir-btn--plein" onclick={() => void tirelire.chargerOperations()}><Icon name="actualise" size={16} />Réessayer</button>
    </section>
  {:else if !tirelire.operations}
    <div class="tir-etat" role="status"><Loader text="Chargement des opérations…" /></div>
  {:else}
    <div class="tir-g3" role="group" aria-label={tirelire.filtre ? 'Total de la sélection' : 'Résumé'}>
      <div class="tir-carte tir-carte--serre tir-carte--creux"><span class="tir-note">Opérations</span><span class="tir-h3">{nombre(tirelire.total)}</span></div>
      {#if connus}
        <div class="tir-carte tir-carte--serre tir-carte--creux"><span class="tir-note">Débits</span><span class="tir-h3 tir-fixe">{connus.debits ? eurosSignes(-connus.debits) : euros(0)}</span></div>
        <div class="tir-carte tir-carte--serre tir-carte--creux"><span class="tir-note">Crédits</span><span class="tir-h3 tir-fixe" class:tir-credit={connus.credits > 0}>{connus.credits ? eurosSignes(connus.credits) : euros(0)}</span></div>
      {/if}
    </div>

    {#if !jours.length}
      <section class="tir-carte tir-etat" aria-label="Aucune opération">
        <h2 class="tir-h2">Aucune opération</h2>
        <p class="tir-note tir-note--texte">{tirelire.filtres.q ? `Rien ne correspond à « ${tirelire.filtres.q} »${puces.length > 1 ? ' avec ces filtres' : ''}.` : tirelire.filtre ? 'Rien ne correspond à ces filtres.' : 'Les relevés importés ne contiennent aucune opération.'}</p>
        {#if tirelire.filtre}<button class="tir-btn tir-btn--contour" onclick={effacer}>Réinitialiser les filtres</button>{/if}
      </section>
    {:else}
      <section class="tir-carte tir-carte--liste" aria-label="Liste des opérations">
        {#each jours as j, i (j.jour)}
          {@const complet = i < jours.length - 1 || !tirelire.reste}
          <h3 class="tir-jour"><span>{jourLong(j.jour, annee)}</span>{#if complet}<span class="tir-fixe" class:tir-credit={j.solde > 0}>{eurosSignes(j.solde)}</span>{/if}</h3>
          <ul class="tir-liste" aria-label={jourLong(j.jour, anneeDe(j.jour) ?? annee)}>
            {#each j.operations as o (o.id)}
              <li>
                <a class="tir-li" href={hrefs.tirelire('operations', o.id)}>
                  <span class="tir-col tir-grandit"><span class={o.merchant_name ? 'tir-h3' : 'tir-mono'}>{titreDe(o)}</span><span class="tir-sous"><TirelirePastille categorie={tirelire.categorie(o.category_id)} /><span class="tir-note tir-fixe">{typeDe(o)}</span></span></span>
                  <span class="tir-fort tir-fixe" class:tir-credit={o.direction === 'credit'}>{montant(o.amount, o.direction)}</span>
                </a>
              </li>
            {/each}
          </ul>
        {/each}
        {#if tirelire.erreurOperations}<p class="tir-alerte" role="alert" style:margin-top="8px"><Icon name="alerte" size={18} />{tirelire.erreurOperations}</p>{/if}
        {#if tirelire.reste}
          <button class="tir-btn tir-btn--large" style:margin-top="8px" disabled={tirelire.suiteEnCours} onclick={() => void tirelire.suite()}>{tirelire.suiteEnCours ? 'Chargement…' : `Afficher la suite (${nombre(tirelire.reste)})`}</button>
        {:else if tirelire.total > 0}
          <p class="tir-note" style:padding-top="10px" style:text-align="center">{tirelire.filtres.du || tirelire.filtres.au ? `Fin de la période${tirelire.filtres.du && tirelire.filtres.au ? ` · ${periode(tirelire.filtres.du, tirelire.filtres.au).toLowerCase()}` : ''}` : 'Toutes les opérations sont affichées'}</p>
        {/if}
      </section>
    {/if}
  {/if}
{/if}
