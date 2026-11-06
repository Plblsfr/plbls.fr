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
  <form class="stack cc" onsubmit={(e) => { e.preventDefault(); void create() }}>
    <div class="cc-cards" role="radiogroup" aria-label="Type de salon">
      {#each KINDS as k (k.kind)}
        <label class="cc-card" class:on={kind === k.kind}>
          <input type="radio" name="cc-kind" value={k.kind} bind:group={kind} onchange={() => (serverError = '')} />
          <Icon name={k.icon} size={20} />
          <strong>{k.label}</strong>
          <span class="small muted">{k.hint}</span>
        </label>
      {/each}
    </div>

    <div class="plbls-field">
      <label class="plbls-field__label" for="cc-name">Nom du salon</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="cc-name" class="plbls-input" bind:value={name} autofocus autocomplete="off" aria-invalid={touched && !!error} aria-describedby="cc-err" oninput={() => { touched = true; serverError = '' }} />
      <p id="cc-err" class="small cc-err" role="alert">{touched ? error : ''}</p>
      <div class="nav-item cc-preview" aria-label="Aperçu dans la barre latérale">
        <Icon name={priv ? 'lock' : icon} size={16} /><span class="truncate">{name.trim() || 'nom-du-salon'}</span>
      </div>
    </div>

    <label class="plbls-checkbox">
      <input type="checkbox" class="plbls-checkbox__input" bind:checked={priv} />
      <span class="plbls-checkbox__case" aria-hidden="true"></span>
      <span class="plbls-checkbox__texte">Salon privé <span class="muted small">— visible seulement par les rôles et membres choisis</span></span>
    </label>
    {#if priv}
      <fieldset class="cc-pick">
        <legend class="small muted">Rôles ayant accès</legend>
        {#each roles as r (r.id)}
          <label class="plbls-checkbox">
            <input type="checkbox" class="plbls-checkbox__input" checked={roleIds.includes(r.id)} onchange={() => (roleIds = toggle(roleIds, r.id))} />
            <span class="plbls-checkbox__case" aria-hidden="true"></span>
            <span class="plbls-checkbox__texte truncate">{r.name}</span>
          </label>
        {:else}<span class="small muted">Aucun rôle sur ce serveur.</span>{/each}
      </fieldset>
      <fieldset class="cc-pick">
        <legend class="small muted">Membres ayant accès (tu y as toujours accès)</legend>
        {#each members as m (m.id)}
          <label class="plbls-checkbox">
            <input type="checkbox" class="plbls-checkbox__input" checked={memberIds.includes(m.id)} onchange={() => (memberIds = toggle(memberIds, m.id))} />
            <span class="plbls-checkbox__case" aria-hidden="true"></span>
            <span class="plbls-checkbox__texte truncate">{m.nickname || m.pseudo}</span>
          </label>
        {:else}<span class="small muted">Aucun autre membre.</span>{/each}
      </fieldset>
    {/if}

    <details class="cc-adv">
      <summary>Réglages avancés</summary>
      <div class="stack">
        <div class="plbls-field">
          <label class="plbls-field__label" for="cc-topic">Sujet du salon</label>
          <input id="cc-topic" class="plbls-input" bind:value={topic} maxlength={MAX_TOPIC} />
        </div>
        {#if kind === 'text'}
          <div class="plbls-field">
            <label class="plbls-field__label" for="cc-slow">Mode lent</label>
            <select id="cc-slow" class="plbls-select" bind:value={slowmode}>{#each SLOW as [label, v]}<option value={v}>{label}</option>{/each}</select>
          </div>
        {:else if isVoiceKind(kind)}
          <div class="plbls-field">
            <label class="plbls-field__label" for="cc-limit">Limite de participants</label>
            <select id="cc-limit" class="plbls-select" bind:value={userLimit}>
              <option value={0}>Aucune (10 au plus)</option>
              {#each [2, 3, 4, 5, 6, 7, 8, 9] as n}<option value={n}>{n} participants</option>{/each}
            </select>
          </div>
        {/if}
      </div>
    </details>

    <div class="row cc-actions">
      <button type="button" class="plbls-button plbls-button--contour" onclick={onclose}>Annuler</button>
      <button class="plbls-button" disabled={!valid || busy}>Créer le salon</button>
    </div>
  </form>
</Modal>
