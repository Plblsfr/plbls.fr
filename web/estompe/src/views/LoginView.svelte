<script lang="ts">
  import { loginUrl, registerUrl } from '../lib/auth'
  import { app } from '../lib/store.svelte'
  import { platform } from '../lib/platform'
  import { messageOf, notify } from '../lib/ui'

  const auth = platform.auth
  const Downloads = platform.slots.login
  let busy = $state(false)

  async function signIn(register: boolean) {
    busy = true
    try {
      await auth!.login(register)
      await app.boot()
      if (app.phase === 'login') notify.fail(`Connexion refusée par le serveur : ${await auth!.diagnose()}`)
    } catch (e) {
      // Tauri rejette avec une simple chaîne : on l'affiche telle quelle plutôt que le message générique
      notify.fail(typeof e === 'string' && e ? e : e instanceof Error && e.message ? e.message : messageOf(e, 'Connexion impossible'))
    } finally {
      busy = false
    }
  }
</script>

<div class="center welcome">
  <div class="card">
    <h1>Estompe</h1>
    <p class="muted">Messagerie écrite et vocale pour un petit groupe d'amis.</p>
    {#if auth}
      <button class="plbls-button plbls-button--block" class:is-busy={busy} disabled={busy} onclick={() => signIn(false)}>Se connecter</button>
      <button class="plbls-button plbls-button--contour plbls-button--block" disabled={busy} onclick={() => signIn(true)}>Créer un compte</button>
      {#if busy}<p class="muted small">Termine la connexion dans ton navigateur, puis reviens ici.</p>{/if}
    {:else}
      <a class="plbls-button plbls-button--block" href={loginUrl()}>Se connecter</a>
      <a class="plbls-button plbls-button--contour plbls-button--block" href={registerUrl()}>Créer un compte</a>
    {/if}
    {#if Downloads}<Downloads />{/if}
    <p class="muted small">
      L'accès à Estompe est accordé par l'administrateur (permission.plbls.fr).
    </p>
  </div>
</div>
