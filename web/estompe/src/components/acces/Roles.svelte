<script lang="ts">
  import Icon from '../Icon.svelte'
  import Attente from './Attente.svelte'
  import { charger, ecrire, executer, lire, type Catalogue, type Effet, type RoleDetail, type RoleResume } from '../../lib/acces.svelte'
  import RoleEditeur from './RoleEditeur.svelte'

  let { catalogue }: { catalogue: Catalogue } = $props()
  const roles = charger<RoleResume[]>(() => '/roles')
  let details = $state.raw<Record<string, RoleDetail>>({})
  let choisi = $state<string | null>(null)
  let cible = $state<string | null>(null)
  let app = $state('')
  let nom = $state('')

  $effect(() => {
    const liste = roles.data
    if (!liste) return
    let perime = false
    void Promise.all(liste.map((r) => lire<RoleDetail>(`/roles/${r.id}`).catch(() => null))).then((ds) => {
      if (!perime) details = Object.fromEntries(ds.filter((x): x is RoleDetail => !!x).map((x) => [x.id, x]))
    })
    return () => { perime = true }
  })

  const courante = $derived(catalogue.apps.find((a) => a.key === app) ?? catalogue.apps[0])
  const categories = $derived.by(() => {
    const perms = (courante?.permissions ?? []).filter((p) => !p.obsolete_at)
    return [...new Set(perms.map((p) => p.category || 'Autres'))].map((c) => ({ nom: c, perms: perms.filter((p) => (p.category || 'Autres') === c) }))
  })
  const ETAT: Record<Effet | '', { mot: string; icone: 'ok' | 'x' | 'minus' }> = { allow: { mot: 'autorise', icone: 'ok' }, deny: { mot: 'refuse', icone: 'x' }, '': { mot: 'ne dit rien', icone: 'minus' } }
  const cellule = (role: string, permission: string) => {
    const regles = details[role]?.rules.filter((x) => x.permission === permission) ?? []
    return { effet: (regles.find((x) => x.resource === '*')?.effect ?? '') as Effet | '', exceptions: regles.filter((x) => x.resource !== '*').length }
  }
  const qui = (r: RoleResume) => (r.system === 'tout-le-monde' ? 'tous les comptes' : `${r.accounts_count} compte${r.accounts_count > 1 ? 's' : ''}`)
  const ouvrir = (id: string, permission: string | null = null) => { cible = permission; choisi = id }
  const creer = async (e: SubmitEvent) => {
    e.preventDefault()
    const r = await executer(ecrire<RoleDetail>('POST', '/roles', { name: nom, rules: [] }), 'Rôle créé')
    if (r) { nom = ''; roles.reload(); ouvrir(r.id) }
  }
</script>

{#if choisi}
  {#key choisi}<RoleEditeur id={choisi} {catalogue} {cible} appInitiale={courante?.key} onChange={roles.reload} onBack={() => { choisi = null; roles.reload() }} onDelete={() => { choisi = null; roles.reload() }} />{/key}
{:else if roles.data === null}
  <Attente error={roles.error} retry={roles.reload} />
{:else}
  <div class="acc-outils">
    <div class="v-seg" role="group" aria-label="Application">
      {#each catalogue.apps as a (a.key)}<button aria-pressed={courante?.key === a.key} onclick={() => (app = a.key)}>{a.name}</button>{/each}
    </div>
    <ul class="acc-legende" aria-label="Légende">
      <li><span class="acc-case" data-e="allow"><Icon name="ok" size={12} /></span>autorise</li>
      <li><span class="acc-case" data-e="deny"><Icon name="x" size={12} /></span>refuse</li>
      <li><span class="acc-case" data-e=""><Icon name="minus" size={12} /></span>ne dit rien</li>
    </ul>
  </div>
  <section class="v-card acc-grille-c">
    <div class="perm-defile">
      <table class="acc-grille">
        <thead>
          <tr>
            <th scope="col" class="acc-grille-p">Permission</th>
            {#each roles.data as r (r.id)}
              <th scope="col"><button class="acc-grille-r" onclick={() => ouvrir(r.id)}><b>{r.name}</b><span class="v-note">{qui(r)}</span><span class="acc-grille-m">Modifier</span></button></th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each categories as c (c.nom)}
            <tr class="acc-grille-cat"><th scope="colgroup" colspan={roles.data.length + 1}>{c.nom}</th></tr>
            {#each c.perms as p (p.name)}
              <tr>
                <th scope="row" class="acc-grille-p"><span>{p.label}</span><span class="v-mono v-mute">{p.name}</span></th>
                {#each roles.data as r (r.id)}
                  {@const x = cellule(r.id, p.name)}
                  <td>
                    <button class="acc-case" data-e={x.effet} aria-label="{r.name} {ETAT[x.effet].mot} {p.label}{x.exceptions ? `, ${x.exceptions} exception${x.exceptions > 1 ? 's' : ''}` : ''}" onclick={() => ouvrir(r.id, p.name)}><Icon name={ETAT[x.effet].icone} size={14} /></button>
                    {#if x.exceptions}<span class="acc-case-x">{x.exceptions} exception{x.exceptions > 1 ? 's' : ''}</span>{/if}
                  </td>
                {/each}
              </tr>
            {/each}
          {:else}
            <tr><td colspan={roles.data.length + 1} class="v-note">Cette application ne déclare aucune permission.</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>
  <p class="v-note">Une case ouvre l'éditeur du rôle sur cette permission. « Tout le monde » s'applique à chaque compte connecté, les autres rôles s'y ajoutent ; un refus l'emporte toujours sur un accord.</p>
  <details class="perm-nouveau acc-nouveau">
    <summary>Nouveau rôle</summary>
    <form class="perm-form" onsubmit={creer}>
      <div class="plbls-field"><label class="plbls-field__label" for="acc-role-nom">Nom</label><input class="plbls-input" id="acc-role-nom" bind:value={nom} maxlength="80" required /></div>
      <button class="plbls-button">Créer</button>
    </form>
  </details>
{/if}
