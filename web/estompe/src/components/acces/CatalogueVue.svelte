<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import { charger, ecrire, enc, executer, libellesPermissions, quand, typeDe, type Catalogue, type Detenteur } from '../../lib/acces.svelte'
  import NomRessource from './NomRessource.svelte'

  let { catalogue, onChange }: { catalogue: Catalogue; onChange: () => void } = $props()
  let ouverte = $state<string | null>(null)
  const detenteurs = charger<Detenteur[]>(() => (ouverte ? `/permissions/${enc(ouverte)}/holders` : null))
  const obsoletes = $derived(catalogue.apps.flatMap((a) => a.permissions.filter((p) => p.obsolete_at)).length)
  const purger = async () => {
    if (!(await confirmer({ titre: 'Purger les permissions obsolètes', message: `Supprimer ${obsoletes} permission(s) retirée(s) par leur application, et leurs règles ?`, action: 'Purger', destructif: true }))) return
    if ((await executer(ecrire('POST', '/permissions/purge-obsolete'), 'Permissions purgées')) !== null) onChange()
  }
  const noms = $derived(libellesPermissions(catalogue))
  const sujet = (d: Detenteur) => (d.rule.subject_kind === 'role' ? 'rôle' : d.rule.subject_kind === 'group' ? 'groupe' : 'compte')
</script>

<div class="perm-pile">
  {#if obsoletes}
    <div class="v-alert info"><span class="grow">{obsoletes} permission(s) ne sont plus déclarées par leur application : leurs règles sont gardées mais ignorées.</span><button class="plbls-button plbls-button--lien plbls-button--compact" onclick={purger}>Purger</button></div>
  {/if}
  {#each catalogue.apps as a (a.key)}
    <section class="v-card perm-carte">
      <div class="perm-tete">
        <h2 class="perm-t2">{a.name} <span class="v-mono v-mute">{a.key}</span></h2>
        <span class="v-note">manifeste reçu {quand(a.seen_at)}{a.resource_types.length ? ` · ressources : ${a.resource_types.map((t) => t.name).join(', ')}` : ''}</span>
      </div>
      <div class="perm-defile">
        <table class="perm-tb">
          <thead><tr><th scope="col">Permission</th><th scope="col">Catégorie</th><th scope="col">Ressource</th><th scope="col">Défaut</th><th scope="col">Règles</th></tr></thead>
          <tbody>
            {#each a.permissions as p (p.name)}
              <tr class:acc-obsolete={!!p.obsolete_at}>
                <td><button type="button" class="acc-lien" aria-expanded={ouverte === p.name} onclick={() => (ouverte = ouverte === p.name ? null : p.name)}><b>{p.label}</b></button><br /><span class="v-mono v-mute">{p.name}</span><br /><span class="v-note">{p.description}</span></td>
                <td>{p.category || '—'}</td>
                <td>{p.resource_type ?? '—'}</td>
                <td>{#if p.open_default}<span class="v-tag ok">ouverte</span>{:else}<span class="v-tag">fermée</span>{/if}</td>
                <td>{p.rules_count}</td>
              </tr>
              {#if ouverte === p.name}
                <tr><td colspan="5">
                  {#if detenteurs.data === null}<span class="v-note">Chargement…</span>
                  {:else if detenteurs.data.length === 0}<span class="v-note">Aucune règle ne mentionne cette permission : refusée à tous.</span>
                  {:else}<ul class="acc-sources">{#each detenteurs.data as d (d.rule.id)}<li><span class="v-tag {d.rule.effect === 'allow' ? 'ok' : 'dgr'}">{d.rule.effect === 'allow' ? 'accorde' : 'refuse'}</span> {sujet(d)} <b>{d.subject_label ?? d.rule.subject_id}</b>{#if d.rule.resource !== '*'} · <NomRessource type={typeDe(noms, d.rule.permission)} id={d.rule.resource} />{/if}{d.rule.expires_at ? ` · jusqu'au ${quand(d.rule.expires_at)}` : ''}</li>{/each}</ul>{/if}
                </td></tr>
              {/if}
            {/each}
          </tbody>
        </table>
      </div>
    </section>
  {/each}
</div>
