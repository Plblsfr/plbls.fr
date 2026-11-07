<script lang="ts">
  import Modal from './Modal.svelte'
  import Avatar from './Avatar.svelte'
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
  <form class="mod" onsubmit={(e) => { e.preventDefault(); void submit() }}>
    <div class="mod-who">
      <Avatar {pseudo} {userId} {serverId} size="sm" />
      {#if kind === 'ban'}
        <p class="v-note">{pseudo} sera retiré du serveur et ne pourra plus le rejoindre, même avec une invitation, jusqu'à la fin du bannissement.</p>
      {:else if kind === 'kick'}
        <p class="v-note">{pseudo} sera retiré du serveur et du vocal ; il pourra revenir avec une nouvelle invitation.</p>
      {:else if kind === 'timeout'}
        <p class="v-note">{pseudo} reste membre et peut lire, mais ne peut plus écrire, réagir, parler ni rejoindre un vocal. Il est déconnecté du vocal.</p>
      {:else if kind === 'warn'}
        <p class="v-note">{pseudo} recevra un avertissement avec la raison indiquée.</p>
      {:else}
        <p class="v-note">Le pseudo ne change que sur ce serveur.</p>
      {/if}
    </div>
    {#if kind === 'ban' || kind === 'timeout'}
      <div class="plbls-field">
        <span class="plbls-field__label" id="m-dur">Durée</span>
        <div class="chips" role="group" aria-labelledby="m-dur">
          {#each kind === 'ban' ? BAN_DURATIONS : TIMEOUTS as [label, v] (v)}
            <button type="button" class="v-chip" class:line={v === -1 && duration !== -1} aria-pressed={duration === v} onclick={() => (duration = v)}>{label}</button>
          {/each}
        </div>
      </div>
      {#if duration === -1}
        <div class="plbls-field">
          <label class="plbls-field__label" for="m-hours">Durée personnalisée (heures)</label>
          <input id="m-hours" class="plbls-input v-mono" type="number" min="0.0003" max={kind === 'ban' ? 87600 : 672} step="any" bind:value={customHours} />
        </div>
      {/if}
    {/if}
    {#if kind === 'ban'}
      <div class="plbls-field">
        <span class="plbls-field__label" id="m-purge">Supprimer ses messages récents</span>
        <div class="v-seg" role="group" aria-labelledby="m-purge">
          {#each PURGES as [label, v] (v)}<button type="button" aria-pressed={purge === v} onclick={() => (purge = v)}>{label}</button>{/each}
        </div>
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
        <textarea id="m-reason" class="plbls-input plbls-textarea" rows="2" bind:value={reason} onkeydown={(e) => { if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); e.currentTarget.form?.requestSubmit() } }} maxlength="200" required={kind === 'warn'}></textarea>
      </div>
    {/if}
    <div class="mod-f">
      <button type="button" class="plbls-button plbls-button--contour" onclick={onclose}>Annuler</button>
      <button class="plbls-button" class:plbls-button--danger={kind === 'kick' || kind === 'ban'} disabled={busy || !valid}>{kind === 'kick' || kind === 'ban' ? `${TITLE[kind]} ${pseudo}` : 'Confirmer'}</button>
    </div>
  </form>
</Modal>

<style>
  .mod { display: flex; flex-direction: column; gap: 14px; }
  .mod-who { display: flex; align-items: flex-start; gap: 12px; }
  .mod-who p { margin: 0; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .v-chip.line { border-style: dashed; }
  .v-seg { flex-wrap: wrap; border-radius: var(--r); }
  .v-chip:focus-visible, .v-seg > button:focus-visible { outline: none; box-shadow: var(--anneau); }
  .v-seg > button[aria-pressed='false']:hover { color: var(--encre); }
  textarea { resize: vertical; }
  .mod-f { display: flex; gap: 8px; justify-content: flex-end; flex-wrap: wrap; padding-top: 4px; }
  @media (pointer: coarse) { .v-chip, .v-seg > button { min-height: 44px; } }
</style>
