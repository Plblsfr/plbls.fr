<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { aimes, type GenreLike } from './sillon.svelte'

  let { kind, id, titre, grand = false }: { kind: GenreLike; id: string; titre: string; grand?: boolean } = $props()
  const oui = $derived(aimes.a(kind, id))
</script>

<button class="v-ib sillon-like" class:sm={!grand} class:oui aria-pressed={oui} aria-label={`${oui ? 'Ne plus aimer' : 'Aimer'} ${titre}`} title={oui ? 'Retirer le like' : 'Aimer'} onclick={(e) => { e.stopPropagation(); void aimes.basculer(kind, id) }}>
  <Icon name={oui ? 'coeurplein' : 'coeur'} size={grand ? 20 : 16} />
</button>

<style>
  .sillon-like[aria-pressed='true'] { background: transparent; color: var(--rose-fonce); }
  .sillon-like[aria-pressed='true']:hover { background: var(--papier-2); }
</style>
