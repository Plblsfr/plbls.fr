<script lang="ts">
  import Icon, { type IconName } from '../../components/Icon.svelte'
  import { router, hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import { lecteur } from './sillon.svelte'
  import { QUALITES, type Qualite } from './sillon'

  const route = $derived(router.current)
  const section = $derived(route.name === 'musique' ? route.section : undefined)
  const actif = (id: string | undefined) => section === id || (id === 'artists' && section === 'artist')
  const links: { id: string | undefined; label: string; icon: IconName }[] = [
    { id: undefined, label: 'Accueil', icon: 'maison' },
    { id: 'artists', label: 'Artistes', icon: 'perso' },
    { id: 'search', label: 'Rechercher', icon: 'cherche' },
  ]
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
  <label class="sillon-qualite">
    <span class="v-note">Qualité de lecture</span>
    <select class="plbls-select film-select" value={lecteur.qualite ?? 'originale'} onchange={(e) => changer(e.currentTarget.value as Qualite)}>
      {#each QUALITES as q (q.id)}<option value={q.id}>{q.label}</option>{/each}
    </select>
  </label>
</div>

<style>
  .sillon-sub { margin: 0 0 var(--espace-2) var(--espace-2); }
  .sillon-qualite { display: grid; gap: 6px; margin: var(--espace-4) var(--espace-2) 0; }
  .sillon-qualite select { width: 100%; }
</style>
