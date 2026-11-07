<script lang="ts">
  import Loader from './components/Loader.svelte'
  import Icon from './components/Icon.svelte'
  import { loadDrafts } from './lib/drafts.svelte'
  import { app } from './lib/store.svelte'
  import { prefs } from './lib/prefs.svelte'
  import { hrefs } from './lib/route.svelte'
  import { platform } from './lib/platform'
  import LoginView from './views/LoginView.svelte'
  import ActivateView from './views/ActivateView.svelte'
  import DeniedView from './views/DeniedView.svelte'
  import Shell from './views/Shell.svelte'
  import ForcedUpdate from './components/ForcedUpdate.svelte'
  import MediaPicker from './components/MediaPicker.svelte'
  import ContextMenu from './components/ContextMenu.svelte'
  import ConfirmModal from './components/ConfirmModal.svelte'

  void app.boot()
  void loadDrafts()
  platform.notifications.listenActions?.((conv) => app.readConv(conv))

  $effect(() => {
    document.documentElement.dataset.density = prefs.density
  })
  // Thème de plbls-ui : « auto » suit le système ; la couleur de la barre du navigateur suit le thème affiché.
  $effect(() => {
    const root = document.documentElement
    root.setAttribute('data-theme', prefs.theme)
    document.querySelector('meta[name="theme-color"]')?.setAttribute('content', getComputedStyle(root).getPropertyValue('--papier').trim())
  })
  $effect(() => {
    const n = app.totalMentions
    document.title = (n ? `(${n}) ` : '') + 'Estompe'
    // Badge de l'icône (PWA installée, app de bureau) ; absent de certains navigateurs.
    void (n ? navigator.setAppBadge?.(n) : navigator.clearAppBadge?.())?.catch(() => {})
    void platform.notifications.setBadge?.(n)
  })
</script>

{#if app.phase === 'loading'}
  <div class="acces">
    <div class="acces-attente">
      <span class="acces-logo grand" aria-hidden="true">E</span>
      <Loader size="lg" text="Ouverture d'Estompe…" />
    </div>
  </div>
{:else if app.phase === 'login'}
  <LoginView />
{:else if app.phase === 'denied'}
  <DeniedView />
{:else if app.phase === 'activate'}
  <ActivateView />
{:else if app.phase === 'error'}
  <div class="acces">
    <div class="acces-carte">
      <span class="acces-pastille dgr" aria-hidden="true"><Icon name="alerte" size={22} /></span>
      <h1>Oups.</h1>
      <div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{app.error || 'Le serveur est injoignable.'}</p></div>
      <button class="plbls-button plbls-button--block" onclick={() => app.boot()}><Icon name="actualise" size={18} />Réessayer</button>
    </div>
  </div>
{:else}
  <Shell />
{/if}

<ForcedUpdate />
<MediaPicker />
<ContextMenu />
<ConfirmModal />
