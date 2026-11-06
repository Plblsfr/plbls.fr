<script lang="ts">
  import Icon from './Icon.svelte'
  import Modal from './Modal.svelte'
  import { app } from '../lib/store.svelte'
  import { api } from '../lib/api'
  import { notify } from '../lib/ui'
  import RoleBadge from './RoleBadge.svelte'
  import { timeOf, isBot } from '../lib/format'
  import { pollTotal, pollWinners, remaining } from '../lib/poll'
  import { renderMarkdown } from '../lib/markdown'
  import { emojis } from '../blocks/emojis/emojis.svelte'
  import type { Message } from '../lib/types'

  let { message: m, convKey, pseudos }: { message: Message; convKey: string; pseudos: string[] } = $props()
  const poll = $derived(m.poll!)

  // Horloge réveillée à l'échéance du sondage : la carte passe en résultats sans nouveau rendu.
  let now = $state(Date.now())
  $effect(() => {
    const end = poll.closes_at ? new Date(poll.closes_at).getTime() : 0
    if (end <= Date.now()) { now = Date.now(); return }
    const t = setTimeout(() => (now = Date.now()), Math.min(end - Date.now() + 50, 2 ** 31 - 1)) // au-delà de ~24 j, on se réveille et on re-planifie
    return () => clearTimeout(t)
  })
  const closed = $derived(!!poll.closes_at && new Date(poll.closes_at).getTime() <= now)
  const total = $derived(pollTotal(poll.options))
  const winners = $derived(closed ? pollWinners(poll.options) : [])
  const canManage = $derived(poll.created_by.id === app.meId || app.canConv(convKey, 'manage_messages') || app.isAdmin)

  let voters = $state<Record<string, { pseudo: string }[]> | null>(null)
  const toggleVoters = () => (voters ? (voters = null) : api.pollVotes(m.id).then((v) => (voters = v), (e) => notify.error(e, 'Votants indisponibles')))
  let confirmDelete = $state(false)
  const remove = () => app.deleteMessage(convKey, m.id).then(() => (confirmDelete = false), (e) => notify.error(e, 'Suppression impossible'))
</script>

<section class="poll-card" class:closed aria-label="Sondage">
  <header class="poll-head">
    <span class="muted small poll-by"><Icon name="chart" size={14} /> Sondage lancé par <strong>{poll.created_by.pseudo}</strong>{#if isBot(poll.created_by.id)} <RoleBadge kind="bot" />{/if} · <time datetime={m.created_at}>{timeOf(m.created_at)}</time></span>
    {#if canManage}
      <span class="poll-tools">
        {#if !closed}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => api.closePoll(m.id).catch((e) => notify.error(e, 'Clôture impossible'))}>Clore</button>{/if}
        <button class="ib sm" onclick={() => (confirmDelete = true)} aria-label="Supprimer le sondage" title="Supprimer le sondage"><Icon name="trash" size={14} /></button>
      </span>
    {/if}
  </header>
  <div class="poll-question msg-body">{@html renderMarkdown(m.content, pseudos, emojis.names())}</div>
  <div class="poll-opts">
    {#each poll.options as o (o.id)}
      {@const pct = total ? Math.round((o.votes / total) * 100) : 0}
      <button class="poll-opt" class:me={o.me} class:win={winners.includes(o.id)} aria-pressed={o.me} disabled={closed}
        onclick={() => app.vote(m, o.id).catch((e) => notify.error(e, 'Vote impossible'))}>
        <span class="poll-bar" style="width: {pct}%"></span>
        <span class="grow">{#if o.me}✓ {/if}{o.label}{#if winners.includes(o.id)} 🏆{/if}</span><span>{pct} % · {o.votes}</span>
      </button>
      {#if voters?.[o.id]}<span class="muted small poll-voters">{voters[o.id].map((v) => v.pseudo).join(', ')}</span>{/if}
    {/each}
  </div>
  <footer class="muted small poll-foot">
    <span>{total} vote{total > 1 ? 's' : ''} · {poll.multiple ? 'plusieurs réponses possibles' : 'réponse unique'} · {#if !poll.closes_at}sans clôture{:else if closed}<strong>Clos</strong>{:else}reste {remaining(new Date(poll.closes_at).getTime(), now)}{/if}</span>
    {#if total}<button class="plbls-button plbls-button--compact plbls-button--contour" onclick={toggleVoters}>{voters ? 'Masquer' : 'Voir'} les votants</button>{/if}
  </footer>
</section>

{#if confirmDelete}
  <Modal title="Supprimer ce sondage ?" onclose={() => (confirmDelete = false)}>
    <p>Le sondage et ses votes seront supprimés pour tout le monde.</p>
    <div style="display: flex; gap: 8px; justify-content: flex-end">
      <button class="plbls-button plbls-button--contour" onclick={() => (confirmDelete = false)}>Annuler</button>
      <button class="plbls-button plbls-button--danger" onclick={remove}>Supprimer</button>
    </div>
  </Modal>
{/if}
