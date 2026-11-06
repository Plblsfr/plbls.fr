<script lang="ts">
  import { confirmer } from '../lib/confirm.svelte'
  import Icon from './Icon.svelte'
  import { app } from '../lib/store.svelte'
  import { api } from '../lib/api'
  import { notify } from '../lib/ui'
  import { PERM_GROUPS, rankOf } from '../lib/roles'
  import type { PermKey, Role, ServerDetail } from '../lib/types'

  let { detail }: { detail: ServerDetail } = $props()

  const sid = $derived(detail.server.id)
  const owner = $derived(detail.server.owner_id === app.meId)
  const myTop = $derived(rankOf(detail, app.meId))
  const mine = $derived<PermKey[]>(detail.permissions ?? [])
  const roles = $derived(detail.roles ?? [])
  const list = $derived(roles.filter((r) => !r.is_default))
  const everyone = $derived(roles.find((r) => r.is_default))

  let selectedId = $state<string | null>(null)
  let draft = $state<{ name: string; color: string; icon: string; hoist: boolean; permissions: PermKey[] } | null>(null)
  let query = $state('')
  let dragId = $state<string | null>(null)

  const selected = $derived(roles.find((r) => r.id === selectedId))
  // Un rôle au niveau du vôtre ou au-dessus se consulte sans se modifier (« @tout le monde » : seulement ses permissions).
  const editable = $derived(!!selected && (selected.is_default || owner || selected.position < myTop))
  const movable = (r: Role) => owner || r.position < myTop

  function select(r: Role) {
    selectedId = r.id
    draft = { name: r.name, color: r.color, icon: r.icon, hoist: r.hoist, permissions: [...r.permissions] }
    query = ''
  }

  async function run(fn: () => Promise<unknown>, ok?: string) {
    try {
      await fn()
      if (ok) notify.ok(ok)
    } catch (e) {
      notify.error(e)
    }
  }

  const create = () => run(async () => {
    const r = await api.createRole(sid, { name: 'Nouveau rôle', color: '', icon: '', hoist: false, permissions: [] })
    select(r)
  })

  const save = () => run(async () => {
    if (!selected || !draft) return
    select(await api.updateRole(sid, selected.id, { ...draft, name: draft.name.trim() }))
  }, 'Rôle enregistré')

  const remove = async () => {
    if (!selected || !(await confirmer({ titre: 'Supprimer le rôle', message: `Supprimer le rôle « ${selected.name} » ? Ses membres le perdent aussitôt.`, action: 'Supprimer le rôle', destructif: true }))) return
    void run(async () => {
      await api.deleteRole(sid, selected!.id)
      selectedId = null
      draft = null
    })
  }

  function toggle(key: PermKey, on: boolean) {
    if (!draft) return
    draft.permissions = on ? [...draft.permissions, key] : draft.permissions.filter((k) => k !== key)
  }

  const canChange = (key: PermKey) => owner || mine.includes(key)

  // Ordre : du plus élevé au plus bas ; un rôle ne passe pas au-dessus de ceux que l'on ne peut pas déplacer.
  async function reorder(from: number, to: number) {
    if (from === to || to < 0 || to >= list.length || !movable(list[from]) || !movable(list[to])) return
    const ids = list.map((r) => r.id)
    ids.splice(to, 0, ...ids.splice(from, 1))
    await run(() => api.reorderRoles(sid, ids))
  }

  const members = $derived(selected ? detail.members.filter((m) => m.roles?.includes(selected.id)) : [])
  const manageable = (id: string) => owner ? true : myTop > rankOf(detail, id)
  const matches = $derived(
    selected && query.trim()
      ? detail.members.filter((m) => !m.roles?.includes(selected.id) && manageable(m.id) && m.pseudo.toLowerCase().includes(query.trim().toLowerCase())).slice(0, 6)
      : [],
  )
  const setMember = (uid: string, on: boolean) => run(async () => { await api.setMemberRole(sid, uid, selected!.id, on); query = '' })
</script>

<div class="roles">
  <div class="stack role-list">
    <div class="row"><strong class="grow">Rôles</strong><button class="plbls-button plbls-button--compact" onclick={create}>Nouveau rôle</button></div>
    {#each list as r, i (r.id)}
      <div
        class="row role-row"
        class:active={r.id === selectedId}
        role="listitem"
        draggable={movable(r)}
        ondragstart={() => (dragId = r.id)}
        ondragover={(e) => { if (dragId) e.preventDefault() }}
        ondrop={(e) => { e.preventDefault(); const from = list.findIndex((x) => x.id === dragId); dragId = null; void reorder(from, i) }}
      >
        <button class="plbls-button plbls-button--compact plbls-button--contour grow role-name" onclick={() => select(r)}>
          <span class="dot" style:background={r.color || 'var(--gris)'} aria-hidden="true"></span>
          <span class="truncate">{`${r.icon} ${r.name}`.trim()}</span>
          {#if r.hoist}<span class="muted small">séparé</span>{/if}
        </button>
        {#if movable(r)}
          <button class="ib sm" onclick={() => reorder(i, i - 1)} disabled={i === 0 || !movable(list[i - 1])} aria-label="Monter {r.name}"><Icon name="up" size={14} /></button>
          <button class="ib sm" onclick={() => reorder(i, i + 1)} disabled={i === list.length - 1} aria-label="Descendre {r.name}"><Icon name="down" size={14} /></button>
        {/if}
      </div>
    {/each}
    {#if everyone}
      <div class="row role-row" class:active={everyone.id === selectedId}>
        <button class="plbls-button plbls-button--compact plbls-button--contour grow role-name" onclick={() => select(everyone)}>{everyone.name}</button>
      </div>
    {/if}
    <p class="muted small">Les rôles du haut l'emportent. On ne gère que les rôles situés sous le sien. Glisser-déposer ou flèches pour réordonner.</p>
  </div>

  {#if selected && draft}
    <form class="stack role-edit" onsubmit={(e) => { e.preventDefault(); void save() }}>
      {#if !editable}<p class="muted small">Ce rôle est au niveau du vôtre ou au-dessus : lecture seule.</p>{/if}
      <fieldset class="stack" disabled={!editable} style="border: 0; padding: 0; margin: 0">
        {#if !selected.is_default}
          <div class="plbls-field">
            <label class="plbls-field__label" for="r-name">Nom</label>
            <input id="r-name" class="plbls-input" bind:value={draft.name} maxlength="30" required />
          </div>
          <div class="row">
            <div class="plbls-field">
              <label class="plbls-field__label" for="r-color">Couleur</label>
              <div class="row">
                <input id="r-color" type="color" value={draft.color || '#99aab5'} oninput={(e) => draft && (draft.color = e.currentTarget.value)} />
                {#if draft.color}<button type="button" class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => draft && (draft.color = '')}>Aucune</button>{/if}
              </div>
            </div>
            <div class="plbls-field grow">
              <label class="plbls-field__label" for="r-icon">Icône (emoji)</label>
              <input id="r-icon" class="plbls-input" bind:value={draft.icon} maxlength="8" placeholder="facultatif" />
            </div>
          </div>
          <label class="plbls-toggle">
            <input class="plbls-toggle__input" type="checkbox" bind:checked={draft.hoist} />
            <span class="plbls-toggle__piste"></span>
            <span class="plbls-toggle__texte">Afficher séparément dans la liste des membres</span>
          </label>
        {:else}
          <p class="muted small">Appliqué à tous les membres, il ne se supprime pas. Les rôles s'ajoutent à ces permissions.</p>
        {/if}

        {#each PERM_GROUPS as g (g.title)}
          <div class="stack">
            <strong>{g.title}</strong>
            {#each g.perms as p (p.key)}
              <label class="plbls-toggle perm">
                <input class="plbls-toggle__input" type="checkbox" checked={draft.permissions.includes(p.key)} disabled={!canChange(p.key)} onchange={(e) => toggle(p.key, e.currentTarget.checked)} />
                <span class="plbls-toggle__piste"></span>
                <span class="plbls-toggle__texte">{p.label}<span class="muted small perm-desc">{p.desc}</span></span>
              </label>
            {/each}
          </div>
        {/each}
        <p class="muted small">Vous ne pouvez accorder ou retirer que des permissions que vous avez vous-même.</p>
      </fieldset>

      {#if !selected.is_default}
        <section class="stack">
          <strong>Membres · {members.length}</strong>
          {#each members as m (m.id)}
            <div class="row">
              <span class="grow truncate">{m.pseudo}</span>
              {#if editable && manageable(m.id)}<button type="button" class="ib sm" onclick={() => setMember(m.id, false)} aria-label="Retirer {m.pseudo} du rôle"><Icon name="x" size={14} /></button>{/if}
            </div>
          {:else}
            <p class="muted small">Personne n'a ce rôle.</p>
          {/each}
          {#if editable}
            <input class="plbls-input" bind:value={query} placeholder="Ajouter un membre…" aria-label="Rechercher un membre à ajouter" />
            {#each matches as m (m.id)}
              <button type="button" class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => setMember(m.id, true)}>Ajouter {m.pseudo}</button>
            {/each}
          {/if}
        </section>
      {/if}

      {#if editable}
        <div class="row actions">
          <button class="plbls-button" disabled={!draft.name.trim()}>Enregistrer</button>
          {#if !selected.is_default}<button type="button" class="plbls-button plbls-button--danger plbls-button--contour" onclick={remove}>Supprimer le rôle</button>{/if}
        </div>
      {/if}
    </form>
  {:else}
    <p class="muted">Choisissez un rôle pour le modifier, ou créez-en un.</p>
  {/if}
</div>

<style>
  .roles { display: grid; gap: var(--espace-4); min-height: 0; overflow-y: auto; }
  .actions { position: sticky; bottom: 0; padding: var(--espace-2) 0; background: var(--surface); }
  .role-row.active .role-name { background: var(--rose); }
  .role-name { display: flex; align-items: center; gap: var(--espace-2); justify-content: flex-start; }
  .dot { width: 10px; height: 10px; border-radius: 50%; flex: none; }
  .perm-desc { display: block; }
  @media (min-width: 720px) {
    .roles { grid-template-columns: minmax(180px, 240px) 1fr; grid-template-rows: minmax(0, 1fr); overflow: hidden; }
    .role-list, .role-edit { min-height: 0; overflow-y: auto; align-content: start; }
  }
</style>
