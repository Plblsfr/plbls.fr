<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { ApiError } from '../../lib/api'
  import { confirmer } from '../../lib/confirm.svelte'
  import { notify } from '../../lib/ui'
  import RegieImport from './RegieImport.svelte'
  import { envoyerFichier } from './regie.svelte'
  import { bytes, depositComplete, depositState, fileName, invalidFiles, matchFiles, percent, regieApi, serviceName, type Deposit, type DepositFile } from './regie'

  let { deposit, open, ontoggle, onchange }: { deposit: Deposit; open: boolean; ontoggle: () => void; onchange: (d: Deposit | null) => void } = $props()

  let sending = $state(false)
  let stop = false
  let progress = $state<Record<number, number>>({})
  let busy = $state(false)
  let importing = $state(false)
  let picker = $state<HTMLInputElement>()
  let folder = $state<HTMLInputElement>()
  let replacer = $state<HTMLInputElement>()
  let replaced = 0

  const etat = $derived(depositState(deposit))
  const complete = $derived(depositComplete(deposit))
  const invalid = $derived(invalidFiles(deposit))
  const got = (f: DepositFile) => (sending && progress[f.index] !== undefined ? progress[f.index] : f.received)
  const total = $derived(deposit.files.reduce((n, f) => n + Math.min(got(f), f.size), 0))
  const accepts = $derived(deposit.state === 'upload' || deposit.state === 'mismatch')

  async function refresh() {
    try { onchange(await regieApi.deposit(deposit.id)) } catch (e) { if (e instanceof ApiError && e.status === 404) onchange(null) }
  }

  async function send(files: File[], replace?: number) {
    const locaux = files.map((f) => ({ name: f.name, size: f.size, path: f.webkitRelativePath || f.name }))
    const cibles = replace !== undefined ? deposit.files.filter((f) => f.index === replace) : deposit.files.filter((f) => f.received < f.size)
    const liens = replace !== undefined && files.length === 1 && files[0].size === cibles[0]?.size ? new Map([[replace, 0]]) : matchFiles(cibles, locaux)
    if (!liens.size) { notify.fail(replace !== undefined ? 'Ce fichier n’a pas la taille annoncée par le torrent.' : 'Aucun de ces fichiers n’a la taille d’un fichier attendu par le torrent.'); return }
    sending = true
    stop = false
    progress = {}
    try {
      for (const cible of cibles) {
        const i = liens.get(cible.index)
        if (i === undefined) continue
        await envoyerFichier(files[i], cible.received, (offset, morceau, reset) => regieApi.sendChunk(deposit.id, cible.index, offset, morceau, reset), (recu) => (progress = { ...progress, [cible.index]: recu }), { remplacer: replace !== undefined, arret: () => stop })
      }
      const restants = cibles.length - liens.size
      if (restants > 0) notify.info(`${restants} fichier${restants > 1 ? 's' : ''} attendu${restants > 1 ? 's' : ''} sans correspondance parmi ceux choisis.`)
    } catch (e) {
      if (!(e instanceof ApiError && e.code === 'cancelled')) notify.error(e, 'L’envoi s’est interrompu, il reprendra où il s’est arrêté')
    } finally {
      await refresh()
      sending = false
    }
  }

  function picked(e: Event & { currentTarget: HTMLInputElement }, replace?: number) {
    const files = [...(e.currentTarget.files ?? [])]
    e.currentTarget.value = ''
    if (files.length) void send(files, replace)
  }

  async function watch() {
    let courant = deposit
    while (courant.state === 'checking') {
      try { courant = await regieApi.deposit(deposit.id, 'checking') } catch (e) {
        if (e instanceof ApiError && e.code === 'timeout') continue
        notify.error(e, 'Le suivi de la vérification s’est interrompu')
        return
      }
      onchange(courant)
    }
    if (courant.state === 'seeding') notify.ok(`« ${courant.name} » est vérifié et en partage.`)
    else if (courant.state === 'mismatch') notify.fail(`« ${courant.name} » : des fichiers ne correspondent pas au torrent.`)
  }
  let watching = false
  $effect(() => {
    if (deposit.state === 'checking' && !watching) { watching = true; void watch().finally(() => (watching = false)) }
  })

  async function seed() {
    busy = true
    try { onchange(await regieApi.seed(deposit.id)) } catch (e) { notify.error(e, 'La vérification n’a pas pu être lancée') } finally { busy = false }
  }

  async function remove() {
    const ok = await confirmer({
      titre: `Supprimer le dépôt « ${deposit.name} » ?`,
      message: deposit.state === 'seeding' ? 'Le torrent est retiré de qBittorrent et les fichiers du dépôt sont effacés. Ceux déjà importés dans la bibliothèque y restent.' : 'Les fichiers déjà envoyés sont effacés.',
      action: 'Supprimer',
      destructif: true,
    })
    if (!ok) return
    busy = true
    stop = true
    try { await regieApi.removeDeposit(deposit.id); onchange(null) } catch (e) { notify.error(e, 'Le dépôt n’a pas pu être supprimé') } finally { busy = false }
  }
</script>

<article class="rg-depot">
  <button class="rg-tete" onclick={ontoggle} aria-expanded={open}>
    <span class="grow rg-body">
      <strong class="rg-wrap">{deposit.name}</strong>
      <span class="v-note">{bytes(deposit.size) || '0 o'} · {deposit.files.length} fichier{deposit.files.length > 1 ? 's' : ''}{deposit.tracker ? ` · ${deposit.tracker}` : ''}{deposit.imported.length ? ` · importé dans ${deposit.imported.map(serviceName).join(', ')}` : ''}</span>
      {#if deposit.state === 'upload' && deposit.size > 0}
        <span class="rg-barre" role="progressbar" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.floor((total / deposit.size) * 100)} aria-label="Fichiers reçus"><span style:width="{Math.min(total / deposit.size, 1) * 100}%"></span></span>
      {/if}
    </span>
    <span class="v-tag {etat.tone}">{etat.label}</span>
    <Icon name={open ? 'up' : 'bas'} size={16} />
  </button>

  {#if open}
    <div class="rg-corps">
      {#if deposit.error}<p class="v-note rg-err" role="alert"><Icon name="alerte" size={14} /> {deposit.error}</p>{/if}
      <ul class="rg-fichiers">
        {#each deposit.files as f (f.index)}
          {@const recu = Math.min(got(f), f.size)}
          <li>
            <span class="grow rg-body">
              <span class="rg-wrap">{f.path}</span>
              <span class="v-note" class:rg-err={f.valid !== null && f.valid < 1}>
                {bytes(f.size) || '0 o'}
                {#if f.valid !== null} · {f.valid >= 1 ? 'conforme' : `${percent(f.valid)} conforme`}{:else if recu < f.size} · {f.size ? percent(recu / f.size) : '0 %'} reçu{:else if deposit.state === 'upload'} · reçu{/if}
              </span>
            </span>
            {#if deposit.state === 'mismatch' && f.valid !== null && f.valid < 1}
              <button class="plbls-button plbls-button--compact plbls-button--contour" disabled={sending || busy} onclick={() => { replaced = f.index; replacer?.click() }} aria-label="Remplacer {fileName(f.path)}">Remplacer</button>
            {/if}
          </li>
        {/each}
      </ul>

      <input class="v-sr" type="file" multiple aria-label="Fichiers du dépôt" bind:this={picker} onchange={(e) => picked(e)} />
      <input class="v-sr" type="file" webkitdirectory aria-label="Dossier du dépôt" bind:this={folder} onchange={(e) => picked(e)} />
      <input class="v-sr" type="file" aria-label="Fichier de remplacement" bind:this={replacer} onchange={(e) => picked(e, replaced)} />
      <div class="rg-actions">
        {#if sending}
          <span class="v-note grow">Envoi en cours : {bytes(total) || '0 o'} sur {bytes(deposit.size)}. Garde cette page ouverte.</span>
          <button class="plbls-button plbls-button--compact plbls-button--contour" onclick={() => (stop = true)}>Interrompre</button>
        {:else}
          {#if deposit.state === 'upload' && !complete}
            <button class="plbls-button plbls-button--compact" disabled={busy} onclick={() => picker?.click()}><Icon name="envoie" size={14} /> Choisir les fichiers</button>
            {#if deposit.files.length > 1}<button class="plbls-button plbls-button--compact plbls-button--contour" disabled={busy} onclick={() => folder?.click()}>Choisir le dossier</button>{/if}
          {/if}
          {#if accepts && complete}
            <button class="plbls-button plbls-button--compact" disabled={busy || (deposit.state === 'mismatch' && invalid.length > 0)} onclick={() => void seed()}>{deposit.state === 'mismatch' ? 'Revérifier' : 'Vérifier et partager'}</button>
          {/if}
          {#if deposit.state === 'mismatch' && invalid.length}<span class="v-note grow">Remplace les fichiers non conformes, puis relance la vérification.</span>{/if}
          {#if deposit.state === 'checking'}<span class="v-note grow">qBittorrent vérifie les fichiers…</span>{/if}
          {#if deposit.state === 'seeding'}
            <button class="plbls-button plbls-button--compact" disabled={busy} onclick={() => (importing = true)}>Importer dans la bibliothèque</button>
          {/if}
          <span class="grow"></span>
          <button class="plbls-button plbls-button--compact plbls-button--contour plbls-button--danger" disabled={busy} onclick={() => void remove()}>Supprimer</button>
        {/if}
      </div>
    </div>
  {/if}
</article>
{#if importing}<RegieImport {deposit} onclose={() => (importing = false)} ondone={() => void refresh()} />{/if}

<style>
  .rg-depot { border-radius: var(--r); background: var(--surface); box-shadow: var(--o1); }
  .rg-tete { display: flex; align-items: center; gap: var(--espace-2); width: 100%; padding: var(--espace-2) var(--espace-3); border: 0; border-radius: var(--r); background: none; color: inherit; font: inherit; text-align: left; cursor: pointer; }
  .rg-tete:focus-visible { outline: none; box-shadow: var(--anneau-in); }
  .rg-body { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .rg-wrap { overflow-wrap: anywhere; }
  .rg-barre { display: block; height: 6px; border-radius: 3px; background: var(--papier-3); overflow: hidden; }
  .rg-barre > span { display: block; height: 100%; border-radius: 3px; background: var(--info); }
  .rg-corps { display: flex; flex-direction: column; gap: var(--espace-3); padding: 0 var(--espace-3) var(--espace-3); }
  .rg-corps .v-note { margin: 0; }
  .rg-fichiers { list-style: none; margin: 0; padding: 0; max-height: 40vh; overflow-y: auto; }
  .rg-fichiers li { display: flex; align-items: center; gap: var(--espace-2); padding: var(--espace-1) 0; box-shadow: inset 0 1px 0 var(--trait); }
  .rg-actions { display: flex; flex-wrap: wrap; align-items: center; gap: var(--espace-2); }
  .rg-err { display: inline-flex; align-items: center; gap: 4px; color: var(--danger); }
</style>
