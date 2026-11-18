<script lang="ts">
  import { onMount } from 'svelte'
  import Icon, { NOMS_ICONES, traceIcone, type IconName } from '../../components/Icon.svelte'
  import Loader from '../../components/Loader.svelte'
  import { router, hrefs } from '../../lib/route.svelte'
  import { notify } from '../../lib/ui'
  import { choisirExploit, choisirStat, cleDepuisNom, estSerie, exploitDe, formulaireDe, formulaireVide, ICONES_SUGGEREES, MAX_PALIERS, NOM_DOMAINE_COURT, NOM_GRAIN, statDe, UNITE_SAISIE, uniteDe, versRequete, type Erreurs, type Formulaire } from './admin'
  import { messageDe } from './admin.svelte'
  import { adminApi, type Catalogue, type GenreTrophee, type GrainTrophee, type TropheeAdmin } from './sablier'
  import SablierErreur from './SablierErreur.svelte'

  let { id }: { id: number | null } = $props()
  let catalogue = $state.raw<Catalogue | null>(null)
  let original = $state.raw<TropheeAdmin | null>(null)
  let f = $state<Formulaire | null>(null)
  let erreurs = $state.raw<Erreurs>({})
  let chargement = $state('')
  let refus = $state('')
  let envoi = $state(false)
  let toutes = $state(false)
  let cleLibre = $state(true)

  async function charger() {
    chargement = ''
    try {
      const [c, liste] = await Promise.all([adminApi.catalogue(), id === null ? Promise.resolve([]) : adminApi.trophees()])
      catalogue = c
      if (id === null) { f = formulaireVide(c); return }
      const t = liste.find((x) => x.id === id)
      if (!t) { chargement = 'Ce trophée n’existe pas.'; return }
      original = t
      f = formulaireDe(t, c)
    } catch (e) {
      chargement = messageDe(e)
    }
  }
  onMount(() => { void charger() })

  const retour = hrefs.stats('admin', 'trophees')
  const unite = $derived(f ? uniteDe(catalogue, f) : 'count')
  const exploit = $derived(f ? exploitDe(catalogue, f.exploit) : undefined)
  const icones = $derived<IconName[]>(toutes ? NOMS_ICONES : [...new Set([...(f && traceIcone(f.icone as IconName) ? [f.icone as IconName] : []), ...ICONES_SUGGEREES])])

  function nommer(nom: string) {
    if (!f) return
    f.nom = nom
    if (id === null && cleLibre) f.cle = cleDepuisNom(nom)
  }

  async function enregistrerTrophee(e: SubmitEvent) {
    e.preventDefault()
    if (!f || envoi) return
    refus = ''
    const r = versRequete($state.snapshot(f), catalogue, NOMS_ICONES, original)
    if (!r.ok) { erreurs = r.erreurs; return }
    erreurs = {}
    if (original && !Object.keys(r.corps).length) { router.go(retour); return }
    envoi = true
    try {
      const t = original ? await adminApi.modifier(original.id, r.corps) : await adminApi.creer(r.corps)
      notify.ok(original ? `« ${t.name} » est modifié.` : `« ${t.name} » est créé.`)
      router.go(retour)
    } catch (err) {
      refus = messageDe(err)
    } finally {
      envoi = false
    }
  }
</script>

<section class="sab-sec" aria-label={id === null ? 'Créer un trophée' : 'Modifier un trophée'}>
  <div class="sab-ligne"><a class="plbls-button plbls-button--contour plbls-button--compact" href={retour}><Icon name="gauche" size={16} />Trophées</a></div>
  {#if chargement}
    <SablierErreur message={chargement} onretry={() => void charger()} />
  {:else if !f}
    <div class="sab-attente"><Loader text="Ouverture du trophée…" /></div>
  {:else}
    <h2 class="sab-h2">{original ? `Modifier « ${original.name} »` : 'Nouveau trophée'}</h2>
    <form class="sab-sec" novalidate onsubmit={enregistrerTrophee}>
      <div class="sab-g sab-g2">
        <div class="sab-card">
          <div class="plbls-field">
            <label class="plbls-field__label" for="sab-t-nom">Nom</label>
            <input id="sab-t-nom" class="plbls-input" class:plbls-input--error={erreurs.nom} type="text" maxlength="60" value={f.nom} oninput={(e) => nommer(e.currentTarget.value)} aria-invalid={!!erreurs.nom} aria-describedby={erreurs.nom ? 'sab-e-nom' : undefined} />
            {#if erreurs.nom}<p class="plbls-field__message" id="sab-e-nom">{erreurs.nom}</p>{/if}
          </div>
          <div class="plbls-field">
            <label class="plbls-field__label" for="sab-t-desc">Description</label>
            <input id="sab-t-desc" class="plbls-input" class:plbls-input--error={erreurs.description} type="text" maxlength="200" bind:value={f.description} aria-invalid={!!erreurs.description} aria-describedby={erreurs.description ? 'sab-e-desc' : undefined} />
            {#if erreurs.description}<p class="plbls-field__message" id="sab-e-desc">{erreurs.description}</p>{/if}
          </div>
          {#if !original}
            <div class="plbls-field">
              <label class="plbls-field__label" for="sab-t-cle">Clé</label>
              <input id="sab-t-cle" class="plbls-input" class:plbls-input--error={erreurs.cle} type="text" maxlength="60" autocapitalize="none" spellcheck="false" bind:value={f.cle} oninput={() => (cleLibre = false)} aria-invalid={!!erreurs.cle} aria-describedby="sab-a-cle" />
              <p class="plbls-field__aide" id="sab-a-cle">{erreurs.cle ?? 'Identifiant stable du trophée, il ne se modifie plus ensuite.'}</p>
            </div>
          {/if}
          <div class="plbls-field">
            <span class="plbls-field__label" id="sab-t-icone">Icône</span>
            <div class="sab-ligne" role="group" aria-labelledby="sab-t-icone">
              {#each icones as nom (nom)}
                <button class="v-ib sab-icone" type="button" aria-label="Icône {nom}" title={nom} aria-pressed={f.icone === nom} onclick={() => { if (f) f.icone = nom }}><Icon name={nom} size={22} /></button>
              {/each}
              <button class="plbls-button plbls-button--contour plbls-button--compact" type="button" aria-pressed={toutes} onclick={() => (toutes = !toutes)}>{toutes ? 'Moins d’icônes' : 'Toutes les icônes'}</button>
            </div>
            {#if erreurs.icone}<p class="plbls-field__message">{erreurs.icone}</p>{/if}
          </div>
          <label class="sab-reg sab-reg--trait plbls-toggle"><span class="sab-col sab-grow plbls-toggle__texte" style:gap="0"><b>Trophée caché</b><span class="sab-note">Invisible tant qu’il n’est pas débloqué.</span></span><input class="plbls-toggle__input" type="checkbox" role="switch" aria-label="Trophée caché" bind:checked={f.cache} /><span class="plbls-toggle__piste"></span></label>
          <label class="sab-reg sab-reg--trait plbls-toggle"><span class="sab-col sab-grow plbls-toggle__texte" style:gap="0"><b>Actif</b><span class="sab-note">Désactivé, il n’est plus affiché à personne ; il revient s’il est réactivé.</span></span><input class="plbls-toggle__input" type="checkbox" role="switch" aria-label="Actif" bind:checked={f.actif} /><span class="plbls-toggle__piste"></span></label>
        </div>
        <div class="sab-card">
          {#if !original}
            <div class="plbls-field">
              <span class="plbls-field__label" id="sab-t-genre">Genre</span>
              <div class="v-seg sab-onglets" role="group" aria-labelledby="sab-t-genre">
                {#each [['tiers', 'Paliers'], ['exploit', 'Exploit codé']] as [g, label] (g)}
                  <button type="button" aria-pressed={f.genre === g} onclick={() => { if (f) f.genre = g as GenreTrophee }}>{label}</button>
                {/each}
              </div>
            </div>
          {/if}
          {#if f.genre === 'tiers'}
            <div class="plbls-field">
              <label class="plbls-field__label" for="sab-t-stat">Stat du catalogue</label>
              <select id="sab-t-stat" class="plbls-select" value={f.stat} onchange={(e) => { if (f) f = choisirStat($state.snapshot(f), catalogue, e.currentTarget.value) }}>
                {#each (catalogue?.stats ?? []).filter((s) => s.tiers) as s (s.key)}<option value={s.key}>{NOM_DOMAINE_COURT[s.domain]} · {s.label.toLowerCase()}</option>{/each}
              </select>
              {#if erreurs.stat}<p class="plbls-field__message">{erreurs.stat}</p>{/if}
            </div>
            {#if !estSerie(f.stat)}
              <div class="plbls-field">
                <label class="plbls-field__label" for="sab-t-periode">Période</label>
                <select id="sab-t-periode" class="plbls-select" value={f.periode} onchange={(e) => { if (f) f.periode = e.currentTarget.value as GrainTrophee }}>
                  {#each catalogue?.periods ?? [] as p (p)}<option value={p}>{NOM_GRAIN[p]}</option>{/each}
                </select>
                {#if erreurs.periode}<p class="plbls-field__message">{erreurs.periode}</p>{/if}
              </div>
            {/if}
            <fieldset class="sab-fieldset">
              <legend class="plbls-field__label">Seuils par palier{UNITE_SAISIE[statDe(catalogue, f.stat)?.unit ?? 'count'] ? `, ${UNITE_SAISIE[statDe(catalogue, f.stat)?.unit ?? 'count']}` : ''}</legend>
              <div class="sab-g sab-g3 sab-g--serre">
                {#each f.seuils as _, i (i)}
                  <div class="plbls-field">
                    <label class="plbls-field__label" for="sab-t-seuil-{i}">Palier {i + 1}</label>
                    <input id="sab-t-seuil-{i}" class="plbls-input" class:plbls-input--error={erreurs.seuils} type="text" inputmode={unite === 'seconds' ? 'decimal' : 'numeric'} bind:value={f.seuils[i]} aria-invalid={!!erreurs.seuils} />
                  </div>
                {/each}
              </div>
              {#if erreurs.seuils}<p class="plbls-field__message" role="alert">{erreurs.seuils}</p>{/if}
              <div class="sab-ligne">
                {#if f.seuils.length < MAX_PALIERS}<button class="plbls-button plbls-button--contour plbls-button--compact" type="button" onclick={() => f?.seuils.push('')}><Icon name="plus" size={16} />Ajouter un palier</button>{/if}
                {#if f.seuils.length > 1}<button class="plbls-button plbls-button--contour plbls-button--compact" type="button" onclick={() => f?.seuils.pop()}><Icon name="minus" size={16} />Retirer le dernier</button>{/if}
              </div>
            </fieldset>
            {#if original}<div class="sab-alerte" role="note"><Icon name="trophee" size={18} /><p>Un seuil abaissé débloque à l’enregistrement les personnes dont l’historique l’atteint ; un seuil relevé ne retire rien. Un trophée déjà débloqué le reste.</p></div>{/if}
          {:else}
            <div class="plbls-field">
              <label class="plbls-field__label" for="sab-t-exploit">Exploit codé</label>
              <select id="sab-t-exploit" class="plbls-select" value={f.exploit} onchange={(e) => { if (f) f = choisirExploit($state.snapshot(f), catalogue, e.currentTarget.value) }}>
                {#each catalogue?.exploits ?? [] as x (x.key)}<option value={x.key}>{NOM_DOMAINE_COURT[x.domain]} · {x.label.toLowerCase()}</option>{/each}
              </select>
              {#if exploit}<p class="plbls-field__aide">Condition : {exploit.condition}.</p>{/if}
              {#if erreurs.exploit}<p class="plbls-field__message">{erreurs.exploit}</p>{/if}
            </div>
            {#if exploit && exploit.default_parameter !== null}
              <div class="plbls-field">
                <label class="plbls-field__label" for="sab-t-param">Paramètre{UNITE_SAISIE[exploit.unit] ? `, ${UNITE_SAISIE[exploit.unit]}` : ''}</label>
                <input id="sab-t-param" class="plbls-input" class:plbls-input--error={erreurs.parametre} type="text" inputmode={exploit.unit === 'seconds' ? 'decimal' : 'numeric'} bind:value={f.parametre} aria-invalid={!!erreurs.parametre} />
                {#if erreurs.parametre}<p class="plbls-field__message" role="alert">{erreurs.parametre}</p>{/if}
              </div>
            {/if}
          {/if}
        </div>
      </div>
      {#if refus}<div class="plbls-alert plbls-alert--erreur" role="alert"><p class="plbls-alert__corps">Le serveur a refusé : {refus}</p></div>{/if}
      <div class="sab-ligne sab-ligne--fin">
        <a class="plbls-button plbls-button--contour" href={retour}>Annuler</a>
        <button class="plbls-button" type="submit" disabled={envoi}>{envoi ? 'Enregistrement…' : 'Enregistrer'}</button>
      </div>
    </form>
  {/if}
</section>
