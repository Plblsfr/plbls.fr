<script lang="ts">
  import { app } from '../lib/store.svelte'
  import { logout } from '../lib/auth'
  import { ApiError } from '../lib/api'
  import { messageOf } from '../lib/ui'

  let pseudo = $state('')
  let error = $state('')
  let field = $state(false)
  let busy = $state(false)

  async function submit(e: Event) {
    e.preventDefault()
    error = ''
    field = false
    busy = true
    try {
      await app.activate(pseudo)
    } catch (err) {
      if (err instanceof ApiError && err.code === 'already_activated') return void app.boot()
      if (err instanceof ApiError && err.code === 'forbidden') return void app.boot()
      field = err instanceof ApiError && err.code.includes('pseudo')
      error = messageOf(err)
    } finally {
      busy = false
    }
  }
</script>

<div class="center welcome">
  <form class="card" onsubmit={submit}>
    <h1>Bienvenue</h1>
    <p class="muted">Tu es connecté. Pour activer ton compte Estompe, choisis un pseudo.</p>
    {#if error}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>{/if}
    <div class="plbls-field">
      <label class="plbls-field__label" for="pseudo">Pseudo</label>
      <input id="pseudo" class="plbls-input" class:plbls-input--error={field} bind:value={pseudo} minlength="3" maxlength="20" pattern="[A-Za-z0-9_.\-]+" autocomplete="nickname" required />
      <span class="plbls-field__aide">3 à 20 caractères : lettres, chiffres, _ . -</span>
    </div>
    <button class="plbls-button plbls-button--block" class:is-busy={busy} disabled={busy}>Activer mon compte</button>
    <button type="button" class="plbls-button plbls-button--lien" onclick={() => logout()}>Se déconnecter</button>
  </form>
</div>
