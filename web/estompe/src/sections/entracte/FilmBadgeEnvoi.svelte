<script lang="ts">
  import { api } from '../../lib/api'
  import { app } from '../../lib/store.svelte'
  import { notify } from '../../lib/ui'
  import Icon from '../../components/Icon.svelte'
  import { etiquetteBadge, lienBadge, type FilmBadge } from './badges'

  let { badge, onsent }: { badge: FilmBadge; onsent?: () => void } = $props()

  interface Cible { cle: string; nom: string; detail: string; envoyer: (contenu: string) => Promise<unknown> }

  let filtre = $state('')
  let envoi = $state('')

  $effect(() => {
    for (const s of app.servers) if (!app.details[s.id]) void app.loadServer(s.id).catch(() => {})
  })

  const cibles = $derived.by((): Cible[] => {
    const dms = app.dms.map((d) => ({ cle: `d:${d.id}`, nom: d.other.pseudo, detail: 'Message privé', envoyer: (c: string) => api.sendDMMessage(d.id, c) }))
    const salons = app.servers.flatMap((s) => (app.details[s.id]?.channels ?? [])
      .filter((c) => c.kind === 'text' && !c.locked && (!c.permissions || c.permissions.includes('send_messages')))
      .map((c) => ({ cle: `c:${c.id}`, nom: `# ${c.name}`, detail: s.name, envoyer: (contenu: string) => api.sendChannelMessage(c.id, contenu) })))
    const q = filtre.trim().toLowerCase()
    return [...dms, ...salons].filter((c) => !q || `${c.nom} ${c.detail}`.toLowerCase().includes(q))
  })

  const contenu = $derived(`${etiquetteBadge(badge)} : ${badge.title}\n${lienBadge(badge)}`)

  async function envoyer(c: Cible) {
    envoi = c.cle
    try {
      await c.envoyer(contenu)
      notify.ok(`Badge envoyé à ${c.nom}.`)
      onsent?.()
    } catch (e) {
      notify.error(e, 'Le badge n’a pas pu être envoyé')
    } finally {
      envoi = ''
    }
  }
</script>

<div class="film-envoi">
  <label class="film-envoi-filtre">
    <Icon name="cherche" size={16} />
    <span class="v-sr">Chercher une conversation</span>
    <input class="plbls-input" type="search" placeholder="Une personne, un salon…" maxlength="60" bind:value={filtre} />
  </label>
  {#if !cibles.length}
    <p class="v-note">{filtre ? 'Aucune conversation ne correspond.' : 'Aucune conversation pour l’instant.'}</p>
  {:else}
    <ul class="film-envoi-liste">
      {#each cibles as c (c.cle)}
        <li>
          <button type="button" class="film-envoi-cible" disabled={!!envoi} onclick={() => void envoyer(c)}>
            <span class="film-envoi-texte"><b class="truncate">{c.nom}</b><span class="v-note truncate">{c.detail}</span></span>
            <span class="v-note">{envoi === c.cle ? 'Envoi…' : 'Envoyer'}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .film-envoi { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  .film-envoi-filtre { display: flex; align-items: center; gap: 8px; }
  .film-envoi-filtre input { flex: 1; min-width: 0; }
  .film-envoi-liste { display: flex; flex-direction: column; gap: 4px; max-height: 260px; margin: 0; padding: 0; overflow-y: auto; list-style: none; }
  .film-envoi-cible { display: flex; align-items: center; gap: 10px; width: 100%; min-height: 44px; padding: 6px 10px; border: 0; border-radius: var(--r-m, 8px); background: none; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .film-envoi-cible:hover:not(:disabled) { background: var(--papier-2); }
  .film-envoi-cible:focus-visible { outline: none; box-shadow: var(--anneau); }
  .film-envoi-cible:disabled { cursor: progress; opacity: 0.6; }
  .film-envoi-texte { display: flex; flex-direction: column; flex: 1; min-width: 0; }
</style>
