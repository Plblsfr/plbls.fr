<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { entracteApi } from '../../lib/api'
  import { confirmer } from '../../lib/confirm.svelte'
  import { messageOf, notify } from '../../lib/ui'
  import { age, bytes, filterReleases, releaseLanguages, serviceName, type ArrRelease, type ReleaseFilter, type ReleaseTarget } from './arr'

  let { target, onclose, ondone }: { target: ReleaseTarget; onclose: () => void; ondone: () => void } = $props()

  let items = $state<ArrRelease[] | null>(null)
  let error = $state('')
  let busy = $state('')
  let sent = $state<string[]>([])
  let filtre = $state<ReleaseFilter>({ approved: '', resolution: '', language: '', sort: 'score' })

  const service = $derived(serviceName(target.service))
  const visibles = $derived(filterReleases(items ?? [], filtre))
  const langues = $derived(releaseLanguages(items ?? []))
  const acceptees = $derived((items ?? []).filter((r) => r.approved).length)
  const ETATS: [ReleaseFilter['approved'], string][] = [['', 'Toutes'], ['yes', 'Acceptées'], ['no', 'Rejetées']]
  const RESOLUTIONS: [string, string][] = [['', 'Toute qualité'], ['2160', '2160p'], ['1080', '1080p'], ['720', '720p']]

  let tour = 0
  async function load() {
    const moi = ++tour
    items = null
    error = ''
    try {
      const r = await entracteApi.arrReleases(target)
      if (moi === tour) items = r.items
    } catch (e) {
      if (moi === tour) { items = []; error = messageOf(e, `${service} n’a pas répondu : la recherche sur les indexeurs a échoué ou a pris trop de temps.`) }
    }
  }
  $effect(() => { void target; void load(); return () => { tour++ } })

  async function grab(r: ArrRelease) {
    if (!r.approved && !(await confirmer({ titre: 'Forcer cette release', message: `${service} l’a rejetée : ${r.rejections.join(' ; ') || 'motif inconnu'}. Elle sera téléchargée quand même.`, action: 'Télécharger quand même', destructif: true }))) return
    busy = r.guid
    try {
      await entracteApi.arrGrab(target.service, r)
      sent = [...sent, r.guid]
      notify.ok('Release envoyée au client de téléchargement.')
      ondone()
    } catch (e) {
      notify.error(e, `${service} n’a pas pu récupérer cette release. Relance la recherche si elle date de plus de quelques minutes.`)
    } finally { busy = '' }
  }
</script>

<div class="film-page">
  <div class="film-filters">
    <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={onclose}><Icon name="gauche" size={14} /> Retour</button>
    <div class="grow adm-body"><span class="v-eyebrow">Recherche interactive · {service}</span><h2 class="v-titre film-filters-title">{target.label}</h2></div>
    <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={!items} onclick={() => void load()}><Icon name="actualise" size={14} /> Interroger à nouveau</button>
  </div>

  {#if !items}
    <div class="v-empty" role="status"><Loader /><p class="v-note">{service} interroge tous ses indexeurs : compter de dix secondes à plus d’une minute.</p></div>
  {:else}
    {#if error}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>{/if}
    {#if items.length}
      <div class="film-filters">
        <div class="v-seg adm-filtres" role="group" aria-label="Décision">
          {#each ETATS as [valeur, nom] (valeur)}<button aria-pressed={filtre.approved === valeur} onclick={() => (filtre.approved = valeur)}>{nom}{valeur === 'yes' ? ` · ${acceptees}` : valeur === 'no' ? ` · ${items.length - acceptees}` : ` · ${items.length}`}</button>{/each}
        </div>
        <div class="v-seg adm-filtres" role="group" aria-label="Qualité">
          {#each RESOLUTIONS as [valeur, nom] (valeur)}<button aria-pressed={filtre.resolution === valeur} onclick={() => (filtre.resolution = valeur)}>{nom}</button>{/each}
        </div>
        {#if langues.length > 1}
          <label class="adm-ligne"><span class="v-note">Langue</span>
            <select class="plbls-select" bind:value={filtre.language}><option value="">Toutes</option>{#each langues as l (l)}<option value={l}>{l}</option>{/each}</select>
          </label>
        {/if}
        <label class="adm-ligne"><span class="v-note">Tri</span>
          <select class="plbls-select" bind:value={filtre.sort}><option value="score">Score du profil</option><option value="seeders">Sources</option><option value="size">Taille</option><option value="age">Âge</option></select>
        </label>
      </div>
    {/if}
    {#if !visibles.length}
      <div class="v-empty"><Icon name="cherche" size={28} /><p class="v-note">{items.length ? 'Aucune release ne passe ces filtres.' : error ? 'Aucun résultat.' : 'Les indexeurs n’ont rien trouvé pour ce titre.'}</p></div>
    {:else}
      <ul class="adm-list">
        {#each visibles as r (r.guid)}
          <li class="adm-item" class:adm-rejetee={!r.approved}>
            <span class="grow adm-body rel-nom">
              <span class="adm-mono">{r.title}</span>
              <span class="adm-tags">
                {#if r.quality}<span class="v-tag encre">{r.quality}</span>{/if}
                {#each r.languages as l (l)}<span class="v-tag">{l}</span>{/each}
                {#if r.full_season}<span class="v-tag info">Saison complète</span>{/if}
                {#each r.formats as f (f)}<span class="v-tag rose">{f}</span>{/each}
                <span class="v-note">{[r.indexer, age(r.age)].filter(Boolean).join(' · ')}</span>
              </span>
              {#if r.rejections.length}<span class="v-note adm-att"><Icon name="alerte" size={14} /> Rejetée par {service} : {r.rejections.join(' ; ')}</span>{/if}
            </span>
            <span class="adm-col"><span class="v-eyebrow">Taille</span><span class="adm-mono">{bytes(r.size) || '?'}</span></span>
            <span class="adm-col"><span class="v-eyebrow">Sources</span><span class="adm-mono">{r.seeders === null ? r.protocol ?? '?' : `${r.seeders} / ${r.leechers ?? 0}`}</span></span>
            <span class="adm-col"><span class="v-eyebrow">Score</span><span class="adm-mono">{r.score > 0 ? '+' : ''}{r.score}</span></span>
            {#if sent.includes(r.guid)}
              <span class="v-tag ok"><Icon name="ok" size={12} /> Envoyée</span>
            {:else}
              <button class="plbls-button plbls-button--compact rel-action" class:plbls-button--contour={!r.approved} disabled={!!busy} aria-label={`${r.approved ? 'Télécharger' : 'Forcer'} ${r.title}`} onclick={() => void grab(r)}>{#if r.approved}<Icon name="telecharge" size={14} /> Télécharger{:else}Forcer…{/if}</button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>

<style>
  .rel-nom { flex-basis: 380px; gap: 4px; }
  .rel-action { flex: 0 0 132px; }
</style>
