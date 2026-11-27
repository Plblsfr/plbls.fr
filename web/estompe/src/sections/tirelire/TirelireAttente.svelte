<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { euros, periode, pluriel } from './formats'
  import type { Releve } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let { vue, releves }: { vue: 'accueil' | 'mois' | 'evolution' | 'habitudes'; releves: Releve[] } = $props()

  const TEXTES = {
    accueil: 'Le tableau de bord (dépenses du mois, budget, ce qui dépasse) arrive avec les statistiques. En attendant, classe tes opérations : il s’appuiera dessus.',
    mois: 'La vue du mois répartira les dépenses par catégorie. Elle arrive avec les statistiques.',
    evolution: 'L’évolution comparera les catégories d’un mois à l’autre. Elle arrive avec les statistiques.',
    habitudes: 'Les habitudes (commerçants réguliers, abonnements, prélèvements) arrivent avec les statistiques.',
  }
  const operations = $derived(releves.reduce((n, r) => n + r.operations, 0))
  const etendue = $derived(releves.length ? periode(releves.reduce((a, r) => (r.from < a ? r.from : a), releves[0].from), releves.reduce((a, r) => (r.to > a ? r.to : a), releves[0].to)) : '')
</script>

{#if vue === 'accueil' && tirelire.compteur && tirelire.aClasser > 0}
  <a class="tir-bandeau" href={hrefs.tirelire('a-classer')}>
    <span class="tir-pas tir-pas--rose"><Icon name="boite" size={18} /></span>
    <span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">{pluriel(tirelire.aClasser, 'opération', 'opérations')} à classer</span><span class="tir-note">{euros(tirelire.compteur.debits)} de débits sans catégorie · {pluriel(tirelire.compteur.merchants, 'commerçant', 'commerçants')}</span></span>
    <Icon name="droite" size={18} />
  </a>
{/if}

<section class="tir-carte tir-etat" aria-label="Pas encore disponible">
  <span class="tir-pas tir-pas--grand tir-pas--rose"><Icon name="horloge" size={22} /></span>
  <h2 class="tir-h2">Bientôt disponible</h2>
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
  {#if releves.length}
    <a class="tir-li" href={hrefs.tirelire('categories')}>
      <span class="tir-pas"><Icon name="curseurs" size={18} /></span>
      <span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">Catégories et règles</span><span class="tir-note">{tirelire.categories ? `${pluriel(tirelire.categories.filter((c) => !c.archived).length, 'catégorie', 'catégories')} · plafonds, règles, commerçants` : 'plafonds, règles, commerçants'}</span></span>
      <Icon name="droite" size={18} />
    </a>
  {/if}
  <a class="tir-li" href={hrefs.tirelire('import')}>
    <span class="tir-pas"><Icon name="envoie" size={18} /></span>
    <span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">Import</span><span class="tir-note">{releves.length ? `${pluriel(releves.length, 'relevé importé', 'relevés importés')} · en ajouter ou en supprimer` : 'Aucun relevé importé · déposer un PDF'}</span></span>
    <Icon name="droite" size={18} />
  </a>
</section>
