<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import Modal from '../../components/Modal.svelte'
  import { entracteApi } from '../../lib/api'
  import { messageOf, notify } from '../../lib/ui'
  import { bytes, episodeCode, serviceName, titleLabel, type ArrImportFile, type ArrQueueItem } from './arr'

  let { item, onclose, ondone }: { item: ArrQueueItem; onclose: () => void; ondone: () => void } = $props()

  let files = $state<ArrImportFile[] | null>(null)
  let chosen = $state<string[]>([])
  let error = $state('')
  let busy = $state(false)

  const service = $derived(serviceName(item.service))
  const importable = (f: ArrImportFile) => !!f.title && (item.service === 'radarr' || f.episodes.length > 0)

  $effect(() => {
    let gone = false
    void entracteApi.arrImportable(item.service, item.id).then((r) => {
      if (gone) return
      files = r.items
      chosen = r.items.filter((f) => importable(f) && !f.rejections.length).map((f) => f.name)
    }).catch((e) => { if (!gone) { files = []; error = messageOf(e, `${service} ne retrouve pas les fichiers de ce téléchargement.`) } })
    return () => { gone = true }
  })

  const toggle = (nom: string) => { chosen = chosen.includes(nom) ? chosen.filter((n) => n !== nom) : [...chosen, nom] }

  async function send() {
    busy = true
    error = ''
    try {
      const r = await entracteApi.arrImport(item.service, item.id, chosen)
      notify.ok(`${r.files} fichier${r.files > 1 ? 's' : ''} envoyé${r.files > 1 ? 's' : ''} à l’import.`)
      ondone()
      onclose()
    } catch (e) {
      error = messageOf(e, 'L’import n’a pas pu être lancé.')
    } finally { busy = false }
  }
</script>

<Modal title="Importer « {titleLabel(item)} »" wide {onclose}>
  <form class="imp" onsubmit={(e) => { e.preventDefault(); void send() }}>
    {#if !files}
      <Loader />
    {:else if !files.length}
      {#if !error}<p class="v-note">{service} ne trouve aucun fichier dans ce téléchargement.</p>{/if}
    {:else}
      <p class="v-note">Fichiers trouvés par {service} dans le téléchargement. Un fichier qu’il ne rattache à aucun titre ne peut pas être importé d’ici.</p>
      <ul class="adm-sous">
        {#each files as f (f.name)}
          <li>
            <label class="adm-coche grow"><input class="adm-case" type="checkbox" checked={chosen.includes(f.name)} disabled={!importable(f)} onchange={() => toggle(f.name)} />
              <span class="adm-body"><span class="adm-mono">{f.name}</span>
                <span class="v-note">{[f.title, f.episodes.map(episodeCode).filter(Boolean).join(', '), f.quality, f.languages.join(', '), bytes(f.size)].filter(Boolean).join(' · ') || 'non reconnu'}</span>
                {#if f.rejections.length}<span class="v-note adm-att"><Icon name="alerte" size={14} /> {f.rejections.join(' ; ')}</span>{/if}
              </span>
            </label>
          </li>
        {/each}
      </ul>
    {/if}
    {#if error}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">{error}</p></div>{/if}
    <div class="imp-actions">
      <button type="button" class="plbls-button plbls-button--contour" onclick={onclose}>Annuler</button>
      <button type="submit" class="plbls-button" disabled={busy || !chosen.length}>Importer {chosen.length || ''}</button>
    </div>
  </form>
</Modal>

<style>
  .imp { display: flex; flex-direction: column; gap: var(--espace-3); }
  .imp-actions { display: flex; justify-content: flex-end; flex-wrap: wrap; gap: var(--espace-2); }
</style>
