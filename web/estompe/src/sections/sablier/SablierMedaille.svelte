<script lang="ts">
  import Icon, { traceIcone, type IconName } from '../../components/Icon.svelte'
  import type { Matiere } from './stats'

  let { icone, taille = 'l', etat = 'acquis', pose = false, matiere, anneau, nomAnneau = '' }: { icone: string; taille?: 's' | 'm' | 'l' | 'g' | 'xl'; etat?: 'acquis' | 'cours' | 'cache'; pose?: boolean; matiere?: Matiere; anneau?: { pct: number; vers: Matiere }; nomAnneau?: string } = $props()
  const nom = $derived<IconName>(traceIcone(icone as IconName) ? (icone as IconName) : 'trophee')
  const cote = $derived(taille === 'xl' ? 44 : taille === 'g' ? 30 : taille === 'l' ? 26 : 18)
</script>

{#snippet medaille()}
  <span class="sab-med" class:sab-med--s={taille === 's'} class:sab-med--m={taille === 'm'} class:sab-med--g={taille === 'g'} class:sab-med--xl={taille === 'xl'} class:sab-med--cours={etat === 'cours' || matiere === 'aucune'} class:sab-med--cache={etat === 'cache'} class:sab-med--bronze={matiere === 'bronze'} class:sab-med--argent={matiere === 'argent'} class:sab-med--or={matiere === 'or'} class:sab-pose={pose}><Icon name={nom} size={cote} /></span>
{/snippet}

{#if anneau && nomAnneau}
  <span class="sab-anneau" data-vers={anneau.vers} style:--p="{anneau.pct}%" role="progressbar" aria-valuenow={anneau.pct} aria-valuemin={0} aria-valuemax={100} aria-label={nomAnneau}>{@render medaille()}</span>
{:else if anneau}
  <span class="sab-anneau" data-vers={anneau.vers} style:--p="{anneau.pct}%" aria-hidden="true">{@render medaille()}</span>
{:else}
  {@render medaille()}
{/if}
