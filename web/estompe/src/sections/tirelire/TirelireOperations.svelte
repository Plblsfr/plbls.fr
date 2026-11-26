<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { anneeDe, dateCourte, eurosSignes, jourLong, montant, nombre, periode } from './formats'
  import { nomReleve, parJour, titreDe, totauxConnus, typeDe } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let { annee = new Date().getFullYear() }: { annee?: number } = $props()

  let ouverts = $state(false)
  let du = $state('')
  let au = $state('')
  const inverse = $derived(!!du && !!au && du > au)
  const jours = $derived(parJour(tirelire.operations ?? []))
  const connus = $derived(tirelire.operations ? totauxConnus(tirelire.filtres, tirelire.releves ?? [], tirelire.operations, tirelire.total) : null)
  const sansReleve = $derived(tirelire.releves?.length === 0)

  onMount(() => {
    du = tirelire.filtres.du
    au = tirelire.filtres.au
    ouverts = !!du || !!au
    if (!tirelire.operations) void tirelire.chargerOperations()
  })

  function dates() {
    if (inverse) return
    void tirelire.filtrer({ du, au })
  }

  function effacer() {
    du = ''
    au = ''
    void tirelire.effacerFiltres()
  }
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
    <label class="tir-champ tir-grandit">
      <Icon name="file" size={18} />
      <select aria-label="Relevé" value={tirelire.filtres.releve === null ? '' : String(tirelire.filtres.releve)} onchange={(e) => void tirelire.filtrer({ releve: e.currentTarget.value ? Number(e.currentTarget.value) : null })}>
        <option value="">Tous les relevés</option>
        {#each tirelire.releves ?? [] as r (r.id)}<option value={String(r.id)}>{nomReleve(r)} · {dateCourte(r.to)}</option>{/each}
      </select>
      <Icon name="bas" size={16} />
    </label>
    <button class="tir-ib tir-ib--fond" aria-expanded={ouverts} aria-controls="tir-filtres" aria-label={ouverts ? 'Masquer les filtres de période' : 'Afficher les filtres de période'} onclick={() => (ouverts = !ouverts)}><Icon name="filtre" size={20} /></button>
  </div>

  {#if ouverts}
    <section class="tir-carte" id="tir-filtres" aria-label="Filtres" style:gap="10px">
      <div class="tir-g2">
        <label class="tir-fl"><span>Du</span><span class="tir-champ"><input type="date" bind:value={du} max={au || undefined} onchange={dates} /></span></label>
        <label class="tir-fl"><span>Au</span><span class="tir-champ"><input type="date" bind:value={au} min={du || undefined} onchange={dates} /></span></label>
      </div>
      {#if inverse}<p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />La date de début est après la date de fin.</p>{/if}
      <div class="tir-ligne"><button class="tir-btn tir-btn--contour" disabled={!tirelire.filtre && !du && !au} onclick={effacer}>Réinitialiser</button></div>
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
    <div class="tir-g3" role="group" aria-label="Résumé">
      <div class="tir-carte tir-carte--serre tir-carte--creux"><span class="tir-note">Opérations</span><span class="tir-h3">{nombre(tirelire.total)}</span></div>
      {#if connus}
        <div class="tir-carte tir-carte--serre tir-carte--creux"><span class="tir-note">Débits</span><span class="tir-h3 tir-fixe">{eurosSignes(-connus.debits)}</span></div>
        <div class="tir-carte tir-carte--serre tir-carte--creux"><span class="tir-note">Crédits</span><span class="tir-h3 tir-fixe tir-credit">{eurosSignes(connus.credits)}</span></div>
      {/if}
    </div>

    {#if !jours.length}
      <section class="tir-carte tir-etat" aria-label="Aucune opération">
        <h2 class="tir-h2">Aucune opération</h2>
        <p class="tir-note tir-note--texte">{tirelire.filtre ? 'Rien ne correspond à ces filtres.' : 'Les relevés importés ne contiennent aucune opération.'}</p>
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
                  <span class="tir-col tir-col--serre tir-grandit"><span class="tir-mono">{titreDe(o)}</span><span class="tir-note">{typeDe(o)}</span></span>
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
