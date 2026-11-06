<script lang="ts">
  import Icon from './Icon.svelte'
  import Loader from './Loader.svelte'
  import AttachmentView from './AttachmentView.svelte'
  import { ApiError } from '../lib/api'
  import { formatSize } from '../lib/format'
  import { notify } from '../lib/ui'
  import { uploadAttachment } from '../blocks/videos/video'
  import type { Attachment } from '../lib/types'

  export interface EditItem { key: number; att?: Attachment; name: string; failed?: boolean }

  let { items = $bindable(), canAttach, max = 10 }: { items: EditItem[]; canAttach: boolean; max?: number } = $props()

  // svelte-ignore state_referenced_locally
  let seq = items.length
  let from = -1
  let over = $state(false)

  export async function add(list: Iterable<File>) {
    if (!canAttach) return
    for (const file of list) {
      if (items.length >= max) return void notify.info(`${max} fichiers maximum par message`)
      items.push({ key: ++seq, name: file.name })
      const entry = items[items.length - 1] // proxy réactif de l'élément ajouté
      try {
        entry.att = await uploadAttachment(file)
        entry.name = entry.att.name
      } catch (e) {
        entry.failed = true
        notify.error(e instanceof ApiError ? e : new Error(), `Envoi de « ${file.name} » impossible`)
        if (!(e instanceof ApiError) && e instanceof Error && e.message) notify.info(e.message)
      }
    }
  }

  const move = (to: number) => {
    if (from < 0 || from === to) return
    const [it] = items.splice(from, 1)
    items.splice(to, 0, it)
    from = -1
  }

  function ondrop(e: DragEvent) {
    if (!e.dataTransfer?.files.length) return
    e.preventDefault()
    over = false
    void add(e.dataTransfer.files)
  }
</script>

{#if items.length || canAttach}
  <div class="pending" class:over role="group" aria-label="Pièces jointes"
    ondragover={(e) => { if (e.dataTransfer?.types.includes('Files')) { e.preventDefault(); over = true } }}
    ondragleave={() => (over = false)} {ondrop}>
    {#each items as it, i (it.key)}
      <span class="edit-att" role="listitem" draggable="true" ondragstart={() => (from = i)} ondragover={(e) => { if (from >= 0) e.preventDefault() }} ondrop={(e) => { if (from >= 0) { e.preventDefault(); e.stopPropagation(); move(i) } }}>
        {#if it.att && it.att.mime.startsWith('image/') && !it.att.expired}
          <span class="thumb"><AttachmentView attachment={it.att} /></span>
        {:else}
          <span class="file-chip" class:expired={it.failed || it.att?.expired}>
            <Icon name="file" size={16} /> <span class="truncate">{it.name}</span>
            <span class="muted small">{it.failed ? 'échec' : it.att ? formatSize(it.att.size) : ''}</span>
            {#if !it.att && !it.failed}<Loader size="sm" label="Envoi de {it.name}" />{/if}
          </span>
        {/if}
        <button class="edit-att-x" onclick={() => items.splice(i, 1)} aria-label="Retirer {it.name}"><Icon name="x" size={14} /></button>
      </span>
    {/each}
    {#if !items.length}<span class="muted small">Aucune pièce jointe : glissez ou collez un fichier.</span>{/if}
  </div>
{/if}

<style>
  .edit-att { position: relative; display: inline-flex; cursor: grab; }
  .edit-att :global(.thumb img) { pointer-events: none; }
  .edit-att-x { position: absolute; top: 4px; right: 4px; display: grid; place-items: center; width: 22px; height: 22px; border: 0; border-radius: 50%; background: rgb(0 0 0 / .6); color: #fff; cursor: pointer; z-index: 1; }
  .over { outline: 2px dashed var(--bleu, currentColor); outline-offset: 2px; }
</style>
