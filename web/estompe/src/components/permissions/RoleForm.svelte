<script lang="ts">
  import { untrack } from 'svelte'
  import type { Permission, PermRole } from '../../lib/permissions.svelte'

  interface Valeurs { key: string; name: string; description: string; permissions: string[] }
  let { role, perms, dirtyRef, onSubmit, onCancel, onDelete }: {
    role?: PermRole; perms: Permission[]; dirtyRef: { current: boolean }; onSubmit: (v: Valeurs) => Promise<unknown>; onCancel?: () => void; onDelete?: () => void
  } = $props()

  const init = untrack(() => role)
  let key = $state(init?.key ?? '')
  let name = $state(init?.name ?? '')
  let description = $state(init?.description ?? '')
  let coches = $state.raw(new Set(init?.permissions ?? []))
  let occupe = $state(false)

  const modifie = $derived.by(() => {
    if (!role) return true
    const initial = new Set(role.permissions)
    return name !== role.name || description !== role.description || coches.size !== initial.size || [...coches].some((k) => !initial.has(k))
  })
  $effect(() => {
    dirtyRef.current = !!role && modifie
    return () => { dirtyRef.current = false }
  })
  const basculer = (k: string) => { const n = new Set(coches); if (!n.delete(k)) n.add(k); coches = n }
  const annuler = () => { name = role?.name ?? ''; description = role?.description ?? ''; coches = new Set(role?.permissions ?? []) }
  const envoyer = async () => {
    occupe = true
    await onSubmit({ key, name, description, permissions: perms.map((p) => p.key).filter((k) => coches.has(k)) })
    occupe = false
  }
</script>

<form class="perm-pile" onsubmit={(e) => { e.preventDefault(); void envoyer() }}>
  {#if onDelete}<div><button type="button" class="perm-suppr" onclick={onDelete}>Supprimer le rôle</button></div>{/if}
  <div class="perm-form perm-form--haut">
    <div class="plbls-field">
      <label class="plbls-field__label" for="perm-r-key">Clé</label>
      <input class="plbls-input" id="perm-r-key" bind:value={key} placeholder="moderator" pattern={'[a-z][a-z0-9_.\\-]{0,59}'} required disabled={!!role} />
    </div>
    <div class="plbls-field">
      <label class="plbls-field__label" for="perm-r-name">Nom</label>
      <input class="plbls-input" id="perm-r-name" bind:value={name} placeholder="Modérateur" required />
    </div>
    <div class="plbls-field">
      <label class="plbls-field__label" for="perm-r-desc">Description</label>
      <input class="plbls-input" id="perm-r-desc" bind:value={description} />
    </div>
  </div>

  <div>
    <div class="perm-ligne-h">
      <b>Permissions accordées <span class="v-note">({coches.size} sur {perms.length})</span></b>
      <span class="perm-en-ligne">
        <button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => (coches = new Set(perms.map((p) => p.key)))}>Tout cocher</button>
        <button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => (coches = new Set())}>Tout décocher</button>
      </span>
    </div>
    <ul class="perm-cases">
      {#each perms as p (p.key)}
        <li>
          <label class="plbls-checkbox">
            <input class="plbls-checkbox__input" type="checkbox" checked={coches.has(p.key)} onchange={() => basculer(p.key)} />
            <span class="plbls-checkbox__case" aria-hidden="true"></span>
            <span class="plbls-checkbox__texte"><span class="v-mono">{p.key}</span>{#if p.description}<br /><span class="v-note">{p.description}</span>{/if}</span>
          </label>
        </li>
      {/each}
    </ul>
  </div>

  <div class="perm-barre">
    <span class="v-note">{role ? (modifie ? 'Modifications non enregistrées' : '') : "Rien n'est créé avant « Créer le rôle »."}</span>
    <span class="perm-en-ligne">
      {#if role}<button type="button" class="plbls-button plbls-button--discret" disabled={!modifie || occupe} onclick={annuler}>Annuler les changements</button>{/if}
      {#if onCancel}<button type="button" class="plbls-button plbls-button--discret" onclick={onCancel}>Fermer</button>{/if}
      <button class="plbls-button" disabled={!modifie || occupe}>{role ? 'Enregistrer' : 'Créer le rôle'}</button>
    </span>
  </div>
</form>
