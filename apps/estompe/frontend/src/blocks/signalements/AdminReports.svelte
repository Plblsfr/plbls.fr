<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import { api } from '../../lib/api'
  import { REPORT_CATEGORIES, REPORT_PLATFORMS, REPORT_STATUS, formatReport, label } from './report'
  import { notify } from '../../lib/ui'
  import type { Report, ReportStatus } from '../../lib/types'

  let { fail }: { fail: (e: unknown) => void } = $props()

  const when = (d: string) => new Date(d).toLocaleString('fr-FR')
  let filters = $state({ status: '', category: '', platform: '', author: '', q: '' })
  let list = $state<Report[]>([])
  let open = $state<Report | null>(null)
  let shots = $state<{ id: string; url: string }[]>([])
  let status = $state<ReportStatus>('new')
  let duplicateOf = $state('')
  let notes = $state('')

  const load = () => api.adminReports(filters).then((l) => (list = l)).catch(fail)
  $effect(() => { void load() })

  async function show(id: string) {
    try {
      const r = await api.adminReport(id)
      open = r
      status = r.status
      duplicateOf = r.duplicate_of ?? ''
      notes = r.notes ?? ''
      shots = []
      for (const a of r.attachments ?? []) if (!a.expired) api.fileBlobUrl(a.id).then((url) => { if (open?.id === r.id) shots.push({ id: a.id, url }) }).catch(() => {})
      void load()
    } catch (e) { fail(e) }
  }
  const close = () => { shots.forEach((s) => URL.revokeObjectURL(s.url)); shots = []; open = null }

  async function save() {
    if (!open) return
    try {
      open = await api.updateReport(open.id, { status, duplicate_of: status === 'duplicate' ? duplicateOf : undefined, notes })
      notify.ok('Signalement mis à jour.')
      void load()
    } catch (e) { fail(e) }
  }
  async function drop() {
    if (!open || !(await confirmer({ titre: 'Supprimer le signalement', message: `Supprimer définitivement le signalement « ${open.title} » ?`, action: 'Supprimer le signalement', destructif: true }))) return
    try { await api.deleteReport(open.id); close(); void load() } catch (e) { fail(e) }
  }
  const copy = () => navigator.clipboard.writeText(formatReport(open!)).then(() => notify.info('Signalement copié.'), () => notify.info('Copie impossible.'))
  function downloadLogs() {
    const a = document.createElement('a')
    a.href = URL.createObjectURL(new Blob([open?.logs ?? ''], { type: 'text/plain' }))
    a.download = `signalement-${open?.id}.log`
    a.click()
    URL.revokeObjectURL(a.href)
  }
</script>

{#if open}
  <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={close}>← Liste</button>
  <h2>{open.title}</h2>
  <p class="muted small">{open.author} · {when(open.created_at)} · {label(REPORT_CATEGORIES, open.category)} · {label(REPORT_PLATFORMS, open.platform)}</p>
  <p class="desc">{open.description}</p>
  {#each shots as s (s.id)}<a href={s.url} target="_blank" rel="noopener"><img class="shot" src={s.url} alt="Capture jointe" /></a>{/each}
  {#if Object.keys(open.tech ?? {}).length}
    <h3>Informations techniques</h3>
    <ul class="adm">{#each Object.entries(open.tech ?? {}) as [k, v]}<li><strong>{k}</strong> : {v}</li>{/each}</ul>
  {/if}
  {#if open.logs}
    <h3>Logs</h3>
    <pre class="logs">{open.logs}</pre>
    <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={downloadLogs}>Télécharger les logs</button>
  {/if}
  <h3>Traitement</h3>
  <form class="stack" onsubmit={(e) => { e.preventDefault(); void save() }}>
    <select class="plbls-select" aria-label="Statut" bind:value={status}>{#each Object.entries(REPORT_STATUS) as [id, l] (id)}<option value={id}>{l}</option>{/each}</select>
    {#if status === 'duplicate'}
      <select class="plbls-select" aria-label="Signalement d'origine" bind:value={duplicateOf} required>
        <option value="">Signalement d'origine…</option>
        {#each list.filter((r) => r.id !== open?.id) as r (r.id)}<option value={r.id}>{r.title} ({r.author})</option>{/each}
      </select>
    {/if}
    {#if open.duplicate_of && status === 'duplicate'}<button type="button" class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void show(open!.duplicate_of!)}>Voir l'original</button>{/if}
    <textarea class="plbls-input" rows="3" placeholder="Notes internes (invisibles de l'utilisateur)" aria-label="Notes internes" bind:value={notes} maxlength="4000"></textarea>
    <div class="row">
      <button class="plbls-button">Enregistrer</button>
      <button type="button" class="plbls-button plbls-button--contour" onclick={copy}>Copier</button>
      <button type="button" class="plbls-button plbls-button--contour" onclick={drop}>Supprimer</button>
    </div>
  </form>
{:else}
  <form class="row" onsubmit={(e) => { e.preventDefault(); void load() }}>
    <select class="plbls-select" aria-label="Statut" bind:value={filters.status}><option value="">Tous statuts</option>{#each Object.entries(REPORT_STATUS) as [id, l] (id)}<option value={id}>{l}</option>{/each}</select>
    <select class="plbls-select" aria-label="Catégorie" bind:value={filters.category}><option value="">Toutes catégories</option>{#each REPORT_CATEGORIES.slice(1) as c (c.id)}<option value={c.id}>{c.label}</option>{/each}</select>
    <select class="plbls-select" aria-label="Plateforme" bind:value={filters.platform}><option value="">Toutes plateformes</option>{#each REPORT_PLATFORMS as p (p.id)}<option value={p.id}>{p.label}</option>{/each}</select>
    <input class="plbls-input" type="search" placeholder="Auteur" aria-label="Auteur" bind:value={filters.author} />
    <input class="plbls-input" type="search" placeholder="Rechercher" aria-label="Rechercher" bind:value={filters.q} />
    <button class="plbls-button plbls-button--compact">Filtrer</button>
  </form>
  <ul class="adm">
    {#each list as r (r.id)}
      <li>
        <button class="link" onclick={() => void show(r.id)}>{#if !r.seen}<span class="plbls-badge">nouveau</span> {/if}<strong>{r.title}</strong></button>
        <span class="muted small">{r.author} · {when(r.created_at)} · {label(REPORT_CATEGORIES, r.category)} · {label(REPORT_PLATFORMS, r.platform)} · {REPORT_STATUS[r.status]}</span>
      </li>
    {:else}<li class="muted">Aucun signalement</li>{/each}
  </ul>
{/if}

<style>
  .row { flex-wrap: wrap; gap: 0.5rem; margin: 0.25rem 0 }
  .adm { margin: 0; padding-left: 1.25rem; overflow-wrap: anywhere }
  .adm > li { margin: 0.35rem 0 }
  .link { all: unset; cursor: pointer }
  .desc { white-space: pre-wrap; overflow-wrap: anywhere }
  .shot { max-width: 100%; max-height: 24rem; border-radius: 0.375rem; margin: 0.25rem 0.5rem 0.25rem 0 }
  .logs { max-height: 16rem; overflow: auto; font-size: 0.75rem; white-space: pre-wrap; word-break: break-word; padding: 0.5rem; border: 1px solid var(--border, currentColor); border-radius: 0.5rem }
  textarea { resize: vertical }
</style>
