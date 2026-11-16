<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { clock } from '../../blocks/musique/music'
  import { ctx, type MenuItem } from '../../lib/ctxmenu.svelte'
  import { hrefs, router } from '../../lib/route.svelte'
  import SillonCover from './SillonCover.svelte'
  import { lecteur, type Contexte } from './sillon.svelte'
  import { disque, type Piste } from './sillon'

  let { pistes, contexte = { type: 'pistes' }, album = false }: { pistes: Piste[]; contexte?: Contexte; album?: boolean } = $props()

  const parDisque = $derived(album && disque(pistes))
  const jouer = (i: number) => lecteur.jouer(pistes, i, contexte)
  const menu = (p: Piste, i: number): MenuItem[] => [
    ...(p.available ? [{ label: 'Lire à partir de ce titre', run: () => jouer(i) }] : []),
    ...(album ? [] : [{ label: 'Voir l’album', run: () => router.go(hrefs.musique('album', p.album.id)) }]),
    { label: 'Voir l’artiste', run: () => router.go(hrefs.musique('artist', p.artist.id)) },
  ]
</script>

<ol class="sillon-pistes">
  {#each pistes as p, i (p.id)}
    {@const cur = lecteur.piste?.id === p.id}
    {#if parDisque && (i === 0 || pistes[i - 1].disc !== p.disc)}<li class="sillon-disque v-mono">Disque {p.disc}</li>{/if}
    <li use:ctx={() => menu(p, i)}>
      <button class="sillon-piste" class:cur class:off={!p.available} disabled={!p.available} onclick={() => jouer(i)} aria-current={cur ? 'true' : undefined}
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
    </li>
  {/each}
</ol>

<style>
  .sillon-pistes { display: flex; flex-direction: column; gap: 2px; margin: 0; padding: 0; list-style: none; }
  .sillon-disque { padding: 14px 10px 4px; color: var(--gris); font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase; }
  .sillon-piste { display: flex; align-items: center; gap: 14px; width: 100%; min-height: 52px; padding: 6px 10px; border: 0; border-radius: var(--r); background: transparent; color: inherit; font: inherit; text-align: left; cursor: pointer; transition: background var(--d) var(--sortie); }
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
