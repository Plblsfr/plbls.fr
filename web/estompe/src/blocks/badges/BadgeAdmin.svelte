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
  <span class="bar-q"><Icon name="cherche" size={16} /><input class="plbls-input" type="search" placeholder="Rechercher un badge" aria-label="Rechercher un badge" bind:value={query} /></span>
  <select class="plbls-select bar-sort" aria-label="Trier par" bind:value={sort}>
    {#each BADGE_SORTS as [v, label]}<option value={v}>{label}</option>{/each}
  </select>
  <button class="plbls-button" onclick={() => show()}><Icon name="plus" size={16} />Nouveau badge</button>
</div>

<ul class="grid">
  {#each shown as b (b.id)}
    <li>
      <button class="bd" onclick={() => show(b)} aria-label="Modifier le badge {b.name}" style:--accent={b.color}>
        <span class="bi"><BadgeIcon badge={b} size={26} /></span>
        <span class="bd-txt">
          <b class="v-ell">{b.name}</b>
          {#if b.description}<span class="v-note desc">{b.description}</span>{/if}
          <span class="v-mono v-mute">{count(b.holders.length)}</span>
        </span>
      </button>
    </li>
  {:else}<li class="v-note">{badges.length ? 'Aucun badge ne correspond' : 'Aucun badge'}</li>{/each}
</ul>

{#if open}
  {#if confirming && current}
    <Modal title="Supprimer le badge" onclose={() => (confirming = false)}>
      <div class="stk">
        <p class="v-note">Supprimer le badge « {current.name} » ? {#if current.holders.length}Il sera retiré à {count(current.holders.length).toLowerCase()}.{:else}Personne ne le détient.{/if} Cette action est définitive.</p>
        <div class="actions">
          <button class="plbls-button plbls-button--contour" onclick={() => (confirming = false)}>Annuler</button>
          <button class="plbls-button plbls-button--danger" disabled={busy} onclick={remove}>Supprimer</button>
        </div>
      </div>
    </Modal>
  {:else}
    <Modal title={current ? 'Modifier le badge' : 'Nouveau badge'} wide onclose={close}>
      <div class="stk">
        <form class="stk" onsubmit={(e) => { e.preventDefault(); void save() }}>
          <div class="icon-row" style:--accent={form.color}>
            <span class="bi big" title="Aperçu">
              {#if icon}<img src={icon.url} alt="" width="44" height="44" />
              {:else if current && !dropIcon}<BadgeIcon badge={current} size={44} />
              {:else}<Icon name="medaille" size={32} />{/if}
            </span>
            <span class="bi real" title="Aperçu à la taille réelle">
              {#if icon}<img src={icon.url} alt="" width="12" height="12" />
              {:else if current && !dropIcon}<BadgeIcon badge={current} size={12} />{/if}
            </span>
            <div class="stk tight grow">
              <span class="v-note">Icône : SVG, WebP, PNG ou JPEG (carrée, 512 px au plus)</span>
              <div class="row-w">
                <label class="plbls-button plbls-button--compact plbls-button--contour">Choisir une image<input type="file" accept={BADGE_ICON_ACCEPT} hidden onchange={pickIcon} /></label>
                {#if icon || (current?.icon_version && !dropIcon)}
                  <button type="button" class="plbls-button plbls-button--compact plbls-button--discret dgr" onclick={() => { const had = !!icon; clearIcon(); dropIcon = !had || !!current?.icon_version }}>Retirer l'image</button>
                {/if}
              </div>
            </div>
          </div>
          <div class="plbls-field"><label class="plbls-field__label" for="bd-name">Nom</label><input id="bd-name" class="plbls-input" bind:value={form.name} required minlength="2" maxlength="32" /></div>
          <div class="plbls-field"><label class="plbls-field__label" for="bd-desc">Description</label><input id="bd-desc" class="plbls-input" bind:value={form.description} maxlength="200" /></div>
          <div class="row-w">
            <div class="plbls-field grow"><label class="plbls-field__label" for="bd-color">Couleur d'accent</label><span class="row-w"><input id="bd-color" class="swatch" type="color" bind:value={form.color} /><span class="v-mono v-mute">{form.color}</span></span></div>
            <div class="plbls-field"><label class="plbls-field__label" for="bd-order">Ordre d'affichage</label><input id="bd-order" class="plbls-input v-mono num" type="number" min="0" max="9999" bind:value={form.sort_order} /></div>
          </div>
          {#if message}<p class="err" role="alert"><Icon name="alerte" size={14} />{message}</p>{/if}
          <div class="actions">
            {#if current}<button type="button" class="plbls-button plbls-button--discret dgr" onclick={() => (confirming = true)}>Supprimer</button>{/if}
            <span class="grow"></span>
            <button type="button" class="plbls-button plbls-button--contour" onclick={close}>Annuler</button>
            <button class="plbls-button" disabled={busy}>{current ? 'Enregistrer' : 'Créer'}</button>
          </div>
        </form>

        {#if current}
          <section class="stk holders-sec" aria-label="Détenteurs">
            <h3 class="v-lbl">{count(current.holders.length)}</h3>
            <ul class="holders">
              {#each current.holders as h (h.id)}
                <li class="v-chip">
                  <Avatar pseudo={h.pseudo} userId={h.id} size="sm" />
                  <span class="v-ell">{h.pseudo}</span>
                  <button class="chip-x" onclick={() => revoke(h.pseudo)} aria-label="Retirer à {h.pseudo}" title="Retirer"><Icon name="x" size={12} /></button>
                </li>
              {/each}
            </ul>
            <div class="stk tight">
              <input class="plbls-input" type="search" placeholder="Ajouter des détenteurs : chercher un pseudo" aria-label="Chercher un pseudo à ajouter" role="combobox" aria-expanded={choices.length > 0} aria-controls="holder-choices" autocomplete="off" bind:value={q} />
              {#if choices.length}
                <ul class="choices v-menu" id="holder-choices" role="listbox">
                  {#each choices as u (u.id)}
                    <li role="option" aria-selected="false"><button type="button" class="v-mi" onclick={() => pick(u)}><Avatar pseudo={u.pseudo} userId={u.id} size="sm" /> {u.pseudo}</button></li>
                  {/each}
                </ul>
              {/if}
              {#if picked.length}
                <div class="row-w">
                  {#each picked as u (u.id)}
                    <span class="v-chip on">{u.pseudo} <button type="button" class="chip-x" onclick={() => (picked = picked.filter((p) => p.id !== u.id))} aria-label="Ne pas ajouter {u.pseudo}"><Icon name="x" size={12} /></button></span>
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
  .bar { display: flex; flex-wrap: wrap; gap: 8px; }
  .bar-q { position: relative; display: flex; align-items: center; flex: 1 1 14rem; min-width: 0; color: var(--gris-2); }
  .bar-q :global(svg) { position: absolute; left: 14px; pointer-events: none; }
  .bar-q input { padding-left: 38px; }
  .bar-sort { width: auto; flex: 0 1 220px; }
  .grow { flex: 1 1 auto; min-width: 0; }
  .grid { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 18rem), 1fr)); gap: 12px; }
  .bd { width: 100%; height: 100%; display: flex; align-items: flex-start; gap: 14px; padding: 16px; text-align: left; cursor: pointer; border: 0; border-radius: var(--r-l); background: var(--surface); color: inherit; font: inherit;
    box-shadow: inset 3px 0 0 var(--accent), var(--o1); transition: background var(--d) var(--sortie), box-shadow var(--d) var(--sortie); }
  .bd:hover { background: var(--surface-2); box-shadow: inset 3px 0 0 var(--accent), var(--o2); }
  .bd:focus-visible { outline: none; box-shadow: inset 3px 0 0 var(--accent), var(--anneau); }
  .bd-txt { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
  .bi { display: grid; place-items: center; flex: none; width: 44px; height: 44px; border-radius: 14px; background: color-mix(in srgb, var(--accent) 18%, var(--surface)); color: color-mix(in srgb, var(--accent) 70%, var(--encre)); font-size: 22px; }
  .bi.big { width: 72px; height: 72px; border-radius: 22px; }
  .bi.real { width: 18px; height: 18px; border-radius: 6px; font-size: 11px; }
  .desc { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; font-size: 13px; }
  .icon-row { display: flex; gap: 16px; align-items: center; flex-wrap: wrap; }
  .stk { display: flex; flex-direction: column; gap: 14px; }
  .stk.tight { gap: 6px; }
  .row-w { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .num { width: 7rem; }
  .swatch { width: 42px; height: 42px; padding: 0; border: 0; border-radius: var(--r-s); background: none; cursor: pointer; }
  .swatch::-webkit-color-swatch-wrapper { padding: 0; }
  .swatch::-webkit-color-swatch { border: 0; border-radius: var(--r-s); box-shadow: inset 0 0 0 1px var(--trait-fort); }
  .swatch::-moz-color-swatch { border: 0; border-radius: var(--r-s); }
  .swatch:focus-visible { outline: none; box-shadow: var(--anneau); }
  .err { display: flex; align-items: center; gap: 6px; margin: 0; color: var(--danger); font-size: 13px; }
  .dgr { color: var(--danger); }
  .actions { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; justify-content: flex-end; }
  h3 { margin: 0; }
  .holders-sec { padding-top: 14px; box-shadow: 0 -1px 0 var(--trait); }
  .holders { list-style: none; margin: 0; padding: 0; display: flex; flex-wrap: wrap; gap: 6px; max-height: 14rem; overflow-y: auto; }
  .holders .v-chip { padding-left: 4px; max-width: 100%; }
  .holders :global(.av) { width: 20px; height: 20px; font-size: 9px; }
  .chip-x { display: inline-flex; padding: 2px; margin-right: -4px; border: 0; border-radius: var(--r-p); background: none; color: inherit; opacity: 0.7; cursor: pointer; }
  .chip-x:hover { opacity: 1; background: var(--papier-3); }
  .chip-x:focus-visible { outline: none; opacity: 1; box-shadow: var(--anneau); }
  .choices { list-style: none; margin: 0; max-height: 12rem; overflow-y: auto; min-width: 0; }
  .choices :global(.av) { width: 22px; height: 22px; font-size: 10px; }
  @media (pointer: coarse) { .chip-x { padding: 8px; margin: -6px -8px -6px 0; } .v-mi { height: 44px; } }
</style>
