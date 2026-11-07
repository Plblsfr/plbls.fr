<script lang="ts">
  import { onMount } from 'svelte'
  import { app } from '../lib/store.svelte'
  import { api } from '../lib/api'
  import { notify } from '../lib/ui'
  import Icon from './Icon.svelte'
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

  const STATES: ['inherit' | 'allow' | 'deny', string][] = [['inherit', 'Hériter'], ['allow', 'Autoriser'], ['deny', 'Refuser']]
  async function set(key: PermKey, value: string) {
    if (value === stateOf(key)) return
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

<div class="channel-perms">
  <div class="cp-h">
    <label class="v-lbl" for="cp-role-{channelId}">Rôle</label>
    <select id="cp-role-{channelId}" class="plbls-select cp-role" bind:value={roleId}>
      {#each roles as r (r.id)}<option value={r.id}>{r.name}</option>{/each}
    </select>
    <span class="grow"></span>
    <span class="cp-legend v-note" aria-hidden="true">
      <span class="tri"><span class="h"><i class="pt"></i></span></span>hériter
      <span class="tri"><span class="a"><Icon name="ok" size={14} /></span></span>autoriser
      <span class="tri"><span class="r"><Icon name="x" size={14} /></span></span>refuser
    </span>
  </div>
  {#each groups as g (g.title)}
    {#each g.perms as p (p.key)}
      <div class="cp-row">
        <span class="grow">{p.label}</span>
        <span class="tri" role="radiogroup" aria-label="{p.label} pour ce rôle">
          {#each STATES as [v, label] (v)}
            <button type="button" role="radio" aria-checked={stateOf(p.key) === v} aria-label={label} title={label} disabled={!canSet(p.key)}
              class:h={v === 'inherit' && stateOf(p.key) === v} class:a={v === 'allow' && stateOf(p.key) === v} class:r={v === 'deny' && stateOf(p.key) === v}
              onclick={() => set(p.key, v)}>
              {#if v === 'inherit'}<i class="pt"></i>{:else}<Icon name={v === 'allow' ? 'ok' : 'x'} size={14} />{/if}
            </button>
          {/each}
        </span>
      </div>
    {/each}
  {/each}
  <p class="v-note cp-help">« Hériter » applique les permissions du rôle sur le serveur. Pour réserver un salon : refusez « Voir le salon » à « @tout le monde », puis autorisez-le au rôle voulu.</p>
</div>

<style>
  .channel-perms { display: flex; flex-direction: column; gap: 2px; margin: 4px 0 8px 34px; padding: 14px; border-radius: var(--r); background: var(--papier-2); }
  .cp-h { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; padding-bottom: 6px; }
  .cp-role { width: 220px; max-width: 100%; height: 36px; }
  .cp-legend { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; }
  .cp-legend .tri { margin-left: 6px; }
  .cp-row { display: flex; align-items: center; gap: 10px; min-height: 44px; box-shadow: 0 1px 0 var(--trait); font-size: 14px; }
  .tri { display: inline-flex; flex: none; padding: 2px; border-radius: var(--r-p); background: var(--papier-3); }
  .tri > span, .tri button { display: inline-flex; align-items: center; justify-content: center; width: 32px; height: 28px; padding: 0; border: 0; border-radius: var(--r-p); background: transparent; color: var(--gris); }
  .tri button { cursor: pointer; }
  .tri button:hover:not(:disabled) { color: var(--encre); }
  .tri button:focus-visible { outline: none; box-shadow: var(--anneau); }
  .tri button:disabled { cursor: not-allowed; opacity: 0.5; }
  .tri .a { background: var(--ok-fond); color: var(--ok); }
  .tri .r { background: var(--danger-fond); color: var(--danger); }
  .tri .h { background: var(--surface); color: var(--encre); box-shadow: var(--o1); }
  .pt { display: block; width: 5px; height: 5px; border-radius: 50%; background: currentColor; }
  .cp-help { font-size: 12px; padding-top: 8px; }
  @media (pointer: coarse) { .tri button { width: 44px; height: 40px; } }
  @media (max-width: 560px) { .channel-perms { margin-left: 0; } .cp-legend { display: none; } }
</style>
