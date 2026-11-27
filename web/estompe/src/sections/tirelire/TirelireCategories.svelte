<script lang="ts">
  import { onMount, untrack } from 'svelte'
  import Icon from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { confirmer } from '../../lib/confirm.svelte'
  import { hrefs } from '../../lib/route.svelte'
  import { centimes, euros, pluriel, saisieEuros } from './formats'
  import type { Volet } from './onglets'
  import TirelirePastille from './TirelirePastille.svelte'
  import { actives, budget, conditionDe, deplacer, filtres, motCle, NATURES, NATURES_LONGUES, ORDRE_NATURES, teinte, type Categorie, type Commercant, type Nature, type Regle } from './tirelire'
  import { tirelire } from './tirelire.svelte'

  let { volet }: { volet: Volet } = $props()

  const VOLETS: { id: Volet; label: string }[] = [{ id: 'categories', label: 'Catégories' }, { id: 'regles', label: 'Règles' }, { id: 'commercants', label: 'Commerçants' }]
  const NOUVEAU = 0

  let edition = $state<number | null>(null)
  let nom = $state('')
  let nature = $state<Nature>('variable')
  let plafond = $state('')
  let regle = $state<number | null>(null)
  let mots = $state<string[]>([])
  let mot = $state('')
  let cible = $state('')
  let info = $state('')
  let recherche = $state('')
  let choisis = $state<number[]>([])
  let ouvert = $state<Commercant | null>(null)

  const categories = $derived(tirelire.categories ?? [])
  const proposees = $derived(actives(categories))
  const plafondLu = $derived(plafond.trim() ? centimes(plafond) : null)
  const plafondFaux = $derived(!!plafond.trim() && plafondLu === null)
  const listes = $derived((tirelire.commercants ?? []).filter((c) => !recherche.trim() || c.name.toLowerCase().includes(recherche.trim().toLowerCase())))

  onMount(() => { void tirelire.chargerReference() })

  $effect(() => {
    const v = volet
    untrack(() => {
      info = ''
      if (v === 'regles' && !tirelire.regles) void tirelire.chargerRegles()
      if (v === 'commercants' && !tirelire.commercants) void tirelire.chargerCommercants('')
    })
  })

  function editer(c: Categorie | null) {
    edition = c?.id ?? NOUVEAU
    nom = c?.name ?? ''
    nature = c?.kind ?? 'variable'
    plafond = saisieEuros(c?.monthly_cap ?? null)
  }

  async function enregistrer() {
    if (edition === null || !nom.trim() || plafondFaux) return
    if (await tirelire.enregistrerCategorie(edition === NOUVEAU ? null : edition, { name: nom.trim(), kind: nature, monthly_cap: plafondLu })) edition = null
  }

  async function archiver(c: Categorie) {
    if (!c.archived) {
      const ok = await confirmer({ titre: `Archiver « ${c.name} » ?`, message: `Elle ne sera plus proposée. ${c.operations ? `${pluriel(c.operations, 'opération garde', 'opérations gardent')} cette catégorie.` : 'Aucune opération ne la porte.'}`, action: 'Archiver' })
      if (!ok) return
    }
    if (await tirelire.enregistrerCategorie(c.id, { archived: !c.archived })) edition = null
  }

  function editerRegle(r: Regle | null) {
    regle = r?.id ?? NOUVEAU
    mots = [...(r?.keywords ?? [])]
    mot = ''
    cible = r ? String(r.category_id) : ''
  }

  function ajouterMot() {
    const propre = motCle(mot)
    if (propre.length >= 2 && !mots.includes(propre)) mots = [...mots, propre]
    mot = ''
  }

  const reclassees = (n: number) => (n ? `${pluriel(n, 'opération reclassée', 'opérations reclassées')}.` : 'Aucune opération ne change de catégorie.')

  async function enregistrerRegle(r: Regle | null) {
    if (mot.trim()) ajouterMot()
    if (!cible || (!mots.length && !(r?.type || r?.section))) return
    const n = await tirelire.enregistrerRegle(r?.id ?? null, { keywords: mots, category_id: Number(cible) })
    if (n === null) return
    regle = null
    info = `Règle enregistrée. ${reclassees(n)}`
  }

  async function supprimerRegle(r: Regle) {
    const ok = await confirmer({ titre: 'Supprimer cette règle ?', message: `${r.operations ? `${pluriel(r.operations, 'opération qu’elle classe sera reclassée', 'opérations qu’elle classe seront reclassées')} par les règles suivantes, ou reviendront à classer.` : 'Elle ne classe aucune opération pour l’instant.'}`, action: 'Supprimer', destructif: true })
    if (!ok) return
    const n = await tirelire.supprimerRegle(r.id)
    if (n === null) return
    regle = null
    info = `Règle supprimée. ${reclassees(n)}`
  }

  async function monter(i: number, pas: number) {
    const liste = tirelire.regles ?? []
    await tirelire.ordonnerRegles(deplacer(liste, i, i + pas).map((r) => r.id))
  }

  function chercher() { void tirelire.chargerCommercants(recherche.trim()) }

  function cocher(id: number, oui: boolean) { choisis = oui ? [...choisis, id] : choisis.filter((x) => x !== id) }

  async function regrouper() {
    const liste = (tirelire.commercants ?? []).filter((c) => choisis.includes(c.id))
    if (liste.length < 2) return
    const [garde, ...autres] = [...liste].sort((a, b) => b.operations - a.operations)
    const ok = await confirmer({ titre: `Regrouper ${liste.length} commerçants ?`, message: `Ils deviennent « ${garde.name} ». Les prochains imports suivront ce regroupement ; tu pourras dissocier une variante.`, action: 'Regrouper' })
    if (!ok) return
    if (await tirelire.fusionner(garde.id, autres.map((c) => c.id))) {
      choisis = []
      info = `${liste.length} commerçants regroupés sous « ${garde.name} ».`
      ouvert = null
      await ouvrir(garde.id)
    }
  }

  async function ouvrir(id: number) {
    if (ouvert?.id === id) { ouvert = null; return }
    try { ouvert = await tirelire.lireCommercant(id) } catch { ouvert = null }
  }

  async function dissocier(c: Commercant, cle: string) {
    if (await tirelire.separer(c.id, cle)) {
      info = `« ${cle} » est de nouveau un commerçant à part.`
      try { ouvert = await tirelire.lireCommercant(c.id) } catch { ouvert = null }
    }
  }

  async function classer(c: Commercant, valeur: string) {
    const n = await tirelire.classerCommercant(c.id, valeur ? Number(valeur) : null)
    if (n === null) return
    info = reclassees(n)
    if (ouvert?.id === c.id) try { ouvert = await tirelire.lireCommercant(c.id) } catch { ouvert = null }
  }
</script>

{#snippet choixCategorie(valeur: string, change: (v: string) => void, vide: string, etiquette: string)}
  <span class="tir-champ">
    <select aria-label={etiquette} value={valeur} onchange={(e) => change(e.currentTarget.value)} disabled={tirelire.occupe}>
      <option value="">{vide}</option>
      {#each ORDRE_NATURES as n (n)}
        {#if proposees.some((c) => c.kind === n)}
          <optgroup label={NATURES_LONGUES[n]}>
            {#each proposees.filter((c) => c.kind === n) as c (c.id)}<option value={String(c.id)}>{c.name}</option>{/each}
          </optgroup>
        {/if}
      {/each}
    </select>
    <Icon name="bas" size={16} />
  </span>
{/snippet}

<nav class="tir-seg" aria-label="Catégories, règles et commerçants">
  {#each VOLETS as v (v.id)}
    <a href={hrefs.tirelire('categories', v.id === 'categories' ? undefined : v.id)} aria-current={volet === v.id ? 'page' : undefined}>{v.label}</a>
  {/each}
</nav>

{#if tirelire.erreurClassement}<p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />{tirelire.erreurClassement}</p>{/if}
{#if info}<p class="tir-fait" role="status"><Icon name="ok" size={18} /><span class="tir-note tir-note--texte">{info}</span></p>{/if}

{#if !tirelire.categories}
  {#if !tirelire.erreurClassement}<div class="tir-etat" role="status"><Loader text="Chargement des catégories…" /></div>{/if}
{:else if volet === 'categories'}
  {#snippet formulaire(c: Categorie | null)}
    <form class="tir-edite" aria-label={c ? `Modifier ${c.name}` : 'Nouvelle catégorie'} onsubmit={(e) => { e.preventDefault(); void enregistrer() }}>
      <label class="tir-fl"><span>Nom</span><span class="tir-champ"><input type="text" maxlength="40" bind:value={nom} /></span></label>
      <div class="tir-g2">
        <label class="tir-fl"><span>Nature</span><span class="tir-champ"><select bind:value={nature}>{#each ORDRE_NATURES as n (n)}<option value={n}>{NATURES_LONGUES[n]}</option>{/each}</select><Icon name="bas" size={16} /></span></label>
        <label class="tir-fl"><span>Plafond mensuel (€)</span><span class="tir-champ"><input type="text" inputmode="decimal" placeholder="aucun" bind:value={plafond} /></span></label>
      </div>
      {#if plafondFaux}<p class="tir-alerte" role="alert"><Icon name="alerte" size={18} />Plafond : un montant en euros, par exemple 150 ou 150,50.</p>{/if}
      <div class="tir-actions">
        {#if c}<button type="button" class="tir-btn tir-btn--contour" disabled={tirelire.occupe} onclick={() => void archiver(c)}>{c.archived ? 'Désarchiver' : 'Archiver'}</button>{/if}
        <button type="button" class="tir-btn tir-btn--contour" onclick={() => (edition = null)}>Annuler</button>
        <button class="tir-btn tir-btn--plein tir-btn--grandit" disabled={tirelire.occupe || !nom.trim() || plafondFaux}>Enregistrer</button>
      </div>
    </form>
  {/snippet}

  <section class="tir-carte tir-carte--liste" aria-label="Catégories">
    <div class="tir-ligne tir-ligne--ecart" style:padding="12px 0 4px"><h2 class="tir-h2">Catégories</h2><span class="tir-note">plafond mensuel</span></div>
    <ul class="tir-liste">
      {#each categories as c (c.id)}
        <li>
          {#if edition === c.id}
            <div style:padding="8px 0">{@render formulaire(c)}</div>
          {:else}
            <div class="tir-li" style:opacity={c.archived ? 0.6 : undefined}>
              <span class="tir-grandit tir-ligne"><span class="tir-pt tir-pt--{teinte(c)}" aria-hidden="true"></span><span class="tir-h3" style:overflow-wrap="break-word">{c.name}</span></span>
              <span class="tir-etq" class:tir-etq--info={c.kind === 'fixe'} class:tir-etq--ok={c.kind === 'epargne'}>{c.archived ? 'archivée' : NATURES[c.kind]}</span>
              <span class="tir-fort tir-fixe" style:min-width="64px" style:text-align="right">{c.monthly_cap === null ? '—' : euros(c.monthly_cap)}</span>
              <button class="tir-ib" aria-label="Modifier {c.name}" onclick={() => editer(c)}><Icon name="crayon" size={18} /></button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
    <dl class="tir-liste"><div class="tir-kv"><dt class="tir-fort" style:color="var(--encre)">Budget du mois</dt><dd class="tir-fixe">{euros(budget(categories))}</dd></div></dl>
    <p class="tir-note">Somme des plafonds des dépenses fixes et variables. L’épargne, les revenus et les transferts neutres n’en font pas partie.</p>
    {#if edition === NOUVEAU}
      <div style:padding-top="8px">{@render formulaire(null)}</div>
    {:else}
      <button class="tir-btn tir-btn--large" style:margin-top="8px" onclick={() => editer(null)}><Icon name="plus" size={16} />Nouvelle catégorie</button>
    {/if}
  </section>
{:else if volet === 'regles'}
  {#snippet formulaireRegle(r: Regle | null)}
    <form class="tir-edite" aria-label={r ? 'Modifier la règle' : 'Nouvelle règle'} onsubmit={(e) => { e.preventDefault(); void enregistrerRegle(r) }}>
      <div class="tir-ligne tir-ligne--ecart"><h3 class="tir-h3">{r ? 'Modifier la règle' : 'Nouvelle règle'}</h3>{#if r}<span class="tir-note">{pluriel(r.operations, 'opération', 'opérations')}</span>{/if}</div>
      {#if r && conditionDe(r)}<p class="tir-note tir-note--texte">Condition fixe : {conditionDe(r)}.</p>{/if}
      <span class="tir-note">Mots-clés (l’un ou l’autre), cherchés en mots entiers dans le libellé</span>
      {#if mots.length}
        <div class="tir-mots">
          {#each mots as m (m)}<span class="tir-mc tir-mc--retire">{m}<button type="button" class="tir-ib" aria-label="Retirer {m}" onclick={() => (mots = mots.filter((x) => x !== m))}><Icon name="x" size={16} /></button></span>{/each}
        </div>
      {/if}
      <div class="tir-ligne">
        <span class="tir-champ tir-grandit"><input type="text" aria-label="Ajouter un mot-clé" placeholder="Ajouter un mot-clé" maxlength="60" bind:value={mot} onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); ajouterMot() } }} /></span>
        <button type="button" class="tir-btn" disabled={motCle(mot).length < 2} onclick={ajouterMot}>Ajouter</button>
      </div>
      <label class="tir-fl"><span>Catégorie</span>{@render choixCategorie(cible, (v) => (cible = v), 'Choisir…', 'Catégorie de la règle')}</label>
      <p class="tir-note">La règle s’applique aussi aux opérations déjà importées.</p>
      <div class="tir-actions">
        {#if r}<button type="button" class="tir-btn tir-btn--contour" disabled={tirelire.occupe} onclick={() => void supprimerRegle(r)}>Supprimer</button>{/if}
        <button type="button" class="tir-btn tir-btn--contour" onclick={() => (regle = null)}>Annuler</button>
        <button class="tir-btn tir-btn--plein tir-btn--grandit" disabled={tirelire.occupe || !cible || (!mots.length && motCle(mot).length < 2 && !(r?.type || r?.section))}>Enregistrer</button>
      </div>
    </form>
  {/snippet}

  <section class="tir-carte" aria-label="Règles par mots-clés">
    <div class="tir-col tir-col--colle"><h2 class="tir-h2">Règles par mots-clés</h2><span class="tir-note">le libellé contient… → catégorie. La première règle qui convient l’emporte ; la catégorie choisie pour un commerçant passe avant.</span></div>
    {#if regle === NOUVEAU}
      {@render formulaireRegle(null)}
    {:else}
      <button class="tir-btn tir-btn--large" onclick={() => editerRegle(null)}><Icon name="plus" size={16} />Nouvelle règle</button>
    {/if}
    {#if !tirelire.regles}
      <div class="tir-etat" role="status"><Loader text="Chargement des règles…" /></div>
    {:else}
      <ol class="tir-liste" aria-label="Règles, par ordre de priorité">
        {#each tirelire.regles as r, i (r.id)}
          <li>
            {#if regle === r.id}
              <div style:padding="8px 0">{@render formulaireRegle(r)}</div>
            {:else}
              <div class="tir-rang">
                <div class="tir-col tir-grandit">
                  <div class="tir-mots">
                    {#if conditionDe(r)}<span class="tir-etq tir-etq--info" style:white-space="normal" style:height="auto" style:min-height="24px">{conditionDe(r)}</span>{/if}
                    {#each r.keywords as m (m)}<span class="tir-mc">{m}</span>{/each}
                  </div>
                  <span class="tir-sous"><span class="tir-note">→</span><TirelirePastille categorie={tirelire.categorie(r.category_id)} /><span class="tir-note tir-fixe">· {pluriel(r.operations, 'opération', 'opérations')}</span></span>
                </div>
                <div class="tir-ligne" style:gap="0">
                  <button class="tir-ib" aria-label="Monter la règle {i + 1}" disabled={i === 0 || tirelire.occupe} onclick={() => void monter(i, -1)}><Icon name="up" size={18} /></button>
                  <button class="tir-ib" aria-label="Descendre la règle {i + 1}" disabled={i === tirelire.regles.length - 1 || tirelire.occupe} onclick={() => void monter(i, 1)}><Icon name="bas" size={18} /></button>
                  <button class="tir-ib" aria-label="Modifier la règle {i + 1}" onclick={() => editerRegle(r)}><Icon name="crayon" size={18} /></button>
                </div>
              </div>
            {/if}
          </li>
        {/each}
      </ol>
    {/if}
  </section>
{:else}
  <section class="tir-carte" aria-label="Commerçants">
    <div class="tir-col tir-col--colle"><h2 class="tir-h2">Variantes d’un commerçant</h2><span class="tir-note">Plusieurs libellés, un seul commerçant : coche ceux qui vont ensemble, puis regroupe-les. Sur un relevé, la ville fait partie du libellé : deux villes donnent deux commerçants.</span></div>
    <form class="tir-ligne" role="search" onsubmit={(e) => { e.preventDefault(); chercher() }}>
      <span class="tir-champ tir-grandit"><Icon name="cherche" size={18} /><input type="search" aria-label="Chercher un commerçant" placeholder="Chercher un commerçant" bind:value={recherche} /></span>
      <button class="tir-btn">Chercher</button>
    </form>
    {#if choisis.length}
      <div class="tir-ligne tir-ligne--plis">
        <button class="tir-btn tir-btn--plein tir-btn--grandit" disabled={choisis.length < 2 || tirelire.occupe} onclick={() => void regrouper()}>{choisis.length < 2 ? 'Coche un autre commerçant' : `Regrouper ${choisis.length} commerçants`}</button>
        <button class="tir-btn tir-btn--contour" onclick={() => (choisis = [])}>Ignorer</button>
      </div>
    {/if}
    {#if !tirelire.commercants}
      <div class="tir-etat" role="status"><Loader text="Chargement des commerçants…" /></div>
    {:else if !listes.length}
      <p class="tir-note tir-note--texte">Aucun commerçant{recherche.trim() ? ' pour cette recherche' : ' : importe un relevé pour en voir'}.</p>
    {:else}
      <ul class="tir-liste" aria-label="Liste des commerçants">
        {#each listes as c (c.id)}
          <li>
            <div class="tir-li">
              <label class="tir-case" style:min-width="44px" style:justify-content="center"><input type="checkbox" aria-label="Sélectionner {c.name}" checked={choisis.includes(c.id)} onchange={(e) => cocher(c.id, e.currentTarget.checked)} /></label>
              <button class="tir-li tir-li--bouton tir-grandit" style:padding="0" style:border-top="0" aria-expanded={ouvert?.id === c.id} onclick={() => void ouvrir(c.id)}>
                <span class="tir-col tir-col--serre tir-grandit"><span class="tir-h3">{c.name}</span><span class="tir-sous"><TirelirePastille categorie={tirelire.categorie(c.category_id)} /><span class="tir-note tir-fixe">{pluriel(c.operations, 'opération', 'opérations')}{c.variants > 1 ? ` · ${c.variants} variantes` : ''}</span></span></span>
                <Icon name={ouvert?.id === c.id ? 'up' : 'bas'} size={18} />
              </button>
            </div>
            {#if ouvert?.id === c.id}
              <div class="tir-col" style:padding="0 0 12px" style:gap="10px">
                <label class="tir-fl"><span>Catégorie de tout le commerçant</span>{@render choixCategorie(ouvert.category_id === null ? '' : String(ouvert.category_id), (v) => void classer(c, v), 'Aucune : suivre les règles', `Catégorie de ${c.name}`)}</label>
                <span class="tir-note">{pluriel(ouvert.variant_list?.length ?? 0, 'variante', 'variantes regroupées')} · {pluriel(ouvert.operations, 'opération', 'opérations')}</span>
                {#each ouvert.variant_list ?? [] as v (v.key)}
                  <div class="tir-ligne tir-ligne--ecart">
                    <span class="tir-mc">{v.key}</span>
                    {#if (ouvert.variant_list?.length ?? 0) > 1}<button class="tir-btn tir-btn--contour" disabled={tirelire.occupe} aria-label="Dissocier {v.key}" onclick={() => void dissocier(c, v.key)}>Dissocier</button>{/if}
                  </div>
                {/each}
                <a class="tir-btn tir-btn--large" href={hrefs.tirelire('operations')} onclick={() => void tirelire.filtrer(filtres({ commercant: c.id }))}>Voir ses opérations</a>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}
