<script lang="ts">
  import Icon, { traceIcone, type IconName } from '../../components/Icon.svelte'

  let { icone, taille = 'l', etat = 'acquis', pose = false }: { icone: string; taille?: 's' | 'm' | 'l' | 'xl'; etat?: 'acquis' | 'cours' | 'cache'; pose?: boolean } = $props()
  const nom = $derived<IconName>(traceIcone(icone as IconName) ? (icone as IconName) : 'trophee')
  const cote = $derived(taille === 'xl' ? 44 : taille === 'l' ? 26 : 18)
</script>

<span class="sab-med" class:sab-med--s={taille === 's'} class:sab-med--m={taille === 'm'} class:sab-med--xl={taille === 'xl'} class:sab-med--cours={etat === 'cours'} class:sab-med--cache={etat === 'cache'} class:sab-pose={pose}><Icon name={nom} size={cote} /></span>
