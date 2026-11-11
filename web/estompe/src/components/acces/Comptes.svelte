<script lang="ts">
  import Attente from './Attente.svelte'
  import { charger, enc, nomCompte, quand, type Catalogue, type PageComptes } from '../../lib/acces.svelte'
  import FicheCompte from './FicheCompte.svelte'

  let { catalogue }: { catalogue: Catalogue } = $props()
  let q = $state('')
  let recherche = $state('')
  let choisi = $state<string | null>(null)
  $effect(() => {
    const t = q.trim()
    const h = setTimeout(() => (recherche = t), 250)
    return () => clearTimeout(h)
  })
  const page = charger<PageComptes>(() => `/accounts?limit=100${recherche ? `&q=${enc(recherche)}` : ''}`)
</script>

<div class="perm-cols acc-cols">
  <section class="v-card perm-carte">
    <h2 class="perm-t2">Comptes</h2>
    <div class="plbls-field">
      <label class="plbls-field__label" for="acc-q">Rechercher</label>
      <input class="plbls-input" id="acc-q" type="search" bind:value={q} placeholder="e-mail, nom ou sub" autocomplete="off" />
    </div>
    {#if page.data === null}
      <Attente error={page.error} retry={page.reload} />
    {:else}
      {#if page.data.from_keycloak}<p class="v-note">{page.data.from_keycloak} compte(s) trouvé(s) dans Keycloak et ajoutés à l'annuaire.</p>{/if}
      <div class="perm-lignes">
        {#each page.data.accounts as c (c.sub)}
          <button type="button" class="perm-ligne" aria-current={choisi === c.sub} onclick={() => (choisi = c.sub)}>
            <b>{nomCompte(c)}</b>
            <span class="acc-puces">
              {#if !c.in_keycloak}<span class="v-tag dgr">supprimé de Keycloak</span>{/if}
              {#if c.roles_count}<span class="v-tag rose">{c.roles_count} rôle{c.roles_count > 1 ? 's' : ''}</span>{/if}
              {#if c.groups_count}<span class="v-tag info">{c.groups_count} groupe{c.groups_count > 1 ? 's' : ''}</span>{/if}
              {#if c.denies_count}<span class="v-tag dgr">{c.denies_count} refus</span>{/if}
              <span class="v-mute">vu {quand(c.last_seen_at)}</span>
            </span>
          </button>
        {:else}
          <p class="v-note">{recherche ? 'Aucun compte trouvé.' : "L'annuaire est vide : les comptes y entrent à leur première connexion ou par la synchronisation Keycloak."}</p>
        {/each}
      </div>
      {#if page.data.total > page.data.accounts.length}<p class="v-note">{page.data.accounts.length} sur {page.data.total} : affinez la recherche.</p>{/if}
    {/if}
  </section>
  {#if choisi}
    {#key choisi}<FicheCompte sub={choisi} {catalogue} onChange={page.reload} />{/key}
  {:else}
    <section class="v-card perm-carte perm-vide"><p class="v-note">Choisissez un compte pour voir et régler ses droits.</p></section>
  {/if}
</div>
