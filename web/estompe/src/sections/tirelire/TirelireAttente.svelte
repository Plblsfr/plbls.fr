<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { periode, pluriel } from './formats'
  import type { Releve } from './tirelire'

  let { vue, releves }: { vue: 'accueil' | 'mois' | 'evolution' | 'habitudes'; releves: Releve[] } = $props()

  const TEXTES = {
    accueil: 'Le tableau de bord (dépenses du mois, budget, ce qui dépasse) a besoin d’opérations classées par catégorie.',
    mois: 'La vue du mois répartit les dépenses par catégorie : elle a besoin d’opérations classées.',
    evolution: 'L’évolution compare les catégories d’un mois à l’autre : elle a besoin d’opérations classées.',
    habitudes: 'Les habitudes (commerçants réguliers, abonnements, prélèvements) ont besoin de commerçants et de catégories.',
  }
  const operations = $derived(releves.reduce((n, r) => n + r.operations, 0))
  const etendue = $derived(releves.length ? periode(releves.reduce((a, r) => (r.from < a ? r.from : a), releves[0].from), releves.reduce((a, r) => (r.to > a ? r.to : a), releves[0].to)) : '')
</script>

<section class="tir-carte tir-etat" aria-label="Pas encore disponible">
  <span class="tir-pas tir-pas--grand tir-pas--rose"><Icon name="horloge" size={22} /></span>
  <h2 class="tir-h2">Disponible quand les catégories seront en place</h2>
  <p class="tir-note tir-note--texte">{TEXTES[vue]}</p>
</section>

<section class="tir-carte tir-liste" aria-label="Déjà disponible">
  {#if releves.length}
    <a class="tir-li" href={hrefs.tirelire('operations')}>
      <span class="tir-pas"><Icon name="list" size={18} /></span>
      <span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">Opérations</span><span class="tir-note">{pluriel(operations, 'opération', 'opérations')} · {etendue.charAt(0).toLowerCase() + etendue.slice(1)}</span></span>
      <Icon name="droite" size={18} />
    </a>
  {/if}
  <a class="tir-li" href={hrefs.tirelire('import')}>
    <span class="tir-pas"><Icon name="envoie" size={18} /></span>
    <span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">Import</span><span class="tir-note">{releves.length ? `${pluriel(releves.length, 'relevé importé', 'relevés importés')} · en ajouter ou en supprimer` : 'Aucun relevé importé · déposer un PDF'}</span></span>
    <Icon name="droite" size={18} />
  </a>
</section>
