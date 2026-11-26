<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { app } from '../../lib/store.svelte'
  import { dateCourte, periode, pluriel } from './formats'
  import { ONGLETS, ongletActif, vueDe } from './onglets'
  import TirelireAttente from './TirelireAttente.svelte'
  import TirelireImport from './TirelireImport.svelte'
  import TirelireOnglets from './TirelireOnglets.svelte'
  import TirelireOperation from './TirelireOperation.svelte'
  import TirelireOperations from './TirelireOperations.svelte'
  import TirelireTete from './TirelireTete.svelte'
  import TirelireVide from './TirelireVide.svelte'
  import { natureDe, nomReleve } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let { section, id, onmenu }: { section?: string; id?: string; onmenu: () => void } = $props()

  const vue = $derived(vueDe(section, id))
  const actif = $derived(ongletActif(vue))
  const nom = $derived(app.tirelireMe?.name ?? '')
  const vide = $derived(tirelire.releves?.length === 0)
  const operation = $derived(vue === 'operation' ? tirelire.operation(Number(id)) : null)
  const filtre = $derived(tirelire.filtres.releve === null ? null : tirelire.releve(tirelire.filtres.releve))
  const portee = $derived.by(() => {
    if (tirelire.filtres.du && tirelire.filtres.au) return periode(tirelire.filtres.du, tirelire.filtres.au).toLowerCase()
    if (tirelire.filtres.du) return `depuis le ${dateCourte(tirelire.filtres.du)}`
    if (tirelire.filtres.au) return `jusqu’au ${dateCourte(tirelire.filtres.au)}`
    if (filtre) return nomReleve(filtre).toLowerCase()
    return tirelire.releves?.length ? pluriel(tirelire.releves.length, 'relevé', 'relevés') : ''
  })
  const titre = $derived(vue === 'accueil' ? (vide ? 'Bienvenue' : 'Accueil') : (ONGLETS.find((o) => o.id === actif)?.long ?? 'Tirelire'))

  onMount(() => { void tirelire.ouvrir() })
</script>

<div class="tir" data-vue={vue}>
  {#if vue === 'import'}
    <TirelireTete titre="Import" sous="relevés de compte au format PDF" retour={hrefs.tirelire()} retourLabel="Retour à l’accueil" />
  {:else if vue === 'operation'}
    <TirelireTete titre="Opération" sous={operation ? `${dateCourte(operation.date)} · ${natureDe(operation)}` : ''} retour={hrefs.tirelire('operations')} retourLabel="Retour aux opérations" />
  {:else}
    <TirelireTete {titre} sur={['Tirelire', vue === 'operations' ? portee : nom].filter(Boolean).join(' · ')} {onmenu}>
      <a class="tir-ib" href={hrefs.tirelire('import')} aria-label="Importer des relevés" title="Importer des relevés"><Icon name="envoie" size={20} /></a>
    </TirelireTete>
  {/if}

  <div class="tir-defile">
    <div class="tir-corps">
      {#if vue === 'import'}
        <TirelireImport />
      {:else if vue === 'operation'}
        <TirelireOperation id={Number(id)} />
      {:else if vue === 'operations'}
        <TirelireOperations />
      {:else if tirelire.releves}
        {#if vide && vue === 'accueil'}<TirelireVide />{:else}<TirelireAttente {vue} releves={tirelire.releves} />{/if}
      {:else if tirelire.erreur}
        <section class="tir-carte tir-etat" role="alert">
          <span class="tir-pas tir-pas--grand tir-pas--danger"><Icon name="alerte" size={22} /></span>
          <h2 class="tir-h2">Tirelire ne répond pas</h2>
          <p class="tir-note tir-note--texte">{tirelire.erreur} Le reste de l’app fonctionne normalement.</p>
          <button class="tir-btn tir-btn--plein" onclick={() => void tirelire.chargerReleves()}><Icon name="actualise" size={16} />Réessayer</button>
        </section>
      {:else}
        <div class="tir-etat" role="status"><Loader text="Chargement de Tirelire…" /></div>
      {/if}
    </div>
  </div>

  {#if vue !== 'import'}<TirelireOnglets {actif} />{/if}
</div>
