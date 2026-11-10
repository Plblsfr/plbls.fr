<script lang="ts">
  import { entracteApi } from '../../lib/api'
  import { tint } from '../../blocks/films/films'

  let { url, title, seed }: { url: string | null | undefined; title: string; seed: string } = $props()

  let src = $state('')
  const colors = $derived(tint(seed))

  $effect(() => {
    src = ''
    if (!url) return
    let gone = false
    let made = ''
    void entracteApi.poster(url).then((u) => {
      made = u
      if (gone) URL.revokeObjectURL(u)
      else src = u
    }).catch(() => {})
    return () => { gone = true; if (made) URL.revokeObjectURL(made) }
  })
</script>

{#if src}
  <img class="film-art" {src} alt="" />
{:else}
  <span class="film-art film-art--flat" style="--bg:{colors.bg};--fg:{colors.fg};--ac:{colors.ac}"><span class="film-disc"></span><span class="film-art-title">{title}</span></span>
{/if}
