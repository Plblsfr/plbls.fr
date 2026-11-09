<script lang="ts">
  import { ACTIONS, nomCompte, quand, tonAction, typeDe, type EntreeJournal, type PermissionVue } from '../../lib/acces.svelte'
  import NomRessource from './NomRessource.svelte'

  let { ligne, noms }: { ligne: EntreeJournal; noms: Map<string, PermissionVue> } = $props()
  const d = $derived(ligne.detail ?? {})
  const cible = $derived(ligne.target_kind === 'permission' ? (noms.get(ligne.target_id)?.label ?? ligne.target_label ?? ligne.target_id) : (ligne.target_label ?? ligne.target_id))
  const acteur = $derived(ligne.actor.startsWith('service:') || ligne.actor.startsWith('manifeste:') || ligne.actor === 'aegis' ? ligne.actor : nomCompte(ligne.actor_account, ligne.actor))
  const extra = $derived([
    typeof d.expires_at === 'string' ? `jusqu'au ${quand(d.expires_at)}` : '',
    typeof d.note === 'string' && d.note ? `« ${d.note} »` : '',
    Array.isArray(d.added) && d.added.length ? `+ ${d.added.join(', ')}` : '',
    Array.isArray(d.removed) && d.removed.length ? `− ${d.removed.join(', ')}` : '',
  ].filter(Boolean).join(' · '))
</script>

<li class="acc-ligne-journal">
  <span class="v-mono v-mute">{quand(ligne.at)}</span>
  <span><b>{acteur}</b> · <span class="v-tag {tonAction(ligne.action)}">{ACTIONS[ligne.action] ?? ligne.action}</span> · {cible}{#if ligne.subject && ligne.target_kind !== 'account'} pour <b>{nomCompte(ligne.subject_account, ligne.subject)}</b>{/if}</span>
  {#if typeof d.resource === 'string' && d.resource !== '*'}<span class="v-note">sur <NomRessource type={typeDe(noms, ligne.target_id)} id={d.resource} /></span>{/if}
  {#if typeof d.scope_id === 'string'}<span class="v-note">sur <NomRessource type={typeof d.scope_type === 'string' ? d.scope_type : null} id={d.scope_id} /></span>{/if}
  {#if extra}<span class="v-note">{extra}</span>{/if}
</li>
