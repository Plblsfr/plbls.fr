<script lang="ts">
  import { ecrire, executer, versIso, type Catalogue, type Effet, type Sujet } from '../../lib/acces.svelte'
  import ChoixRessource from './ChoixRessource.svelte'

  let { catalogue, sujet, id, onDone }: { catalogue: Catalogue; sujet: Exclude<Sujet, 'role'>; id: string; onDone: () => void } = $props()
  const uid = $props.id()
  let permission = $state('')
  let effet = $state<Effet>('allow')
  let ressource = $state('*')
  let ressourceNom = $state('')
  let expire = $state('')
  let note = $state('')
  const perms = $derived(catalogue.apps.flatMap((a) => a.permissions.filter((p) => !p.obsolete_at).map((p) => ({ ...p, appName: a.name }))))
  const choisie = $derived(perms.find((p) => p.name === permission))
  const typeRessource = $derived(choisie?.resource_type ? `${choisie.app}:${choisie.resource_type}` : null)

  const envoyer = async () => {
    const r = await executer(ecrire('POST', '/rules', { subject_kind: sujet, subject_id: id, permission, resource: ressource, effect: effet, expires_at: versIso(expire), note }), effet === 'allow' ? 'Permission accordée' : 'Permission refusée')
    if (r !== null) { permission = ''; ressource = '*'; ressourceNom = ''; expire = ''; note = ''; onDone() }
  }
</script>

<form class="perm-form perm-form--haut" onsubmit={(e) => { e.preventDefault(); void envoyer() }}>
  <div class="plbls-field">
    <label class="plbls-field__label" for="{uid}-p">Permission</label>
    <select class="plbls-select" id="{uid}-p" bind:value={permission} onchange={() => { ressource = '*'; ressourceNom = '' }} required>
      <option value="">Choisir…</option>
      {#each catalogue.apps as a (a.key)}
        <optgroup label={a.name}>
          {#each a.permissions.filter((p) => !p.obsolete_at) as p (p.name)}<option value={p.name}>{p.category ? `${p.category} · ` : ''}{p.label}</option>{/each}
        </optgroup>
      {/each}
    </select>
  </div>
  <div class="plbls-field">
    <label class="plbls-field__label" for="{uid}-e">Effet</label>
    <select class="plbls-select" id="{uid}-e" bind:value={effet}><option value="allow">Accorder</option><option value="deny">Refuser</option></select>
  </div>
  {#if typeRessource}
    <div class="plbls-field perm-compte">
      <label class="plbls-field__label" for="{uid}-r">Portée</label>
      {#if ressource === '*'}
        <p class="perm-en-ligne"><b>Partout</b></p>
        <ChoixRessource id="{uid}-r" type={typeRessource} onPick={(r) => { ressource = r.id; ressourceNom = r.name }} />
      {:else}
        <p class="perm-en-ligne"><b>{ressourceNom || ressource}</b><button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => { ressource = '*'; ressourceNom = '' }}>Partout</button></p>
      {/if}
    </div>
  {/if}
  <div class="plbls-field">
    <label class="plbls-field__label" for="{uid}-x">Expire le (facultatif)</label>
    <input class="plbls-input" id="{uid}-x" type="datetime-local" bind:value={expire} />
  </div>
  <div class="plbls-field">
    <label class="plbls-field__label" for="{uid}-n">Note</label>
    <input class="plbls-input" id="{uid}-n" bind:value={note} maxlength="200" placeholder="raison, ticket…" />
  </div>
  <button class="plbls-button" disabled={!permission}>{effet === 'allow' ? 'Accorder' : 'Refuser'}</button>
</form>
{#if choisie}<p class="v-note">{choisie.description}{choisie.resource_type ? ' · se restreint à une ressource' : ''}</p>{/if}
