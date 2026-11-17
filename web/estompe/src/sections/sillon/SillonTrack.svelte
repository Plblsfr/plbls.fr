<script lang="ts">
  import { ApiError } from '../../lib/api'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import SillonAlbum from './SillonAlbum.svelte'
  import { echec } from './sillon.svelte'
  import { sillonApi, type Piste } from './sillon'

  let { id }: { id: string } = $props()
  let piste = $state<Piste | null>(null)
  let manque = $state(false)

  $effect(() => {
    let gone = false
    sillonApi.piste(id).then((p) => { if (!gone) piste = p }).catch((e) => { if (gone) return; manque = true; if (!(e instanceof ApiError && e.status === 404)) echec(e, 'Ce titre est indisponible') })
    return () => { gone = true }
  })
</script>

{#if piste}
  <SillonAlbum id={piste.album.id} focus={piste.id} />
{:else if manque}
  <div class="film-page"><div class="film-missing"><Icon name="alerte" /><span class="grow">Ce titre est indisponible.</span><a class="plbls-button plbls-button--compact plbls-button--contour" href={hrefs.musique()}>Retour à l’accueil</a></div></div>
{:else}
  <div class="film-page"><Loader /></div>
{/if}
