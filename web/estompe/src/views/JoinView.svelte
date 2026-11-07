<script lang="ts">
  import Icon from '../components/Icon.svelte'
  import Loader from '../components/Loader.svelte'
  import { onMount } from 'svelte'
  import { api, ApiError } from '../lib/api'
  import { app } from '../lib/store.svelte'
  import { router, hrefs } from '../lib/route.svelte'
  import { messageOf } from '../lib/ui'

  let { code }: { code: string } = $props()
  let error = $state('')
  let busy = $state(true)
  let autre = $state('')

  const codeDe = (saisie: string) => {
    const m = /([A-Za-z0-9]{5}-[A-Za-z0-9]{5})\s*$/.exec(saisie.trim())
    return (m ? m[1] : saisie.trim()).toUpperCase()
  }

  async function rejoindre(c: string) {
    busy = true
    error = ''
    try {
      const s = await api.joinServer(c)
      await app.refreshServers()
      router.go(hrefs.channel(s.id))
    } catch (e) {
      error = e instanceof ApiError && e.status === 404 ? 'Cette invitation est invalide ou a été révoquée.' : messageOf(e)
    } finally {
      busy = false
    }
  }

  onMount(() => void rejoindre(code))
</script>

<div class="acces">
  {#if error}
    <div class="acces-carte">
      <span class="acces-pastille" aria-hidden="true"><Icon name="lien" size={22} /></span>
      <h1>Invitation</h1>
      <div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>
      <p class="v-note">Demande un nouveau lien à la personne qui t'a invité·e, ou colle un lien ou un code ci-dessous.</p>
      <form class="autre" onsubmit={(e) => { e.preventDefault(); if (autre.trim()) void rejoindre(codeDe(autre)) }}>
        <input class="plbls-input v-mono" bind:value={autre} placeholder="ABCDE-23456" autocomplete="off" aria-label="Lien ou code d'invitation" required />
        <button class="plbls-button" class:is-busy={busy} disabled={busy || !autre.trim()}>Rejoindre</button>
      </form>
      <a class="plbls-button plbls-button--contour plbls-button--block" href={hrefs.home()}>Retour à l'accueil</a>
    </div>
  {:else}
    <div class="acces-attente">
      <span class="acces-logo grand" aria-hidden="true">E</span>
      <Loader text="Tu rejoins le serveur…" />
    </div>
  {/if}
</div>

<style>
  .autre { display: flex; gap: 8px; }
  .autre input { flex: 1; min-width: 0; font-size: 14px; }
</style>
