<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import { menuFrom } from '../../lib/ctxmenu.svelte'
  import { notify } from '../../lib/ui'
  import Icon from '../Icon.svelte'
  import Attente from './Attente.svelte'
  import {
    charger, compterEffectifs, ecrire, enc, ETATS, executer, FILTRES_EFFECTIFS, ICONES_ETAT, initiales, libellesPermissions, nomCompte, origine, quand,
    resumeEffectif, teinte, typeDe, versIso,
    type Catalogue, type Compte, type CompteDetail, type Decision, type EffectifsExpliques, type EntreeJournal, type Explication, type FiltreEffectifs,
    type GroupeResume, type LiaisonVue, type RegleVue,
  } from '../../lib/acces.svelte'
  import ChoixCompte from './ChoixCompte.svelte'
  import ChoixRessource from './ChoixRessource.svelte'
  import FormLiaison from './FormLiaison.svelte'
  import FormRegle from './FormRegle.svelte'
  import LigneJournal from './LigneJournal.svelte'
  import NomRessource from './NomRessource.svelte'

  let { sub, catalogue, onChange, onBack }: { sub: string; catalogue: Catalogue; onChange: () => void; onBack?: () => void } = $props()
  type Ajout = 'role' | 'groupe' | 'regle' | 'copie'
  let vue = $state<'droits' | 'historique'>('droits')
  let ajout = $state<Ajout | null>(null)
  let filtre = $state<FiltreEffectifs>('distingue')
  const detail = charger<CompteDetail>(() => `/accounts/${enc(sub)}`)
  const effectifs = charger<EffectifsExpliques>(() => `/accounts/${enc(sub)}/effective`)
  const journal = charger<EntreeJournal[]>(() => (vue === 'historique' ? `/audit?subject=${enc(sub)}&limit=200` : null))
  const groupes = charger<GroupeResume[]>(() => (ajout === 'groupe' ? '/groups' : null))
  const noms = $derived(libellesPermissions(catalogue))
  const recharger = () => { detail.reload(); effectifs.reload(); onChange() }
  const basculer = (a: Ajout) => (ajout = ajout === a ? null : a)

  const permissions = $derived(effectifs.data?.permissions ?? [])
  const compteurs = $derived(compterEffectifs(permissions))
  const parApp = $derived.by(() => {
    const garde = FILTRES_EFFECTIFS[filtre].garde
    const vus = new Set<string>()
    const groupesApp = catalogue.apps.map((a) => ({
      cle: a.key,
      nom: a.name,
      lignes: a.permissions.flatMap((p) => { const e = permissions.find((x) => x.permission === p.name); if (e) vus.add(e.permission); return e && garde(e) ? [e] : [] }),
    }))
    const autres = permissions.filter((e) => !vus.has(e.permission) && garde(e))
    return [...groupesApp, { cle: '', nom: 'Autres', lignes: autres }].filter((g) => g.lignes.length)
  })
  const heritees = $derived(compteurs.tout - compteurs.distingue)

  let ouverte = $state<string | null>(null)
  let essaiRes = $state<{ id: string; name: string } | null>(null)
  let decision = $state.raw<Decision | null>(null)
  const simuler = async (permission: string, resource: string | null) => {
    decision = null
    const d = await executer(ecrire<Decision>('POST', '/simulate', { sub, permission, resource }))
    if (ouverte === permission) decision = d
  }
  const pourquoi = (e: Explication) => {
    essaiRes = null
    decision = null
    ouverte = ouverte === e.permission ? null : e.permission
    if (ouverte) void simuler(e.permission, null)
  }

  let source = $state<Compte | null>(null)
  let mode = $state<'ajouter' | 'remplacer'>('ajouter')
  const copier = async () => {
    if (!source) return
    const essai = await executer(ecrire<{ bindings: number; groups: number; rules: number; removed: number }>('POST', `/accounts/${enc(sub)}/copy`, { from: source.sub, mode, dry_run: true }))
    if (!essai) return
    const msg = `${essai.bindings} rôle(s), ${essai.groups} groupe(s) et ${essai.rules} règle(s) de ${nomCompte(source)} seront copiés${mode === 'remplacer' ? `, après retrait des ${essai.removed} droit(s) actuels` : ''}.`
    if (!(await confirmer({ titre: 'Copier les droits', message: msg, action: 'Copier', destructif: mode === 'remplacer' }))) return
    if ((await executer(ecrire('POST', `/accounts/${enc(sub)}/copy`, { from: source.sub, mode }), 'Droits copiés')) !== null) { source = null; ajout = null; recharger() }
  }
  const purger = async () => {
    if (!(await confirmer({ titre: 'Retirer tous les droits', message: `Retirer à ${nomCompte(detail.data?.account, sub)} tous ses rôles, groupes et règles directes ? Il gardera ce que donne « Tout le monde ».`, action: 'Tout retirer', destructif: true }))) return
    if ((await executer(ecrire('DELETE', `/accounts/${enc(sub)}/grants`), 'Droits retirés')) !== null) recharger()
  }
  const quitterGroupe = async (gid: string, nom: string) => {
    if (!(await confirmer({ titre: 'Retirer du groupe', message: `Retirer ce compte du groupe « ${nom} » ?`, action: 'Retirer', destructif: true }))) return
    if ((await executer(ecrire('DELETE', `/groups/${gid}/members/${enc(sub)}`), 'Retiré du groupe')) !== null) recharger()
  }
  const retirerRole = async (l: LiaisonVue) => {
    if (!(await confirmer({ titre: 'Retirer le rôle', message: `Retirer le rôle « ${l.role} » à ce compte ?`, action: 'Retirer', destructif: true }))) return
    if ((await executer(ecrire('DELETE', `/bindings/${l.id}`), 'Rôle retiré')) !== null) recharger()
  }
  const supprimerRegle = async (r: RegleVue) => {
    const nom = noms.get(r.permission)?.label ?? r.permission
    if (!(await confirmer({ titre: 'Supprimer la règle', message: `${r.effect === 'allow' ? 'Retirer' : 'Lever le refus de'} « ${nom} » ?`, action: 'Supprimer', destructif: true }))) return
    if ((await executer(ecrire('DELETE', `/rules/${r.id}`), 'Règle supprimée')) !== null) recharger()
  }
  let groupeChoisi = $state('')
  let groupeExpire = $state('')
  const rejoindre = async () => {
    if ((await executer(ecrire('PUT', `/groups/${groupeChoisi}/members/${enc(sub)}`, { expires_at: versIso(groupeExpire) }), 'Ajouté au groupe')) !== null) { groupeChoisi = ''; groupeExpire = ''; ajout = null; recharger() }
  }
  const copierIdentifiant = () => navigator.clipboard.writeText(sub).then(() => notify.ok('Identifiant copié.'), () => notify.fail('Copie impossible.'))
  const menu = (e: MouseEvent) => menuFrom(e.currentTarget as Element, [
    { label: "Copier l'identifiant", run: () => void copierIdentifiant() },
    { label: 'Tout retirer…', run: () => void purger(), danger: true, sep: true },
  ])
</script>

<section class="v-card perm-carte acc-fiche">
  {#if detail.data === null}
    <Attente error={detail.error} retry={detail.reload} />
  {:else}
    {@const c = detail.data.account}
    {@const d = detail.data}
    <header class="acc-tete">
      {#if onBack}<button class="ib acc-retour" onclick={onBack} aria-label="Retour aux comptes"><Icon name="gauche" /></button>{/if}
      <span class="acc-av acc-av--grand acc-av--{teinte(c.sub)}" aria-hidden="true">{initiales(c)}</span>
      <div class="acc-tete-t">
        <h2 class="perm-t">{nomCompte(c)}</h2>
        <p class="v-note">{c.display_name && c.email ? `${c.display_name} · ` : ''}vu {quand(c.last_seen_at)}{#if !c.in_keycloak} · <span class="v-tag dgr">supprimé de Keycloak</span>{/if}</p>
      </div>
      <div class="acc-tete-a">
        <button class="plbls-button plbls-button--contour plbls-button--compact" aria-expanded={ajout === 'copie'} onclick={() => basculer('copie')}>Copier les droits d'un autre compte</button>
        <button class="plbls-button plbls-button--lien plbls-button--compact" aria-pressed={vue === 'historique'} onclick={() => (vue = vue === 'historique' ? 'droits' : 'historique')}>{vue === 'historique' ? 'Revenir aux droits' : 'Historique'}</button>
        <button class="ib" aria-label="Autres actions" aria-haspopup="menu" onclick={menu}><Icon name="more" /></button>
      </div>
    </header>

    {#if ajout === 'copie'}
      <div class="v-well acc-ajout-zone">
        <h3 class="acc-sur">Copier les droits d'un autre compte</h3>
        {#if source}
          <form class="perm-form" onsubmit={(e) => { e.preventDefault(); void copier() }}>
            <p class="perm-en-ligne">Depuis <b>{nomCompte(source)}</b><button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => (source = null)}>Changer</button></p>
            <div class="plbls-field">
              <label class="plbls-field__label" for="acc-copie-mode">Mode</label>
              <select class="plbls-select" id="acc-copie-mode" bind:value={mode}><option value="ajouter">Ajouter à l'existant</option><option value="remplacer">Remplacer l'existant</option></select>
            </div>
            <button class="plbls-button">Copier…</button>
          </form>
        {:else}
          <ChoixCompte id="acc-copie" onPick={(x) => (source = x.sub === sub ? null : x)} />
        {/if}
      </div>
    {/if}

    {#if vue === 'historique'}
      <h3 class="perm-t2">Historique</h3>
      {#if journal.data === null}<Attente error={journal.error} retry={journal.reload} />
      {:else if journal.data.length === 0}<p class="v-note">Aucune modification pour ce compte.</p>
      {:else}<ul class="acc-journal">{#each journal.data as l (l.id)}<LigneJournal ligne={l} {noms} />{/each}</ul>{/if}
    {:else}
      <section class="v-well acc-origines" aria-labelledby="acc-origines-t">
        <h3 class="acc-sur" id="acc-origines-t">D'où viennent ses droits</h3>
        <ul class="acc-puces-l">
          <li class="acc-puce acc-puce--base">Tout le monde</li>
          {#each d.bindings as l (l.id)}
            <li class="acc-puce acc-puce--role"><span class="acc-puce-k">rôle</span><b>{l.role}</b>{#if l.scope_id}<span class="acc-puce-k">sur <NomRessource type={l.scope_type} id={l.scope_id} /></span>{/if}{#if l.expires_at}<span class="acc-puce-k">jusqu'au {quand(l.expires_at)}</span>{/if}<button class="acc-puce-x" aria-label="Retirer le rôle {l.role}" onclick={() => retirerRole(l)}><Icon name="x" size={12} /></button></li>
          {/each}
          {#each d.groups as g (g.group_id)}
            <li class="acc-puce acc-puce--groupe"><span class="acc-puce-k">groupe</span><b>{g.name}</b>{#if g.expires_at}<span class="acc-puce-k">jusqu'au {quand(g.expires_at)}</span>{/if}<button class="acc-puce-x" aria-label="Retirer du groupe {g.name}" onclick={() => quitterGroupe(g.group_id, g.name)}><Icon name="x" size={12} /></button></li>
          {/each}
          {#each d.group_bindings as l (l.id)}
            <li class="acc-puce acc-puce--herite" title="Hérité du groupe : se règle dans l'onglet Groupes"><span class="acc-puce-k">rôle</span><b>{l.role}</b><span class="acc-puce-k">via {l.subject_label ?? 'un groupe'}</span></li>
          {/each}
          {#each d.rules as r (r.id)}
            <li class="acc-puce" class:acc-puce--refus={r.effect === 'deny'} class:acc-puce--accord={r.effect === 'allow'} class:acc-obsolete={r.obsolete} title={r.note || undefined}><span class="acc-puce-k">{r.effect === 'allow' ? 'accord direct' : 'refus direct'}</span><b>{noms.get(r.permission)?.label ?? r.permission}</b>{#if r.resource !== '*'}<span class="acc-puce-k">sur <NomRessource type={typeDe(noms, r.permission)} id={r.resource} /></span>{/if}{#if r.expires_at}<span class="acc-puce-k">jusqu'au {quand(r.expires_at)}</span>{/if}<button class="acc-puce-x" aria-label="Supprimer la règle directe {noms.get(r.permission)?.label ?? r.permission}" onclick={() => supprimerRegle(r)}><Icon name="x" size={12} /></button></li>
          {/each}
          <li><button class="acc-plus" aria-expanded={ajout === 'role'} onclick={() => basculer('role')}>+ Rôle</button></li>
          <li><button class="acc-plus" aria-expanded={ajout === 'groupe'} onclick={() => basculer('groupe')}>+ Groupe</button></li>
          <li><button class="acc-plus" aria-expanded={ajout === 'regle'} onclick={() => basculer('regle')}>+ Règle directe</button></li>
        </ul>
        {#if ajout === 'role'}
          <FormLiaison {catalogue} sujet="account" id={sub} onDone={() => { ajout = null; recharger() }} />
        {:else if ajout === 'groupe'}
          {#if groupes.data === null}<Attente error={groupes.error} retry={groupes.reload} />
          {:else}
            {@const libres = groupes.data.filter((g) => !d.groups.some((x) => x.group_id === g.id))}
            {#if libres.length === 0}<p class="v-note">Aucun autre groupe : ils se créent dans l'onglet Groupes.</p>
            {:else}
              <form class="perm-form" onsubmit={(e) => { e.preventDefault(); void rejoindre() }}>
                <div class="plbls-field"><label class="plbls-field__label" for="acc-gr">Groupe</label><select class="plbls-select" id="acc-gr" bind:value={groupeChoisi} required><option value="">Choisir…</option>{#each libres as g (g.id)}<option value={g.id}>{g.name}</option>{/each}</select></div>
                <div class="plbls-field"><label class="plbls-field__label" for="acc-gr-x">Jusqu'au (facultatif)</label><input class="plbls-input" id="acc-gr-x" type="datetime-local" bind:value={groupeExpire} /></div>
                <button class="plbls-button" disabled={!groupeChoisi}>Ajouter au groupe</button>
              </form>
            {/if}
          {/if}
        {:else if ajout === 'regle'}
          <FormRegle {catalogue} sujet="account" id={sub} onDone={() => { ajout = null; recharger() }} />
        {/if}
      </section>

      <div class="acc-barre-f">
        <h3 class="perm-t2">Ce qu'il peut faire</h3>
        {#if effectifs.data}
          <div class="v-seg acc-filtres" role="group" aria-label="Filtrer les permissions">
            {#each Object.entries(FILTRES_EFFECTIFS) as [v, f] (v)}
              <button aria-pressed={filtre === v} onclick={() => (filtre = v as FiltreEffectifs)}>{f.label} · {compteurs[v as FiltreEffectifs]}</button>
            {/each}
          </div>
        {/if}
      </div>
      {#if effectifs.data === null}
        <Attente error={effectifs.error} retry={effectifs.reload} />
      {:else}
        {#each parApp as g (g.cle)}
          <h4 class="acc-sur">{g.nom}</h4>
          <ul class="acc-droits">
            {#each g.lignes as e (e.permission)}
              {@const p = noms.get(e.permission)}
              {@const type = typeDe(noms, e.permission)}
              <li class="acc-droit" class:ouvert={ouverte === e.permission}>
                <div class="acc-droit-l">
                  <span class="acc-droit-n"><b>{p?.label ?? e.permission}</b><span class="v-mono v-mute">{e.permission}</span></span>
                  <span class="acc-etat v-tag {ETATS[e.state].ton}"><Icon name={ICONES_ETAT[e.state]} size={12} />{ETATS[e.state].label}</span>
                  <span class="acc-droit-p">
                    {#if e.state === 'partiel'}
                      {#if e.allow.includes('*')}autorisé partout{:else}autorisé sur {#each e.allow as id, i (id)}{i ? ', ' : ''}<b><NomRessource {type} {id} /></b>{/each}{/if}{#if e.deny.length}, sauf sur {#each e.deny as id, i (id)}{i ? ', ' : ''}<b><NomRessource {type} {id} /></b>{/each}{/if}
                    {:else}{resumeEffectif(e)}{/if}
                  </span>
                  <button class="acc-lien acc-pourquoi" aria-expanded={ouverte === e.permission} onclick={() => pourquoi(e)}>{ouverte === e.permission ? 'Fermer' : 'Pourquoi ?'}</button>
                </div>
                {#if ouverte === e.permission}
                  <div class="acc-pourquoi-z">
                    <ol class="acc-chaine">
                      {#each e.rules as r (r.rule_id + r.resource + origine(r.source))}
                        <li><span class="acc-effet" class:refus={r.effect === 'deny'}>{r.effect === 'allow' ? 'accorde' : 'refuse'}</span><span>{origine(r.source)} · {#if r.resource === '*'}partout{:else}sur <b><NomRessource {type} id={r.resource} /></b>{/if}{r.expires_at ? ` · jusqu'au ${quand(r.expires_at)}` : ''}</span></li>
                      {:else}
                        <li><span class="v-note">Aucune règle ne mentionne cette permission : elle est refusée.</span></li>
                      {/each}
                      <li class="v-note">Le refus l'emporte toujours, d'où qu'il vienne. Pour lever un refus, on le supprime.</li>
                    </ol>
                    <div class="acc-essai">
                      {#if type}
                        <div class="plbls-field">
                          <label class="plbls-field__label" for="acc-essai-r">Tester sur une ressource</label>
                          {#if essaiRes}<p class="perm-en-ligne"><b>{essaiRes.name}</b><button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => { essaiRes = null; void simuler(e.permission, null) }}>Partout</button></p>
                          {:else}<ChoixRessource id="acc-essai-r" {type} onPick={(r) => { essaiRes = r; void simuler(e.permission, r.id) }} />{/if}
                        </div>
                      {/if}
                      {#if decision}
                        <p class="acc-verdict" class:refus={!decision.allowed} role="status"><Icon name={decision.allowed ? 'ok' : 'x'} size={16} /><b>{decision.allowed ? 'Autorisé' : 'Refusé'} {essaiRes ? `sur ${essaiRes.name}` : type ? 'sans ressource précise' : ''}</b></p>
                        <p class="v-note">{decision.reason}</p>
                      {/if}
                    </div>
                  </div>
                {/if}
              </li>
            {/each}
          </ul>
        {:else}
          <p class="v-note">{filtre === 'distingue' ? 'Rien ne distingue ce compte : il a exactement ce que donne « Tout le monde ».' : 'Aucune permission dans ce filtre.'}</p>
        {/each}
        {#if filtre === 'distingue' && heritees > 0}
          <button class="plbls-button plbls-button--discret plbls-button--compact acc-heritees" onclick={() => (filtre = 'tout')}>Afficher les {heritees} autres permissions (« Tout le monde » ou non définies)</button>
        {/if}
      {/if}
    {/if}
  {/if}
</section>
