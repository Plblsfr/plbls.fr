<script lang="ts">
  import { onMount, tick } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { app } from '../../lib/store.svelte'
  import { ApiError } from '../../lib/api'
  import { ecransDuRecap, titreRecap, type Ecran } from './recap'
  import type { Recap } from './sablier'
  import SablierMedaille from './SablierMedaille.svelte'
  import { sablier } from './sablier.svelte'
  import type { NomSalon } from './stats'

  let { id, onclose }: { id: number; onclose: () => void } = $props()
  const BALAYAGE = 48
  let recap = $state.raw<Recap | null>(null)
  let erreur = $state('')
  let i = $state(0)
  let racine = $state<HTMLElement>()
  let depart: { x: number; y: number } | null = null
  let balaye = false

  const nomSalon: NomSalon = (serveur, salon) => app.details[serveur]?.channels.find((c) => c.id === salon)?.name ?? null
  const ecrans = $derived<Ecran[]>(recap ? ecransDuRecap(recap, nomSalon) : [])
  const ecran = $derived(ecrans[i])
  const dernier = $derived(i === ecrans.length - 1)

  async function charger() {
    erreur = ''
    try { recap = await sablier.lireRecap(id) } catch (e) { erreur = e instanceof ApiError && e.status === 404 ? 'Ce récap n’existe pas ou n’est pas le tien.' : 'Les stats sont injoignables pour le moment.' }
  }
  onMount(() => {
    const avant = document.activeElement instanceof HTMLElement ? document.activeElement : null
    void charger()
    void tick().then(() => racine?.focus())
    return () => avant?.focus()
  })

  const avancer = () => { if (i < ecrans.length - 1) i++ }
  const reculer = () => { if (i > 0) i-- }

  function touche(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); onclose(); return }
    if (e.key === 'ArrowRight' || e.key === 'PageDown') { e.preventDefault(); avancer(); return }
    if (e.key === 'ArrowLeft' || e.key === 'PageUp') { e.preventDefault(); reculer(); return }
    if (e.key === 'Home') { e.preventDefault(); i = 0; return }
    if (e.key === 'End') { e.preventDefault(); i = Math.max(0, ecrans.length - 1); return }
    if (e.key !== 'Tab' || !racine) return
    const f = [...racine.querySelectorAll<HTMLElement>('button, a[href]')].filter((el) => !el.hasAttribute('disabled'))
    if (!f.length) return
    const rang = f.indexOf(document.activeElement as HTMLElement)
    const cible = e.shiftKey ? (rang <= 0 ? f.length - 1 : rang - 1) : rang === f.length - 1 ? 0 : rang + 1
    e.preventDefault()
    f[cible].focus()
  }

  function appui(e: PointerEvent) { depart = { x: e.clientX, y: e.clientY }; balaye = false }
  function relache(e: PointerEvent) {
    if (!depart) return
    const dx = e.clientX - depart.x
    const dy = e.clientY - depart.y
    depart = null
    if (Math.abs(dx) < BALAYAGE || Math.abs(dx) < Math.abs(dy)) return
    balaye = true
    if (dx < 0) avancer()
    else reculer()
  }
  const zone = (action: () => void) => () => { if (balaye) { balaye = false; return } action() }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="sab-story" bind:this={racine} role="dialog" aria-modal="true" aria-label={recap ? titreRecap(recap) : 'Récap'} tabindex="-1" onkeydown={touche} onpointerdown={appui} onpointerup={relache} onpointercancel={() => (depart = null)}>
  <div class="sab-story-seg" role="img" aria-label={ecrans.length ? `Écran ${i + 1} sur ${ecrans.length}` : 'Chargement'}>
    {#each ecrans as e, n (e.cle)}<i class:on={n <= i}></i>{/each}
  </div>
  <div class="sab-story-tete">
    <p>{recap ? titreRecap(recap) : 'Récap'}</p>
    <button class="v-ib" type="button" aria-label="Fermer le récap" onclick={onclose}><Icon name="x" size={26} /></button>
  </div>
  {#if erreur}
    <div class="sab-story-corps">
      <p class="sab-story-t">Ce récap ne s’ouvre pas</p>
      <p class="sab-story-p" role="alert">{erreur}</p>
      <div class="sab-ligne sab-story-actions">
        <button class="plbls-button" type="button" onclick={() => void charger()}>Réessayer</button>
        <button class="plbls-button plbls-button--contour" type="button" onclick={onclose}>Retour aux récaps</button>
      </div>
    </div>
  {:else if !ecran}
    <div class="sab-story-corps"><Loader text="Ouverture du récap…" /></div>
  {:else}
    <button class="sab-story-nav sab-story-nav--g" type="button" aria-label="Écran précédent" aria-disabled={i === 0} onclick={zone(reculer)}></button>
    <button class="sab-story-nav sab-story-nav--d" type="button" aria-label="Écran suivant" aria-disabled={dernier} onclick={zone(avancer)}></button>
    {#key ecran.cle}
      <div class="sab-story-corps" aria-live="polite" data-ecran={ecran.cle}>
        <p class="sab-story-p">{ecran.surtitre}</p>
        <p class="sab-story-xl">{#each ecran.grand as m, n (n)}{#if m.fort}<span class="sab-hl sab-hl--pose">{m.texte}</span>{:else}{m.texte}{/if}{/each}</p>
        {#if ecran.titre.length}<p class="sab-story-t">{#each ecran.titre as m, n (n)}{#if m.fort}<span class="sab-hl">{m.texte}</span>{:else}{m.texte}{/if}{/each}</p>{/if}
        {#if ecran.texte}<p class="sab-story-p">{ecran.texte}</p>{/if}
        {#if ecran.trophees.length}
          <ul class="sab-ligne" aria-label="Trophées débloqués" style:list-style="none" style:margin="0" style:padding="0">
            {#each ecran.trophees as t, n (n)}<li title={t.name}><SablierMedaille icone={t.icon} pose /><span class="v-sr">{t.name}</span></li>{/each}
            {#if ecran.reste > 0}<li><span class="sab-etiquette">+ {ecran.reste}</span></li>{/if}
          </ul>
        {/if}
        {#if dernier}
          <div class="sab-ligne sab-story-actions">
            <button class="plbls-button" type="button" onclick={onclose}>Terminer</button>
            {#if ecrans.length > 1}<button class="plbls-button plbls-button--contour" type="button" onclick={() => (i = 0)}>Revoir</button>{/if}
          </div>
        {/if}
      </div>
    {/key}
  {/if}
</div>
