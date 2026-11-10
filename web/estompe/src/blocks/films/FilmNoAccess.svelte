<script lang="ts">
  import { api, entracteOn } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import { tint } from './films'
  import Icon from '../../components/Icon.svelte'

  let { channelId, roomName, people }: { channelId: string; roomName: string; people: { user_id: string; pseudo: string }[] } = $props()

  const s = $derived(app.cinema[channelId]?.state)
  const colors = $derived(tint(s?.video ?? ''))
  const others = $derived(people.filter((p) => p.user_id !== app.meId).map((p) => p.pseudo))
  const watching = $derived(others.length === 0 ? 'Une séance est en cours. Tu restes en vocal' : others.length === 1 ? `${others[0]} regarde cette séance. Tu restes en vocal` : `${others.join(', ')} regardent cette séance. Tu restes en vocal avec eux`)
  let sent = $state(false)
  let busy = $state(false)

  async function ask() {
    busy = true
    try {
      await api.sendReport({
        title: 'Accès aux films et séries demandé',
        description: `${app.pseudo} demande la permission ${entracteOn() ? 'entracte:access' : 'estompe:films.use'} pour rejoindre une séance dans le salon cinéma « ${roomName} ».`,
        category: 'autre', platform: '', tech: {}, logs: '', file_ids: [],
      })
      sent = true
    } catch (e) { notify.error(e, 'La demande n’a pas pu être envoyée') } finally { busy = false }
  }
</script>

<section class="film-noaccess" aria-label="Séance en cours">
  <div class="film-noaccess-art" style="--bg:{colors.bg};--fg:{colors.fg};--ac:{colors.ac}">
    <span class="film-disc"></span>
    <div><div class="v-eyebrow film-noaccess-room">Session en cours dans {roomName}</div><strong>{s?.meta?.title ?? 'Film ou série'}</strong></div>
  </div>
  <div class="film-noaccess-body">
    <p>{watching}, mais tu n'as pas encore accès aux films et séries {entracteOn() ? 'd’Entracte' : 'd’Estompe'}.</p>
    {#if sent}
      <p class="film-noaccess-sent v-alert ok" role="status"><Icon name="ok" size={16} /> Demande envoyée aux administrateurs d'Estompe.</p>
    {:else if app.on('signalements')}
      <button class="plbls-button v-rose" disabled={busy} onclick={() => void ask()}>Demander l'accès</button>
    {/if}
  </div>
</section>
