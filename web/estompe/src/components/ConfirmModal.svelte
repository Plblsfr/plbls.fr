<script lang="ts">
  import { tick } from 'svelte'
  import Icon from './Icon.svelte'
  import { confirmation, peutValider } from '../lib/confirm.svelte'

  const d = $derived(confirmation.en)
  let valeur = $state('')
  let coche = $state(false)
  let boite = $state<HTMLElement>()
  let precedent: Element | null = null

  $effect(() => {
    if (!d) return
    valeur = d.saisie?.valeur ?? ''
    coche = false
    precedent = document.activeElement
    void tick().then(() => {
      const cible = d.destructif ? boite?.querySelector<HTMLElement>('[data-annuler]') : (boite?.querySelector<HTMLElement>('input') ?? boite?.querySelector<HTMLElement>('[data-ok]'))
      cible?.focus()
    })
    return () => (precedent as HTMLElement | null)?.focus?.()
  })

  const valide = $derived(d ? peutValider(d, valeur, coche) : false)
  const fermer = (ok: boolean) => d?.resoudre({ ok, valeur })
  const valider = () => valide && fermer(true)

  function key(e: KeyboardEvent) {
    if (!d) return
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      return fermer(false)
    }
    if (e.key === 'Enter' && !d.destructif && !(e.target instanceof HTMLButtonElement)) {
      e.preventDefault()
      return valider()
    }
    if (e.key !== 'Tab') return
    const f = [...boite!.querySelectorAll<HTMLElement>('input:not([disabled]), button:not([disabled])')]
    const i = f.indexOf(document.activeElement as HTMLElement)
    const suivant = e.shiftKey ? (i <= 0 ? f.length - 1 : i - 1) : i === f.length - 1 ? 0 : i + 1
    e.preventDefault()
    f[suivant].focus()
  }
</script>

<svelte:window onkeydowncapture={key} />

{#if d}
  <div class="confirm-back" role="presentation" onpointerdown={(e) => { if (e.target === e.currentTarget) fermer(false) }}>
    <div class="confirm" class:destructif={d.destructif} bind:this={boite} role={d.destructif ? 'alertdialog' : 'dialog'} aria-modal="true" aria-labelledby="confirm-titre" aria-describedby={d.message ? 'confirm-msg' : undefined}>
      <div class="confirm-head">
        <span class="confirm-icon" aria-hidden="true"><Icon name={d.destructif ? 'alerte' : 'info'} size={22} /></span>
        <div class="confirm-text">
          <h3 id="confirm-titre">{d.titre}</h3>
          {#if d.message}<p id="confirm-msg">{d.message}</p>{/if}
        </div>
      </div>
      {#if d.saisie || d.case}
        <div class="confirm-body">
          {#if d.saisie}
            <div class="plbls-field">
              <label class="plbls-field__label" for="confirm-saisie">{d.saisie.label}</label>
              <input id="confirm-saisie" class="plbls-input" bind:value={valeur} autocomplete="off" />
            </div>
          {/if}
          {#if d.case}<label class="confirm-case"><input type="checkbox" bind:checked={coche} /><span>{d.case}</span></label>{/if}
        </div>
      {/if}
      <div class="confirm-actions">
        <button class="plbls-button plbls-button--contour" data-annuler onclick={() => fermer(false)}>Annuler</button>
        <button class="plbls-button" class:plbls-button--danger={d.destructif} data-ok disabled={!valide} onclick={valider}>{d.action ?? 'Confirmer'}</button>
      </div>
    </div>
  </div>
{/if}
