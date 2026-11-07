<script lang="ts">
  import { tick } from 'svelte'
  import { api } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import DrawCanvas from './DrawCanvas.svelte'
  import Icon from '../../components/Icon.svelte'
  import { draw } from './draw.svelte'
  import { chatText, formatClock, PALETTE, REASONS, remainingMs, SIZES, statusOf } from './draw'

  let { channelId }: { channelId: string } = $props()

  const g = $derived(draw.games[channelId])
  const s = $derived(g.state)
  const me = $derived(s.players.find((p) => p.id === app.meId))
  const drawer = $derived(s.players.find((p) => p.id === s.drawer))
  const isDrawer = $derived(s.drawer === app.meId)
  const isHost = $derived(s.host === app.meId)
  const canDraw = $derived(isDrawer && s.phase === 'drawing')
  const canGuess = $derived(!!me && !me.spectator)
  const word = $derived(isDrawer && g.me.word ? g.me.word.toUpperCase() : s.mask)

  let tool = $state<'pen' | 'eraser' | 'fill'>('pen')
  let color = $state(PALETTE[0])
  let size = $state(SIZES[1])
  let text = $state('')
  let now = $state(Date.now())
  let log = $state<HTMLElement>()
  const left = $derived(formatClock(remainingMs(s.deadline, g.offset, now)))
  const fail = (e: unknown) => notify.error(e, 'Action refusée')
  const placeholder = $derived(!canGuess ? 'Spectateur : tu joueras à la prochaine manche' : isDrawer || me?.found ? 'Message aux joueurs qui ont trouvé' : 'Devine le mot')

  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 250)
    return () => clearInterval(t)
  })

  $effect(() => {
    void g.chat.length
    void tick().then(() => log?.scrollTo({ top: log.scrollHeight }))
  })

  function send(e: Event) {
    e.preventDefault()
    const t = text.trim()
    if (!t) return
    text = ''
    void api.drawGuess(channelId, t).catch(fail)
  }

  const undo = () => { draw.apply(channelId, { op: 'undo' }); draw.send(channelId, { op: 'undo' }) }
  const clear = () => { draw.apply(channelId, { op: 'clear' }); draw.send(channelId, { op: 'clear' }) }
  const vote = (kind: 'report' | 'skip') => void api.drawVote(channelId, kind).catch(fail)
</script>

<div class="play">
  <div class="bar">
    <span class="round"><span class="v-tag rose v-mono">Manche {s.round} / {s.settings.rounds}</span>{#if drawer && !isDrawer}<b>{drawer.pseudo} dessine</b>{/if}</span>
    <div class="word">
      {#if s.phase === 'choosing'}<span class="v-note">Choix du mot</span>
      {:else}{#if isDrawer}<span class="v-note">Tu dessines</span>{/if}<strong aria-label={isDrawer ? 'Mot à dessiner' : 'Mot à deviner'}>{s.reveal ?? word}</strong>{/if}
    </div>
    <span class="clock"><Icon name="horloge" size={18} /><span role="timer">{left}</span></span>
  </div>

  <aside class="players" aria-label="Joueurs">
    <span class="v-eyebrow players-h">Joueurs</span>
    {#each s.players as p (p.id)}
      <div class="pl" class:cur={p.id === s.drawer} class:found={p.found} class:away={!p.connected}>
        <span class="pl-ic" aria-hidden="true">{#if p.id === s.drawer}<Icon name="pinceau" size={14} />{:else if p.found}<Icon name="ok" size={14} />{/if}</span>
        <span class="n">{p.id === app.meId ? 'Toi' : p.pseudo}</span>
        {#if statusOf(p, s)}<span class="st" class:ok={p.found}>{statusOf(p, s)}</span>{/if}
        <b class="v-mono">{p.points}</b>
      </div>
    {/each}
  </aside>

  <div class="board">
    <div class="paper">
      <DrawCanvas {channelId} {canDraw} {tool} {color} {size} />
      {#if s.phase === 'choosing'}
        <div class="veil">
          {#if isDrawer && g.me.choices}
            <p>Choisis un mot à dessiner</p>
            <div class="choices">{#each g.me.choices as c, i}<button class="plbls-button plbls-button--contour" onclick={() => void api.drawChoose(channelId, i).catch(fail)}>{c}</button>{/each}</div>
          {:else}<p>{drawer?.pseudo ?? 'Le dessinateur'} choisit un mot…</p>{/if}
        </div>
      {:else if s.phase === 'turn_end'}
        <div class="veil">
          <p>{#if s.reveal}Le mot était <span class="v-surl">{s.reveal}</span>{:else}Tour terminé{/if}</p>
          <p class="v-note">{REASONS[s.reason] ?? ''}</p>
          <ul>{#each s.players.filter((p) => p.turn_points > 0) as p (p.id)}<li>{p.pseudo} <span class="v-mono pts">+{p.turn_points}</span></li>{/each}</ul>
        </div>
      {/if}
    </div>
    {#if canDraw}
      <div class="tools">
        {#each PALETTE as c}<button class="sw" class:on={tool !== 'eraser' && color === c} style="background: {c}" aria-label="Couleur {c}" aria-pressed={color === c} onclick={() => { color = c; if (tool === 'eraser') tool = 'pen' }}></button>{/each}
        <span class="sep"></span>
        <button class="v-ib" aria-pressed={tool === 'pen'} onclick={() => (tool = 'pen')} aria-label="Crayon" title="Crayon"><Icon name="crayon" /></button>
        <button class="v-ib" aria-pressed={tool === 'eraser'} onclick={() => (tool = 'eraser')} aria-label="Gomme" title="Gomme"><Icon name="gomme" /></button>
        <button class="v-ib" aria-pressed={tool === 'fill'} onclick={() => (tool = 'fill')} aria-label="Remplir" title="Remplir"><Icon name="cible" /></button>
        <span class="v-seg size" role="group" aria-label="Épaisseur">{#each SIZES as z}<button class="v-mono" aria-pressed={size === z} onclick={() => (size = z)}>{z}</button>{/each}</span>
        <button class="v-ib push" onclick={undo} aria-label="Annuler" title="Annuler"><Icon name="annule" /></button>
        <button class="v-ib danger" onclick={clear} aria-label="Tout effacer" title="Tout effacer"><Icon name="corbeille" /></button>
      </div>
    {:else if me && !me.spectator && (s.phase === 'drawing' || s.phase === 'choosing') && !isDrawer}
      <div class="tools">
        <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={s.phase !== 'drawing'} onclick={() => vote('report')}><Icon name="drapeau" size={14} /> Signaler le dessin{s.reports ? ` (${s.reports})` : ''}</button>
        <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => vote('skip')}>{isHost ? 'Passer le tour' : `Voter pour passer${s.skip_votes ? ` (${s.skip_votes})` : ''}`}</button>
      </div>
    {:else if isHost && (s.phase === 'drawing' || s.phase === 'choosing')}
      <div class="tools"><button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => vote('skip')}>Passer le tour</button></div>
    {/if}
  </div>

  <section class="chat" aria-label="Chat de la partie">
    <div class="log" bind:this={log}>
      {#each g.chat as l}<div class="line" class:ok={l.kind === 'found'} class:close={l.kind === 'close'} class:priv={l.kind === 'chat_private'}>{chatText(l, app.meId)}</div>{/each}
    </div>
    <form onsubmit={send}>
      <label class="sr" for="d2g-text">Ta proposition</label>
      <input id="d2g-text" class="plbls-input" bind:value={text} {placeholder} maxlength="100" autocomplete="off" disabled={!canGuess} />
    </form>
  </section>
</div>

<style>
  .play { display: grid; grid-template-columns: 200px minmax(0, 1fr) 280px; grid-template-rows: auto minmax(0, 1fr); grid-template-areas: 'bar bar bar' 'players board chat'; gap: 14px; padding: 18px; min-height: 0; overflow: auto; }
  .bar { grid-area: bar; display: flex; justify-content: space-between; align-items: center; gap: 12px; flex-wrap: wrap; }
  .round { display: inline-flex; align-items: center; gap: 10px; min-width: 0; }
  .word { text-align: center; display: grid; }
  .word strong { font-family: var(--f-mono); font-size: 22px; letter-spacing: 0.3em; overflow-wrap: anywhere; }
  .clock { display: inline-flex; align-items: center; gap: 8px; font-family: var(--f-mono); font-size: 18px; font-weight: 600; color: var(--rose-texte); font-variant-numeric: tabular-nums; }
  .players { grid-area: players; display: flex; flex-direction: column; gap: 2px; align-content: start; overflow: auto; }
  .players-h { padding: 0 8px 6px; }
  .pl { display: flex; gap: 8px; align-items: center; padding: 6px 8px; border-radius: var(--r-s); font-size: 13.5px; }
  .pl.cur { background: var(--rose-doux); }
  .pl.found { background: var(--ok-fond); }
  .pl.away { opacity: 0.6; }
  .pl-ic { display: inline-flex; width: 15px; flex: none; }
  .pl.found .pl-ic { color: var(--ok); }
  .pl .n { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  .st { font-size: 12px; color: var(--gris); }
  .st.ok { color: var(--ok); }
  .board { grid-area: board; display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  .paper { position: relative; }
  .veil { position: absolute; inset: 0; display: grid; place-content: center; justify-items: center; gap: 10px; text-align: center; padding: 12px; background: color-mix(in srgb, var(--surface) 90%, transparent); border-radius: var(--r-l); overflow: auto; }
  .veil p { margin: 0; font-family: var(--f-titre); font-size: 20px; font-weight: 700; letter-spacing: -0.02em; }
  .veil p.v-note { font-family: var(--f-texte); font-size: 13px; font-weight: 400; letter-spacing: 0; }
  .veil ul { list-style: none; margin: 0; padding: 0; font-size: 14px; }
  .pts { color: var(--ok); }
  .choices { display: flex; flex-wrap: wrap; gap: 8px; justify-content: center; }
  .tools { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; justify-content: center; }
  .sw { width: 28px; height: 28px; border-radius: 8px; border: 0; box-shadow: inset 0 0 0 1px rgb(0 0 0 / 12%); cursor: pointer; padding: 0; }
  .sw.on { box-shadow: 0 0 0 2px var(--surface), 0 0 0 4px var(--encre); }
  .sw:focus-visible { outline: none; box-shadow: var(--anneau); }
  .sep { width: 1px; height: 24px; margin: 0 4px; background: var(--trait); }
  .size > button { min-width: 36px; justify-content: center; padding: 0 8px; }
  .push { margin-left: 4px; }
  .v-ib.danger { color: var(--danger); }
  .chat { grid-area: chat; display: flex; flex-direction: column; gap: 8px; min-height: 0; }
  .log { flex: 1; min-height: 120px; overflow: auto; display: flex; flex-direction: column; gap: 4px; padding: 10px; border-radius: var(--r); background: var(--papier-2); font-size: 13.5px; }
  .line { overflow-wrap: anywhere; }
  .line.ok { color: var(--ok); font-weight: 600; }
  .line.close { color: var(--attention); }
  .line.priv { color: var(--gris); }
  .chat input { width: 100%; box-sizing: border-box; height: 44px; font-size: 16px; }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
  @media (pointer: coarse) { .sw { width: 44px; height: 44px; } .tools .v-ib, .size > button { min-width: 44px; height: 44px; } }
  @media (max-width: 760px) {
    .play { grid-template-columns: minmax(0, 1fr); grid-template-rows: none; grid-template-areas: 'bar' 'board' 'players' 'chat'; padding: 12px; }
    .players { flex-direction: row; flex-wrap: wrap; overflow: visible; gap: 6px; }
    .players-h { display: none; }
    .pl { padding: 5px 10px; border-radius: var(--r-p); background: var(--papier-2); font-size: 13px; }
    .word strong { font-size: 20px; letter-spacing: 0.2em; }
    .chat input { height: 48px; }
  }
</style>
