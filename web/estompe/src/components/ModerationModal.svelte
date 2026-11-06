<script lang="ts">
  import Modal from './Modal.svelte'
  import { api } from '../lib/api'
  import { app } from '../lib/store.svelte'
  import { notify } from '../lib/ui'
  import type { ModKind } from '../lib/moderation.svelte'

  let { kind, serverId, userId, pseudo, onclose }: { kind: ModKind; serverId: string; userId: string; pseudo: string; onclose: () => void } = $props()

  const TITLE: Record<ModKind, string> = { kick: 'Expulser', ban: 'Bannir', timeout: 'Exclure temporairement', warn: 'Avertir', nickname: 'Changer le pseudo de' }
  const DONE: Record<ModKind, string> = { kick: 'Membre expulsé', ban: 'Membre banni', timeout: 'Membre exclu temporairement', warn: 'Avertissement envoyé', nickname: 'Pseudo modifié' }
  const BAN_DURATIONS: [string, number][] = [['Définitif', 0], ['1 heure', 3600], ['24 heures', 86400], ['7 jours', 604800], ['Personnalisé…', -1]]
  const TIMEOUTS: [string, number][] = [['60 secondes', 60], ['5 minutes', 300], ['10 minutes', 600], ['1 heure', 3600], ['1 jour', 86400], ['1 semaine', 604800], ['Personnalisé…', -1]]
  const PURGES: [string, number][] = [['Ne rien supprimer', 0], ['Dernière heure', 3600], ['Dernières 24 heures', 86400], ['7 derniers jours', 604800]]

  let reason = $state('')
  // svelte-ignore state_referenced_locally
  let duration = $state(kind === 'ban' ? 0 : 600)
  let customHours = $state(1)
  let purge = $state(0)
  // Valeur initiale volontaire : le champ est ensuite libre jusqu'à l'envoi.
  // svelte-ignore state_referenced_locally
  let nickname = $state(app.details[serverId]?.members.find((m) => m.id === userId)?.nickname ?? '')
  let busy = $state(false)

  const seconds = $derived(duration === -1 ? Math.round(customHours * 3600) : duration)
  const valid = $derived(kind === 'nickname' ? nickname.length <= 32 : kind === 'warn' ? !!reason.trim() : kind === 'timeout' ? seconds >= 1 && seconds <= 28 * 86400 : true)

  async function submit() {
    busy = true
    try {
      const r = reason.trim()
      if (kind === 'kick') await api.kickMember(serverId, userId, r)
      else if (kind === 'ban') await api.banMember(serverId, userId, { reason: r, duration: seconds, delete_messages: purge })
      else if (kind === 'timeout') await api.timeoutMember(serverId, userId, seconds, r)
      else if (kind === 'warn') await api.warnMember(serverId, userId, r)
      else await api.setNickname(serverId, userId, nickname.trim())
      notify.ok(DONE[kind])
      onclose()
    } catch (e) {
      notify.error(e, 'Action impossible')
    } finally {
      busy = false
    }
  }
</script>

<Modal title="{TITLE[kind]} {pseudo}" {onclose}>
  <form class="stack" onsubmit={(e) => { e.preventDefault(); void submit() }}>
    {#if kind === 'ban'}
      <p class="muted small">{pseudo} sera retiré du serveur et ne pourra plus le rejoindre, même avec une invitation, jusqu'à la fin du bannissement.</p>
    {:else if kind === 'kick'}
      <p class="muted small">{pseudo} sera retiré du serveur et du vocal ; il pourra revenir avec une nouvelle invitation.</p>
    {:else if kind === 'timeout'}
      <p class="muted small">{pseudo} reste membre et peut lire, mais ne peut plus écrire, réagir, parler ni rejoindre un vocal. Il est déconnecté du vocal.</p>
    {/if}
    {#if kind === 'ban' || kind === 'timeout'}
      <div class="plbls-field">
        <label class="plbls-field__label" for="m-dur">Durée</label>
        <select id="m-dur" class="plbls-select" bind:value={duration}>
          {#each kind === 'ban' ? BAN_DURATIONS : TIMEOUTS as [label, v]}<option value={v}>{label}</option>{/each}
        </select>
      </div>
      {#if duration === -1}
        <div class="plbls-field">
          <label class="plbls-field__label" for="m-hours">Durée personnalisée (heures)</label>
          <input id="m-hours" class="plbls-input" type="number" min="0.0003" max={kind === 'ban' ? 87600 : 672} step="any" bind:value={customHours} />
        </div>
      {/if}
    {/if}
    {#if kind === 'ban'}
      <div class="plbls-field">
        <label class="plbls-field__label" for="m-purge">Supprimer ses messages récents</label>
        <select id="m-purge" class="plbls-select" bind:value={purge}>
          {#each PURGES as [label, v]}<option value={v}>{label}</option>{/each}
        </select>
      </div>
    {/if}
    {#if kind === 'nickname'}
      <div class="plbls-field">
        <label class="plbls-field__label" for="m-nick">Pseudo sur ce serveur (vide : pseudo d'origine)</label>
        <input id="m-nick" class="plbls-input" bind:value={nickname} maxlength="32" />
      </div>
    {:else}
      <div class="plbls-field">
        <label class="plbls-field__label" for="m-reason">Raison{kind === 'warn' ? '' : ' (facultative)'}</label>
        <input id="m-reason" class="plbls-input" bind:value={reason} maxlength="200" required={kind === 'warn'} />
      </div>
    {/if}
    <div style="display: flex; gap: 8px; justify-content: flex-end">
      <button type="button" class="plbls-button plbls-button--contour" onclick={onclose}>Annuler</button>
      <button class="plbls-button" class:plbls-button--danger={kind === 'kick' || kind === 'ban'} disabled={busy || !valid}>Confirmer</button>
    </div>
  </form>
</Modal>
