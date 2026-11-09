<script lang="ts">
  import { charger, ecrire, executer, versIso, type Catalogue, type RoleResume, type Sujet } from '../../lib/acces.svelte'
  import ChoixRessource from './ChoixRessource.svelte'

  let { catalogue, sujet, id, onDone }: { catalogue: Catalogue; sujet: Exclude<Sujet, 'role'>; id: string; onDone: () => void } = $props()
  const uid = $props.id()
  const roles = charger<RoleResume[]>(() => '/roles')
  let role = $state('')
  let portee = $state('')
  let ressource = $state<{ id: string; name: string } | null>(null)
  let expire = $state('')
  const types = $derived(catalogue.apps.flatMap((a) => a.resource_types.filter((t) => t.listable).map((t) => ({ cle: `${a.key}:${t.key}`, nom: `${t.name} (${a.name})` }))))

  const envoyer = async () => {
    const corps = { role_id: role, subject_kind: sujet, subject_id: id, scope_type: portee && ressource ? portee : null, scope_id: portee && ressource ? ressource.id : null, expires_at: versIso(expire) }
    if ((await executer(ecrire('POST', '/bindings', corps), 'Rôle donné')) !== null) { role = ''; portee = ''; ressource = null; expire = ''; onDone() }
  }
</script>

<form class="perm-form perm-form--haut" onsubmit={(e) => { e.preventDefault(); void envoyer() }}>
  <div class="plbls-field">
    <label class="plbls-field__label" for="{uid}-r">Rôle</label>
    <select class="plbls-select" id="{uid}-r" bind:value={role} required>
      <option value="">Choisir…</option>
      {#each (roles.data ?? []).filter((r) => r.system !== 'tout-le-monde') as r (r.id)}<option value={r.id}>{r.name}</option>{/each}
    </select>
  </div>
  <div class="plbls-field">
    <label class="plbls-field__label" for="{uid}-t">Portée</label>
    <select class="plbls-select" id="{uid}-t" bind:value={portee} onchange={() => (ressource = null)}>
      <option value="">Partout</option>
      {#each types as t (t.cle)}<option value={t.cle}>Une ressource : {t.nom}</option>{/each}
    </select>
  </div>
  {#if portee}
    <div class="plbls-field perm-compte">
      <label class="plbls-field__label" for="{uid}-s">Ressource</label>
      {#if ressource}<p class="perm-en-ligne"><b>{ressource.name}</b><button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => (ressource = null)}>Changer</button></p>
      {:else}<ChoixRessource id="{uid}-s" type={portee} onPick={(r) => (ressource = r)} />{/if}
    </div>
  {/if}
  <div class="plbls-field">
    <label class="plbls-field__label" for="{uid}-x">Expire le (facultatif)</label>
    <input class="plbls-input" id="{uid}-x" type="datetime-local" bind:value={expire} />
  </div>
  <button class="plbls-button" disabled={!role || (!!portee && !ressource)}>Donner le rôle</button>
</form>
{#if portee}<p class="v-note">Avec une portée, seules les permissions du rôle qui se restreignent à ce type de ressource s'appliquent, et seulement sur cette ressource.</p>{/if}
