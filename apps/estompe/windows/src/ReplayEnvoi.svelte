<script lang="ts">
  import { onDestroy } from 'svelte'
  import Modal from '../../../../web/estompe/src/components/Modal.svelte'
  import { ApiError } from '../../../../web/estompe/src/lib/api'
  import { app } from '../../../../web/estompe/src/lib/store.svelte'
  import type { ConvKey } from '../../../../web/estompe/src/lib/types'
  import { messageOf, notify } from '../../../../web/estompe/src/lib/ui'
  import { decisionEnvoi, taille, type Clip } from './replay'
  import { envoyerClip, texteErreur, TropGros } from './replay.svelte'

  let { clip, onclose }: { clip: Clip; onclose: () => void } = $props()

  let filtre = $state('')
  let cible = $state<ConvKey>('')
  let leger = $state(false)
  let occupe = $state(false)
  let erreur = $state('')
  let monte = true
  onDestroy(() => (monte = false))

  const mode = $derived(leger ? 'leger' : decisionEnvoi(clip, app.maxVideo))
  const destinations = $derived.by(() => {
    const out: { cle: ConvKey; nom: string; ou: string }[] = []
    for (const s of app.servers) for (const c of app.details[s.id]?.channels ?? []) {
      if (c.kind === 'text' && app.canConv(`c:${c.id}`, 'send_messages') && app.canConv(`c:${c.id}`, 'attach_files')) out.push({ cle: `c:${c.id}`, nom: `# ${c.name}`, ou: s.name })
    }
    for (const d of app.dms) out.push({ cle: `d:${d.id}`, nom: d.other.pseudo, ou: 'Message privé' })
    const q = filtre.trim().toLowerCase()
    return q ? out.filter((d) => `${d.nom} ${d.ou}`.toLowerCase().includes(q)) : out
  })

  async function envoyer() {
    if (!cible || mode === 'refus' || occupe) return
    occupe = true
    erreur = ''
    try {
      await envoyerClip(clip, cible, mode)
      notify.ok(`Clip « ${clip.nom} » envoyé.`)
      if (monte) onclose()
    } catch (e) {
      if (e instanceof TropGros && mode !== 'leger') leger = true
      else if (monte) erreur = e instanceof TropGros ? 'Le clip réencodé dépasse encore la limite d’envoi.' : e instanceof ApiError ? messageOf(e, 'Envoi impossible') : texteErreur(e)
      else notify.fail(`Envoi de « ${clip.nom} » impossible.`)
    } finally { occupe = false }
  }
</script>

<Modal title="Envoyer dans un salon" onclose={onclose}>
  <div class="re">
    <p class="v-note re-clip"><strong class="truncate">{clip.nom}</strong> · {taille(clip.tailleOctets)}</p>
    {#if mode === 'refus'}
      <div class="plbls-alert plbls-alert--erreur" role="alert">
        <p class="plbls-alert__titre">Clip trop long pour être envoyé</p>
        <p class="plbls-alert__corps">Ce clip dépasse la limite d’envoi ({taille(app.maxVideo)}) et, même réencodé, il ne tiendrait pas dedans avec une qualité lisible. Enregistre un clip plus court.</p>
      </div>
    {:else}
      {#if mode === 'leger'}
        <div class="plbls-alert" role="status">
          <p class="plbls-alert__titre">Clip plus gros que la limite d’envoi ({taille(app.maxVideo)})</p>
          <p class="plbls-alert__corps">Il peut être réencodé plus léger, en H.264, pour tenir dans la limite : la qualité de la copie envoyée baisse, le clip d’origine n’est pas modifié.</p>
        </div>
      {:else if mode === 'h264'}
        <p class="v-note">{clip.codec === 'h264' ? 'Les pistes du jeu et du micro sont réunies en une seule avant l’envoi.' : 'Le clip est converti en H.264 avant l’envoi, pour être lisible sur le web et sur Android.'} Le clip d’origine n’est pas modifié.</p>
      {/if}
      <div class="plbls-field">
        <label class="plbls-field__label" for="re-filtre">Salon ou conversation</label>
        <input id="re-filtre" class="plbls-input" type="search" placeholder="Chercher…" bind:value={filtre} />
      </div>
      <div class="re-liste" role="radiogroup" aria-label="Destination">
        {#each destinations as d (d.cle)}
          <button class="re-dest" role="radio" aria-checked={cible === d.cle} onclick={() => (cible = d.cle)}><span class="truncate">{d.nom}</span><span class="v-note truncate">{d.ou}</span></button>
        {:else}
          <p class="v-note">Aucun salon où tu peux joindre un fichier.</p>
        {/each}
      </div>
      {#if erreur}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{erreur}</p></div>{/if}
      {#if occupe}<p class="v-note" role="status">{mode === 'direct' ? 'Envoi en cours…' : 'Conversion puis envoi en cours… Tu peux fermer cette fenêtre : l’envoi continue.'}</p>{/if}
    {/if}
    <div class="re-actions">
      <button class="plbls-button plbls-button--contour" onclick={onclose}>{mode === 'refus' || occupe ? 'Fermer' : 'Annuler'}</button>
      {#if mode !== 'refus'}<button class="plbls-button" disabled={!cible || occupe} onclick={envoyer}>{mode === 'leger' ? 'Réencoder et envoyer' : 'Envoyer'}</button>{/if}
    </div>
  </div>
</Modal>

<style>
  .re { display: flex; flex-direction: column; gap: 12px; min-width: 0; }
  .re-clip { display: flex; gap: 6px; margin: 0; }
  .re .plbls-alert, .re .plbls-field, .re p { margin: 0; }
  .re-liste { display: flex; flex-direction: column; gap: 2px; max-height: 260px; overflow-y: auto; scrollbar-width: thin; }
  .re-dest { display: flex; align-items: baseline; gap: 10px; min-height: 36px; padding: 4px 10px; border: 0; border-radius: var(--r-s); background: transparent; color: var(--encre); font: inherit; text-align: left; cursor: pointer; }
  .re-dest:hover { background: var(--papier-2); }
  .re-dest[aria-checked='true'] { background: var(--rose); }
  .re-dest:focus-visible { outline: none; box-shadow: var(--anneau-in); }
  .re-actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
