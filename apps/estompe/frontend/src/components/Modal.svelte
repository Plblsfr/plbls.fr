<script lang="ts">
  import type { Snippet } from 'svelte'

  let { title, wide = false, onclose, children }: { title: string; wide?: boolean; onclose: () => void; children: Snippet } = $props()
</script>

<plbls-modal open {title} data-wide={wide || undefined} onplbls-close={onclose}>
  {@render children()}
</plbls-modal>
