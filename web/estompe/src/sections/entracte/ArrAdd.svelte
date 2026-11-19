<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import Modal from '../../components/Modal.svelte'
  import { entracteApi } from '../../lib/api'
  import { messageOf, notify } from '../../lib/ui'
  import { serviceName, serviceOf } from './arr'
  import { freeSpace, initialChoice, type RequestKind, type SendChoice, type SendOptions } from './entracte'

  let { kind, tmdbId, title, onclose, ondone }: { kind: RequestKind; tmdbId: number; title: string; onclose: () => void; ondone: () => void } = $props()

  let options = $state<SendOptions | null>(null)
  let choice = $state<SendChoice | null>(null)
  let seasons = $state<{ number: number; title: string }[]>([])
  let chosen = $state<number[]>([])
  let search = $state(true)
  let unavailable = $state('')
  let error = $state('')
  let busy = $state(false)

  const service = $derived(serviceOf(kind))
  const nom = $derived(serviceName(service))

  $effect(() => {
    let gone = false
    void entracteApi.sendOptions(kind).then((o) => {
      if (gone) return
      options = o
      choice = initialChoice(o)
      if (!choice) unavailable = `${nom} ne déclare aucun emplacement ou aucun profil de qualité.`
    }).catch((e) => { if (!gone) unavailable = messageOf(e, `Les réglages de ${nom} sont indisponibles.`) })
    if (kind === 'series') {
      void entracteApi.series(tmdbId).then((s) => {
        if (gone) return
        seasons = s.seasons.filter((x) => x.number > 0).map((x) => ({ number: x.number, title: x.title }))
        chosen = seasons.map((x) => x.number)
      }).catch(() => {})
    }
    return () => { gone = true }
  })

  const toggleTag = (id: number) => { if (choice) choice.tags = choice.tags.includes(id) ? choice.tags.filter((t) => t !== id) : [...choice.tags, id] }
  const toggleSeason = (n: number) => { chosen = chosen.includes(n) ? chosen.filter((x) => x !== n) : [...chosen, n] }

  async function send() {
    busy = true
    error = ''
    try {
      const partielle = kind === 'series' && seasons.length > 0 && chosen.length < seasons.length
      await entracteApi.arrAdd(service, { tmdb_id: tmdbId, ...(choice ?? {}), ...(partielle ? { seasons: [...chosen].sort((a, b) => a - b) } : {}), search })
      notify.ok(`« ${title} » ajouté à ${nom}${search ? ', recherche lancée' : ''}.`)
      ondone()
      onclose()
    } catch (e) {
      error = messageOf(e, `${nom} n’a pas pu ajouter ce titre.`)
    } finally { busy = false }
  }
</script>

<Modal title="Ajouter « {title} »" {onclose}>
  <form class="aj" onsubmit={(e) => { e.preventDefault(); void send() }}>
    {#if unavailable}
      <div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{unavailable} Le titre peut être ajouté avec les réglages habituels de {nom}.</p></div>
    {:else if !options || !choice}
      <Loader />
    {:else}
      <p class="v-note">L’ajout ne crée pas de demande : le titre entre dans la bibliothèque dès qu’un fichier est importé.</p>
      <label class="adm-champ"><span>Emplacement</span>
        <select class="plbls-select" bind:value={choice.folder}>{#each options.folders as f (f.path)}<option value={f.path}>{f.path}{freeSpace(f.free) ? ` · ${freeSpace(f.free)}` : ''}</option>{/each}</select>
      </label>
      <label class="adm-champ"><span>Profil de qualité</span>
        <select class="plbls-select" bind:value={choice.profile}>{#each options.profiles as p (p.id)}<option value={p.id}>{p.name || `Profil ${p.id}`}</option>{/each}</select>
      </label>
      {#if options.tags.length}
        <fieldset class="aj-groupe"><legend>Tags</legend>
          {#each options.tags as t (t.id)}<label class="adm-coche"><input class="adm-case" type="checkbox" checked={choice.tags.includes(t.id)} onchange={() => toggleTag(t.id)} /> {t.label || `Tag ${t.id}`}</label>{/each}
        </fieldset>
      {/if}
    {/if}
    {#if seasons.length}
      <fieldset class="aj-groupe"><legend>Saisons surveillées</legend>
        {#each seasons as s (s.number)}<label class="adm-coche"><input class="adm-case" type="checkbox" checked={chosen.includes(s.number)} onchange={() => toggleSeason(s.number)} /> {s.title || `Saison ${s.number}`}</label>{/each}
      </fieldset>
    {/if}
    <label class="adm-coche"><input class="adm-case" type="checkbox" bind:checked={search} /> Lancer la recherche dès l’ajout</label>
    {#if error}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>{/if}
    <div class="aj-actions">
      <button type="button" class="plbls-button plbls-button--contour" onclick={onclose}>Annuler</button>
      <button type="submit" class="plbls-button" disabled={busy || (!choice && !unavailable) || (seasons.length > 0 && !chosen.length)}><Icon name="plus" size={16} /> Ajouter à {nom}</button>
    </div>
  </form>
</Modal>

<style>
  .aj { display: flex; flex-direction: column; gap: var(--espace-3); }
  .aj-groupe { margin: 0; padding: 0; border: 0; display: flex; flex-wrap: wrap; gap: 0 var(--espace-4); }
  .aj-groupe legend { padding: 0; margin-bottom: var(--espace-1); font-size: 13px; font-weight: 600; }
  .aj-actions { display: flex; justify-content: flex-end; flex-wrap: wrap; gap: var(--espace-2); }
</style>
