<script lang="ts">
  import { onMount } from 'svelte'
  import { formatDuration } from '../lib/format'

  let { since }: { since: number } = $props()
  let now = $state(Date.now())

  onMount(() => {
    const t = setInterval(() => (now = Date.now()), 1000)
    return () => clearInterval(t)
  })
</script>

<span class="small muted voice-timer" title="Présence dans le salon">{formatDuration((now - since) / 1000)}</span>
