<script lang="ts">
  import { tick } from 'svelte'
  import { api } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import DrawCanvas from './DrawCanvas.svelte'
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
    <span class="muted small">Manche {s.round} / {s.settings.rounds}{drawer && !isDrawer ? ` · ${drawer.pseudo} dessine` : ''}</span>
    <div class="word">
      {#if s.phase === 'choosing'}<span class="small muted">Choix du mot</span>
      {:else}{#if isDrawer}<span class="small muted">Tu dessines</span>{/if}<strong aria-label={isDrawer ? 'Mot à dessiner' : 'Mot à deviner'}>{s.reveal ?? word}</strong>{/if}
    </div>
    <span class="clock" role="timer">{left}</span>
  </div>

  <aside class="players" aria-label="Joueurs">
    {#each s.players as p (p.id)}
      <div class="pl" class:cur={p.id === s.drawer}>
        <span class="n">{p.id === app.meId ? 'Toi' : p.pseudo}</span>
        {#if statusOf(p, s)}<span class="st" class:ok={p.found}>{statusOf(p, s)}</span>{/if}
        <b>{p.points}</b>
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
            <div class="choices">{#each g.me.choices as c, i}<button class="plbls-button" onclick={() => void api.drawChoose(channelId, i).catch(fail)}>{c}</button>{/each}</div>
          {:else}<p>{drawer?.pseudo ?? 'Le dessinateur'} choisit un mot…</p>{/if}
        </div>
      {:else if s.phase === 'turn_end'}
        <div class="veil">
          <p>{s.reveal ? `Le mot était ${s.reveal}` : 'Tour terminé'}</p>
          <p class="muted small">{REASONS[s.reason] ?? ''}</p>
          <ul>{#each s.players.filter((p) => p.turn_points > 0) as p (p.id)}<li>{p.pseudo} +{p.turn_points}</li>{/each}</ul>
        </div>
      {/if}
    </div>
    {#if canDraw}
      <div class="tools">
        {#each PALETTE as c}<button class="sw" class:on={tool !== 'eraser' && color === c} style="background: {c}" aria-label="Couleur {c}" aria-pressed={color === c} onclick={() => { color = c; if (tool === 'eraser') tool = 'pen' }}></button>{/each}
        <span class="sep"></span>
        <button class="plbls-button plbls-button--compact plbls-button--contour" aria-pressed={tool === 'pen'} onclick={() => (tool = 'pen')}>Crayon</button>
        <button class="plbls-button plbls-button--compact plbls-button--contour" aria-pressed={tool === 'eraser'} onclick={() => (tool = 'eraser')}>Gomme</button>
        <button class="plbls-button plbls-button--compact plbls-button--contour" aria-pressed={tool === 'fill'} onclick={() => (tool = 'fill')}>Remplir</button>
        <label class="size">Épaisseur
          <select class="plbls-select" bind:value={size}>{#each SIZES as z}<option value={z}>{z}</option>{/each}</select>
        </label>
        <button class="plbls-button plbls-button--compact plbls-button--contour push" onclick={undo}>Annuler</button>
        <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={clear}>Tout effacer</button>
      </div>
    {:else if me && !me.spectator && (s.phase === 'drawing' || s.phase === 'choosing') && !isDrawer}
      <div class="tools">
        <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={s.phase !== 'drawing'} onclick={() => vote('report')}>Signaler le dessin{s.reports ? ` (${s.reports})` : ''}</button>
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
  .play { display: grid; grid-template-columns: 200px minmax(0, 1fr) 280px; grid-template-rows: auto minmax(0, 1fr); grid-template-areas: 'bar bar bar' 'players board chat'; gap: var(--espace-3); padding: var(--espace-3); min-height: 0; overflow: auto; }
  .bar { grid-area: bar; display: flex; justify-content: space-between; align-items: center; gap: var(--espace-3); }
  .word { text-align: center; display: grid; }
  .word strong { font-size: 26px; letter-spacing: 0.3em; overflow-wrap: anywhere; }
  .clock { font-size: 22px; font-weight: 700; color: var(--rose-texte); font-variant-numeric: tabular-nums; }
  .players { grid-area: players; background: var(--surface); box-shadow: 0 0 0 1px var(--trait); border-radius: var(--rayon); padding: var(--espace-3); display: flex; flex-direction: column; gap: 6px; align-content: start; overflow: auto; }
  .pl { display: flex; gap: 8px; align-items: baseline; padding: 6px 8px; border-radius: 8px; font-size: 14px; }
  .pl.cur { background: var(--rose); color: var(--noir); }
  .pl .n { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  .st { font-size: 12px; color: var(--gris); }
  .st.ok { color: var(--ok); }
  .pl.cur .st { color: var(--noir); }
  .board { grid-area: board; display: flex; flex-direction: column; gap: var(--espace-3); min-width: 0; }
  .paper { position: relative; }
  .veil { position: absolute; inset: 0; display: grid; place-content: center; justify-items: center; gap: var(--espace-2); text-align: center; padding: var(--espace-3); background: color-mix(in srgb, var(--creme) 88%, transparent); border-radius: var(--rayon); overflow: auto; }
  .veil p { margin: 0; font-size: 18px; }
  .veil ul { list-style: none; margin: 0; padding: 0; }
  .choices { display: flex; flex-wrap: wrap; gap: var(--espace-2); justify-content: center; }
  .tools { display: flex; flex-wrap: wrap; gap: var(--espace-2); align-items: center; background: var(--surface); box-shadow: 0 0 0 1px var(--trait); border-radius: var(--rayon); padding: 10px 14px; }
  .sw { width: 32px; height: 32px; border-radius: 50%; border: 2px solid var(--creme); box-shadow: 0 0 0 1px var(--trait); cursor: pointer; padding: 0; }
  .sw.on { box-shadow: 0 0 0 3px var(--noir); }
  .sep { width: 1px; height: 28px; background: var(--trait); }
  .size { display: flex; align-items: center; gap: 6px; font-size: 13px; }
  .push { margin-left: auto; }
  [aria-pressed='true'].plbls-button { background: var(--rose); color: var(--noir); }
  .chat { grid-area: chat; background: var(--surface); box-shadow: 0 0 0 1px var(--trait); border-radius: var(--rayon); padding: var(--espace-3); display: flex; flex-direction: column; gap: var(--espace-2); min-height: 0; }
  .log { flex: 1; min-height: 120px; overflow: auto; display: flex; flex-direction: column; gap: 4px; font-size: 14px; }
  .line { overflow-wrap: anywhere; }
  .line.ok { color: var(--ok); }
  .line.close { color: var(--rose-texte); }
  .line.priv { color: var(--gris); }
  .chat input { width: 100%; box-sizing: border-box; height: 44px; font-size: 16px; }
  .sr { position: absolute; left: -9999px; }
  @media (max-width: 760px) {
    .play { grid-template-columns: minmax(0, 1fr); grid-template-rows: none; grid-template-areas: 'bar' 'board' 'players' 'chat'; }
    .players { flex-direction: row; flex-wrap: wrap; overflow: visible; padding: 0; background: none; box-shadow: none; gap: 6px; }
    .pl { padding: 5px 10px; border-radius: var(--rayon-pilule); background: var(--calque); font-size: 13px; }
    .word strong { font-size: 22px; letter-spacing: 0.2em; }
    .chat input { height: 48px; }
  }
</style>
