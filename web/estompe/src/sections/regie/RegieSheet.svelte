<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { notify } from '../../lib/ui'
  import { LIBRARIES, regieApi, sheetGroups, sheetText, sheetTitle, type LibraryKey, type LibraryTitle, type Sheet } from './regie'

  let service = $state<LibraryKey>('radarr')
  let query = $state('')
  let found = $state<LibraryTitle[] | null>(null)
  let searching = $state(false)
  let sheet = $state<Sheet | null>(null)
  let loading = $state(false)
  let group = $state('')

  const groups = $derived(sheet ? sheetGroups(sheet) : [])
  const text = $derived(sheet ? sheetText(sheet, group) : '')
  const placeholder = $derived(service === 'radarr' ? 'Titre du film' : service === 'sonarr' ? 'Titre de la série' : 'Album ou artiste')

  function choose(s: LibraryKey) {
    service = s
    found = null
    sheet = null
    query = ''
  }

  async function search(e: SubmitEvent) {
    e.preventDefault()
    if (!query.trim()) return
    searching = true
    sheet = null
    try { found = (await regieApi.library(service, query.trim())).items } catch (err) { found = null; notify.error(err, 'La recherche a échoué') } finally { searching = false }
  }

  async function open(t: LibraryTitle) {
    loading = true
    group = ''
    try {
      sheet = await regieApi.sheet(service, t.id)
      const g = sheetGroups(sheet)
      if (g.length > 1) group = g[0]
    } catch (err) { notify.error(err, 'La fiche est indisponible') } finally { loading = false }
  }

  const copy = () => navigator.clipboard.writeText(text).then(() => notify.ok('Fiche copiée.')).catch((e) => notify.error(e, 'Copie impossible'))
</script>

<div class="film-page">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">Fiche technique</h2>
    <div class="v-seg" role="group" aria-label="Bibliothèque">
      {#each LIBRARIES as l (l.key)}<button aria-pressed={service === l.key} onclick={() => choose(l.key)}>{l.label}</button>{/each}
    </div>
  </div>
  <p class="v-note rg-aide">Cherche un titre de la bibliothèque : sa fiche (fichier, vidéo, audio, sous-titres, liens) se copie telle quelle dans le formulaire d’un tracker.</p>

  <form class="film-filters" onsubmit={search}>
    <span class="film-search"><Icon name="cherche" size={16} /><input class="plbls-input" type="search" aria-label="Chercher dans la bibliothèque" {placeholder} maxlength="80" bind:value={query} /></span>
    <button class="plbls-button plbls-button--compact" disabled={searching || !query.trim()}>Chercher</button>
  </form>

  {#if searching || loading}
    <Loader />
  {:else if sheet}
    <section class="film-section">
      <div class="film-section-head">
        <h2 class="v-titre rg-wrap">{sheetTitle(sheet)}</h2>
        <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (sheet = null)}>Retour aux résultats</button>
        <button class="plbls-button plbls-button--compact" onclick={() => void copy()}><Icon name="copie" size={14} /> Copier</button>
      </div>
      {#if groups.length > 1}
        <label class="rg-saison"><span class="v-note">Fichiers</span>
          <select class="plbls-select" bind:value={group}>
            <option value="">Toutes les saisons</option>
            {#each groups as g (g)}<option value={g}>{g}</option>{/each}
          </select>
        </label>
      {/if}
      <pre class="v-well rg-fiche" aria-label="Fiche technique">{text}</pre>
    </section>
  {:else if found}
    {#if !found.length}
      <div class="v-empty"><Icon name="cherche" size={28} /><p class="v-note">Aucun titre de ce nom dans la bibliothèque.</p></div>
    {:else}
      <ul class="rg-list">
        {#each found as t (t.id)}
          <li><button class="rg-resultat" onclick={() => void open(t)}><span class="grow rg-body"><strong class="rg-wrap">{t.title}{t.year ? ` (${t.year})` : ''}</strong>{#if t.detail}<span class="v-note">{t.detail}</span>{/if}</span><Icon name="droite" size={16} /></button></li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>

<style>
  .rg-aide { margin: 0; }
  .rg-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--espace-2); }
  .rg-resultat { display: flex; align-items: center; gap: var(--espace-2); width: 100%; padding: var(--espace-2) var(--espace-3); border: 0; border-radius: var(--r); background: var(--surface); box-shadow: var(--o1); color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .rg-resultat:focus-visible { outline: none; box-shadow: var(--anneau-in); }
  .rg-body { display: flex; flex-direction: column; min-width: 0; }
  .rg-body .v-note { margin: 0; }
  .rg-wrap { overflow-wrap: anywhere; }
  .rg-saison { display: flex; align-items: center; gap: var(--espace-2); }
  .rg-fiche { margin: 0; font-family: var(--f-mono); font-size: 13px; line-height: 1.6; white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; }
</style>
