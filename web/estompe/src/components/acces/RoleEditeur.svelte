<script lang="ts">
  import { confirmer } from '../../lib/confirm.svelte'
  import Attente from '../permissions/Attente.svelte'
  import { charger, ecrire, executer, type Catalogue, type Effet, type RegleSaisie, type RoleDetail } from '../../lib/acces.svelte'
  import ChoixRessource from './ChoixRessource.svelte'
  import ListeLiaisons from './ListeLiaisons.svelte'
  import NomRessource from './NomRessource.svelte'

  let { id, catalogue, onChange, onDelete }: { id: string; catalogue: Catalogue; onChange: () => void; onDelete: () => void } = $props()
  const role = charger<RoleDetail>(() => `/roles/${id}`)
  type Choix = '' | Effet
  let nom = $state('')
  let description = $state('')
  let joker = $state<Record<string, Choix>>({})
  let precises = $state<RegleSaisie[]>([])
  let filtre = $state('')
  let modifie = $state(false)
  let ajoutPour = $state<string | null>(null)
  let ajoutEffet = $state<Effet>('deny')

  $effect(() => {
    const r = role.data
    if (!r) return
    nom = r.name
    description = r.description
    joker = Object.fromEntries(r.rules.filter((x) => x.resource === '*').map((x) => [x.permission, x.effect]))
    precises = r.rules.filter((x) => x.resource !== '*').map((x) => ({ permission: x.permission, resource: x.resource, effect: x.effect }))
    modifie = false
  })

  const groupes = $derived(catalogue.apps.map((a) => {
    const f = filtre.trim().toLowerCase()
    const perms = a.permissions.filter((p) => !p.obsolete_at && (!f || `${p.label} ${p.name} ${p.category}`.toLowerCase().includes(f)))
    const cats = [...new Set(perms.map((p) => p.category || 'Autres'))]
    return { app: a, cats: cats.map((c) => ({ nom: c, perms: perms.filter((p) => (p.category || 'Autres') === c) })) }
  }).filter((g) => g.cats.length))
  const compte = $derived(Object.values(joker).filter(Boolean).length + precises.length)

  const regler = (perm: string, v: Choix) => { joker = { ...joker, [perm]: v }; modifie = true }
  const toutRegler = (perms: { name: string }[], v: Choix) => { joker = { ...joker, ...Object.fromEntries(perms.map((p) => [p.name, v])) }; modifie = true }
  const ajouterPrecise = (perm: string, r: { id: string }) => {
    precises = [...precises.filter((x) => !(x.permission === perm && x.resource === r.id)), { permission: perm, resource: r.id, effect: ajoutEffet }]
    ajoutPour = null
    modifie = true
  }
  const retirerPrecise = (i: number) => { precises = precises.filter((_, j) => j !== i); modifie = true }

  const enregistrer = async () => {
    const rules: RegleSaisie[] = [
      ...Object.entries(joker).filter(([, v]) => v).map(([permission, effect]) => ({ permission, resource: '*', effect: effect as Effet })),
      ...precises,
    ]
    const r = await executer(ecrire<RoleDetail>('PUT', `/roles/${id}`, { name: nom, description, rules }), 'Rôle enregistré')
    if (r) { role.reload(); onChange() }
  }
  const supprimer = async () => {
    if (!(await confirmer({ titre: 'Supprimer le rôle', message: `Supprimer « ${nom} » ? Ses ${role.data?.bindings.length ?? 0} liaison(s) disparaissent.`, action: 'Supprimer', destructif: true }))) return
    if ((await executer(ecrire('DELETE', `/roles/${id}`), 'Rôle supprimé')) !== null) onDelete()
  }
</script>

<section class="v-card perm-carte acc-fiche">
  {#if role.data === null}
    <Attente error={role.error} retry={role.reload} />
  {:else}
    {@const r = role.data}
    <div class="perm-tete">
      <h2 class="perm-t">{r.name}</h2>
      {#if !r.system}<button class="plbls-button plbls-button--secondaire plbls-button--compact" onclick={supprimer}>Supprimer</button>{/if}
    </div>
    {#if r.system === 'tout-le-monde'}
      <p class="v-alert info"><span class="grow">Ce rôle s'applique à <b>tous les comptes connectés</b>. Retirer une permission ici la ferme pour tous, sauf pour qui la reçoit par un autre rôle, un groupe ou une règle directe.</span></p>
    {:else if r.system === 'admin'}
      <p class="v-alert dgr"><span class="grow">Ce rôle donne <b>aegis:admin</b> : l'administration de toutes les permissions.</span></p>
    {/if}
    <div class="perm-form">
      <div class="plbls-field"><label class="plbls-field__label" for="acc-re-nom">Nom</label><input class="plbls-input" id="acc-re-nom" bind:value={nom} oninput={() => (modifie = true)} disabled={!!r.system} maxlength="80" /></div>
      <div class="plbls-field"><label class="plbls-field__label" for="acc-re-desc">Description</label><input class="plbls-input" id="acc-re-desc" bind:value={description} oninput={() => (modifie = true)} maxlength="300" /></div>
      <div class="plbls-field"><label class="plbls-field__label" for="acc-re-f">Filtrer</label><input class="plbls-input" id="acc-re-f" type="search" bind:value={filtre} placeholder="permission, catégorie" /></div>
    </div>

    {#each groupes as g (g.app.key)}
      <h3 class="perm-t2">{g.app.name}</h3>
      {#each g.cats as c (c.nom)}
        <div class="acc-cat">
          <div class="perm-ligne-h">
            <h4 class="acc-cat-t">{c.nom}</h4>
            <span class="perm-en-ligne">
              <button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => toutRegler(c.perms, 'allow')}>Tout autoriser</button>
              <button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => toutRegler(c.perms, '')}>Tout effacer</button>
            </span>
          </div>
          <ul class="perm-cases">
            {#each c.perms as p (p.name)}
              <li class="acc-perm">
                <div class="acc-perm-t">
                  <b>{p.label}</b>{#if p.open_default}<span class="v-tag" title="Ouverte à tous par défaut lors de sa déclaration">ouverte par défaut</span>{/if}
                  <span class="v-note">{p.description}</span>
                  <span class="v-mono v-mute">{p.name}</span>
                </div>
                <div class="acc-tri" role="radiogroup" aria-label={p.label}>
                  {#each [['', 'Non défini'], ['allow', 'Autorisé'], ['deny', 'Refusé']] as [v, l] (v)}
                    <label class="acc-tri-o" data-v={v}><input type="radio" name="acc-{p.name}" checked={(joker[p.name] ?? '') === v} onchange={() => regler(p.name, v as Choix)} />{l}</label>
                  {/each}
                </div>
                {#if p.resource_type}
                  <div class="acc-precis">
                    {#each precises.map((x, i) => [x, i] as const).filter(([x]) => x.permission === p.name) as [x, i] (x.resource)}
                      <span class="v-tag {x.effect === 'allow' ? 'ok' : 'dgr'}">{x.effect === 'allow' ? 'autorisé' : 'refusé'} sur <NomRessource type="{p.app}:{p.resource_type}" id={x.resource} /> <button class="perm-suppr" aria-label="Retirer" onclick={() => retirerPrecise(i)}>×</button></span>
                    {/each}
                    {#if ajoutPour === p.name}
                      <span class="acc-ajout">
                        <select class="plbls-select" bind:value={ajoutEffet} aria-label="Effet"><option value="deny">Refuser sur…</option><option value="allow">Autoriser sur…</option></select>
                        <ChoixRessource id="acc-re-{p.key}" type="{p.app}:{p.resource_type}" onPick={(res) => ajouterPrecise(p.name, res)} />
                      </span>
                    {:else}
                      <button type="button" class="plbls-button plbls-button--lien plbls-button--compact" onclick={() => (ajoutPour = p.name)}>+ exception sur une ressource</button>
                    {/if}
                  </div>
                {/if}
              </li>
            {/each}
          </ul>
        </div>
      {/each}
    {/each}

    <h3 class="perm-t2">Comptes et groupes qui ont ce rôle</h3>
    {#if r.system === 'tout-le-monde'}<p class="v-note">Tous les comptes ({r.accounts_count} dans l'annuaire).</p>
    {:else}<ListeLiaisons liaisons={r.bindings} onChange={() => { role.reload(); onChange() }} sujet vide="Personne n'a ce rôle." />{/if}

    <div class="perm-barre">
      <span class="v-note">{compte} règle{compte > 1 ? 's' : ''}{modifie ? ' · modifications non enregistrées' : ''}</span>
      <span class="perm-en-ligne">
        <button class="plbls-button plbls-button--secondaire" disabled={!modifie} onclick={() => role.reload()}>Annuler</button>
        <button class="plbls-button" disabled={!modifie} onclick={enregistrer}>Enregistrer</button>
      </span>
    </div>
  {/if}
</section>
