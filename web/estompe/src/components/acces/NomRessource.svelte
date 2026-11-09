<script lang="ts">
  import { nomRessource } from '../../lib/acces.svelte'

  let { type, id }: { type: string | null | undefined; id: string } = $props()
  let nom = $state('')
  $effect(() => {
    const [t, i] = [type, id]
    nom = i === '*' ? 'partout' : i
    let perime = false
    void nomRessource(t, i).then((n) => { if (!perime) nom = n })
    return () => { perime = true }
  })
</script>

<span title={id === '*' ? undefined : id}>{nom}</span>
