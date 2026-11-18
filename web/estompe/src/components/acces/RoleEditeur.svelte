<script lang="ts">
  import { tick, untrack } from 'svelte'
  import { confirmer } from '../../lib/confirm.svelte'
  import Icon from '../Icon.svelte'
  import Attente from './Attente.svelte'
  import {
    charger, ecrire, executer, permissionsModifiees, quand, resumeModifications,
    type Catalogue, type Compte, type Effet, type LiaisonVue, type RegleSaisie, type RoleDetail,
  } from '../../lib/acces.svelte'
  import ChoixCompte from './ChoixCompte.svelte'
  import ChoixRessource from './ChoixRessource.svelte'
  import NomRessource from './NomRessource.svelte'

  let { id, catalogue, onChange, onDelete, onBack, cible = null, appInitiale = '' }: { id: string; catalogue: Catalogue; onChange: () => void; onDelete: () => void; onBack?: () => void; cible?: string | null; appInitiale?: string } = $props()
  const role = charger<RoleDetail>(() => `/roles/${id}`)
  type Choix = '' | Effet
  const CHOIX: [Choix, string][] = [['', 'Ne dit rien'], ['allow', 'Autorise'], ['deny', 'Refuse']]
  let nom = $state('')
  let description = $state('')
  let joker = $state<Record<string, Choix>>({})
  let precises = $state<RegleSaisie[]>([])
  let depart = $state.raw<RegleSaisie[]>([])
  let filtre = $state('')
  let seulementDefini = $state(false)
  let app = $state(untrack(() => appInitiale))
  let ajoutPour = $state<string | null>(null)
  let ajoutEffet = $state<Effet>('deny')
  let donner = $state(false)
  let vise = false

  $effect(() => {
    const r = role.data
    if (!r) return
    nom = r.name
    description = r.description
    joker = Object.fromEntries(r.rules.filter((x) => x.resource === '*').map((x) => [x.permission, x.effect]))
    precises = r.rules.filter((x) => x.resource !== '*').map((x) => ({ permission: x.permission, resource: x.resource, effect: x.effect }))
    depart = r.rules.map((x) => ({ permission: x.permission, resource: x.resource, effect: x.effect }))
    if (cible && !vise) {
      vise = true
      const a = catalogue.apps.find((x) => x.permissions.some((p) => p.name === cible))
      if (a) app = a.key
      void tick().then(() => document.getElementById(`acc-perm-${cible}`)?.scrollIntoView?.({ block: 'center' }))
    }
  })

  const regles = $derived<RegleSaisie[]>([
    ...Object.entries(joker).filter(([, v]) => v).map(([permission, effect]) => ({ permission, resource: '*', effect: effect as Effet })),
    ...precises,
  ])
  const touchees = $derived(permissionsModifiees(depart, regles))
  const libelles = $derived(new Map(catalogue.apps.flatMap((a) => a.permissions.map((p) => [p.name, p.label] as const))))
  const autres = $derived(role.data ? Number(nom !== role.data.name) + Number(description !== role.data.description) : 0)
  const modifie = $derived(touchees.length + autres > 0)
  const resume = $derived(resumeModifications(touchees.map((t) => libelles.get(t) ?? t), autres))
  const defini = (permission: string) => !!joker[permission] || precises.some((x) => x.permission === permission)
  const compteApp = (cle: string) => catalogue.apps.find((a) => a.key === cle)?.permissions.filter((p) => !p.obsolete_at && defini(p.name)).length ?? 0
  const courante = $derived(catalogue.apps.find((a) => a.key === app) ?? catalogue.apps[0])
  const categories = $derived.by(() => {
    const f = filtre.trim().toLowerCase()
    const perms = (courante?.permissions ?? []).filter((p) => !p.obsolete_at && (!f || `${p.label} ${p.name} ${p.category}`.toLowerCase().includes(f)) && (!seulementDefini || defini(p.name) || touchees.includes(p.name)))
    return [...new Set(perms.map((p) => p.category || 'Autres'))].map((c) => ({ nom: c, perms: perms.filter((p) => (p.category || 'Autres') === c) }))
  })

  const regler = (perm: string, v: Choix) => { joker = { ...joker, [perm]: v } }
  const toutRegler = (perms: { name: string }[], v: Choix) => { joker = { ...joker, ...Object.fromEntries(perms.map((p) => [p.name, v])) } }
  const ajouterPrecise = (perm: string, r: { id: string }) => {
    precises = [...precises.filter((x) => !(x.permission === perm && x.resource === r.id)), { permission: perm, resource: r.id, effect: ajoutEffet }]
    ajoutPour = null
  }
  const retirerPrecise = (i: number) => { precises = precises.filter((_, j) => j !== i) }

  const enregistrer = async () => {
    const r = await executer(ecrire<RoleDetail>('PUT', `/roles/${id}`, { name: nom, description, rules: regles }), 'Rôle enregistré')
    if (r) { role.reload(); onChange() }
  }
  const supprimer = async () => {
    if (!(await confirmer({ titre: 'Supprimer le rôle', message: `Supprimer « ${nom} » ? Ses ${role.data?.bindings.length ?? 0} liaison(s) disparaissent.`, action: 'Supprimer', destructif: true }))) return
    if ((await executer(ecrire('DELETE', `/roles/${id}`), 'Rôle supprimé')) !== null) onDelete()
  }
  const retirer = async (l: LiaisonVue) => {
    if (!(await confirmer({ titre: 'Retirer le rôle', message: `Retirer le rôle « ${l.role} » à ${l.subject_label ?? l.subject_id} ?`, action: 'Retirer', destructif: true }))) return
    if ((await executer(ecrire('DELETE', `/bindings/${l.id}`), 'Rôle retiré')) !== null) { role.reload(); onChange() }
  }
  const donnerA = async (c: Compte) => {
    if ((await executer(ecrire('POST', '/bindings', { role_id: id, subject_kind: 'account', subject_id: c.sub, scope_type: null, scope_id: null, expires_at: null }), 'Rôle donné')) !== null) { donner = false; role.reload(); onChange() }
  }
</script>

{#if role.data === null}
  <section class="v-card perm-carte"><Attente error={role.error} retry={role.reload} /></section>
{:else}
  {@const r = role.data}
  {@const groupes = r.bindings.filter((l) => l.subject_kind === 'group')}
  {@const comptes = r.bindings.filter((l) => l.subject_kind !== 'group')}
  <header class="acc-tete">
    {#if onBack}<button class="plbls-button plbls-button--contour plbls-button--compact" onclick={onBack}><Icon name="gauche" size={14} /> Rôles</button>{/if}
    <div class="acc-tete-t">
      <h2 class="perm-t acc-role-t">{r.name}</h2>
      <p class="v-note">{r.system === 'tout-le-monde' ? `Rôle système, appliqué d'office · ${r.accounts_count} compte${r.accounts_count > 1 ? 's' : ''} dans l'annuaire` : `S'ajoute à « Tout le monde » · ${comptes.length} compte${comptes.length > 1 ? 's' : ''} et ${groupes.length} groupe${groupes.length > 1 ? 's' : ''}`}</p>
    </div>
    {#if !r.system}<button class="perm-suppr" onclick={supprimer}>Supprimer le rôle</button>{/if}
  </header>
  {#if r.system === 'tout-le-monde'}
    <p class="v-alert info"><span class="grow">Ce rôle s'applique à <b>tous les comptes connectés</b>. Retirer une permission ici la ferme pour tous, sauf pour qui la reçoit par un autre rôle, un groupe ou une règle directe.</span></p>
  {:else if r.system === 'admin'}
    <p class="v-alert dgr"><span class="grow">Ce rôle donne <b>aegis:admin</b> : l'administration de toutes les permissions.</span></p>
  {/if}

  <div class="acc-role-cols">
    <section class="acc-role-p">
      <div class="perm-form">
        <div class="plbls-field"><label class="plbls-field__label" for="acc-re-nom">Nom</label><input class="plbls-input" id="acc-re-nom" bind:value={nom} disabled={!!r.system} maxlength="80" /></div>
        <div class="plbls-field"><label class="plbls-field__label" for="acc-re-desc">Description</label><input class="plbls-input" id="acc-re-desc" bind:value={description} maxlength="300" /></div>
        <div class="plbls-field"><label class="plbls-field__label" for="acc-re-f">Filtrer</label><input class="plbls-input" id="acc-re-f" type="search" bind:value={filtre} placeholder="permission, catégorie" /></div>
      </div>
      <div class="acc-outils">
        <div class="v-seg" role="group" aria-label="Application">
          {#each catalogue.apps as a (a.key)}<button aria-pressed={courante?.key === a.key} onclick={() => (app = a.key)}>{a.name} · {compteApp(a.key)}</button>{/each}
        </div>
        <label class="plbls-checkbox"><input type="checkbox" bind:checked={seulementDefini} /> Seulement ce que ce rôle définit</label>
      </div>

      <div class="v-card acc-regles">
        {#each categories as c (c.nom)}
          <div class="acc-regles-cat">
            <h3 class="acc-sur">{c.nom}</h3>
            <span class="perm-en-ligne">
              <button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => toutRegler(c.perms, 'allow')}>Tout autoriser</button>
              <button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => toutRegler(c.perms, '')}>Tout effacer</button>
            </span>
          </div>
          <ul class="acc-regles-l">
            {#each c.perms as p (p.name)}
              {@const change = touchees.includes(p.name)}
              <li class="acc-regle" class:change id="acc-perm-{p.name}">
                <div class="acc-regle-t">
                  <span class="acc-regle-n"><b>{p.label}</b>{#if change}<span class="v-tag encre">modifié</span>{/if}</span>
                  <span class="v-note"><span class="v-mono">{p.name}</span> · {p.open_default ? 'ouverte par défaut' : 'fermée par défaut'}{p.description ? ` · ${p.description}` : ''}</span>
                  {#if p.resource_type}
                    <div class="acc-precis">
                      {#each precises.map((x, i) => [x, i] as const).filter(([x]) => x.permission === p.name) as [x, i] (x.resource)}
                        <span class="v-tag {x.effect === 'allow' ? 'ok' : 'dgr'}">{x.effect === 'allow' ? 'autorisé' : 'refusé'} sur <NomRessource type="{p.app}:{p.resource_type}" id={x.resource} /> <button class="acc-puce-x" aria-label="Retirer l'exception" onclick={() => retirerPrecise(i)}><Icon name="x" size={10} /></button></span>
                      {/each}
                      {#if ajoutPour === p.name}
                        <span class="acc-ajout">
                          <select class="plbls-select" bind:value={ajoutEffet} aria-label="Effet"><option value="deny">Refuser sur…</option><option value="allow">Autoriser sur…</option></select>
                          <ChoixRessource id="acc-re-{p.key}" type="{p.app}:{p.resource_type}" onPick={(res) => ajouterPrecise(p.name, res)} />
                        </span>
                      {:else}
                        <button type="button" class="acc-lien v-note" onclick={() => (ajoutPour = p.name)}>+ exception</button>
                      {/if}
                    </div>
                  {/if}
                </div>
                <div class="acc-tri" role="radiogroup" aria-label={p.label}>
                  {#each CHOIX as [v, l] (v)}
                    <label class="acc-tri-o" data-v={v}><input type="radio" name="acc-{p.name}" checked={(joker[p.name] ?? '') === v} onchange={() => regler(p.name, v)} />{#if v}<Icon name={v === 'allow' ? 'ok' : 'x'} size={12} />{/if}{l}</label>
                  {/each}
                </div>
              </li>
            {/each}
          </ul>
        {:else}
          <p class="v-note acc-regles-vide">{seulementDefini ? 'Ce rôle ne définit rien dans cette application.' : 'Aucune permission ne correspond.'}</p>
        {/each}
      </div>

      <div class="perm-barre acc-barre" class:active={modifie} role="status">
        <span class="acc-barre-t">{modifie ? resume : `${regles.length} règle${regles.length > 1 ? 's' : ''} · aucune modification`}</span>
        <span class="perm-en-ligne">
          <button class="plbls-button plbls-button--contour" disabled={!modifie} onclick={() => role.reload()}>Annuler</button>
          <button class="plbls-button acc-enregistrer" disabled={!modifie} onclick={enregistrer}>Enregistrer</button>
        </span>
      </div>
    </section>

    <aside class="v-well acc-qui">
      <h3 class="perm-t2">Qui a ce rôle</h3>
      {#if r.system === 'tout-le-monde'}
        <p class="v-note">Tous les comptes ({r.accounts_count} dans l'annuaire).</p>
      {:else}
        {#each [['Groupes', groupes], ['Comptes', comptes]] as const as [titre, liste] (titre)}
          <h4 class="acc-sur">{titre} · {liste.length}</h4>
          <ul class="acc-qui-l">
            {#each liste as l (l.id)}
              <li><span class="acc-qui-n"><b class="truncate">{l.subject_label ?? l.subject_id}</b><span class="v-note">{#if l.scope_id}sur <NomRessource type={l.scope_type} id={l.scope_id} />{:else}partout{/if}{l.expires_at ? ` · jusqu'au ${quand(l.expires_at)}` : ''}</span></span><button class="perm-suppr" onclick={() => retirer(l)}>Retirer</button></li>
            {:else}
              <li class="v-note">Aucun.</li>
            {/each}
          </ul>
        {/each}
        {#if donner}
          <div class="plbls-field"><label class="plbls-field__label" for="acc-re-donner">Donner ce rôle à un compte, partout</label><ChoixCompte id="acc-re-donner" onPick={donnerA} /></div>
          <p class="v-note">Pour une portée, une expiration ou un groupe : fiche du compte ou onglet Groupes.</p>
        {:else}
          <button class="plbls-button plbls-button--compact" onclick={() => (donner = true)}>+ Donner ce rôle</button>
        {/if}
      {/if}
    </aside>
  </div>
{/if}
