<script lang="ts">
  import { onMount } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { dateCourte, eurosSignes, nombre, periode, pluriel } from './formats'
  import { actives, natureDe, resteDe, teinte, type Categorie } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  const COURT = 6
  let choix = $state<number | null>(null)
  let tout = $state(true)
  let toutes = $state(false)

  const groupes = $derived(tirelire.groupes)
  const courant = $derived(groupes[0] ?? null)
  const suivants = $derived(groupes.slice(1))
  const categories = $derived([...actives(tirelire.categories ?? [])].sort((a, b) => Number(b.kind === 'variable') - Number(a.kind === 'variable')))
  const visibles = $derived(toutes ? categories : categories.filter((c, i) => i < COURT || c.id === choix))
  const choisie = $derived(categories.find((c) => c.id === choix) ?? null)
  const total = $derived(tirelire.classeesIci + tirelire.aClasser)
  const part = $derived(total ? Math.round((tirelire.classeesIci / total) * 100) : 0)
  const cle = $derived(courant ? tirelire.cleDe(courant) : '')

  onMount(() => {
    if (!tirelire.categories) void tirelire.chargerReference(false)
    void tirelire.chargerFile()
  })

  $effect(() => {
    void cle
    choix = null
    tout = true
  })

  async function classer(c: Categorie) {
    if (!courant) return
    await tirelire.classerGroupe(courant, c, tout)
  }
</script>

{#if tirelire.file && tirelire.categories}
  {#if total > 0}
    <div class="tir-col" role="group" aria-label="Avancement">
      <div class="tir-ligne tir-ligne--ecart tir-note"><span>{pluriel(tirelire.classeesIci, 'classée', 'classées')} sur {nombre(total)}</span><span>{part} %</span></div>
      <div class="tir-jauge" aria-hidden="true"><span style:width="{part}%"></span></div>
    </div>
  {/if}

  {#if tirelire.dernier}
    <div class="tir-fait" role="status">
      <span class="tir-pas tir-pas--ok"><Icon name="ok" size={18} /></span>
      <span class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">{tirelire.dernier.titre}</span><span class="tir-note">{tirelire.dernier.detail}</span></span>
      <button class="tir-btn tir-btn--contour" disabled={tirelire.occupe} onclick={() => void tirelire.annuler()}>Annuler</button>
    </div>
  {/if}

  {#if tirelire.erreurClassement}<p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />{tirelire.erreurClassement}</p>{/if}

  {#if courant}
    {@const o = courant.sample[0]}
    <section class="tir-carte" aria-label="À classer maintenant">
      <div class="tir-ligne tir-ligne--ecart tir-ligne--haut tir-ligne--plis">
        <div class="tir-col tir-col--serre tir-grandit">
          <span class="tir-note">{courant.operations > 1 ? periode(courant.first_date, courant.last_date).toLowerCase() : dateCourte(o.date)} · {natureDe(o)}</span>
          <h2 class="tir-h2" style:overflow-wrap="anywhere">{courant.merchant_name ?? o.label}</h2>
        </div>
        <span class="tir-val tir-fixe" class:tir-credit={resteDe(courant) > 0}>{eurosSignes(resteDe(courant))}</span>
      </div>
      <p class="tir-mono tir-bloc">{courant.sample.map((s) => s.label).join('\n')}</p>
      <p class="tir-note tir-note--texte">{courant.merchant_id === null ? 'Aucun commerçant reconnu dans ce libellé : l’opération se classe seule.' : courant.operations > 1 ? `${pluriel(courant.operations, 'opération', 'opérations')} de ce commerçant attendent une catégorie.` : 'Une seule opération porte ce commerçant pour l’instant.'}</p>

      <h3 class="tir-h3">Choisir une catégorie</h3>
      <div class="tir-choix" role="group" aria-label="Catégories">
        {#each visibles as c (c.id)}
          <button class="tir-ch" aria-pressed={choix === c.id} disabled={tirelire.occupe} onclick={() => (choix = c.id)}><span class="tir-pt tir-pt--{teinte(c)}" aria-hidden="true"></span><span>{c.name}</span></button>
        {/each}
        {#if categories.length > visibles.length}
          <button class="tir-ch" onclick={() => (toutes = true)}><Icon name="more" size={16} /><span>Autres ({categories.length - visibles.length})</span></button>
        {/if}
        <a class="tir-ch" href={hrefs.tirelire('categories')}><Icon name="plus" size={16} /><span>Nouvelle</span></a>
      </div>

      {#if courant.merchant_id !== null}
        <label class="tir-case"><input type="checkbox" bind:checked={tout} /><span>Créer une règle : toutes les opérations « {courant.merchant_name} »{choisie ? ` iront en ${choisie.name}` : ''}</span></label>
      {/if}
      <div class="tir-ligne">
        <button class="tir-btn tir-btn--contour" disabled={tirelire.occupe || groupes.length < 2} onclick={() => tirelire.passer(courant)}>Passer</button>
        <button class="tir-btn tir-btn--rose tir-btn--grandit" disabled={!choisie || tirelire.occupe} onclick={() => choisie && void classer(choisie)}>{choisie ? `Classer en ${choisie.name}` : 'Choisis une catégorie'}</button>
      </div>
    </section>

    {#if suivants.length}
      <section class="tir-carte tir-carte--liste" aria-label="Ensuite">
        <div class="tir-ligne tir-ligne--ecart" style:padding="12px 0 4px"><h2 class="tir-h2">Ensuite</h2><span class="tir-note">{pluriel(suivants.reduce((n, g) => n + g.operations, 0), 'opération', 'opérations')} · {pluriel(suivants.length, 'groupe', 'groupes')}</span></div>
        <ul class="tir-liste">
          {#each suivants as g (tirelire.cleDe(g))}
            <li>
              <button class="tir-li tir-li--bouton" onclick={() => tirelire.choisir(g)}>
                <span class="tir-col tir-col--serre tir-grandit"><span class="tir-h3">{g.merchant_name ?? g.sample[0].label}</span><span class="tir-note">{g.operations > 1 ? `${pluriel(g.operations, 'opération', 'opérations')} · ${periode(g.first_date, g.last_date).toLowerCase()}` : `${dateCourte(g.last_date)} · ${natureDe(g.sample[0])}`}</span></span>
                <span class="tir-fort tir-fixe" class:tir-credit={resteDe(g) > 0}>{eurosSignes(resteDe(g))}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {:else}
    <section class="tir-carte tir-etat" aria-label="Rien à classer">
      <span class="tir-pas tir-pas--grand tir-pas--ok"><Icon name="ok" size={22} /></span>
      <h2 class="tir-h2">Tout est classé</h2>
      <p class="tir-note tir-note--texte">Chaque opération a une catégorie. Les prochains imports suivront tes règles.</p>
      <a class="tir-btn tir-btn--plein" href={hrefs.tirelire('operations')}>Voir les opérations</a>
    </section>
  {/if}
{:else if tirelire.erreurClassement}
  <section class="tir-carte tir-etat" role="alert">
    <span class="tir-pas tir-pas--grand tir-pas--danger"><Icon name="alerte" size={22} /></span>
    <h2 class="tir-h2">La file ne se charge pas</h2>
    <p class="tir-note tir-note--texte">{tirelire.erreurClassement}</p>
    <button class="tir-btn tir-btn--plein" onclick={() => void Promise.all([tirelire.chargerReference(), tirelire.chargerFile()])}><Icon name="actualise" size={16} />Réessayer</button>
  </section>
{:else}
  <div class="tir-etat" role="status"><Loader text="Chargement de la file…" /></div>
{/if}
