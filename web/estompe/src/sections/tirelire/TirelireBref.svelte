<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { dateCourte, euros } from './formats'
  import { lienDecision, phraseAClasser, phraseDepenses, phraseEpargne, phraseEtat, referenceDe, titreLevier, titreResume, variations, type Comparaison, type Resume } from './decision'

  let { resume: r, extrait = false }: { resume: Resume; extrait?: boolean } = $props()

  const titre = $derived(titreResume(r))
  const blocs = $derived(([r.previous, r.usual].filter((c) => c && (c.up_total || c.down_total)) as Comparaison[]).slice(0, extrait ? 1 : 2))
  const epargne = $derived(phraseEpargne(r))
  const etat = $derived(phraseEtat(r))
  const aClasser = $derived(phraseAClasser(r))
  const levier = $derived(r.levers[0])
  const vers = (id?: string) => { const l = lienDecision(id); return hrefs.tirelire(l.section, l.id, l.requete) }
</script>

<section class="tir-carte tir-bref" aria-label="Résumé de la période">
  <div class="tir-ligne tir-ligne--ecart">
    <h2 class="tir-h2">{titre}</h2>
    {#if extrait}
      {#if r.period.open}<span class="tir-etq">au {dateCourte(r.period.known_through)}</span>{/if}
    {:else}
      <span class="tir-ligne" style:gap="0">
        {#if r.previous_id}<a class="tir-ib" href={vers(r.previous_id)} aria-label="Résumé de la période précédente"><Icon name="gauche" size={20} /></a>{:else}<span class="tir-ib tir-ib--absent" aria-hidden="true"><Icon name="gauche" size={20} /></span>{/if}
        {#if r.next_id}<a class="tir-ib" href={vers(r.next_id)} aria-label="Résumé de la période suivante"><Icon name="droite" size={20} /></a>{:else}<span class="tir-ib tir-ib--absent" aria-hidden="true"><Icon name="droite" size={20} /></span>{/if}
      </span>
    {/if}
  </div>
  {#if aClasser}
    <a class="tir-invite" href={hrefs.tirelire('a-classer')}>
      <span class="tir-pas tir-pas--danger"><Icon name="alerte" size={18} /></span>
      <span class="tir-note tir-note--texte tir-note--clair tir-grandit">{aClasser}</span>
      <Icon name="droite" size={18} />
    </a>
  {/if}
  <p class="tir-bref-phrase">{phraseDepenses(r)}</p>
  {#each blocs as c (c.periods)}
    <div class="tir-col" style:gap="6px" role="group" aria-label={referenceDe(r, c)}>
      <span class="tir-note">{referenceDe(r, c)}</span>
      {#if c.up_total}<div class="tir-bref-ligne"><span class="tir-etq tir-etq--danger"><span aria-hidden="true">▲</span>en hausse</span><p>{variations(c, 'up')}.</p></div>{/if}
      {#if c.down_total}<div class="tir-bref-ligne"><span class="tir-etq tir-etq--ok"><span aria-hidden="true">▼</span>en baisse</span><p>{variations(c, 'down')}.</p></div>{/if}
    </div>
  {/each}
  {#if epargne && !extrait}<div class="tir-bref-ligne"><span class="tir-etq tir-etq--attente">épargne</span><p>{epargne}</p></div>{/if}
  {#if etat}<p class="tir-note tir-note--texte">{etat}</p>{/if}
  {#if extrait}
    {#if levier}<p class="tir-note tir-note--texte tir-note--clair">Premier levier : {titreLevier(levier)}, {euros(levier.monthly)} par mois.</p>{/if}
    <a class="tir-btn tir-btn--large" href={vers(r.next_id === null ? undefined : r.period.id)}>Résumé complet et leviers<Icon name="droite" size={16} /></a>
  {/if}
</section>
