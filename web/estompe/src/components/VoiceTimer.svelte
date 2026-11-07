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

<span class="voice-timer" title="Présence dans le salon">{formatDuration((now - since) / 1000)}</span>

<style>
  .voice-timer { flex: none; margin-left: auto; font-family: var(--f-mono); font-size: 11.5px; font-weight: 400; color: var(--encre-2); font-variant-numeric: tabular-nums; }
</style>
