<script lang="ts">
  import { tick } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import { clock } from '../../blocks/musique/music'
  import { ctx, type MenuItem } from '../../lib/ctxmenu.svelte'
  import SillonCover from './SillonCover.svelte'
  import SillonLike from './SillonLike.svelte'
  import { lecteur, menuPiste, type Contexte } from './sillon.svelte'
  import { disque, type Piste } from './sillon'

  let { pistes, contexte = { type: 'pistes' }, album = false, cles, extra, deplacer, focus }: {
    pistes: Piste[]; contexte?: Contexte; album?: boolean; cles?: string[]; focus?: string; extra?: (p: Piste, i: number) => MenuItem[]; deplacer?: (from: number, to: number) => void
  } = $props()

  const parDisque = $derived(album && disque(pistes))
  const lignes = $derived(pistes.map((p, i) => ({ p, i, cle: cles?.[i] ?? p.id })))
  let liste = $state<HTMLOListElement>()
  let tire = $state(-1)
  let survol = $state(-1)
  const jouer = (i: number) => lecteur.jouer(pistes, i, contexte)
  const ordre = (i: number): MenuItem[] => (deplacer ? [
    ...(i > 0 ? [{ label: 'Monter', run: () => void bouger(i, i - 1), sep: true }] : []),
    ...(i < pistes.length - 1 ? [{ label: 'Descendre', run: () => void bouger(i, i + 1), sep: i === 0 }] : []),
  ] : [])
  const menu = (p: Piste, i: number): MenuItem[] => menuPiste(p, [...(p.available ? [{ label: 'Lire à partir de ce titre', run: () => jouer(i) }] : []), ...ordre(i), ...(extra?.(p, i) ?? [])])

  async function bouger(from: number, to: number) {
    if (!deplacer || to < 0 || to >= pistes.length || from === to) return
    deplacer(from, to)
    await tick()
    liste?.querySelector<HTMLElement>(`[data-ligne="${to}"] .sillon-piste`)?.focus()
  }

  const viser = (node: HTMLElement, oui: boolean) => { if (oui) node.scrollIntoView({ block: 'center' }) }

  function touche(e: KeyboardEvent, i: number) {
    if (!deplacer || !e.altKey || (e.key !== 'ArrowUp' && e.key !== 'ArrowDown')) return
    e.preventDefault()
    void bouger(i, e.key === 'ArrowUp' ? i - 1 : i + 1)
  }

  function lacher(e: DragEvent, i: number) {
    e.preventDefault()
    const from = tire
    tire = -1
    survol = -1
    void bouger(from, i)
  }
</script>

<ol class="sillon-pistes" bind:this={liste}>
  {#each lignes as { p, i, cle } (cle)}
    {@const cur = lecteur.piste?.id === p.id}
    {#if parDisque && (i === 0 || pistes[i - 1].disc !== p.disc)}<li class="sillon-disque v-mono">Disque {p.disc}</li>{/if}
    <li class="sillon-ligne" class:cible={focus === p.id} use:viser={focus === p.id} class:tire={tire === i} class:survol={survol === i && tire !== i} data-ligne={i} use:ctx={() => menu(p, i)}
      draggable={deplacer ? 'true' : undefined}
      ondragstart={deplacer ? (e) => { tire = i; e.dataTransfer?.setData('text/plain', p.title) } : undefined}
      ondragend={deplacer ? () => { tire = -1; survol = -1 } : undefined}
      ondragover={deplacer ? (e) => { if (tire >= 0) { e.preventDefault(); survol = i } } : undefined}
      ondrop={deplacer ? (e) => lacher(e, i) : undefined}>
      {#if deplacer}<span class="sillon-poignee" aria-hidden="true"><Icon name="poignee" size={16} /></span>{/if}
      <button class="sillon-piste" class:cur class:off={!p.available} disabled={!p.available} onclick={() => jouer(i)} onkeydown={(e) => touche(e, i)} aria-current={cur ? 'true' : undefined}
        aria-keyshortcuts={deplacer ? 'Alt+ArrowUp Alt+ArrowDown' : undefined}
        aria-label={`${cur && lecteur.lecture ? 'En cours' : 'Lire'} : ${p.title}, ${p.artist.name}${p.available ? '' : ' (indisponible)'}`}>
        {#if album}
          <span class="sillon-n v-mono">{#if cur}<Icon name={lecteur.lecture ? 'onde' : 'pause'} size={14} />{:else}{p.number ?? ''}{/if}</span>
        {:else}
          <span class="sillon-mini-art"><SillonCover id={p.album.id} title={p.album.title} image={p.album.cover} size="small" /></span>
        {/if}
        <span class="grow sillon-piste-t">
          <strong class="truncate">{p.title}{#if p.explicit}<span class="v-tag sillon-e" title="Explicite">E</span>{/if}</strong>
          <span class="v-note truncate">{p.available ? (album ? p.artist.name : `${p.artist.name} · ${p.album.title}`) : 'Indisponible'}</span>
        </span>
        <span class="v-mono v-mute">{clock(p.duration ?? 0)}</span>
      </button>
      <SillonLike kind="tracks" id={p.id} titre={p.title} />
    </li>
  {/each}
</ol>

<style>
  .sillon-pistes { display: flex; flex-direction: column; gap: 2px; margin: 0; padding: 0; list-style: none; }
  .sillon-ligne { display: flex; align-items: center; gap: 2px; border-radius: var(--r); }
  .sillon-ligne.tire { opacity: 0.45; }
  .sillon-ligne.cible { box-shadow: inset 0 0 0 2px var(--rose-fonce); }
  .sillon-ligne.survol { box-shadow: inset 0 2px 0 var(--rose-fonce); }
  .sillon-poignee { display: inline-grid; place-items: center; flex: none; width: 20px; color: var(--gris); cursor: grab; }
  .sillon-disque { padding: 14px 10px 4px; color: var(--gris); font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase; }
  .sillon-piste { display: flex; flex: 1; align-items: center; gap: 14px; min-width: 0; min-height: 52px; padding: 6px 10px; border: 0; border-radius: var(--r); background: transparent; color: inherit; font: inherit; text-align: left; cursor: pointer; transition: background var(--d) var(--sortie); }
  .sillon-piste:hover:not(:disabled) { background: var(--papier-2); }
  .sillon-piste:focus-visible { outline: none; box-shadow: var(--anneau); }
  .sillon-piste.cur { background: var(--rose-doux); }
  .sillon-piste.off { opacity: 0.55; cursor: default; }
  .sillon-n { display: inline-grid; place-items: center; flex: none; width: 28px; color: var(--gris); }
  .sillon-piste.cur .sillon-n { color: var(--rose-texte); }
  .sillon-mini-art { position: relative; flex: none; width: 40px; height: 40px; overflow: hidden; border-radius: var(--r-s); }
  .sillon-piste-t { display: grid; min-width: 0; line-height: 1.3; }
  .sillon-piste-t strong { display: flex; align-items: center; gap: 6px; font-size: 14.5px; font-weight: 600; }
  .sillon-e { flex: none; padding: 0 5px; font-size: 10px; }
</style>
