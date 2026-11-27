<script lang="ts">
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import Modal from '../../components/Modal.svelte'
  import { sablier } from '../sablier/sablier.svelte'
  import FilmBadgeAnimee from './FilmBadgeAnimee.svelte'
  import FilmBadgeEnvoi from './FilmBadgeEnvoi.svelte'
  import { cleBadge, etiquetteBadge, type FilmBadge } from './badges'

  let { badge, onclose }: { badge: FilmBadge; onclose: () => void } = $props()

  let envoi = $state(false)

  const cle = $derived(cleBadge(badge))
  const messagerie = $derived(app.phase === 'ready')
  const avecProfil = $derived(!!app.sablierMe?.permissions.access)
  const partage = $derived(sablier.partage?.badges?.find((b) => b.key === cle) ?? null)

  $effect(() => { if (avecProfil && !sablier.partage) void sablier.chargerPartage() })

  async function surProfil(champ: HTMLInputElement) {
    envoi = true
    try {
      await sablier.partager({ badges: { [cle]: champ.checked } })
    } catch (e) {
      champ.checked = !champ.checked
      notify.error(e, 'Le réglage n’a pas pu être enregistré')
    } finally {
      envoi = false
    }
  }
</script>

<Modal title={etiquetteBadge(badge)} {onclose}>
  <div class="film-badge-carte">
    <FilmBadgeAnimee {badge} />
    {#if messagerie}
      <section class="film-badge-section" aria-label="Envoyer le badge">
        <h3 class="v-titre film-badge-h">Envoyer dans une conversation</h3>
        <FilmBadgeEnvoi {badge} onsent={onclose} />
      </section>
    {/if}
    {#if avecProfil && partage}
      <label class="plbls-toggle film-badge-profil">
        <span class="plbls-toggle__texte">Afficher sur mon profil</span>
        <input class="plbls-toggle__input" type="checkbox" role="switch" checked={partage.shared} disabled={envoi} onchange={(e) => void surProfil(e.currentTarget)} />
        <span class="plbls-toggle__piste"></span>
      </label>
      <p class="v-note">Visible seulement par les personnes qui peuvent ouvrir ton profil et qui ont accès à Entracte.</p>
    {/if}
  </div>
</Modal>

<style>
  .film-badge-carte { display: flex; flex-direction: column; gap: 14px; align-items: stretch; }
  .film-badge-carte > :global(.film-badge-anim) { margin: 0 auto; }
  .film-badge-section { display: flex; flex-direction: column; gap: 8px; }
  .film-badge-h { margin: 0; font-size: 15px; }
</style>
