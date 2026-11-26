<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { dateLongue, montant, periode, pluriel } from './formats'
  import TirelirePastille from './TirelirePastille.svelte'
  import { actives, NATURES_LONGUES, nomReleve, ORDRE_NATURES, SOURCES, titreDe, typeDe, type Portee } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let { id }: { id: number } = $props()
  const o = $derived(tirelire.operation(id))
  const releve = $derived(o ? tirelire.releve(o.statement_id) : null)
  const categorie = $derived(o ? tirelire.categorie(o.category_id) : null)
  const proposees = $derived(actives(tirelire.categories ?? []))
  let portee = $state<Portee>('merchant')
  let choix = $state('')
  let info = $state('')
  const porteeUtile = $derived<Portee>(o?.merchant_id === null ? 'operation' : portee)
  const inchange = $derived(!!o && choix === (o.category_id === null ? '' : String(o.category_id)) && (porteeUtile === 'merchant' ? o.category_source !== 'operation' : o.category_source === 'operation' || o.category_id === null))

  onMount(() => { void tirelire.chargerDetail(id) })

  $effect(() => {
    choix = o?.category_id === null || !o ? '' : String(o.category_id)
    portee = o?.category_source === 'operation' ? 'operation' : 'merchant'
  })

  async function appliquer() {
    if (!o) return
    const n = await tirelire.reclasser(o, choix ? Number(choix) : null, porteeUtile)
    if (n === null) return
    info = porteeUtile === 'merchant' ? `Règle enregistrée pour tout le commerçant : ${pluriel(n, 'opération reclassée', 'opérations reclassées')}.` : choix ? 'Catégorie enregistrée pour cette opération seulement.' : 'Exception retirée : l’opération suit de nouveau les règles.'
  }
</script>

{#if o}
  <section class="tir-carte" aria-label="Résumé de l’opération">
    <div class="tir-ligne tir-ligne--ecart tir-ligne--haut tir-ligne--plis">
      <div class="tir-col tir-grandit"><h2 class="tir-h2" style:overflow-wrap="anywhere">{titreDe(o)}</h2><span class="tir-sous"><TirelirePastille {categorie} /><span class="tir-note">{typeDe(o)}</span></span></div>
      <span class="tir-gros" class:tir-credit={o.direction === 'credit'}>{montant(o.amount, o.direction)}</span>
    </div>
  </section>

  <section class="tir-carte" aria-label="Catégorie">
    <div class="tir-ligne tir-ligne--ecart tir-ligne--plis"><h2 class="tir-h2">Catégorie</h2>{#if o.category_source}<span class="tir-etq">{SOURCES[o.category_source]}</span>{/if}</div>
    <span class="tir-champ">
      <select aria-label="Catégorie de l’opération" bind:value={choix} disabled={tirelire.occupe || !tirelire.categories}>
        <option value="">{porteeUtile === 'operation' ? 'Aucune : suivre les règles' : 'Aucune : à classer par les règles'}</option>
        {#if categorie?.archived}<option value={String(categorie.id)}>{categorie.name} (archivée)</option>{/if}
        {#each ORDRE_NATURES as n (n)}
          {#if proposees.some((c) => c.kind === n)}
            <optgroup label={NATURES_LONGUES[n]}>{#each proposees.filter((c) => c.kind === n) as c (c.id)}<option value={String(c.id)}>{c.name}</option>{/each}</optgroup>
          {/if}
        {/each}
      </select>
      <Icon name="bas" size={16} />
    </span>
    {#if o.merchant_id !== null}
      <div class="tir-seg" role="group" aria-label="Portée du reclassement">
        <button aria-pressed={portee === 'operation'} onclick={() => (portee = 'operation')}>Cette opération</button>
        <button aria-pressed={portee === 'merchant'} onclick={() => (portee = 'merchant')}>Tout le commerçant</button>
      </div>
      <p class="tir-note">{portee === 'merchant' ? `« Tout le commerçant » crée une règle pour « ${o.merchant_name} » : ses opérations déjà importées et les suivantes prennent cette catégorie.` : '« Cette opération » ne change que celle-ci : les règles ne la modifieront plus.'}</p>
    {:else}
      <p class="tir-note">Aucun commerçant n’a été reconnu dans ce libellé : la catégorie vaut pour cette opération seulement.</p>
    {/if}
    {#if tirelire.erreurClassement}<p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />{tirelire.erreurClassement}</p>{/if}
    {#if info}<p class="tir-fait" role="status"><Icon name="ok" size={18} /><span class="tir-note tir-note--texte">{info}</span></p>{/if}
    <button class="tir-btn tir-btn--plein" disabled={tirelire.occupe || inchange} onclick={() => void appliquer()}>{porteeUtile === 'merchant' ? 'Appliquer à tout le commerçant' : 'Appliquer à cette opération'}</button>
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
    <button class="tir-btn tir-btn--plein" onclick={() => void tirelire.chargerDetail(id)}><Icon name="actualise" size={16} />Réessayer</button>
  </section>
{:else if !tirelire.detailAbsent}
  <div class="tir-etat" role="status"><Loader text="Chargement de l’opération…" /></div>
{:else}
  <section class="tir-carte tir-etat" aria-label="Opération introuvable">
    <h2 class="tir-h2">Opération introuvable</h2>
    <p class="tir-note tir-note--texte">Elle n’existe pas, ou son relevé a été supprimé.</p>
    <a class="tir-btn tir-btn--plein" href={hrefs.tirelire('operations')}>Retour aux opérations</a>
  </section>
{/if}
