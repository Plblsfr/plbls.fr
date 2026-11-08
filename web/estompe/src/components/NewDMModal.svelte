<script lang="ts">
  import { app } from '../lib/store.svelte'
  import { api, ApiError } from '../lib/api'
  import { router, hrefs } from '../lib/route.svelte'
  import { messageOf } from '../lib/ui'
  import Modal from './Modal.svelte'
  import Icon from './Icon.svelte'
  import Avatar from './Avatar.svelte'

  let { onclose }: { onclose: () => void } = $props()

  let pseudo = $state('')
  let error = $state('')
  let busy = $state(false)

  // Pseudos déjà croisés (membres de tes serveurs, conversations) pour l'autocomplétion.
  const known = $derived.by(() => {
    const out = new Map<string, { id: string; pseudo: string; ou: string }>()
    for (const d of app.dms) out.set(d.other.pseudo, { id: d.other.id, pseudo: d.other.pseudo, ou: 'conversation existante' })
    for (const det of Object.values(app.details)) for (const m of det.members) if (!out.has(m.pseudo)) out.set(m.pseudo, { id: m.id, pseudo: m.pseudo, ou: det.server.name })
    out.delete(app.pseudo)
    return [...out.values()].sort((a, b) => a.pseudo.localeCompare(b.pseudo, 'fr', { sensitivity: 'base' }))
  })

  let ouvert = $state(false)
  let active = $state(-1)
  const proposes = $derived.by(() => {
    const q = pseudo.trim().toLowerCase()
    if (!q) return []
    return known.filter((k) => k.pseudo.toLowerCase().includes(q) && k.pseudo.toLowerCase() !== q).slice(0, 6)
  })
  const liste = $derived(ouvert && proposes.length > 0)

  function choisir(p: string) {
    pseudo = p
    ouvert = false
    active = -1
    document.getElementById('dm-pseudo')?.focus()
  }

  function onkeydown(e: KeyboardEvent) {
    if (!liste) return
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      const n = proposes.length
      active = e.key === 'ArrowDown' ? (active + 1) % n : active <= 0 ? n - 1 : active - 1
    } else if (e.key === 'Enter' && active >= 0) {
      e.preventDefault()
      choisir(proposes[active].pseudo)
    } else if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      ouvert = false
      active = -1
    }
  }

  async function submit(e: Event) {
    e.preventDefault()
    busy = true
    error = ''
    try {
      const dm = await api.openDM(pseudo.trim())
      if (dm.online) app.online[dm.other.id] = true
      if (!app.dms.some((d) => d.id === dm.id)) app.dms.unshift(dm)
      onclose()
      router.go(hrefs.dm(dm.id))
    } catch (err) {
      error = err instanceof ApiError && err.code === 'user_not_found' ? 'Aucun utilisateur avec ce pseudo.'
        : err instanceof ApiError && err.code === 'self_dm' ? 'Tu ne peux pas t\'écrire à toi-même.'
        : messageOf(err)
    } finally {
      busy = false
    }
  }
</script>

<Modal title="Nouveau message privé" {onclose}>
  <form class="ndm" onsubmit={submit}>
    {#if error}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>{/if}
    <div class="plbls-field">
      <label class="plbls-field__label" for="dm-pseudo">Pseudo</label>
      <div class="ndm-champ">
        <span class="ndm-arobase" aria-hidden="true"><Icon name="arobase" size={16} /></span>
        <input id="dm-pseudo" class="plbls-input" bind:value={pseudo} autocomplete="off" required role="combobox" aria-autocomplete="list" aria-expanded={liste} aria-controls="dm-known" aria-activedescendant={liste && active >= 0 ? `dm-known-${active}` : undefined} oninput={() => { ouvert = true; active = -1 }} onblur={() => setTimeout(() => (ouvert = false), 150)} {onkeydown} />
      </div>
    </div>
    {#if liste}
      <ul class="v-menu ndm-liste" id="dm-known" role="listbox" aria-label="Pseudos connus">
        {#each proposes as k, i (k.pseudo)}
          <li class="v-mi ndm-option" class:on={i === active} id="dm-known-{i}" role="option" tabindex="-1" aria-selected={i === active} onpointerdown={(e) => { e.preventDefault(); choisir(k.pseudo) }}>
            <Avatar pseudo={k.pseudo} userId={k.id} size="sm" status={app.presenceOf(k.id)} />
            <b class="v-ell">{k.pseudo}</b>
            <span class="v-mute v-ell ndm-ou">{k.ou}</span>
          </li>
        {/each}
      </ul>
    {/if}
    <div class="ndm-pied">
      <button type="button" class="plbls-button plbls-button--contour" onclick={onclose}>Annuler</button>
      <button class="plbls-button" class:is-busy={busy} disabled={busy || !pseudo.trim()}>Ouvrir la conversation</button>
    </div>
  </form>
</Modal>

<style>
  .ndm { display: flex; flex-direction: column; gap: 12px; }
  .ndm .plbls-alert, .ndm .plbls-field { margin: 0; }
  .ndm-champ { position: relative; }
  .ndm-champ .plbls-input { width: 100%; padding-left: 36px; }
  .ndm-arobase { position: absolute; left: 12px; top: 50%; display: inline-flex; color: var(--gris); transform: translateY(-50%); pointer-events: none; }
  .ndm-liste { margin: 0; list-style: none; min-width: 0; }
  .ndm-option { height: auto; min-height: 40px; }
  .ndm-ou { margin-left: auto; font-size: 12px; }
  .ndm-pied { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; margin-top: 4px; }
  @media (pointer: coarse) {
    .ndm-option, .ndm-pied .plbls-button { min-height: 44px; }
  }
</style>
