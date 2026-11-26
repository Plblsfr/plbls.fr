<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { dateLongue, montant, periode, pluriel } from './formats'
  import { nomReleve, titreDe, typeDe } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let { id }: { id: number } = $props()
  const o = $derived(tirelire.operation(id))
  const releve = $derived(o ? tirelire.releve(o.statement_id) : null)

  onMount(() => { if (!tirelire.operations) void tirelire.chargerOperations() })
</script>

{#if o}
  <section class="tir-carte" aria-label="Résumé de l’opération">
    <div class="tir-ligne tir-ligne--ecart tir-ligne--haut tir-ligne--plis">
      <div class="tir-col tir-grandit"><h2 class="tir-h2" style:overflow-wrap="anywhere">{titreDe(o)}</h2><span class="tir-note">{typeDe(o)}</span></div>
      <span class="tir-gros" class:tir-credit={o.direction === 'credit'}>{montant(o.amount, o.direction)}</span>
    </div>
  </section>

  <section class="tir-carte" aria-label="Libellé du relevé">
    <h2 class="tir-h2">Sur le relevé</h2>
    <div class="tir-col"><span class="tir-note">Libellé brut</span><p class="tir-mono tir-bloc">{o.label}</p></div>
    {#if o.details.length}
      <div class="tir-col"><span class="tir-note">Détails ({pluriel(o.details.length, 'ligne', 'lignes')})</span><p class="tir-mono tir-bloc tir-bloc--doux">{o.details.join('\n')}</p></div>
    {/if}
    <dl class="tir-liste">
      {#if o.purchase_date}<div class="tir-kv"><dt>Date d’achat carte</dt><dd>{dateLongue(o.purchase_date)}</dd></div>{/if}
      <div class="tir-kv"><dt>Date de l’opération</dt><dd>{dateLongue(o.date)}</dd></div>
      <div class="tir-kv"><dt>Date de valeur</dt><dd>{dateLongue(o.value_date)}</dd></div>
      <div class="tir-kv"><dt>Section du relevé</dt><dd>{o.section}</dd></div>
      <div class="tir-kv"><dt>Type</dt><dd>{typeDe(o)}</dd></div>
      <div class="tir-kv"><dt>Sens</dt><dd>{o.direction === 'debit' ? 'Débit' : 'Crédit'}</dd></div>
      {#if releve}<div class="tir-kv"><dt>Source</dt><dd>{nomReleve(releve)} · {periode(releve.from, releve.to).toLowerCase()}</dd></div>{/if}
    </dl>
  </section>
{:else if tirelire.erreurOperations}
  <section class="tir-carte tir-etat" role="alert">
    <span class="tir-pas tir-pas--grand tir-pas--danger"><Icon name="alerte" size={22} /></span>
    <h2 class="tir-h2">L’opération ne se charge pas</h2>
    <p class="tir-note tir-note--texte">{tirelire.erreurOperations}</p>
    <button class="tir-btn tir-btn--plein" onclick={() => void tirelire.chargerOperations()}><Icon name="actualise" size={16} />Réessayer</button>
  </section>
{:else if !tirelire.operations}
  <div class="tir-etat" role="status"><Loader text="Chargement de l’opération…" /></div>
{:else}
  <section class="tir-carte tir-etat" aria-label="Opération introuvable">
    <h2 class="tir-h2">Opération introuvable</h2>
    <p class="tir-note tir-note--texte">Elle n’est pas dans la liste chargée, ou son relevé a été supprimé. Retrouve-la depuis la liste.</p>
    <a class="tir-btn tir-btn--plein" href={hrefs.tirelire('operations')}>Retour aux opérations</a>
  </section>
{/if}
