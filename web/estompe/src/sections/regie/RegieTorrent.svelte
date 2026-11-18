<script lang="ts">
  import { untrack } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import Modal from '../../components/Modal.svelte'
  import { notify } from '../../lib/ui'
  import { PRIORITIES, seedingTime, bytes, limitBytes, limitInput, percent, regieApi, serviceName, torrentLabel, type Torrent, type TorrentDetail } from './regie'

  let { torrent, categories, onclose }: { torrent: Torrent; categories: string[]; onclose: () => void } = $props()

  let detail = $state<TorrentDetail | null>(null)
  let missing = $state(false)
  let busy = $state(false)
  let tag = $state('')
  const initial = untrack(() => torrent)
  let download = $state(limitInput(initial.download_limit))
  let upload = $state(limitInput(initial.upload_limit))
  const d = $derived(limitBytes(download))
  const u = $derived(limitBytes(upload))
  const hash = initial.hash
  const nom = $derived(detail?.title ? { title: [detail.title.title, detail.title.detail].filter(Boolean).join(' · '), sub: `${serviceName(detail.title.service)} · ${torrent.name}` } : torrentLabel(torrent))

  async function load() {
    try { detail = await regieApi.torrent(hash) } catch (e) { missing = true; notify.error(e, 'Les fichiers du torrent sont indisponibles') }
  }
  $effect(() => { void load() })

  async function run(action: () => Promise<void>, echec: string, relire = false) {
    busy = true
    try { await action(); if (relire) await load() } catch (e) { notify.error(e, echec) } finally { busy = false }
  }

  function addTag(e: SubmitEvent) {
    e.preventDefault()
    const t = tag.trim()
    if (!t || t.includes(',') || torrent.tags.includes(t)) return
    void run(async () => { await regieApi.setTags([hash], [t], []); tag = '' }, 'Le tag n’a pas pu être ajouté')
  }

  function saveLimits(e: SubmitEvent) {
    e.preventDefault()
    if (d === null || u === null) return
    void run(async () => { await regieApi.setLimits([hash], d, u); notify.ok('Limites enregistrées.') }, 'Les limites n’ont pas pu être enregistrées')
  }
</script>

<Modal title={nom.title} wide {onclose}>
  <div class="rg-detail">
    {#if nom.sub}<p class="v-note rg-wrap">{nom.sub}</p>{/if}
    {#if torrent.progress >= 1 || torrent.seeding_time}
      <p class="v-note">Partage actif : {seedingTime(torrent.seeding_time) || 'moins d’une minute'} · ratio {torrent.ratio.toLocaleString('fr-FR', { maximumFractionDigits: 2 })}{torrent.uploaded ? ` · ${bytes(torrent.uploaded)} envoyés` : ''}</p>
    {/if}

    <div class="rg-ligne">
      <label class="plbls-field rg-champ"><span>Catégorie</span>
        <select class="plbls-select" disabled={busy} value={torrent.category} onchange={(e) => void run(() => regieApi.setCategory([hash], e.currentTarget.value), 'La catégorie n’a pas pu être changée')}>
          <option value="">Aucune</option>
          {#each categories as c (c)}<option value={c}>{c}</option>{/each}
          {#if torrent.category && !categories.includes(torrent.category)}<option value={torrent.category}>{torrent.category}</option>{/if}
        </select>
      </label>
      <form class="rg-limites" onsubmit={saveLimits}>
        <label class="plbls-field rg-champ"><span>Réception (Ko/s)</span><input class="plbls-input" inputmode="decimal" placeholder="Sans limite" aria-invalid={d === null} bind:value={download} /></label>
        <label class="plbls-field rg-champ"><span>Envoi (Ko/s)</span><input class="plbls-input" inputmode="decimal" placeholder="Sans limite" aria-invalid={u === null} bind:value={upload} /></label>
        <button class="plbls-button plbls-button--contour" disabled={busy || d === null || u === null}>Limiter</button>
      </form>
    </div>

    <div class="rg-tags">
      <span class="v-note">Tags</span>
      {#each torrent.tags as t (t)}
        <span class="v-tag">{t}<button class="rg-x" disabled={busy} onclick={() => void run(() => regieApi.setTags([hash], [], [t]), 'Le tag n’a pas pu être retiré')} aria-label="Retirer le tag {t}"><Icon name="x" size={12} /></button></span>
      {/each}
      <form class="rg-tag-form" onsubmit={addTag}><input class="plbls-input" aria-label="Nouveau tag" placeholder="Ajouter un tag" maxlength="50" bind:value={tag} /></form>
    </div>

    <h3 class="v-titre rg-h3">Fichiers</h3>
    {#if !detail}
      {#if missing}<p class="v-note">Fichiers indisponibles.</p>{:else}<Loader />{/if}
    {:else if !detail.files.length}
      <p class="v-note">Les métadonnées du torrent ne sont pas encore reçues.</p>
    {:else}
      <ul class="rg-fichiers">
        {#each detail.files as f (f.index)}
          <li>
            <span class="grow rg-fichier"><span class="rg-wrap" class:rg-ignore={f.priority === 0}>{f.name}</span><span class="v-note">{bytes(f.size)} · {percent(f.progress)}</span></span>
            <select class="plbls-select rg-prio" aria-label="Priorité de {f.name}" disabled={busy} value={f.priority} onchange={(e) => void run(() => regieApi.setPriority(hash, [f.index], Number(e.currentTarget.value)), 'La priorité n’a pas pu être changée', true)}>
              {#each PRIORITIES as p (p.value)}<option value={p.value}>{p.label}</option>{/each}
              {#if !PRIORITIES.some((p) => p.value === f.priority)}<option value={f.priority}>Priorité {f.priority}</option>{/if}
            </select>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</Modal>

<style>
  .rg-detail { display: flex; flex-direction: column; gap: var(--espace-3); }
  .rg-detail .v-note { margin: 0; }
  .rg-wrap { overflow-wrap: anywhere; }
  .rg-ligne, .rg-limites { display: flex; flex-wrap: wrap; align-items: flex-end; gap: var(--espace-2); }
  .rg-champ { flex: 1 1 140px; }
  .rg-limites { flex: 2 1 320px; }
  .rg-tags { display: flex; flex-wrap: wrap; align-items: center; gap: var(--espace-2); }
  .rg-tag-form { flex: 0 1 180px; }
  .rg-tag-form .plbls-input { width: 100%; min-height: 32px; height: 32px; padding-block: 0; font-size: 13px; }
  .rg-x { display: inline-grid; place-items: center; padding: 0; margin-left: 2px; border: 0; background: none; color: inherit; cursor: pointer; }
  .rg-h3 { margin: 0; font-size: 16px; }
  .rg-fichiers { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--espace-1); max-height: 40vh; overflow-y: auto; }
  .rg-fichiers li { display: flex; align-items: center; gap: var(--espace-2); padding: var(--espace-1) 0; box-shadow: inset 0 -1px 0 var(--trait); }
  .rg-fichier { display: flex; flex-direction: column; min-width: 0; }
  .rg-ignore { color: var(--gris); text-decoration: line-through; }
  .rg-prio { flex: none; width: 130px; min-height: 34px; }
</style>
