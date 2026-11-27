<script lang="ts">
  import { filmsApi } from '../../lib/api'
  import { platform } from '../../lib/platform'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import Modal from '../../components/Modal.svelte'
  import { sablier } from '../sablier/sablier.svelte'
  import { enregistrer } from '../sablier/sablier'
  import { afficheBadge, CARTE, cleBadge, dessinerCarte, etiquetteBadge, nomFichierBadge, texteBadge, type FilmBadge } from './badges'

  let { badge, onclose }: { badge: FilmBadge; onclose: () => void } = $props()

  let image = $state<Blob | null>(null)
  let apercu = $state('')
  let echec = $state(false)
  let envoi = $state(false)

  const cle = $derived(cleBadge(badge))
  const avecProfil = $derived(!!app.sablierMe?.permissions.access)
  const partage = $derived(sablier.partage?.badges?.find((b) => b.key === cle) ?? null)

  async function chargerAffiche(): Promise<CanvasImageSource | null> {
    const a = afficheBadge(badge)
    if (!a.image) return null
    try {
      const url = await filmsApi.image(a.id, 'poster', 500)
      try { return await createImageBitmap(await (await fetch(url)).blob()) } finally { URL.revokeObjectURL(url) }
    } catch { return null }
  }

  async function fabriquer(): Promise<Blob> {
    const canvas = document.createElement('canvas')
    canvas.width = CARTE.largeur
    canvas.height = CARTE.hauteur
    const ctx = canvas.getContext('2d')
    if (!ctx) throw new Error('canvas')
    const police = getComputedStyle(document.body).fontFamily || 'system-ui, sans-serif'
    dessinerCarte(ctx, badge, await chargerAffiche(), police)
    return await new Promise<Blob>((ok, ko) => canvas.toBlob((b) => (b ? ok(b) : ko(new Error('image'))), 'image/png'))
  }

  $effect(() => {
    let actif = true
    let url = ''
    image = null
    echec = false
    fabriquer().then((b) => {
      if (!actif) return
      image = b
      url = URL.createObjectURL(b)
      apercu = url
    }, () => { if (actif) echec = true })
    return () => { actif = false; if (url) URL.revokeObjectURL(url) }
  })

  $effect(() => { if (avecProfil && !sablier.partage) void sablier.chargerPartage() })

  const telecharger = () => { if (image) enregistrer(image, nomFichierBadge(badge)) }
  async function partager() {
    if (!image) return
    if (!(await platform.shareImage?.(image, nomFichierBadge(badge), texteBadge(badge)))) telecharger()
  }
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
    {#if apercu}
      <img class="film-badge-apercu" src={apercu} alt={texteBadge(badge)} width={CARTE.largeur} height={CARTE.hauteur} />
    {:else if echec}
      <p class="v-note">L’image n’a pas pu être préparée.</p>
    {:else}
      <Loader text="Préparation de l’image…" />
    {/if}
    <div class="film-badge-actions">
      {#if platform.shareImage}<button class="plbls-button" type="button" disabled={!image} onclick={() => void partager()}><Icon name="envoie" size={16} /> Partager</button>{/if}
      <button class="plbls-button plbls-button--contour" type="button" disabled={!image} onclick={telecharger}><Icon name="telecharge" size={16} /> Enregistrer l’image</button>
    </div>
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
  .film-badge-apercu { width: 100%; max-width: 360px; height: auto; margin: 0 auto; border-radius: var(--r-l); box-shadow: var(--o2, var(--o1)); }
  .film-badge-actions { display: flex; flex-wrap: wrap; gap: 8px; justify-content: center; }
</style>
