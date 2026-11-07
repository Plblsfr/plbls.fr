<script lang="ts">
  import Icon from './Icon.svelte'
  import { app } from '../lib/store.svelte'
  import { api } from '../lib/api'
  import { renderMarkdown } from '../lib/markdown'
  import { emojis } from '../blocks/emojis/emojis.svelte'
  import { timeOf, dayOf } from '../lib/format'
  import { notify } from '../lib/ui'
  import type { Message } from '../lib/types'

  let { convKey, onclose }: { convKey: string; onclose: () => void } = $props()

  let results = $state<Message[]>([])
  let busy = $state(false)
  const pseudos = $derived([...app.pseudosOf(convKey), ...app.groupMentionsOf(convKey)])

  $effect(() => { void loadPins() })

  async function loadPins() {
    busy = true
    try {
      results = (await api.pins(convKey)).messages
    } catch (e) {
      notify.error(e, 'Impossible de charger les messages épinglés')
    } finally {
      busy = false
    }
  }

  function jump(id: string) {
    const el = document.querySelector<HTMLElement>(`[data-id="${CSS.escape(id)}"]`)
    if (!el) return notify.info("Ce message est plus haut dans l'historique : remonte la conversation pour le voir.")
    el.scrollIntoView({ block: 'center', behavior: 'smooth' })
    el.classList.add('flash')
    setTimeout(() => el.classList.remove('flash'), 1500)
  }

  const unpin = (id: string) => api.unpin(id).then(() => (results = results.filter((m) => m.id !== id))).catch((e) => notify.error(e, 'Action impossible'))
</script>

<aside class="panel-side" aria-label="Messages épinglés">
  <header>
    <h2 class="grow pins-title">Messages épinglés</h2>
    <button class="v-ib sm" onclick={onclose} aria-label="Fermer"><Icon name="x" size={16} /></button>
  </header>
  <div class="panel-body">
    {#each results as m (m.id)}
      <article class="pin-card">
        <div class="pin-meta"><Icon name="epingle" size={13} /><span class="pin-author">{m.author.pseudo}</span><span class="pin-date">{dayOf(m.created_at)} {timeOf(m.created_at)}</span></div>
        <div class="msg-body">{@html renderMarkdown(m.content, pseudos, emojis.names())}</div>
        {#if m.attachments.length}<div class="pin-files"><Icon name="trombone" size={14} /> {m.attachments.length} fichier{m.attachments.length > 1 ? 's' : ''}</div>{/if}
        <div class="pin-actions">
          <button class="v-btn sm sec" onclick={() => jump(m.id)}>Voir dans la conversation</button>
          <button class="v-btn sm ghost" onclick={() => unpin(m.id)}>Désépingler</button>
        </div>
      </article>
    {:else}
      {#if !busy}<div class="pins-empty"><Icon name="epingle" size={20} /><p>Aucun message épinglé.</p></div>{/if}
    {/each}
  </div>
</aside>
