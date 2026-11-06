<script lang="ts">
  import { onMount } from 'svelte'
  import { app } from '../lib/store.svelte'
  import { api } from '../lib/api'
  import { notify } from '../lib/ui'
  import { PERM_GROUPS } from '../lib/roles'
  import type { Override, PermKey, ServerDetail } from '../lib/types'

  let { detail, channelId }: { detail: ServerDetail; channelId: string } = $props()

  const sid = $derived(detail.server.id)
  const roles = $derived(detail.roles ?? [])
  const mine = $derived<PermKey[]>(detail.permissions ?? [])
  const owner = $derived(detail.server.owner_id === app.meId)
  const channel = $derived(detail.channels.find((c) => c.id === channelId))
  // Les permissions du type du salon : texte, vocal, vidéos ou musique (celles du texte, plus l'organisation des séances).
  const TITLES = { text: ['Salons texte'], voice: ['Salons vocaux'], cinema: ['Salons vocaux'], videos: ['Salons texte', 'Esquisses'], music: ['Salons texte', 'Esquisses'] }
  const groups = $derived(PERM_GROUPS.filter((g) => TITLES[channel?.kind ?? 'text'].includes(g.title)))

  let overrides = $state<Override[]>([])
  // svelte-ignore state_referenced_locally
  let roleId = $state(roles.find((r) => r.is_default)?.id ?? roles[0]?.id ?? '')

  onMount(() => { void load() })
  async function load() {
    overrides = await api.channelOverrides(sid, channelId).catch((e) => { notify.error(e); return [] })
  }

  const current = $derived(overrides.find((o) => o.role_id === roleId) ?? { role_id: roleId, allow: [], deny: [] })
  const stateOf = (key: PermKey): 'allow' | 'deny' | 'inherit' => (current.allow.includes(key) ? 'allow' : current.deny.includes(key) ? 'deny' : 'inherit')
  const canSet = (key: PermKey) => owner || mine.includes(key)

  async function set(key: PermKey, value: string) {
    const allow = current.allow.filter((k) => k !== key)
    const deny = current.deny.filter((k) => k !== key)
    if (value === 'allow') allow.push(key)
    if (value === 'deny') deny.push(key)
    try {
      await api.setOverride(sid, channelId, roleId, allow, deny)
      await load()
    } catch (e) {
      notify.error(e, 'Permission non modifiée')
    }
  }
</script>

<div class="stack channel-perms">
  <div class="plbls-field">
    <label class="plbls-field__label" for="cp-role-{channelId}">Rôle</label>
    <select id="cp-role-{channelId}" class="plbls-select" bind:value={roleId}>
      {#each roles as r (r.id)}<option value={r.id}>{r.name}</option>{/each}
    </select>
  </div>
  {#each groups as g (g.title)}
    {#each g.perms as p (p.key)}
      <div class="row">
        <span class="grow">{p.label}</span>
        <select class="plbls-select" value={stateOf(p.key)} disabled={!canSet(p.key)} onchange={(e) => set(p.key, e.currentTarget.value)} aria-label="{p.label} pour ce rôle">
          <option value="inherit">Hériter</option>
          <option value="allow">Autoriser</option>
          <option value="deny">Refuser</option>
        </select>
      </div>
    {/each}
  {/each}
  <p class="muted small">« Hériter » applique les permissions du rôle sur le serveur. Pour réserver un salon : refusez « Voir le salon » à « @tout le monde », puis autorisez-le au rôle voulu.</p>
</div>
