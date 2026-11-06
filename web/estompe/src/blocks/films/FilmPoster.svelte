<script lang="ts">
  import { filmsApi } from '../../lib/api'
  import { tint } from './films'

  let { id, title, image, type = 'poster', width = 240 }: { id: string; title: string; image: boolean; type?: 'poster' | 'fond'; width?: number } = $props()

  let url = $state('')
  const colors = $derived(tint(id))

  $effect(() => {
    url = ''
    if (!image) return
    let gone = false
    let made = ''
    void filmsApi.image(id, type, width).then((u) => {
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
  <span class="film-art film-art--flat" style="--bg:{colors.bg};--fg:{colors.fg};--ac:{colors.ac}"><span class="film-disc"></span><span class="film-art-title">{title}</span></span>
{/if}
