<script lang="ts">
  import Modal from './Modal.svelte'
  import Icon from './Icon.svelte'
  import { api } from '../lib/api'
  import { app } from '../lib/store.svelte'
  import { notify, messageOf } from '../lib/ui'
  import { router, hrefs } from '../lib/route.svelte'
  import { SLOW, MAX_TOPIC, channelCreate, channelNameError } from '../lib/channel-create.svelte'
  import { isVoiceKind, type ChannelKind } from '../lib/types'

  let { serverId, kind: initial, after, onclose }: { serverId: string; kind: ChannelKind; after?: string; onclose: () => void } = $props()

  const ALL_KINDS: { kind: ChannelKind; label: string; hint: string; icon: 'hash' | 'volume' | 'video' | 'headphones' | 'film' }[] = [
    { kind: 'text', label: 'Texte', hint: 'Discuter par messages', icon: 'hash' },
    { kind: 'voice', label: 'Vocal', hint: 'Parler, partager son écran, regarder ensemble', icon: 'volume' },
    { kind: 'videos', label: 'Vidéos', hint: 'Esquisses : partager des vidéos et organiser des séances', icon: 'video' },
    { kind: 'music', label: 'Musique', hint: 'Esquisses : partager des morceaux, des playlists et écouter ensemble', icon: 'headphones' },
    { kind: 'cinema', label: 'Cinéma', hint: 'Regarder un film ou une série de la bibliothèque ensemble, en vocal', icon: 'film' },
  ]
  // Le salon cinéma n'existe que si la bibliothèque de films est activée sur l'instance.
  const KINDS = $derived(ALL_KINDS.filter((k) => k.kind !== 'cinema' || app.on('films')))
  const detail = $derived(app.details[serverId])
  // svelte-ignore state_referenced_locally
  let kind = $state<ChannelKind>(initial === 'cinema' && !app.on('films') ? 'voice' : initial)
  let name = $state('')
  let touched = $state(false)
  let priv = $state(false)
  let roleIds = $state<string[]>([])
  let memberIds = $state<string[]>([])
  let topic = $state('')
  let slowmode = $state(0)
  let userLimit = $state(0)
  let busy = $state(false)
  let serverError = $state('')

  const error = $derived(serverError || channelNameError(name, kind, detail?.channels ?? []))
  const valid = $derived(!channelNameError(name, kind, detail?.channels ?? []) && !serverError)
  const roles = $derived((detail?.roles ?? []).filter((r) => !r.is_default))
  const members = $derived((detail?.members ?? []).filter((m) => m.id !== app.meId))
  const icon = $derived(ALL_KINDS.find((k) => k.kind === kind)!.icon)
  const toggle = (list: string[], id: string) => (list.includes(id) ? list.filter((x) => x !== id) : [...list, id])

  async function create() {
    touched = true
    if (!valid || busy) return
    busy = true
    try {
      const ref = detail?.channels.find((c) => c.id === after)
      const c = await api.createChannel(serverId, {
        name: name.trim(), kind,
        ...(topic.trim() ? { topic: topic.trim() } : {}),
        ...(kind === 'text' && slowmode ? { slowmode } : {}),
        ...(isVoiceKind(kind) && userLimit ? { user_limit: userLimit } : {}),
        ...(ref?.kind === kind ? { after } : {}),
        ...(priv ? { private: true, roles: roleIds, members: memberIds } : {}),
      })
      app.handleEvent({ type: 'channel.created', data: { server_id: serverId, channel: c, after: ref?.kind === kind ? after : '' } })
      channelCreate.fresh = c.id
      channelCreate.done = { serverId, channel: c }
      setTimeout(() => { if (channelCreate.fresh === c.id) channelCreate.fresh = '' }, 4000)
      router.go(hrefs.channel(serverId, c.id))
      onclose()
    } catch (e) {
      serverError = messageOf(e, '')
      if (!serverError) notify.error(e, 'Création impossible')
    } finally {
      busy = false
    }
  }
</script>

<Modal title="Créer un salon" {onclose}>
  <form class="cc" onsubmit={(e) => { e.preventDefault(); void create() }}>
    {#if detail}<p class="v-note cc-in">dans {detail.server.name}</p>{/if}
    <div class="cc-cards" role="radiogroup" aria-label="Type de salon">
      {#each KINDS as k (k.kind)}
        <label class="cc-card" class:on={kind === k.kind}>
          <input type="radio" name="cc-kind" value={k.kind} bind:group={kind} onchange={() => (serverError = '')} />
          <span class="cc-card-h"><Icon name={k.icon} size={22} /><strong class="grow">{k.label}</strong><span class="cc-rad" aria-hidden="true"></span></span>
          <span class="v-note">{k.hint}</span>
        </label>
      {/each}
    </div>

    <div class="plbls-field">
      <label class="plbls-field__label" for="cc-name">Nom du salon</label>
      <span class="cc-name">
        <Icon name={priv ? 'lock' : icon} size={16} />
        <!-- svelte-ignore a11y_autofocus -->
        <input id="cc-name" class="plbls-input" bind:value={name} autofocus autocomplete="off" aria-invalid={touched && !!error} aria-describedby="cc-err" oninput={() => { touched = true; serverError = '' }} />
      </span>
      <p id="cc-err" class="cc-err" role="alert">{#if touched && error}<Icon name="alerte" size={14} />{error}{/if}</p>
      <span class="cc-apercu v-note">Aperçu :
        <span class="nav-item cc-preview" aria-label="Aperçu dans la barre latérale">
          <Icon name={priv ? 'lock' : icon} size={16} /><span class="truncate">{name.trim() || 'nom-du-salon'}</span>
        </span>
      </span>
    </div>

    <div class="cc-priv">
      <label class="plbls-toggle">
        <input type="checkbox" class="plbls-toggle__input" bind:checked={priv} />
        <span class="plbls-toggle__piste"></span>
        <span class="plbls-toggle__texte"><b>Salon privé</b> <span class="v-note">— visible seulement par les rôles et membres choisis</span></span>
      </label>
      {#if priv}
        <fieldset class="cc-pick">
          <legend class="v-lbl">Rôles ayant accès</legend>
          <div class="chips">
            {#each roles as r (r.id)}
              <button type="button" class="v-chip" class:line={!roleIds.includes(r.id)} aria-pressed={roleIds.includes(r.id)} onclick={() => (roleIds = toggle(roleIds, r.id))}>{#if roleIds.includes(r.id)}<Icon name="ok" size={14} />{/if}<span class="v-ell">{r.name}</span></button>
            {:else}<span class="v-note">Aucun rôle sur ce serveur.</span>{/each}
          </div>
        </fieldset>
        <fieldset class="cc-pick">
          <legend class="v-lbl">Membres ayant accès (tu y as toujours accès)</legend>
          <div class="chips">
            {#each members as m (m.id)}
              <button type="button" class="v-chip" class:line={!memberIds.includes(m.id)} aria-pressed={memberIds.includes(m.id)} onclick={() => (memberIds = toggle(memberIds, m.id))}>{#if memberIds.includes(m.id)}<Icon name="ok" size={14} />{/if}<span class="v-ell">{m.nickname || m.pseudo}</span></button>
            {:else}<span class="v-note">Aucun autre membre.</span>{/each}
          </div>
        </fieldset>
      {/if}
    </div>

    <details class="cc-adv">
      <summary class="v-lbl">Réglages avancés</summary>
      <div class="cc-adv-b">
        <div class="plbls-field">
          <label class="plbls-field__label" for="cc-topic">Sujet du salon</label>
          <input id="cc-topic" class="plbls-input" bind:value={topic} maxlength={MAX_TOPIC} />
        </div>
        {#if kind === 'text'}
          <div class="plbls-field">
            <span class="plbls-field__label" id="cc-slow">Mode lent</span>
            <div class="chips" role="group" aria-labelledby="cc-slow">
              {#each SLOW as [label, v] (v)}<button type="button" class="v-chip" aria-pressed={slowmode === v} onclick={() => (slowmode = v)}>{label}</button>{/each}
            </div>
          </div>
        {:else if isVoiceKind(kind)}
          <div class="plbls-field">
            <span class="plbls-field__label" id="cc-limit">Limite de participants</span>
            <div class="chips" role="group" aria-labelledby="cc-limit">
              <button type="button" class="v-chip" aria-pressed={userLimit === 0} onclick={() => (userLimit = 0)}>Aucune (10 au plus)</button>
              {#each [2, 3, 4, 5, 6, 7, 8, 9] as n (n)}<button type="button" class="v-chip" aria-pressed={userLimit === n} onclick={() => (userLimit = n)}>{n} participants</button>{/each}
            </div>
          </div>
        {/if}
      </div>
    </details>

    <div class="cc-actions">
      <button type="button" class="plbls-button plbls-button--contour" onclick={onclose}>Annuler</button>
      <button class="plbls-button" disabled={!valid || busy}>Créer le salon</button>
    </div>
  </form>
</Modal>

<style>
  .cc { display: flex; flex-direction: column; gap: 16px; }
  .cc-in { margin: -8px 0 0; }
  .cc-cards { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
  .cc-card { position: relative; display: flex; flex-direction: column; gap: 6px; padding: 14px; border-radius: var(--r); background: var(--surface); box-shadow: inset 0 0 0 1px var(--trait-fort); cursor: pointer; transition: background var(--d) var(--sortie), box-shadow var(--d) var(--sortie); }
  .cc-card:last-child:nth-child(odd) { grid-column: 1 / -1; }
  .cc-card:hover { background: var(--papier-2); }
  .cc-card.on { background: var(--rose-doux); box-shadow: inset 0 0 0 2px var(--encre); }
  .cc-card input { position: absolute; opacity: 0; inset: 0; cursor: pointer; }
  .cc-card:has(input:focus-visible) { box-shadow: inset 0 0 0 2px var(--encre), var(--anneau); }
  .cc-card-h { display: flex; align-items: center; gap: 8px; }
  .cc-rad { width: 18px; height: 18px; border-radius: 50%; box-shadow: inset 0 0 0 1.5px var(--trait-fort); flex: none; }
  .cc-card.on .cc-rad { box-shadow: inset 0 0 0 5px var(--encre); background: var(--surface); }
  .cc-name { position: relative; display: flex; align-items: center; color: var(--gris); }
  .cc-name :global(svg) { position: absolute; left: 14px; pointer-events: none; }
  .cc-name input { flex: 1; min-width: 0; padding-left: 38px; }
  .cc-err { display: flex; align-items: center; gap: 6px; min-height: 1.2em; margin: 4px 0 0; color: var(--danger); font-size: 13px; }
  .cc-apercu { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .cc-preview { pointer-events: none; display: inline-flex; width: min(100%, 260px); height: 30px; background: var(--papier-2); }
  .cc-priv { display: flex; flex-direction: column; gap: 12px; }
  .cc-pick { display: flex; flex-direction: column; gap: 8px; margin: 0; padding: 0 0 0 54px; border: 0; min-width: 0; }
  .cc-pick legend { padding: 0; margin-bottom: 6px; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; max-height: 168px; overflow-y: auto; }
  .chips .v-chip { max-width: 100%; }
  .v-chip:focus-visible { outline: none; box-shadow: var(--anneau); }
  .cc-adv { box-shadow: 0 -1px 0 var(--trait); padding-top: 12px; }
  .cc-adv > summary { cursor: pointer; }
  .cc-adv > summary:focus-visible { outline: none; box-shadow: var(--anneau); border-radius: var(--r-xs); }
  .cc-adv-b { display: flex; flex-direction: column; gap: 12px; margin-top: 12px; }
  .cc-actions { display: flex; justify-content: flex-end; gap: 8px; flex-wrap: wrap; }
  @media (pointer: coarse) { .v-chip { min-height: 44px; } }
  @media (max-width: 720px) {
    .cc-cards { grid-template-columns: minmax(0, 1fr); }
    .cc-pick { padding-left: 0; }
    :global(plbls-modal:has(.cc) .plbls-modal) { max-width: none; width: 100vw; height: 100dvh; max-height: none; border-radius: 0; }
    :global(plbls-modal:has(.cc) .plbls-modal__corps) { max-height: none; }
  }
</style>
