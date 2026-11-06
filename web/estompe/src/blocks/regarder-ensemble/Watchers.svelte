<script lang="ts">
  import Avatar from '../../components/Avatar.svelte'
  import Icon from '../../components/Icon.svelte'

  let { list }: { list: { id: string; name: string }[] } = $props()
  let open = $state(false)
</script>

{#if list.length}
  <span class="watchers">
    <button class="watchers-btn" aria-expanded={open} aria-label="{list.length} spectateur{list.length > 1 ? 's' : ''}" title={list.map((w) => w.name).join(', ')} onclick={(e) => { e.stopPropagation(); open = !open }} ondblclick={(e) => e.stopPropagation()}>
      <Icon name="eye" size={13} /> {list.length}
    </button>
    {#if open}
      <ul class="watchers-list" aria-label="Spectateurs">
        {#each list as w (w.id)}<li class="row"><Avatar pseudo={w.name} userId={w.id} size="sm" /><span class="truncate">{w.name}</span></li>{/each}
      </ul>
    {/if}
  </span>
{/if}
