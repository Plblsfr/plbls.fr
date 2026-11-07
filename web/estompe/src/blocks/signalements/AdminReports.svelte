<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import { api } from '../../lib/api'
  import { REPORT_CATEGORIES, REPORT_PLATFORMS, REPORT_STATUS, formatReport, label } from './report'
  import { notify } from '../../lib/ui'
  import Icon from '../../components/Icon.svelte'
  import type { Report, ReportStatus } from '../../lib/types'

  let { fail }: { fail: (e: unknown) => void } = $props()

  const when = (d: string) => new Date(d).toLocaleString('fr-FR')
  const TONE: Record<ReportStatus, string> = { new: 'info', in_progress: 'att', resolved: 'ok', rejected: 'dgr', duplicate: '' }
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

<section class="v-card rp-card">
  <form class="rp-filtres" onsubmit={(e) => { e.preventDefault(); void load() }}>
    <h2 class="rp-t">Signalements</h2>
    <select class="plbls-select" aria-label="Statut" bind:value={filters.status}><option value="">Tous statuts</option>{#each Object.entries(REPORT_STATUS) as [id, l] (id)}<option value={id}>{l}</option>{/each}</select>
    <select class="plbls-select" aria-label="Catégorie" bind:value={filters.category}><option value="">Toutes catégories</option>{#each REPORT_CATEGORIES.slice(1) as c (c.id)}<option value={c.id}>{c.label}</option>{/each}</select>
    <select class="plbls-select" aria-label="Plateforme" bind:value={filters.platform}><option value="">Toutes plateformes</option>{#each REPORT_PLATFORMS as p (p.id)}<option value={p.id}>{p.label}</option>{/each}</select>
    <input class="plbls-input" type="search" placeholder="Auteur" aria-label="Auteur" bind:value={filters.author} />
    <input class="plbls-input" type="search" placeholder="Rechercher" aria-label="Rechercher" bind:value={filters.q} />
    <button class="plbls-button plbls-button--compact"><Icon name="filtre" size={16} />Filtrer</button>
  </form>

  <div class="rp-panes" class:detail={!!open}>
    <ul class="rp-list">
      {#each list as r (r.id)}
        <li>
          <button class="rp-row" aria-current={open?.id === r.id ? 'true' : undefined} onclick={() => void show(r.id)}>
            <span class="rp-row-t">
              <b class="v-ell">{r.title}</b>
              <span class="v-mono v-mute">{r.author} · {when(r.created_at)} · {label(REPORT_CATEGORIES, r.category)}</span>
            </span>
            <span class="rp-tags">
              <span class="v-tag">{label(REPORT_PLATFORMS, r.platform)}</span>
              {#if !r.seen}<span class="v-tag rose">nouveau</span>{/if}
              <span class="v-tag {TONE[r.status]}">{REPORT_STATUS[r.status]}</span>
            </span>
          </button>
        </li>
      {:else}<li class="v-note rp-vide">Aucun signalement</li>{/each}
    </ul>

    <div class="rp-detail">
      {#if open}
        <button class="plbls-button plbls-button--compact plbls-button--discret rp-back" onclick={close}><Icon name="gauche" size={16} />Liste</button>
        <div class="rp-head">
          <h3 class="v-titre">{open.title}</h3>
          <p class="v-mono v-mute">{open.author} · {when(open.created_at)} · {label(REPORT_CATEGORIES, open.category)} · {label(REPORT_PLATFORMS, open.platform)}</p>
        </div>
        <p class="desc">{open.description}</p>
        {#if shots.length}
          <div class="rp-shots">{#each shots as s (s.id)}<a href={s.url} target="_blank" rel="noopener"><img class="shot" src={s.url} alt="Capture jointe" /></a>{/each}</div>
        {/if}
        {#if Object.keys(open.tech ?? {}).length}
          <details class="rp-det">
            <summary class="v-lbl">Informations techniques</summary>
            <ul class="rp-kv">{#each Object.entries(open.tech ?? {}) as [k, v]}<li><span>{k}</span><span class="v-mono">{v}</span></li>{/each}</ul>
          </details>
        {/if}
        {#if open.logs}
          <details class="rp-det" open>
            <summary class="v-lbl">Logs</summary>
            <pre class="logs">{open.logs}</pre>
            <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={downloadLogs}><Icon name="telecharge" size={16} />Télécharger les logs</button>
          </details>
        {/if}
        <form class="rp-form" onsubmit={(e) => { e.preventDefault(); void save() }}>
          <h4 class="v-eyebrow">Traitement</h4>
          <div class="v-seg rp-seg" role="group" aria-label="Statut">
            {#each Object.entries(REPORT_STATUS) as [id, l] (id)}<button type="button" aria-pressed={status === id} onclick={() => (status = id as ReportStatus)}>{l}</button>{/each}
          </div>
          {#if status === 'duplicate'}
            <div class="rp-dup">
              <select class="plbls-select" aria-label="Signalement d'origine" bind:value={duplicateOf} required>
                <option value="">Signalement d'origine…</option>
                {#each list.filter((r) => r.id !== open?.id) as r (r.id)}<option value={r.id}>{r.title} ({r.author})</option>{/each}
              </select>
              {#if open.duplicate_of}<button type="button" class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => void show(open!.duplicate_of!)}>Voir l'original</button>{/if}
            </div>
          {/if}
          <div class="plbls-field">
            <label class="plbls-field__label" for="rp-notes">Notes internes (invisibles de l'utilisateur)</label>
            <textarea id="rp-notes" class="plbls-input plbls-textarea" rows="3" placeholder="Notes internes (invisibles de l'utilisateur)" aria-label="Notes internes" bind:value={notes} maxlength="4000"></textarea>
          </div>
          <div class="rp-f">
            <button type="button" class="plbls-button plbls-button--discret dgr" onclick={drop}>Supprimer</button>
            <span class="grow"></span>
            <button type="button" class="plbls-button plbls-button--contour" onclick={copy}><Icon name="copie" size={16} />Copier</button>
            <button class="plbls-button">Enregistrer</button>
          </div>
        </form>
      {:else}
        <div class="v-empty rp-empty"><Icon name="drapeau" size={28} /><p class="v-note">Choisis un signalement pour le traiter.</p></div>
      {/if}
    </div>
  </div>
</section>

<style>
  .rp-card { display: flex; flex-direction: column; padding: 0; overflow: hidden; }
  .rp-filtres { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 16px 20px; border-bottom: 1px solid var(--trait); }
  .rp-filtres .plbls-select, .rp-filtres .plbls-input { width: auto; flex: 1 1 140px; height: 36px; min-width: 0; }
  .rp-t { margin: 0 8px 0 0; flex: 1 0 100%; font-family: var(--f-titre); font-size: 20px; font-weight: 700; letter-spacing: -0.02em; }
  .rp-panes { display: grid; grid-template-columns: minmax(260px, 340px) minmax(0, 1fr); min-height: 420px; }
  .rp-list { list-style: none; margin: 0; padding: 0; border-right: 1px solid var(--trait); max-height: 70vh; overflow-y: auto; }
  .rp-row { display: flex; flex-direction: column; align-items: stretch; gap: 6px; width: 100%; padding: 12px 14px; border: 0; border-bottom: 1px solid var(--trait); background: transparent; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .rp-row:hover { background: var(--papier-2); }
  .rp-row[aria-current='true'] { background: var(--rose-doux); }
  .rp-row:focus-visible { outline: none; box-shadow: inset var(--anneau); }
  .rp-row-t { display: flex; flex-direction: column; gap: 2px; min-width: 0; font-size: 13.5px; }
  .rp-row-t .v-mono { font-size: 11px; }
  .rp-tags { display: flex; flex-wrap: wrap; gap: 4px; }
  .rp-vide { padding: 16px; }
  .rp-detail { display: flex; flex-direction: column; gap: 14px; padding: 18px 22px; min-width: 0; }
  .rp-back { align-self: flex-start; }
  .rp-head { display: flex; flex-direction: column; gap: 4px; }
  .rp-head h3 { font-size: 22px; }
  .rp-head p { margin: 0; }
  .desc { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; font-size: 14px; }
  .rp-shots { display: flex; flex-wrap: wrap; gap: 8px; }
  .shot { display: block; max-width: 100%; max-height: 18rem; border-radius: var(--r-s); box-shadow: var(--o1); }
  .rp-det { display: flex; flex-direction: column; gap: 8px; }
  .rp-det > summary { cursor: pointer; margin-bottom: 8px; }
  .rp-det > summary:focus-visible { outline: none; box-shadow: var(--anneau); border-radius: var(--r-xs); }
  .rp-kv { list-style: none; margin: 0; padding: 0; }
  .rp-kv li { display: flex; gap: 10px; padding: 6px 0; border-bottom: 1px solid var(--trait); font-size: 13px; overflow-wrap: anywhere; }
  .rp-kv li > span:first-child { flex: 0 0 120px; color: var(--gris); }
  .logs { margin: 0 0 8px; max-height: 16rem; overflow: auto; padding: 12px; border-radius: var(--r); background: var(--encre); color: var(--papier); font-family: var(--f-mono); font-size: 11.5px; line-height: 1.5; white-space: pre-wrap; word-break: break-word; }
  .rp-form { display: flex; flex-direction: column; gap: 12px; padding-top: 14px; border-top: 1px solid var(--trait); }
  .rp-form h4 { margin: 0; font-weight: 400; }
  .rp-seg { flex-wrap: wrap; align-self: flex-start; border-radius: var(--r); }
  .rp-seg > button:focus-visible { outline: none; box-shadow: var(--anneau); }
  .rp-dup { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  .rp-dup .plbls-select { flex: 1 1 220px; width: auto; }
  textarea { resize: vertical; }
  .rp-f { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  .grow { flex: 1; }
  .dgr { color: var(--danger); }
  .rp-empty { margin: auto 0; }
  @media (pointer: coarse) { .rp-seg > button { min-height: 44px; } }
  @media (max-width: 900px) {
    .rp-panes { grid-template-columns: minmax(0, 1fr); }
    .rp-list { border-right: 0; max-height: none; }
    .rp-panes.detail .rp-list, .rp-panes:not(.detail) .rp-detail { display: none; }
  }
</style>
