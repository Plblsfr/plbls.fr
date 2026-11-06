<script lang="ts">
  import Avatar from '../../components/Avatar.svelte'
  import BadgeIcon from './BadgeIcon.svelte'
  import Icon from '../../components/Icon.svelte'
  import Modal from '../../components/Modal.svelte'
  import { api } from '../../lib/api'
  import { squareAvatar } from '../avatars/avatars.svelte'
  import { BADGE_ICON_ACCEPT, BADGE_SORTS, badgeIconProblem, filterBadges, type BadgeSort } from './badges'
  import type { AdminBadge, AdminUser } from '../../lib/types'

  let { badges, reload, fail }: { badges: AdminBadge[]; reload: () => Promise<unknown>; fail: (e: unknown) => void } = $props()

  let query = $state('')
  let sort = $state<BadgeSort>('order')
  const shown = $derived(filterBadges(badges, query, sort))

  // null : modal fermé ; 'new' : création ; sinon identifiant du badge ouvert
  let open = $state<string | null>(null)
  let confirming = $state(false)
  let busy = $state(false)
  let message = $state('')
  let form = $state({ name: '', description: '', color: '#3b82f6', sort_order: 0 })
  let icon = $state<{ blob: Blob; url: string } | null>(null)
  let dropIcon = $state(false)
  const current = $derived(badges.find((b) => b.id === open))

  function show(b?: AdminBadge) {
    open = b ? b.id : 'new'
    form = b ? { name: b.name, description: b.description, color: b.color, sort_order: b.sort_order } : { name: '', description: '', color: '#3b82f6', sort_order: 0 }
    clearIcon()
    message = ''
    confirming = false
    picked = []
    q = ''
  }
  function close() { open = null; clearIcon() }
  function clearIcon() {
    if (icon) URL.revokeObjectURL(icon.url)
    icon = null
    dropIcon = false
  }

  async function pickIcon(e: Event) {
    const input = e.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (!file) return
    message = badgeIconProblem(file)
    if (message) return
    try {
      // SVG envoyé tel quel (nettoyé par le serveur) ; image matricielle recadrée en carré
      const blob = file.type === 'image/svg+xml' ? file : await squareAvatar(file)
      clearIcon()
      icon = { blob, url: URL.createObjectURL(blob) }
    } catch {
      message = "Image illisible."
    }
  }

  async function save() {
    busy = true
    message = ''
    try {
      const body = { ...form, name: form.name.trim(), description: form.description.trim(), sort_order: Number(form.sort_order) || 0 }
      let id = open!
      if (open === 'new') {
        id = (await api.createBadge(body)).id
        open = id
      } else await api.updateBadge(id, body)
      if (icon) await api.setBadgeIcon(id, icon.blob)
      else if (dropIcon) await api.deleteBadgeIcon(id)
      clearIcon()
      await reload()
      close()
    } catch (e) {
      message = String((e as Error).message ?? e)
      await reload().catch(() => {})
    } finally {
      busy = false
    }
  }

  async function remove() {
    busy = true
    try {
      await api.deleteBadge(open!)
      await reload()
      close()
    } catch (e) { fail(e) } finally { busy = false }
  }

  // Attribution : recherche de pseudos avec autocomplétion, plusieurs comptes à la fois
  let q = $state('')
  let found = $state<AdminUser[]>([])
  let picked = $state<AdminUser[]>([])
  $effect(() => {
    const text = q.trim()
    if (!text || !current) { found = []; return }
    const t = setTimeout(() => api.adminUsers(text).then((l) => (found = l)).catch(() => (found = [])), 200)
    return () => clearTimeout(t)
  })
  const choices = $derived(found.filter((u) => !picked.some((p) => p.id === u.id) && !current?.holders.some((h) => h.id === u.id)))
  const pick = (u: AdminUser) => { picked = [...picked, u]; q = '' }

  async function grant() {
    if (!current || !picked.length) return
    try {
      const r = await api.grantBadges(current.id, picked.map((u) => u.pseudo))
      message = r.missing.length ? `Comptes introuvables : ${r.missing.join(', ')}` : ''
      picked = []
      await reload()
    } catch (e) { fail(e) }
  }
  const revoke = async (pseudo: string) => {
    try { await api.revokeBadge(current!.id, pseudo); await reload() } catch (e) { fail(e) }
  }
  const count = (n: number) => (n === 0 ? 'Aucun détenteur' : n === 1 ? '1 détenteur' : `${n} détenteurs`)
</script>

<div class="bar">
  <input class="plbls-input grow" type="search" placeholder="Rechercher un badge" aria-label="Rechercher un badge" bind:value={query} />
  <select class="plbls-input" aria-label="Trier par" bind:value={sort}>
    {#each BADGE_SORTS as [v, label]}<option value={v}>{label}</option>{/each}
  </select>
  <button class="plbls-button plbls-button--compact" onclick={() => show()}>Nouveau badge</button>
</div>

<ul class="grid">
  {#each shown as b (b.id)}
    <li>
      <button class="card" onclick={() => show(b)} aria-label="Modifier le badge {b.name}" style="--accent: {b.color}">
        <span class="card-icon"><BadgeIcon badge={b} size={40} /></span>
        <strong class="truncate">{b.name}</strong>
        {#if b.description}<span class="small muted desc">{b.description}</span>{/if}
        <span class="small muted">{count(b.holders.length)}</span>
      </button>
    </li>
  {:else}<li class="muted">{badges.length ? 'Aucun badge ne correspond' : 'Aucun badge'}</li>{/each}
</ul>

{#if open}
  {#if confirming && current}
    <Modal title="Supprimer le badge" onclose={() => (confirming = false)}>
      <div class="stack">
        <p>Supprimer le badge « {current.name} » ? {#if current.holders.length}Il sera retiré à {count(current.holders.length).toLowerCase()}.{:else}Personne ne le détient.{/if} Cette action est définitive.</p>
        <div class="actions">
          <button class="plbls-button plbls-button--contour" onclick={() => (confirming = false)}>Annuler</button>
          <button class="plbls-button plbls-button--danger" disabled={busy} onclick={remove}>Supprimer</button>
        </div>
      </div>
    </Modal>
  {:else}
    <Modal title={current ? 'Modifier le badge' : 'Nouveau badge'} wide onclose={close}>
      <div class="stack">
        <form class="stack" onsubmit={(e) => { e.preventDefault(); void save() }}>
          <div class="icon-row">
            <span class="preview big" style="border-color: {form.color}">
              {#if icon}<img src={icon.url} alt="" width="64" height="64" />
              {:else if current && !dropIcon}<BadgeIcon badge={current} size={64} />
              {:else}<Icon name="pin" size={28} />{/if}
            </span>
            <span class="preview real" style="border-color: {form.color}">
              {#if icon}<img src={icon.url} alt="" width="14" height="14" />
              {:else if current && !dropIcon}<BadgeIcon badge={current} size={14} />{/if}
            </span>
            <div class="stack grow">
              <span class="small muted">Icône : SVG, WebP, PNG ou JPEG (carrée, 512 px au plus)</span>
              <div class="row wrap">
                <label class="plbls-button plbls-button--compact plbls-button--contour">Choisir une image<input type="file" accept={BADGE_ICON_ACCEPT} hidden onchange={pickIcon} /></label>
                {#if icon || (current?.icon_version && !dropIcon)}
                  <button type="button" class="plbls-button plbls-button--compact plbls-button--danger plbls-button--contour" onclick={() => { const had = !!icon; clearIcon(); dropIcon = !had || !!current?.icon_version }}>Retirer l'image</button>
                {/if}
              </div>
            </div>
          </div>
          <label class="stack tight">Nom<input class="plbls-input" bind:value={form.name} required minlength="2" maxlength="32" /></label>
          <label class="stack tight">Description<input class="plbls-input" bind:value={form.description} maxlength="200" /></label>
          <div class="row wrap">
            <label class="row">Couleur d'accent<input type="color" bind:value={form.color} /></label>
            <label class="row">Ordre d'affichage<input class="plbls-input num" type="number" min="0" max="9999" bind:value={form.sort_order} /></label>
          </div>
          {#if message}<p class="small" style="color: var(--danger)" role="alert">{message}</p>{/if}
          <div class="actions">
            {#if current}<button type="button" class="plbls-button plbls-button--danger plbls-button--contour" onclick={() => (confirming = true)}>Supprimer</button>{/if}
            <span class="grow"></span>
            <button type="button" class="plbls-button plbls-button--contour" onclick={close}>Annuler</button>
            <button class="plbls-button" disabled={busy}>{current ? 'Enregistrer' : 'Créer'}</button>
          </div>
        </form>

        {#if current}
          <section class="stack" aria-label="Détenteurs">
            <h3>{count(current.holders.length)}</h3>
            <ul class="holders">
              {#each current.holders as h (h.id)}
                <li>
                  <Avatar pseudo={h.pseudo} userId={h.id} size="sm" />
                  <span class="grow truncate">{h.pseudo}</span>
                  <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => revoke(h.pseudo)} aria-label="Retirer à {h.pseudo}">Retirer</button>
                </li>
              {/each}
            </ul>
            <div class="stack tight">
              <input class="plbls-input" type="search" placeholder="Ajouter des détenteurs : chercher un pseudo" aria-label="Chercher un pseudo à ajouter" role="combobox" aria-expanded={choices.length > 0} aria-controls="holder-choices" autocomplete="off" bind:value={q} />
              {#if choices.length}
                <ul class="choices" id="holder-choices" role="listbox">
                  {#each choices as u (u.id)}
                    <li role="option" aria-selected="false"><button type="button" onclick={() => pick(u)}><Avatar pseudo={u.pseudo} userId={u.id} size="sm" /> {u.pseudo}</button></li>
                  {/each}
                </ul>
              {/if}
              {#if picked.length}
                <div class="row wrap">
                  {#each picked as u (u.id)}
                    <span class="plbls-badge chip">{u.pseudo} <button type="button" class="ib sm" onclick={() => (picked = picked.filter((p) => p.id !== u.id))} aria-label="Ne pas ajouter {u.pseudo}"><Icon name="x" size={12} /></button></span>
                  {/each}
                  <button type="button" class="plbls-button plbls-button--compact" onclick={grant}>Attribuer à {picked.length} compte{picked.length > 1 ? 's' : ''}</button>
                </div>
              {/if}
            </div>
          </section>
        {/if}
      </div>
    </Modal>
  {/if}
{/if}

<style>
  .bar { display: flex; flex-wrap: wrap; gap: 0.5rem; margin-bottom: var(--espace-3); }
  .grow { flex: 1 1 12rem; min-width: 0; }
  .grid { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 12rem), 1fr)); gap: var(--espace-3); }
  .card { width: 100%; height: 100%; display: grid; justify-items: start; align-content: start; gap: 4px; padding: var(--espace-3); text-align: left; cursor: pointer;
    background: var(--creme); color: inherit; border: 2px solid var(--noir); border-top-color: var(--accent); border-radius: var(--rayon); }
  .card:hover, .card:focus-visible { background: var(--rose); }
  .card-icon { display: grid; place-items: center; width: 48px; height: 48px; font-size: 32px; }
  .desc { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .icon-row { display: flex; gap: var(--espace-3); align-items: center; flex-wrap: wrap; }
  .preview { display: grid; place-items: center; border: 2px solid; border-radius: var(--rayon); font-size: 40px; flex: none; }
  .preview.big { width: 80px; height: 80px; }
  .preview.real { width: 28px; height: 28px; font-size: 14px; }
  .row.wrap { flex-wrap: wrap; }
  .stack.tight { gap: 4px; }
  .num { width: 6rem; }
  .actions { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; justify-content: flex-end; }
  h3 { margin: 0; }
  .holders, .choices { list-style: none; margin: 0; padding: 0; }
  .holders { max-height: 14rem; overflow-y: auto; }
  .holders li { display: flex; align-items: center; gap: var(--espace-2); padding: 4px 0; }
  .choices { border: 2px solid var(--noir); border-radius: var(--rayon); max-height: 10rem; overflow-y: auto; }
  .choices button { width: 100%; display: flex; align-items: center; gap: var(--espace-2); padding: 6px 8px; text-align: left; background: none; border: 0; color: inherit; cursor: pointer; }
  .choices button:hover, .choices button:focus-visible { background: var(--rose); }
  .chip { display: inline-flex; align-items: center; gap: 4px; }
</style>
