<script lang="ts">
  import { app } from '../../lib/store.svelte'
  import { prefs } from '../../lib/prefs.svelte'
  import { isVoiceKind, type NotifyLevel, type NotifySettings } from '../../lib/types'
  import { api, ApiError } from '../../lib/api'
  import { notify } from '../../lib/ui'
  import { platform } from '../../lib/platform'
  import { pushState } from './push'

  const driver = platform.notifications
  const Extra = platform.slots.settingsNotifications

  const denied = () => typeof Notification !== 'undefined' && Notification.permission === 'denied'
  let blocked = $state(denied())

  // Un seul interrupteur : actif seulement si la permission le permet vraiment ; le push (app fermée) s'y abonne là où la plateforme le gère.
  const deskSupported = driver.supported()
  const desk = $derived(!deskSupported ? 'unsupported' : prefs.desktopNotifications ? (blocked ? 'denied' : 'on') : 'off')
  let subscribed = $state(false)
  let serverOff = $state(false)
  let busy = $state(false)
  const supported = driver.push?.supported() ?? false
  const push = $derived(pushState({ supported, subscribed, denied: false, serverOff }))
  void driver.push?.enabled().then((v) => (subscribed = v))
  if (supported && driver.push?.probeServer) api.pushKey().catch((e) => { serverOff = e instanceof ApiError && e.status === 501 })
  async function testPush() {
    busy = true
    try { await api.pushTest(); notify.info('Notification de test envoyée.') } catch (e) {
      notify.error(e, e instanceof ApiError && e.status === 404 ? 'Aucun abonnement push : réactive l\'interrupteur.' : 'Envoi du test impossible')
    } finally { busy = false }
  }
  async function toggleNotifications(e: Event) {
    const box = e.currentTarget as HTMLInputElement
    busy = true
    try {
      if (box.checked && !(await driver.ask())) {
        box.checked = false
        prefs.desktopNotifications = false
        notify.info('Notifications bloquées : autorise-les dans les réglages du navigateur ou du téléphone.')
      } else if (box.checked) {
        prefs.desktopNotifications = true
        if (supported) {
          try { subscribed = await driver.push!.enable() } catch (err) {
            if (err instanceof ApiError && err.status === 501) serverOff = true
            else notify.info('Notifications actives, mais impossibles application fermée.')
          }
        }
      } else {
        prefs.desktopNotifications = false
        if (supported) { await driver.push!.disable().catch(() => {}); subscribed = false }
      }
    } finally {
      blocked = denied()
      busy = false
      prefs.save()
    }
  }

  const SAY = {
    on: ['Actif', 'on'], off: ['Désactivé', ''], unsupported: ['Non disponible sur cet appareil', 'bad'],
    denied: ['Permission refusée', 'bad'], unavailable: ['Non disponible sur ce serveur', 'bad'],
  } as const
  const FIX = {
    denied: 'Autorise les notifications pour Estompe dans les réglages du navigateur ou du téléphone, puis réactive l\'interrupteur.',
    unavailable: 'Le serveur n\'a pas de configuration pour les notifications push : l\'administrateur doit la renseigner.',
    unsupported: 'Cet appareil ou ce navigateur ne prend pas en charge cette notification.',
  } as const

  // Niveaux par serveur, puis par salon : seuls les salons qui diffèrent du serveur sont listés.
  let sid = $state(app.servers[0]?.id ?? '')
  const srv = $derived(app.servers.find((s) => s.id === sid) ?? app.servers[0])
  const channels = $derived((srv ? app.details[srv.id]?.channels : undefined)?.filter((c) => !isVoiceKind(c.kind)) ?? [])
  const ns = $derived(app.notifySettings)
  const custom = $derived(channels.filter((c) => ns.levels[c.id]))
  const save = (patch: Partial<NotifySettings>) => app.saveNotifySettings(patch).catch(() => notify.info('Impossible d\'enregistrer les réglages.'))
  const setLevel = (id: string, v: string) => {
    const levels = { ...ns.levels }
    if (v) levels[id] = v as NotifyLevel
    else delete levels[id]
    void save({ levels })
  }
  const TYPES = [
    ['dm', 'Messages privés'], ['mention', 'Mentions'], ['channel', 'Messages de salon'], ['voice', 'Activité vocale'],
  ] as const
  const dnd = $derived(app.statuses[app.meId] === 'dnd')
  const toggleDnd = (e: Event) => app.setStatus((e.currentTarget as HTMLInputElement).checked ? 'dnd' : 'online').catch(() => notify.info('Impossible de changer le statut.'))
</script>

{#snippet state(s: keyof typeof SAY)}
  <span class="set-state {SAY[s][1]}">{SAY[s][0]}</span>{#if s in FIX}<p class="muted small">{FIX[s as keyof typeof FIX]}</p>{/if}
{/snippet}
{#snippet levels(id: string, label: string, unset: string)}
  <div class="plbls-field">
    <label class="plbls-field__label" for={'nl-' + id}>{label}</label>
    <select id={'nl-' + id} class="plbls-select" value={ns.levels[id] ?? ''} onchange={(e) => setLevel(id, e.currentTarget.value)}>
      <option value="">{unset}</option>
      <option value="all">Tous les messages</option>
      <option value="mentions">Mentions seulement</option>
      <option value="none">Rien</option>
    </select>
  </div>
{/snippet}

<h3 class="set-sub">Notifications</h3>
<label class="plbls-toggle">
  <input class="plbls-toggle__input" type="checkbox" checked={desk === 'on'} disabled={busy || desk === 'unsupported'} onchange={toggleNotifications} />
  <span class="plbls-toggle__piste"></span>
  <span class="plbls-toggle__texte">Messages privés et mentions</span>
</label>
{@render state(desk)}
{#if driver.hint?.()}<p class="muted small">{driver.hint()}</p>{/if}
{#if desk === 'on' && push === 'on' && driver.testable}<button class="plbls-button plbls-button--compact plbls-button--discret" disabled={busy} onclick={testPush}>Envoyer une notification de test</button>{/if}
{#if desk === 'on' && (push === 'off' || push === 'unavailable')}<p class="muted small">Application fermée : {push === 'unavailable' ? 'non disponible sur ce serveur' : 'inactif sur cet appareil'}.</p>{/if}

<h3 class="set-sub">Ne pas déranger</h3>
<label class="plbls-toggle">
  <input class="plbls-toggle__input" type="checkbox" checked={dnd} onchange={toggleDnd} />
  <span class="plbls-toggle__piste"></span>
  <span class="plbls-toggle__texte">Couper toutes les notifications (statut « Ne pas déranger »)</span>
</label>

<h3 class="set-sub">Types de notifications</h3>
<p class="muted small">Réglages enregistrés sur ton compte, identiques sur tous tes appareils.</p>
{#each TYPES as [k, label] (k)}
  <label class="plbls-toggle">
    <input class="plbls-toggle__input" type="checkbox" checked={ns.types[k]} onchange={(e) => save({ types: { ...ns.types, [k]: e.currentTarget.checked } })} />
    <span class="plbls-toggle__piste"></span>
    <span class="plbls-toggle__texte">{label}</span>
  </label>
{/each}

<h3 class="set-sub">Contenu et son</h3>
<label class="plbls-toggle">
  <input class="plbls-toggle__input" type="checkbox" checked={ns.preview} onchange={(e) => save({ preview: e.currentTarget.checked })} />
  <span class="plbls-toggle__piste"></span>
  <span class="plbls-toggle__texte">Afficher l'aperçu (auteur, salon, début du message) ; sinon « Nouveau message de … »</span>
</label>
<label class="plbls-toggle">
  <input class="plbls-toggle__input" type="checkbox" checked={ns.sound} onchange={(e) => save({ sound: e.currentTarget.checked })} />
  <span class="plbls-toggle__piste"></span>
  <span class="plbls-toggle__texte">Son à la réception d'une notification</span>
</label>

<h3 class="set-sub">Par serveur et par salon</h3>
<p class="muted small">Défaut : mentions seulement (tous les messages en privé).</p>
{#if srv}
  {#if app.servers.length > 1}
    <div class="plbls-field">
      <label class="plbls-field__label" for="nl-server">Serveur</label>
      <select id="nl-server" class="plbls-select" bind:value={sid}>
        {#each app.servers as s (s.id)}<option value={s.id}>{s.name}</option>{/each}
      </select>
    </div>
  {/if}
  {@render levels(srv.id, `Tout le serveur ${srv.name}`, 'Par défaut (mentions seulement)')}
  <label class="plbls-toggle">
    <input class="plbls-toggle__input" type="checkbox" checked={(ns.ignore_group ?? []).includes(srv.id)} onchange={(e) => save({ ignore_group: e.currentTarget.checked ? [...new Set([...(ns.ignore_group ?? []), srv.id])] : (ns.ignore_group ?? []).filter((i) => i !== srv.id) })} />
    <span class="plbls-toggle__piste"></span>
    <span class="plbls-toggle__texte">Ignorer @everyone et @here sur ce serveur</span>
  </label>
  <details open={custom.length > 0}>
    <summary>Salons{custom.length ? ` (${custom.length} personnalisé${custom.length > 1 ? 's' : ''})` : ''}</summary>
    <div class="stack">
      {#each custom as ch (ch.id)}
        <div class="row">
          <div style="flex: 1">{@render levels(ch.id, `#${ch.name}`, 'Comme le serveur')}</div>
          <button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => setLevel(ch.id, '')}>Réinitialiser</button>
        </div>
      {/each}
      {#if channels.length > custom.length}
        <div class="plbls-field">
          <label class="plbls-field__label" for="nl-add">Régler un autre salon</label>
          <select id="nl-add" class="plbls-select" value="" onchange={(e) => { setLevel(e.currentTarget.value, 'all'); e.currentTarget.value = '' }}>
            <option value="">Choisir un salon…</option>
            {#each channels.filter((c) => !ns.levels[c.id]) as ch (ch.id)}<option value={ch.id}>#{ch.name}</option>{/each}
          </select>
        </div>
      {/if}
    </div>
  </details>
{:else}
  <p class="muted small">Rejoins un serveur pour régler ses notifications.</p>
{/if}

{#if Extra}<Extra />{/if}
