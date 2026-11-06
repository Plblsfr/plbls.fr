<script lang="ts">
  import { api } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import { tint } from './films'

  let { channelId, roomName, people }: { channelId: string; roomName: string; people: { user_id: string; pseudo: string }[] } = $props()

  const s = $derived(app.cinema[channelId]?.state)
  const colors = $derived(tint(s?.video ?? ''))
  const others = $derived(people.filter((p) => p.user_id !== app.meId).map((p) => p.pseudo).join(', '))
  let sent = $state(false)
  let busy = $state(false)

  async function ask() {
    busy = true
    try {
      await api.sendReport({
        title: 'Accès aux films et séries demandé',
        description: `${app.pseudo} demande la permission estompe:movies pour rejoindre une séance dans le salon cinéma « ${roomName} ».`,
        category: 'autre', platform: '', tech: {}, logs: '', file_ids: [],
      })
      sent = true
    } catch (e) { notify.error(e, 'La demande n’a pas pu être envoyée') } finally { busy = false }
  }
</script>

<section class="film-noaccess" aria-label="Séance en cours">
  <div class="film-noaccess-art" style="--bg:{colors.bg};--fg:{colors.fg};--ac:{colors.ac}">
    <span class="film-disc"></span>
    <div><div class="small">Session en cours dans {roomName}</div><strong>{s?.meta?.title ?? 'Film ou série'}</strong></div>
  </div>
  <div class="film-noaccess-body">
    <p>{others ? `${others} regardent cette séance.` : 'Une séance est en cours.'} Vous restez en vocal avec eux, mais vous n'avez pas encore accès aux films et séries d'Estompe.</p>
    {#if sent}
      <p class="film-noaccess-sent" role="status">Demande envoyée aux administrateurs d'Estompe.</p>
    {:else if app.on('signalements')}
      <button class="plbls-button" disabled={busy} onclick={() => void ask()}>Demander l'accès</button>
    {/if}
  </div>
</section>
