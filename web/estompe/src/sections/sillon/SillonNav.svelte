<script lang="ts">
  import Icon, { type IconName } from '../../components/Icon.svelte'
  import { router, hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import { app } from '../../lib/store.svelte'
  import SillonMosaique from './SillonMosaique.svelte'
  import { collection, lecteur } from './sillon.svelte'
  import { QUALITES, type Qualite } from './sillon'

  const route = $derived(router.current)
  const section = $derived(route.name === 'musique' ? route.section : undefined)
  const actif = (id: string | undefined) => section === id || (id === 'artists' && section === 'artist')
  const droits = $derived(app.sillonMe?.permissions)
  const links = $derived<{ id: string | undefined; label: string; icon: IconName }[]>([
    { id: undefined, label: 'Accueil', icon: 'maison' },
    { id: 'artists', label: 'Artistes', icon: 'perso' },
    { id: 'search', label: 'Rechercher', icon: 'cherche' },
    { id: 'queue', label: 'File de lecture', icon: 'list' },
    { id: 'history', label: 'Historique', icon: 'horloge' },
    ...(droits?.add ? [{ id: 'add', label: 'Ajouter de la musique', icon: 'telecharge' as IconName }, { id: 'additions', label: 'Mes ajouts', icon: 'boite' as IconName }] : []),
    ...(droits?.admin ? [{ id: 'admin', label: 'Administration', icon: 'bouclier' as IconName }] : []),
  ])
  const playlistActive = (id: string) => section === 'playlist' && route.name === 'musique' && route.id === id
  const creer = () => void collection.creer().then((p) => { if (p) router.go(hrefs.musique('playlist', p.id)) })
  const changer = (q: Qualite) => void lecteur.changerQualite(q).then(() => notify.ok('Qualité de lecture enregistrée.')).catch((e) => notify.error(e, 'La qualité n’a pas pu être changée'))
</script>

<div class="side-head"><h2>Sillon</h2></div>
<div class="side-list">
  <p class="v-note sillon-sub">Musique</p>
  {#each links as l (l.label)}
    <a class="nav-item" class:active={actif(l.id)} href={hrefs.musique(l.id)} aria-current={actif(l.id) ? 'page' : undefined}>
      <Icon name={l.icon} size={18} />
      <span class="grow truncate">{l.label}</span>
    </a>
  {/each}
  <div class="sillon-nav-tete">
    <p class="v-note sillon-sub">Playlists</p>
    <button class="v-ib sm" onclick={creer} aria-label="Nouvelle playlist" title="Nouvelle playlist"><Icon name="plus" size={16} /></button>
  </div>
  <a class="nav-item" class:active={section === 'likes'} href={hrefs.musique('likes')} aria-current={section === 'likes' ? 'page' : undefined}>
    <span class="sillon-nav-art sillon-nav-likes"><Icon name="coeurplein" size={14} /></span>
    <span class="grow truncate">Titres likés</span>
  </a>
  {#each collection.playlists as p (p.id)}
    <a class="nav-item" class:active={playlistActive(p.id)} href={hrefs.musique('playlist', p.id)} aria-current={playlistActive(p.id) ? 'page' : undefined}>
      <span class="sillon-nav-art"><SillonMosaique albums={p.cover_albums} id={p.id} titre={p.name} size="small" /></span>
      <span class="grow truncate">{p.name}</span>
    </a>
  {/each}
  <label class="sillon-qualite">
    <span class="v-note">Qualité de lecture</span>
    <select class="plbls-select film-select" value={lecteur.qualite ?? 'originale'} onchange={(e) => changer(e.currentTarget.value as Qualite)}>
      {#each QUALITES as q (q.id)}<option value={q.id}>{q.label}</option>{/each}
    </select>
  </label>
</div>

<style>
  .sillon-sub { margin: 0 0 var(--espace-2) var(--espace-2); }
  .sillon-nav-tete { display: flex; align-items: center; justify-content: space-between; margin-top: var(--espace-4); }
  .sillon-nav-tete .sillon-sub { margin-bottom: 0; }
  .sillon-nav-art { position: relative; display: block; flex: none; width: 22px; height: 22px; overflow: hidden; border-radius: 4px; }
  .sillon-nav-art :global(.film-art-title) { display: none; }
  .sillon-nav-likes { display: grid; place-items: center; background: var(--rose); color: var(--rose-fonce); }
  .sillon-qualite { display: grid; gap: 6px; margin: var(--espace-4) var(--espace-2) 0; }
  .sillon-qualite select { width: 100%; }
</style>
