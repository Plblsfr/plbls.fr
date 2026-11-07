<script lang="ts">
  import Avatar from '../../components/Avatar.svelte'
  import { api } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import { draw } from './draw.svelte'

  let { channelId, serverId }: { channelId: string; serverId: string } = $props()

  const s = $derived(draw.games[channelId].state)
  const isHost = $derived(s.host === app.meId)
  const enough = $derived(s.players.filter((p) => p.connected).length >= 2)
  let words = $state('')
  const fail = (e: unknown) => notify.error(e, 'Action refusée')
  const set = (b: Record<string, unknown>) => void api.drawSettings(channelId, b).catch(fail)
  const num = (e: Event) => Number((e.currentTarget as HTMLSelectElement).value)
  const saveWords = () => set({ custom_words: words.split('\n').map((w) => w.trim()).filter(Boolean) })
</script>

<div class="lobby">
  <header>
    <div>
      <div class="v-eyebrow">Salon vocal</div>
      <h2>Draw2Gether</h2>
    </div>
    <div class="v-mono v-mute">{s.players.length} / {s.settings.max_players} joueurs</div>
  </header>
  <div class="cols">
    <section class="card">
      <h3 class="v-eyebrow">Joueurs</h3>
      <ul>
        {#each s.players as p (p.id)}
          <li>
            <Avatar pseudo={p.pseudo} userId={p.id} {serverId} size="sm" />
            <span class="name">{p.pseudo}{p.id === app.meId ? ' (toi)' : ''}</span>
            {#if p.id === s.host}<span class="v-tag rose">Hôte</span>{:else if !p.connected}<span class="v-tag">absent</span>{/if}
            {#if isHost && p.id !== s.host}<button class="plbls-button plbls-button--compact plbls-button--discret" onclick={() => void api.drawVote(channelId, 'kick', p.id).catch(fail)}>Exclure</button>{/if}
          </li>
        {/each}
      </ul>
    </section>
    <section class="card">
      <h3 class="v-eyebrow">Réglages{isHost ? '' : ' (hôte)'}</h3>
      <label>Manches
        <select class="plbls-select" disabled={!isHost} value={s.settings.rounds} onchange={(e) => set({ rounds: num(e) })}>{#each [1, 2, 3, 4, 5, 6, 8, 10] as n}<option value={n}>{n}</option>{/each}</select>
      </label>
      <label>Durée du tour
        <select class="plbls-select" disabled={!isHost} value={s.settings.turn_secs} onchange={(e) => set({ turn_secs: num(e) })}>{#each [30, 45, 60, 80, 100, 120, 180, 240] as n}<option value={n}>{n} s</option>{/each}</select>
      </label>
      <label>Mots proposés
        <select class="plbls-select" disabled={!isHost} value={s.settings.word_choices} onchange={(e) => set({ word_choices: num(e) })}>{#each [1, 2, 3, 4, 5] as n}<option value={n}>{n}</option>{/each}</select>
      </label>
      <label>Joueurs au plus
        <select class="plbls-select" disabled={!isHost} value={s.settings.max_players} onchange={(e) => set({ max_players: num(e) })}>{#each [2, 4, 6, 8, 10, 12] as n}<option value={n}>{n}</option>{/each}</select>
      </label>
      <label>Langue des mots
        <select class="plbls-select" disabled={!isHost} value={s.settings.lang} onchange={(e) => set({ lang: (e.currentTarget as HTMLSelectElement).value })}><option value="fr">Français</option><option value="en">English</option></select>
      </label>
      <label>Mots personnalisés
        <select class="plbls-select" disabled={!isHost || s.settings.custom_count === 0} value={s.settings.custom_only ? 'only' : 'add'} onchange={(e) => set({ custom_only: (e.currentTarget as HTMLSelectElement).value === 'only' })}><option value="add">Ajoutés à la liste</option><option value="only">Utilisés seuls</option></select>
      </label>
      {#if isHost}
        <label class="stack">{s.settings.custom_count} mot{s.settings.custom_count > 1 ? 's' : ''} personnalisé{s.settings.custom_count > 1 ? 's' : ''} (un par ligne)
          <textarea class="plbls-input" rows="3" bind:value={words} onchange={saveWords}></textarea>
        </label>
      {/if}
      <p class="rules v-note">Règles : pas de lettres, de chiffres ni du mot dans le dessin. Pas d'indices dans le chat. Ne soufflez pas la réponse.</p>
      {#if isHost}
        <button class="plbls-button v-rose" disabled={!enough} onclick={() => void api.drawStart(channelId).catch(fail)}>Lancer la partie</button>
        {#if !enough}<p class="v-note">Il faut au moins 2 joueurs.</p>{/if}
      {:else}
        <p class="v-note">L'hôte lance la partie.</p>
      {/if}
    </section>
  </div>
</div>

<style>
  .lobby { display: flex; flex-direction: column; gap: 16px; padding: 18px; overflow: auto; min-height: 0; }
  header { display: flex; justify-content: space-between; align-items: flex-end; gap: 12px; }
  h2 { font-family: var(--f-titre); font-size: 28px; font-weight: 700; letter-spacing: -0.03em; margin: 4px 0 0; }
  h3 { margin: 0 0 4px; }
  .cols { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px; }
  .card { background: var(--surface); box-shadow: var(--o1); border-radius: var(--r-l); padding: 18px; display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  li { display: flex; align-items: center; gap: 10px; padding: 6px 8px; border-radius: var(--r-s); font-size: 14px; }
  li:hover { background: var(--papier-2); }
  .name { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  label { display: flex; justify-content: space-between; align-items: center; gap: 12px; font-size: 13.5px; color: var(--encre-2); }
  label .plbls-select { min-height: 36px; width: auto; }
  label.stack { flex-direction: column; align-items: stretch; }
  textarea { font-size: 16px; }
  .rules { margin: auto 0 0; }
  @media (max-width: 760px) { .cols { grid-template-columns: minmax(0, 1fr); } }
</style>
