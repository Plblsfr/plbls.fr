<script lang="ts">
  import { tick } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import { clock } from '../../blocks/musique/music'
  import { ctx, type MenuItem } from '../../lib/ctxmenu.svelte'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import SillonCover from './SillonCover.svelte'
  import SillonLike from './SillonLike.svelte'
  import { choix, collection, lecteur, menuPiste } from './sillon.svelte'
  import { marqueDe, REPETITIONS, SUGGESTIONS, type ElementFile, type Suggestions } from './sillon'

  const courant = $derived(lecteur.element ?? (lecteur.piste ? { id: '', suggested: false, discovery: false, track: lecteur.piste } : null))
  const suite = $derived(lecteur.elements.slice(lecteur.index + 1).map((e, k) => ({ e, i: lecteur.index + 1 + k })))
  const playlist = $derived(lecteur.contexte.type === 'playlist' ? collection.playlists.find((p) => lecteur.contexte.type === 'playlist' && p.id === lecteur.contexte.id) ?? null : null)
  const ajout = $derived(!!app.sillonMe?.permissions.add)
  let ajoutees = $state<string[]>([])
  let liste = $state<HTMLOListElement>()
  let tire = $state(-1)
  let survol = $state(-1)

  $effect(() => { if (!collection.charge) void collection.charger() })

  const echec = (e: unknown) => notify.error(e, 'La file n’a pas pu être modifiée')
  const faire = (p: Promise<unknown>) => void p.catch(echec)

  async function bouger(from: number, to: number) {
    const a = lecteur.elements[from]
    const b = lecteur.elements[to]
    if (!a || !b || to <= lecteur.index) return
    await lecteur.deplacerElement(a.id, b.id).catch(echec)
    await tick()
    liste?.querySelector<HTMLElement>(`[data-element="${a.id}"] .sillon-file-jouer`)?.focus()
  }

  function touche(ev: KeyboardEvent, i: number) {
    if (!ev.altKey || (ev.key !== 'ArrowUp' && ev.key !== 'ArrowDown')) return
    ev.preventDefault()
    void bouger(i, ev.key === 'ArrowUp' ? i - 1 : i + 1)
  }

  async function garder(e: ElementFile) {
    const c = lecteur.contexte
    if (c.type !== 'playlist') { choix.contenu = { tracks: [e.track.id] }; choix.titre = e.track.title; return }
    if (await collection.ajouter(c.id, { tracks: [e.track.id] })) ajoutees = [...ajoutees, e.id]
  }

  async function ecarter(e: ElementFile) {
    try {
      await lecteur.nePlusProposer(e)
      notify.ok('Ce titre ne sera plus proposé.')
    } catch (err) { notify.error(err, 'La suggestion n’a pas pu être écartée') }
  }

  const menu = (e: ElementFile, i: number): MenuItem[] => menuPiste(e.track, [
    { label: 'Lire maintenant', run: () => lecteur.aller(i) },
    ...(i > lecteur.index + 1 ? [{ label: 'Monter', run: () => void bouger(i, i - 1) }] : []),
    ...(i < lecteur.elements.length - 1 ? [{ label: 'Descendre', run: () => void bouger(i, i + 1) }] : []),
    { label: 'Retirer de la file', run: () => faire(lecteur.retirer(e.id)), danger: true },
  ])

  const repetition = $derived(REPETITIONS.find((r) => r.id === lecteur.repetition)?.label ?? '')
  const changerSuggestions = (v: Suggestions) => faire(lecteur.reglerSuggestions(v))
  const changerDecouverte = (v: boolean) => void lecteur.changerDecouverte(v).catch((e) => notify.error(e, 'Le réglage n’a pas pu être enregistré'))
</script>

<div class="film-page">
  <div class="film-filters"><h2 class="v-titre film-filters-title">File de lecture</h2></div>

  <section class="film-section sillon-reglages" aria-label="Lecture aléatoire et suggestions">
    <button class="v-chip" aria-pressed={lecteur.aleatoire} onclick={() => faire(lecteur.basculerAleatoire())}><Icon name="shuffle" size={16} /> Aléatoire</button>
    <button class="v-chip" aria-pressed={lecteur.repetition !== 'off'} onclick={() => faire(lecteur.changerRepetition())} aria-label={`Répétition : ${repetition}`} title="Changer la répétition"><Icon name="repeat" size={16} /> {repetition}</button>
    <div class="sillon-reglage">
      <span class="v-lbl" id="sillon-suggestions">Suggestions</span>
      <div class="v-seg" role="radiogroup" aria-labelledby="sillon-suggestions">
        {#each SUGGESTIONS as s (s.id)}<button role="radio" aria-checked={lecteur.suggestions === s.id} onclick={() => changerSuggestions(s.id)}>{s.label}</button>{/each}
      </div>
    </div>
    {#if ajout}
      <label class="sillon-reglage sillon-interrupteur">
        <input type="checkbox" role="switch" checked={lecteur.decouverte} onchange={(e) => changerDecouverte(e.currentTarget.checked)} />
        <span><span class="v-lbl">Découverte</span><span class="v-note">Télécharge des albums proches de ce que tu écoutes, proposés une fois importés.</span></span>
      </label>
    {/if}
  </section>

  {#if courant}
    <section class="film-section">
      <h2 class="v-titre">En cours</h2>
      <div class="sillon-file-ligne cur">
        <span class="sillon-file-art"><SillonCover id={courant.track.album.id} title={courant.track.album.title} image={courant.track.album.cover} size="small" /></span>
        <span class="grow sillon-file-t"><strong class="truncate">{courant.track.title}</strong><span class="v-note truncate">{courant.track.artist.name} · {courant.track.album.title}</span></span>
        {#if marqueDe(courant)}<span class="v-tag" class:rose={!courant.discovery} class:info={courant.discovery}>{marqueDe(courant)}</span>{/if}
        <SillonLike kind="tracks" id={courant.track.id} titre={courant.track.title} />
      </div>
    </section>
  {/if}

  <section class="film-section">
    <h2 class="v-titre">À suivre</h2>
    {#if suite.length}
      <p class="v-note sillon-aide">Glisse un titre ou utilise Alt + ↑ / ↓ pour changer l’ordre.</p>
      <ol class="sillon-file" bind:this={liste}>
        {#each suite as { e, i } (e.id)}
          {@const marque = marqueDe(e)}
          <li class="sillon-file-ligne" class:marque={!!marque} class:decouverte={e.discovery} class:tire={tire === i} class:survol={survol === i && tire !== i} data-element={e.id} use:ctx={() => menu(e, i)}
            draggable="true" ondragstart={(ev) => { tire = i; ev.dataTransfer?.setData('text/plain', e.track.title) }} ondragend={() => { tire = -1; survol = -1 }}
            ondragover={(ev) => { if (tire >= 0) { ev.preventDefault(); survol = i } }} ondrop={(ev) => { ev.preventDefault(); const f = tire; tire = -1; survol = -1; void bouger(f, i) }}>
            <span class="sillon-poignee" aria-hidden="true"><Icon name="poignee" size={16} /></span>
            <button class="sillon-file-jouer" onclick={() => lecteur.aller(i)} onkeydown={(ev) => touche(ev, i)} aria-keyshortcuts="Alt+ArrowUp Alt+ArrowDown" aria-label={`Lire : ${e.track.title}, ${e.track.artist.name}${marque ? ` (${marque.toLowerCase()})` : ''}`}>
              <span class="sillon-file-art"><SillonCover id={e.track.album.id} title={e.track.album.title} image={e.track.album.cover} size="small" /></span>
              <span class="grow sillon-file-t"><strong class="truncate">{e.track.title}</strong><span class="v-note truncate">{e.track.artist.name} · {e.track.album.title}</span></span>
              {#if marque}<span class="v-tag" class:rose={!e.discovery} class:info={e.discovery}>{marque}</span>{/if}
              <span class="v-mono v-mute sillon-file-duree">{clock(e.track.duration ?? 0)}</span>
            </button>
            {#if marque}
              <span class="sillon-file-actions">
                {#if ajoutees.includes(e.id)}
                  <span class="v-tag ok">Ajouté à « {playlist?.name ?? 'la playlist'} »</span>
                {:else}
                  <button class="v-btn sm sec" onclick={() => void garder(e)}><Icon name="plus" size={14} />{playlist ? `Ajouter à « ${playlist.name} »` : 'Ajouter à la playlist'}</button>
                {/if}
                <button class="v-btn sm ghost" onclick={() => void ecarter(e)}>Ne plus proposer</button>
              </span>
            {/if}
            <SillonLike kind="tracks" id={e.track.id} titre={e.track.title} />
            <button class="v-ib sm" onclick={() => faire(lecteur.retirer(e.id))} aria-label={`Retirer ${e.track.title} de la file`} title="Retirer de la file"><Icon name="x" size={16} /></button>
          </li>
        {/each}
      </ol>
    {:else}
      <div class="v-empty"><Icon name="list" size={28} /><p class="v-note">{lecteur.piste ? 'Rien après ce titre.' : 'La file est vide. Lance un album, une playlist ou un titre.'}</p></div>
    {/if}
  </section>
</div>

<style>
  .sillon-reglages { display: flex; flex-direction: row; flex-wrap: wrap; align-items: center; gap: 12px 24px; }
  .sillon-reglage { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; }
  .sillon-interrupteur { max-width: 420px; cursor: pointer; }
  .sillon-interrupteur input { flex: none; width: 18px; height: 18px; accent-color: var(--rose-fonce); }
  .sillon-interrupteur > span { display: grid; gap: 2px; flex: 1; min-width: 0; }
  .sillon-aide { margin: 0 0 8px 10px; }
  .sillon-file { display: flex; flex-direction: column; gap: 2px; margin: 0; padding: 0; list-style: none; }
  .sillon-file-ligne { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 8px; padding: 4px 6px; border-radius: var(--r); }
  .sillon-file-ligne.cur { padding: 8px 10px; background: var(--rose-doux); }
  .sillon-file-ligne.marque { background: color-mix(in srgb, var(--rose) 28%, transparent); box-shadow: inset 3px 0 0 var(--rose-fonce); }
  .sillon-file-ligne.marque.decouverte { background: var(--info-fond); box-shadow: inset 3px 0 0 var(--info); }
  .sillon-file-ligne.tire { opacity: 0.45; }
  .sillon-file-ligne.survol { box-shadow: inset 0 2px 0 var(--rose-fonce); }
  .sillon-poignee { display: inline-grid; place-items: center; flex: none; width: 20px; color: var(--gris); cursor: grab; }
  .sillon-file-jouer { display: flex; flex: 1 1 260px; align-items: center; gap: 12px; min-width: 0; min-height: 48px; padding: 4px 6px; border: 0; border-radius: var(--r); background: transparent; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .sillon-file-jouer:hover { background: var(--papier-2); }
  .sillon-file-jouer:focus-visible { outline: none; box-shadow: var(--anneau); }
  .sillon-file-art { position: relative; flex: none; width: 40px; height: 40px; overflow: hidden; border-radius: var(--r-s); }
  .sillon-file-t { display: grid; min-width: 0; line-height: 1.3; }
  .sillon-file-t strong { font-size: 14.5px; font-weight: 600; }
  .sillon-file-actions { display: flex; flex-wrap: wrap; gap: 6px; }
  @media (max-width: 720px) {
    .sillon-file-duree { display: none; }
    .sillon-file-actions { order: 3; width: 100%; padding-left: 28px; }
  }
</style>
