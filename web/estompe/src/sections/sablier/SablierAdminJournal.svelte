<script lang="ts">
  import { onMount } from 'svelte'
  import Loader from '../../components/Loader.svelte'
  import { filtrerJournal, NOM_ACTION, nomCible } from './admin'
  import { messageDe } from './admin.svelte'
  import { dateHeure } from './formats'
  import { adminApi, type EntreeJournal } from './sablier'
  import SablierErreur from './SablierErreur.svelte'

  let entrees = $state.raw<EntreeJournal[] | null>(null)
  let suite = $state<number | null>(null)
  let erreur = $state('')
  let attente = $state(false)
  let filtre = $state('')

  async function charger(avant: number | null = null) {
    if (attente) return
    attente = true
    erreur = ''
    try {
      const page = await adminApi.journal(avant)
      entrees = avant && entrees ? [...entrees, ...page.entries] : page.entries
      suite = page.next_before
    } catch (e) {
      erreur = messageDe(e)
    } finally {
      attente = false
    }
  }
  onMount(() => { void charger() })
  const visibles = $derived(entrees ? filtrerJournal(entrees, filtre) : [])
</script>

<section class="sab-sec" aria-label="Journal des consultations">
  <p class="sab-note">Une ligne par ouverture des stats d’une personne par un admin, et par effacement. Le journal ne se modifie pas et ne s’efface pas d’ici.</p>
  {#if erreur && !entrees}
    <SablierErreur message={erreur} onretry={() => void charger()} />
  {:else if !entrees}
    <div class="sab-attente"><Loader text="Ouverture du journal…" /></div>
  {:else if !entrees.length}
    <div class="sab-vide"><h2 class="sab-h2">Aucune consultation notée</h2></div>
  {:else}
    <div class="plbls-field">
      <label class="plbls-field__label" for="sab-filtre">Filtrer</label>
      <input id="sab-filtre" class="plbls-input" type="search" placeholder="Admin ou personne consultée" bind:value={filtre} />
    </div>
    {#if visibles.length}
      <div class="sab-card sab-card--tableau"><div class="sab-scroll">
        <table class="sab-table">
          <thead><tr><th scope="col">Date</th><th scope="col">Admin</th><th scope="col">Action</th><th scope="col">Personne</th></tr></thead>
          <tbody>
            {#each visibles as e (e.id)}<tr><td>{dateHeure(e.date)}</td><th scope="row">{e.admin_name}</th><td>{NOM_ACTION[e.action]}</td><td>{nomCible(e)}</td></tr>{/each}
          </tbody>
        </table>
      </div></div>
    {:else}
      <p class="sab-note">Aucune ligne chargée ne correspond à ce filtre.</p>
    {/if}
    {#if erreur}<p class="sab-note sab-erreur" role="alert">{erreur}</p>{/if}
    {#if suite !== null}
      <div class="sab-ligne"><button class="plbls-button plbls-button--contour plbls-button--compact" type="button" disabled={attente} onclick={() => void charger(suite)}>{attente ? 'Chargement…' : 'Plus anciens'}</button></div>
    {/if}
  {/if}
</section>
