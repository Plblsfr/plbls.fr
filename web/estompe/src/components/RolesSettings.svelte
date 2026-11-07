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
  let permQuery = $state('')
  const groups = $derived(PERM_GROUPS.map((g) => ({ ...g, perms: g.perms.filter((p) => !permQuery.trim() || `${p.label} ${p.desc}`.toLowerCase().includes(permQuery.trim().toLowerCase())) })).filter((g) => g.perms.length))
  let dragId = $state<string | null>(null)

  const selected = $derived(roles.find((r) => r.id === selectedId))
  // Un rôle au niveau du vôtre ou au-dessus se consulte sans se modifier (« @tout le monde » : seulement ses permissions).
  const editable = $derived(!!selected && (selected.is_default || owner || selected.position < myTop))
  const movable = (r: Role) => owner || r.position < myTop

  function select(r: Role) {
    selectedId = r.id
    draft = { name: r.name, color: r.color, icon: r.icon, hoist: r.hoist, permissions: [...r.permissions] }
    query = ''
    permQuery = ''
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
  <div class="role-list">
    <div class="role-list-h"><span class="v-eyebrow grow">Rôles · {list.length}</span><button class="plbls-button plbls-button--compact" onclick={create}><Icon name="plus" size={16} />Nouveau rôle</button></div>
    {#each list as r, i (r.id)}
      <div
        class="role-row"
        class:active={r.id === selectedId}
        role="listitem"
        draggable={movable(r)}
        ondragstart={() => (dragId = r.id)}
        ondragover={(e) => { if (dragId) e.preventDefault() }}
        ondrop={(e) => { e.preventDefault(); const from = list.findIndex((x) => x.id === dragId); dragId = null; void reorder(from, i) }}
      >
        <button class="role-name" aria-current={r.id === selectedId ? 'true' : undefined} onclick={() => select(r)}>
          <span class="swatch" class:none={!r.color} style:background={r.color || undefined} aria-hidden="true"></span>
          <span class="grow v-ell">{`${r.icon} ${r.name}`.trim()}</span>
          {#if r.hoist}<span class="v-tag">séparé</span>{/if}
        </button>
        {#if movable(r)}
          <span class="role-move">
            <button class="v-ib sm" onclick={() => reorder(i, i - 1)} disabled={i === 0 || !movable(list[i - 1])} aria-label="Monter {r.name}"><Icon name="up" size={14} /></button>
            <button class="v-ib sm" onclick={() => reorder(i, i + 1)} disabled={i === list.length - 1} aria-label="Descendre {r.name}"><Icon name="bas" size={14} /></button>
          </span>
        {/if}
      </div>
    {/each}
    {#if everyone}
      <div class="role-row" class:active={everyone.id === selectedId}>
        <button class="role-name" aria-current={everyone.id === selectedId ? 'true' : undefined} onclick={() => select(everyone)}><span class="swatch none" aria-hidden="true"></span><span class="grow v-ell v-mute">{everyone.name}</span></button>
      </div>
    {/if}
    <p class="v-note role-help">Les rôles du haut l'emportent. On ne gère que les rôles situés sous le sien. Glisser-déposer ou flèches pour réordonner.</p>
  </div>

  {#if selected && draft}
    <form class="role-edit" onsubmit={(e) => { e.preventDefault(); void save() }}>
      {#if !editable}<p class="v-alert att role-ro"><Icon name="cadenas" size={16} />Ce rôle est au niveau du vôtre ou au-dessus : lecture seule.</p>{/if}
      <fieldset class="role-fs" disabled={!editable}>
        {#if !selected.is_default}
          <div class="role-id">
            <div class="plbls-field">
              <label class="plbls-field__label" for="r-name">Nom</label>
              <input id="r-name" class="plbls-input" bind:value={draft.name} maxlength="30" required />
            </div>
            <div class="plbls-field">
              <label class="plbls-field__label" for="r-color">Couleur</label>
              <div class="role-colors">
                <input id="r-color" class="role-color" type="color" value={draft.color || '#99aab5'} oninput={(e) => draft && (draft.color = e.currentTarget.value)} />
                {#if draft.color}<button type="button" class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => draft && (draft.color = '')}>Aucune</button>{:else}<span class="v-note">Aucune</span>{/if}
              </div>
            </div>
            <div class="plbls-field">
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
          <p class="v-note">Appliqué à tous les membres, il ne se supprime pas. Les rôles s'ajoutent à ces permissions.</p>
        {/if}

        <div class="perms-h">
          <span class="v-eyebrow grow">Permissions</span>
          <span class="perm-filter"><Icon name="cherche" size={14} /><input class="plbls-input" type="search" bind:value={permQuery} placeholder="Filtrer" aria-label="Filtrer les permissions" /></span>
        </div>
        {#each groups as g (g.title)}
          <div class="perm-group">
            <b class="perm-title">{g.title}</b>
            <div class="perm-grid">
              {#each g.perms as p (p.key)}
                <label class="plbls-toggle perm">
                  <input class="plbls-toggle__input" type="checkbox" checked={draft.permissions.includes(p.key)} disabled={!canChange(p.key)} onchange={(e) => toggle(p.key, e.currentTarget.checked)} />
                  <span class="plbls-toggle__piste"></span>
                  <span class="plbls-toggle__texte">{p.label}<span class="v-note perm-desc">{p.desc}</span></span>
                </label>
              {/each}
            </div>
          </div>
        {:else}
          <p class="v-note">Aucune permission ne correspond.</p>
        {/each}
        <p class="v-note role-info"><Icon name="info" size={14} />Vous ne pouvez accorder ou retirer que des permissions que vous avez vous-même.</p>
      </fieldset>

      {#if !selected.is_default}
        <section class="role-members" aria-label="Membres du rôle">
          <span class="v-eyebrow">Membres · {members.length}</span>
          <div class="chips">
            {#each members as m (m.id)}
              <span class="v-chip">
                <span class="v-ell">{m.pseudo}</span>
                {#if editable && manageable(m.id)}<button type="button" class="chip-x" onclick={() => setMember(m.id, false)} aria-label="Retirer {m.pseudo} du rôle"><Icon name="x" size={12} /></button>{/if}
              </span>
            {:else}
              <span class="v-note">Personne n'a ce rôle.</span>
            {/each}
          </div>
          {#if editable}
            <input class="plbls-input" bind:value={query} placeholder="Ajouter un membre…" aria-label="Rechercher un membre à ajouter" />
            {#if matches.length}
              <div class="chips">
                {#each matches as m (m.id)}
                  <button type="button" class="v-chip line" onclick={() => setMember(m.id, true)}><Icon name="plus" size={12} />Ajouter {m.pseudo}</button>
                {/each}
              </div>
            {/if}
          {/if}
        </section>
      {/if}

      {#if editable}
        <div class="actions">
          {#if !selected.is_default}<button type="button" class="plbls-button plbls-button--discret danger" onclick={remove}>Supprimer le rôle</button>{/if}
          <button class="plbls-button" disabled={!draft.name.trim()}>Enregistrer</button>
        </div>
      {/if}
    </form>
  {:else}
    <div class="role-edit"><div class="v-empty"><Icon name="couronne" size={28} /><p class="v-note">Choisissez un rôle pour le modifier, ou créez-en un.</p></div></div>
  {/if}
</div>

<style>
  .roles { flex: 1; display: grid; min-height: 0; overflow-y: auto; }
  .role-list { display: flex; flex-direction: column; gap: 2px; padding: 14px 10px; }
  .role-list-h { display: flex; align-items: center; gap: 8px; padding: 0 4px 10px 6px; }
  .role-row { display: flex; align-items: center; gap: 2px; border-radius: var(--r-s); }
  .role-row.active { background: var(--surface); box-shadow: var(--o1); }
  .role-name { display: flex; align-items: center; gap: 10px; flex: 1; min-width: 0; height: 42px; padding: 0 10px; border: 0; border-radius: var(--r-s); background: transparent; color: var(--encre-2); font: inherit; font-size: 14px; text-align: left; cursor: pointer; }
  .role-name:hover { background: var(--papier-2); color: var(--encre); }
  .role-row.active .role-name { color: var(--encre); font-weight: 600; }
  .role-row.active .role-name:hover { background: transparent; }
  .role-name:focus-visible { outline: none; box-shadow: var(--anneau); }
  .role-row[draggable='true'] { cursor: grab; }
  .swatch { width: 12px; height: 12px; border-radius: 4px; flex: none; }
  .swatch.none { background: var(--trait-fort); }
  .role-move { display: flex; opacity: 0; }
  .role-row:hover .role-move, .role-row:focus-within .role-move, .role-row.active .role-move { opacity: 1; }
  .role-help { padding: 10px; font-size: 12px; }
  .role-edit { display: flex; flex-direction: column; gap: 20px; padding: 22px 32px 0; min-width: 0; }
  .role-ro { align-items: center; margin: 0; }
  .role-fs { display: flex; flex-direction: column; gap: 16px; min-width: 0; border: 0; padding: 0; margin: 0; }
  .role-id { display: grid; grid-template-columns: minmax(0, 1fr) auto 160px; gap: 12px; align-items: end; }
  .role-colors { display: flex; align-items: center; gap: 6px; height: 42px; }
  .role-color { width: 42px; height: 42px; padding: 0; border: 0; border-radius: var(--r-s); background: none; cursor: pointer; }
  .role-color::-webkit-color-swatch-wrapper { padding: 0; }
  .role-color::-webkit-color-swatch { border: 0; border-radius: var(--r-s); box-shadow: inset 0 0 0 1px var(--trait-fort); }
  .role-color::-moz-color-swatch { border: 0; border-radius: var(--r-s); }
  .role-color:focus-visible { outline: none; box-shadow: var(--anneau); }
  .perms-h { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin-top: 4px; }
  .perm-filter { position: relative; display: inline-flex; align-items: center; width: min(240px, 100%); color: var(--gris-2); }
  .perm-filter :global(svg) { position: absolute; left: 12px; pointer-events: none; }
  .perm-filter input { height: 34px; padding-left: 32px; font-size: 13px; }
  .perm-group { display: flex; flex-direction: column; gap: 4px; }
  .perm-title { font-family: var(--f-titre); font-size: 15px; margin-top: 4px; }
  .perm-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 0 24px; }
  .perm { padding: 10px 0; align-items: flex-start; box-shadow: 0 1px 0 var(--trait); }
  .perm-desc { display: block; font-size: 12px; }
  .role-info { display: flex; align-items: center; gap: 6px; font-size: 12px; }
  .role-members { display: flex; flex-direction: column; gap: 8px; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip-x { display: inline-flex; padding: 2px; margin-right: -4px; border: 0; border-radius: var(--r-p); background: none; color: var(--gris); cursor: pointer; }
  .chip-x:hover { color: var(--encre); background: var(--papier-3); }
  .chip-x:focus-visible { outline: none; box-shadow: var(--anneau); }
  .actions { position: sticky; bottom: 0; display: flex; justify-content: flex-end; gap: 8px; flex-wrap: wrap; padding: 12px 0 16px; background: var(--surface); box-shadow: 0 -1px 0 var(--trait); }
  .danger { color: var(--danger); }
  @media (hover: none) { .role-move { opacity: 1; } }
  @media (pointer: coarse) { .role-name { height: 44px; } .role-move .v-ib { width: 44px; height: 44px; } }
  @media (min-width: 760px) {
    .roles { grid-template-columns: 260px minmax(0, 1fr); grid-template-rows: minmax(0, 1fr); overflow: hidden; }
    .role-list { min-height: 0; overflow-y: auto; box-shadow: inset -1px 0 0 var(--trait); }
    .role-edit { min-height: 0; overflow-y: auto; }
  }
  @media (max-width: 1100px) { .perm-grid { grid-template-columns: minmax(0, 1fr); } }
  @media (max-width: 560px) { .role-id { grid-template-columns: minmax(0, 1fr); } .role-edit { padding: 16px 16px 0; } }
</style>
