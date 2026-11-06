<script lang="ts">
  import { untrack } from "svelte";
  import type { Permission, Role } from "../api";

  interface RoleValues { key: string; name: string; description: string; permissions: string[] }
  let { role, perms, dirtyRef, onSubmit, onCancel, onDelete }: {
    role?: Role; perms: Permission[]; dirtyRef: { current: boolean }; onSubmit: (v: RoleValues) => Promise<unknown>; onCancel?: () => void; onDelete?: () => void;
  } = $props();

  const init = untrack(() => role);
  let key = $state(init?.key ?? "");
  let name = $state(init?.name ?? "");
  let description = $state(init?.description ?? "");
  let checked = $state.raw(new Set(init?.permissions ?? []));
  let busy = $state(false);

  const dirty = $derived.by(() => {
    if (!role) return true;
    const initial = new Set(role.permissions);
    return name !== role.name || description !== role.description || checked.size !== initial.size || [...checked].some((k) => !initial.has(k));
  });
  $effect(() => {
    dirtyRef.current = !!role && dirty;
    return () => { dirtyRef.current = false; };
  });
  const toggle = (k: string) => { const n = new Set(checked); if (!n.delete(k)) n.add(k); checked = n; };
  const reset = () => { name = role?.name ?? ""; description = role?.description ?? ""; checked = new Set(role?.permissions ?? []); };
  const submit = async () => {
    busy = true;
    await onSubmit({ key, name, description, permissions: perms.map((p) => p.key).filter((k) => checked.has(k)) });
    busy = false;
  };
</script>

<form class="app-stack" onsubmit={(e) => { e.preventDefault(); submit(); }}>
  {#if onDelete}<div><button type="button" class="plbls-button plbls-button--lien plbls-button--danger plbls-button--compact" onclick={onDelete}>Supprimer le rôle</button></div>{/if}
  <div class="app-form" style="margin-top: 0px;">
    <div class="plbls-field">
      <label class="plbls-field__label" for="r-key">Clé</label>
      <input class="plbls-input" id="r-key" bind:value={key} placeholder="moderator" pattern={"[a-z][a-z0-9_.\\-]{0,59}"} required disabled={!!role} />
    </div>
    <div class="plbls-field">
      <label class="plbls-field__label" for="r-name">Nom</label>
      <input class="plbls-input" id="r-name" bind:value={name} placeholder="Modérateur" required />
    </div>
    <div class="plbls-field">
      <label class="plbls-field__label" for="r-desc">Description</label>
      <input class="plbls-input" id="r-desc" bind:value={description} />
    </div>
  </div>

  <div>
    <div class="app-inline" style="justify-content: space-between;">
      <b>Permissions accordées <span class="app-sub">({checked.size} sur {perms.length})</span></b>
      <span class="app-inline">
        <button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => { checked = new Set(perms.map((p) => p.key)); }}>Tout cocher</button>
        <button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => { checked = new Set(); }}>Tout décocher</button>
      </span>
    </div>
    <ul class="app-checks">
      {#each perms as p (p.key)}
        <li>
          <label class="plbls-checkbox">
            <input class="plbls-checkbox__input" type="checkbox" checked={checked.has(p.key)} onchange={() => toggle(p.key)} />
            <span class="plbls-checkbox__case" aria-hidden="true"></span>
            <span class="plbls-checkbox__texte"><span class="app-mono">{p.key}</span>{#if p.description}<br /><span class="app-sub">{p.description}</span>{/if}</span>
          </label>
        </li>
      {/each}
    </ul>
  </div>

  <div class="app-savebar">
    <span class="app-sub">{role ? (dirty ? "Modifications non enregistrées" : "") : "Rien n'est créé avant « Créer le rôle »."}</span>
    <span class="app-inline">
      {#if role}<button type="button" class="plbls-button plbls-button--discret" disabled={!dirty || busy} onclick={reset}>Annuler les changements</button>{/if}
      {#if onCancel}<button type="button" class="plbls-button plbls-button--discret" onclick={onCancel}>Fermer</button>{/if}
      <button class="plbls-button" disabled={!dirty || busy}>{role ? "Enregistrer" : "Créer le rôle"}</button>
    </span>
  </div>
</form>
