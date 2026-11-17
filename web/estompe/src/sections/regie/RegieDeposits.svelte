<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { notify } from '../../lib/ui'
  import RegieDeposit from './RegieDeposit.svelte'
  import { bytes, regieApi, type Deposit, type Deposits } from './regie'

  let data = $state<Deposits | null>(null)
  let failed = $state(false)
  let busy = $state(false)
  let opened = $state('')
  let picker = $state<HTMLInputElement>()

  async function load() {
    try { data = await regieApi.deposits(); failed = false } catch (e) { failed = true; notify.error(e, 'Les dépôts sont indisponibles') }
  }
  $effect(() => { void load() })

  async function create(e: Event & { currentTarget: HTMLInputElement }) {
    const fichier = e.currentTarget.files?.[0]
    e.currentTarget.value = ''
    if (!fichier) return
    busy = true
    try {
      const d = await regieApi.createDeposit(fichier)
      opened = d.id
      await load()
    } catch (err) { notify.error(err, 'Ce fichier .torrent n’a pas pu être déposé') } finally { busy = false }
  }

  function changed(d: Deposit | null, id: string) {
    if (!data) return
    data = { ...data, items: d ? data.items.map((x) => (x.id === id ? d : x)) : data.items.filter((x) => x.id !== id) }
    if (!d) { opened = ''; void load() }
  }
</script>

<div class="film-page">
  <div class="film-filters">
    <h2 class="v-titre film-filters-title">Dépôts</h2>
    {#if data}<span class="v-note">{bytes(data.used) || '0 o'} sur {bytes(data.limit)}</span>{/if}
    <input class="v-sr" type="file" accept=".torrent,application/x-bittorrent" aria-label="Fichier .torrent" bind:this={picker} onchange={create} />
    <button class="plbls-button plbls-button--compact" disabled={busy || !data} onclick={() => picker?.click()}><Icon name="plus" size={14} /> Déposer un .torrent</button>
  </div>
  <p class="v-note rg-aide">Dépose le .torrent donné par le tracker, puis ses fichiers : ils sont vérifiés, mis en partage dans qBittorrent, et peuvent rejoindre la bibliothèque.</p>

  {#if !data}
    {#if failed}<div class="v-empty"><Icon name="alerte" size={28} /><p class="v-note">Les dépôts n’ont pas pu être lus.</p></div>{:else}<Loader />{/if}
  {:else if !data.items.length}
    <div class="v-empty"><Icon name="envoie" size={28} /><p class="v-note">Aucun dépôt.</p></div>
  {:else}
    <ul class="rg-list">
      {#each data.items as d (d.id)}
        <li><RegieDeposit deposit={d} open={opened === d.id} ontoggle={() => (opened = opened === d.id ? '' : d.id)} onchange={(n) => changed(n, d.id)} /></li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .rg-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--espace-2); }
  .rg-aide { margin: 0; }
</style>
