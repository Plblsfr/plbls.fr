<script lang="ts">
  import { tint } from '../entracte/films'
  import { sillonApi, type Taille } from './sillon'

  let { id, title, image, kind = 'album', size = 'medium' }: { id: string; title: string; image: boolean; kind?: 'album' | 'artiste'; size?: Taille } = $props()

  let url = $state('')
  const colors = $derived(tint(id))

  $effect(() => {
    url = ''
    if (!image) return
    let gone = false
    let made = ''
    void (kind === 'album' ? sillonApi.pochette(id, size) : sillonApi.portrait(id, size)).then((u) => {
      made = u
      if (gone) URL.revokeObjectURL(u)
      else url = u
    }).catch(() => {})
    return () => { gone = true; if (made) URL.revokeObjectURL(made) }
  })
</script>

{#if url}
  <img class="film-art" src={url} alt="" />
{:else}
  <span class="film-art film-art--flat sillon-flat" style="--bg:{colors.bg};--fg:{colors.fg};--ac:{colors.ac}"><span class="film-disc"></span>{#if size !== 'small' && kind === 'album'}<span class="film-art-title">{title}</span>{/if}</span>
{/if}

<style>
  .sillon-flat .film-disc { left: 22%; top: 14%; width: 56%; }
</style>
