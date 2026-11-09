<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import Attente from '../permissions/Attente.svelte'
  import {
    charger, ecrire, enc, ETATS, executer, libellesPermissions, nomCompte, origine, quand, typeDe,
    type Catalogue, type Compte, type CompteDetail, type Decision, type EffectifsExpliques, type EntreeJournal,
  } from '../../lib/acces.svelte'
  import ChoixCompte from './ChoixCompte.svelte'
  import ChoixRessource from './ChoixRessource.svelte'
  import FormLiaison from './FormLiaison.svelte'
  import FormRegle from './FormRegle.svelte'
  import ListeLiaisons from './ListeLiaisons.svelte'
  import ListeRegles from './ListeRegles.svelte'
  import LigneJournal from './LigneJournal.svelte'
  import NomRessource from './NomRessource.svelte'

  let { sub, catalogue, onChange }: { sub: string; catalogue: Catalogue; onChange: () => void } = $props()
  const ONGLETS = { droits: 'Droits effectifs', attributions: 'Attributions', simulateur: 'Simulateur', historique: 'Historique' } as const
  type Onglet = keyof typeof ONGLETS
  let onglet = $state<Onglet>('droits')
  const detail = charger<CompteDetail>(() => `/accounts/${enc(sub)}`)
  const effectifs = charger<EffectifsExpliques>(() => (onglet === 'droits' ? `/accounts/${enc(sub)}/effective` : null))
  const journal = charger<EntreeJournal[]>(() => (onglet === 'historique' ? `/audit?subject=${enc(sub)}&limit=200` : null))
  const noms = $derived(libellesPermissions(catalogue))
  let filtre = $state<'tout' | 'autorise' | 'refuse'>('tout')
  const recharger = () => { detail.reload(); effectifs.reload(); onChange() }

  let simPerm = $state('')
  let simRes = $state<{ id: string; name: string } | null>(null)
  let decision = $state.raw<Decision | null>(null)
  const simType = $derived.by(() => { const p = noms.get(simPerm); return p?.resource_type ? `${p.app}:${p.resource_type}` : null })
  const simuler = async () => { decision = await executer(ecrire<Decision>('POST', '/simulate', { sub, permission: simPerm, resource: simRes?.id ?? null })) }

  let source = $state<Compte | null>(null)
  let mode = $state<'ajouter' | 'remplacer'>('ajouter')
  const copier = async () => {
    if (!source) return
    const essai = await executer(ecrire<{ bindings: number; groups: number; rules: number; removed: number }>('POST', `/accounts/${enc(sub)}/copy`, { from: source.sub, mode, dry_run: true }))
    if (!essai) return
    const msg = `${essai.bindings} rôle(s), ${essai.groups} groupe(s) et ${essai.rules} règle(s) de ${nomCompte(source)} seront copiés${mode === 'remplacer' ? `, après retrait des ${essai.removed} droit(s) actuels` : ''}.`
    if (!(await confirmer({ titre: 'Copier les droits', message: msg, action: 'Copier', destructif: mode === 'remplacer' }))) return
    if ((await executer(ecrire('POST', `/accounts/${enc(sub)}/copy`, { from: source.sub, mode }), 'Droits copiés')) !== null) { source = null; recharger() }
  }
  const purger = async () => {
    if (!(await confirmer({ titre: 'Retirer tous les droits', message: `Retirer à ${nomCompte(detail.data?.account, sub)} tous ses rôles, groupes et règles directes ? Il gardera ce que donne « Tout le monde ».`, action: 'Tout retirer', destructif: true }))) return
    if ((await executer(ecrire('DELETE', `/accounts/${enc(sub)}/grants`), 'Droits retirés')) !== null) recharger()
  }
  const quitterGroupe = async (gid: string, nom: string) => {
    if (!(await confirmer({ titre: 'Retirer du groupe', message: `Retirer ce compte du groupe « ${nom} » ?`, action: 'Retirer', destructif: true }))) return
    if ((await executer(ecrire('DELETE', `/groups/${gid}/members/${enc(sub)}`), 'Retiré du groupe')) !== null) recharger()
  }
</script>

<section class="v-card perm-carte acc-fiche">
  {#if detail.data === null}
    <Attente error={detail.error} retry={detail.reload} />
  {:else}
    {@const c = detail.data.account}
    <div class="perm-tete">
      <div>
        <h2 class="perm-t">{nomCompte(c)}</h2>
        <p class="v-note">{c.display_name && c.email ? `${c.display_name} · ` : ''}<span class="v-mono" title="sub Keycloak">{c.sub}</span> · vu {quand(c.last_seen_at)}{#if !c.in_keycloak} · <span class="v-tag dgr">supprimé de Keycloak</span>{/if}</p>
      </div>
      <button class="plbls-button plbls-button--secondaire plbls-button--compact" onclick={purger}>Tout retirer</button>
    </div>
    <div class="v-tabs" role="tablist" aria-label="Fiche du compte">
      {#each Object.entries(ONGLETS) as [o, l] (o)}
        <button role="tab" aria-selected={onglet === o} onclick={() => (onglet = o as Onglet)}>{l}</button>
      {/each}
    </div>

    {#if onglet === 'droits'}
      {#if effectifs.data === null}
        <Attente error={effectifs.error} retry={effectifs.reload} />
      {:else}
        <div class="perm-en-ligne" role="radiogroup" aria-label="Filtrer">
          {#each [['tout', 'Toutes'], ['autorise', 'Accordées'], ['refuse', 'Refusées']] as [v, l] (v)}
            <label class="plbls-radio"><input type="radio" bind:group={filtre} value={v} /> {l}</label>
          {/each}
        </div>
        <div class="perm-defile">
          <table class="perm-tb acc-effectifs">
            <thead><tr><th scope="col">Permission</th><th scope="col">État</th><th scope="col">Pourquoi</th></tr></thead>
            <tbody>
              {#each effectifs.data.permissions.filter((e) => filtre === 'tout' || (filtre === 'autorise' ? e.state === 'autorise' || e.state === 'partiel' : e.state === 'refuse' || e.state === 'non_defini')) as e (e.permission)}
                {@const p = noms.get(e.permission)}
                <tr>
                  <td><b>{p?.label ?? e.permission}</b><br /><span class="v-mono v-mute">{e.permission}</span></td>
                  <td>
                    <span class="v-tag {ETATS[e.state].ton}">{ETATS[e.state].label}</span>
                    {#if e.state === 'partiel'}<br /><span class="v-note">{#if e.allow.includes('*')}partout{:else}{#each e.allow as id, i (id)}{i ? ', ' : ''}<NomRessource type={typeDe(noms, e.permission)} {id} />{/each}{/if}{#if e.deny.length} sauf {#each e.deny as id, i (id)}{i ? ', ' : ''}<NomRessource type={typeDe(noms, e.permission)} {id} />{/each}{/if}</span>{/if}
                  </td>
                  <td>
                    {#if e.rules.length === 0}<span class="v-note">aucune règle : refusé</span>{/if}
                    <ul class="acc-sources">
                      {#each e.rules as r (r.rule_id + r.resource + origine(r.source))}
                        <li><span class="v-tag {r.effect === 'allow' ? 'ok' : 'dgr'}">{r.effect === 'allow' ? 'accorde' : 'refuse'}</span> {origine(r.source)}{#if r.resource !== '*'} · <NomRessource type={typeDe(noms, r.permission)} id={r.resource} />{/if}{r.expires_at ? ` · jusqu'au ${quand(r.expires_at)}` : ''}</li>
                      {/each}
                    </ul>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}

    {:else if onglet === 'attributions'}
      <h3 class="perm-t2">Rôles</h3>
      <ListeLiaisons liaisons={detail.data.bindings} onChange={recharger} vide="Aucun rôle donné directement." />
      <details class="perm-nouveau"><summary>Donner un rôle</summary><FormLiaison {catalogue} sujet="account" id={sub} onDone={recharger} /></details>

      <h3 class="perm-t2">Groupes</h3>
      {#if detail.data.groups.length === 0}<p class="v-note">Membre d'aucun groupe.</p>{:else}
        <ul class="perm-puces">
          {#each detail.data.groups as g (g.group_id)}
            <li class="v-tag info">{g.name}{g.expires_at ? ` · jusqu'au ${quand(g.expires_at)}` : ''} <button class="perm-suppr" aria-label="Retirer du groupe {g.name}" onclick={() => quitterGroupe(g.group_id, g.name)}>×</button></li>
          {/each}
        </ul>
      {/if}
      {#if detail.data.group_bindings.length}
        <p class="v-note">Rôles hérités des groupes :</p>
        <ListeLiaisons liaisons={detail.data.group_bindings} onChange={recharger} sujet lecture />
      {/if}

      <h3 class="perm-t2">Règles directes</h3>
      <ListeRegles regles={detail.data.rules} {catalogue} onChange={recharger} vide="Aucune permission accordée ou refusée directement." />
      <details class="perm-nouveau"><summary>Accorder ou refuser une permission</summary><FormRegle {catalogue} sujet="account" id={sub} onDone={recharger} /></details>

      <h3 class="perm-t2">Copier les droits d'un autre compte</h3>
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
        <ChoixCompte id="acc-copie" onPick={(c) => (source = c.sub === sub ? null : c)} />
      {/if}

    {:else if onglet === 'simulateur'}
      <form class="perm-form perm-form--haut" onsubmit={(e) => { e.preventDefault(); void simuler() }}>
        <div class="plbls-field">
          <label class="plbls-field__label" for="acc-sim-p">Permission</label>
          <select class="plbls-select" id="acc-sim-p" bind:value={simPerm} onchange={() => { simRes = null; decision = null }} required>
            <option value="">Choisir…</option>
            {#each catalogue.apps as a (a.key)}<optgroup label={a.name}>{#each a.permissions.filter((p) => !p.obsolete_at) as p (p.name)}<option value={p.name}>{p.label}</option>{/each}</optgroup>{/each}
          </select>
        </div>
        {#if simType}
          <div class="plbls-field perm-compte">
            <label class="plbls-field__label" for="acc-sim-r">Sur (facultatif)</label>
            {#if simRes}<p class="perm-en-ligne"><b>{simRes.name}</b><button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => (simRes = null)}>Partout</button></p>
            {:else}<ChoixRessource id="acc-sim-r" type={simType} onPick={(r) => (simRes = r)} />{/if}
          </div>
        {/if}
        <button class="plbls-button" disabled={!simPerm}>Tester</button>
      </form>
      {#if decision}
        <div class="v-alert {decision.allowed ? 'ok' : 'dgr'}" role="status">
          <span class="grow"><b>{decision.allowed ? 'Autorisé' : 'Refusé'}</b> — {decision.reason}</span>
        </div>
        {#if decision.rules.length}
          <ul class="acc-sources">{#each decision.rules as r (r.rule_id + origine(r.source))}<li><span class="v-tag {r.effect === 'allow' ? 'ok' : 'dgr'}">{r.effect === 'allow' ? 'accorde' : 'refuse'}</span> {origine(r.source)}{#if r.resource !== '*'} · <NomRessource type={typeDe(noms, r.permission)} id={r.resource} />{/if}</li>{/each}</ul>
        {/if}
      {/if}

    {:else}
      {#if journal.data === null}<Attente error={journal.error} retry={journal.reload} />
      {:else if journal.data.length === 0}<p class="v-note">Aucune modification pour ce compte.</p>
      {:else}<ul class="acc-journal">{#each journal.data as l (l.id)}<LigneJournal ligne={l} {noms} />{/each}</ul>{/if}
    {/if}
  {/if}
</section>
