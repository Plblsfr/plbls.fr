<script lang="ts">
  import Loader from '../components/Loader.svelte'
  import { onMount } from 'svelte'
  import { api, ApiError } from '../lib/api'
  import { app } from '../lib/store.svelte'
  import { router, hrefs } from '../lib/route.svelte'
  import { messageOf } from '../lib/ui'

  let { code }: { code: string } = $props()
  let error = $state('')

  onMount(async () => {
    try {
      const s = await api.joinServer(code)
      await app.refreshServers()
      router.go(hrefs.channel(s.id))
    } catch (e) {
      error = e instanceof ApiError && e.status === 404 ? 'Cette invitation est invalide ou a été révoquée.' : messageOf(e)
    }
  })
</script>

<div class="center welcome">
  <div class="card">
    {#if error}
      <h1>Invitation</h1>
      <div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>
      <a class="plbls-button plbls-button--contour" href={hrefs.home()}>Retour à l'accueil</a>
    {:else}
      <Loader text="Tu rejoins le serveur…" />
      
    {/if}
  </div>
</div>
