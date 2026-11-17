<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import SillonCover from './SillonCover.svelte'
  import { echec } from './sillon.svelte'
  import { ETATS_AJOUT, octets, sillonApi, type Ajout } from './sillon'

  let { admin = false, relancer }: { admin?: boolean; relancer?: (a: Ajout) => void } = $props()
  let ajouts = $state<Ajout[]>([])
  let loading = $state(true)
  const date = (s: string) => new Date(s).toLocaleDateString('fr-FR', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })
  const ton = (a: Ajout) => (a.state === 'available' ? 'ok' : a.state === 'failed' ? 'dgr' : a.state === 'deleted' ? '' : 'att')

  export async function charger() {
    loading = true
    try { ajouts = (await (admin ? sillonApi.ajoutsAdmin() : sillonApi.mesAjouts())).items } catch (e) { echec(e, 'Les ajouts sont indisponibles') } finally { loading = false }
  }

  $effect(() => { void charger() })
</script>

<div class="sillon-ajouts-tete">
  <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={loading} onclick={() => void charger()}><Icon name="actualise" size={14} /> Actualiser</button>
</div>
{#if loading && !ajouts.length}
  <Loader />
{:else if !ajouts.length}
  <div class="v-empty"><Icon name="telecharge" size={28} /><p class="v-note">Aucun ajout pour l’instant.</p></div>
{:else}
  <ul class="sillon-ajouts">
    {#each ajouts as a (a.id)}
      <li class="sillon-ajout">
        <span class="sillon-ajout-art" class:sillon-rond={a.kind === 'artist'}><SillonCover id={a.albums[0] ?? a.mbid} title={a.title} image={!!a.albums[0]} size="small" /></span>
        <span class="grow sillon-ajout-t">
          <strong class="truncate">{#if a.albums[0] && a.state === 'available'}<a href={hrefs.musique('album', a.albums[0])}>{a.title}</a>{:else}{a.title}{/if}</strong>
          <span class="v-note truncate">{[a.kind === 'artist' ? 'Artiste' : a.artist.name, date(a.created_at), a.estimated_size ? `≈ ${octets(a.estimated_size)}` : '', a.origin === 'discovery' ? 'Découverte' : '', a.author?.name].filter(Boolean).join(' · ')}</span>
          {#if a.reason}<span class="v-note sillon-raison">{a.reason}</span>{/if}
        </span>
        <span class="v-tag {ton(a)}">{ETATS_AJOUT[a.state]}</span>
        {#if relancer && a.state === 'failed'}<button class="v-btn sm sec" onclick={() => relancer(a)}>Relancer</button>{/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .sillon-ajouts-tete { display: flex; justify-content: flex-end; margin-bottom: 8px; }
  .sillon-ajouts { display: flex; flex-direction: column; gap: 2px; margin: 0; padding: 0; list-style: none; }
  .sillon-ajout { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 14px; min-height: 56px; padding: 6px 10px; border-radius: var(--r); }
  .sillon-ajout-art { position: relative; flex: none; width: 44px; height: 44px; overflow: hidden; border-radius: var(--r-s); }
  .sillon-ajout-art.sillon-rond { border-radius: 50%; }
  .sillon-ajout-t { display: grid; flex: 1 1 0; min-width: 0; line-height: 1.3; }
  .sillon-ajout-t a { color: inherit; }
  .sillon-raison { color: var(--danger); }
</style>
