<script lang="ts">
  import Icon from '../Icon.svelte'
  import Attente from './Attente.svelte'
  import { charger, enc, initiales, nomCompte, quand, teinte, type Catalogue, type CompteResume, type PageComptes } from '../../lib/acces.svelte'
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
  const pluriel = (n: number, mot: string) => `${n} ${mot}${n > 1 ? 's' : ''}`
  const resume = (c: CompteResume) =>
    [c.roles_count ? pluriel(c.roles_count, 'rôle') : '', c.groups_count ? pluriel(c.groups_count, 'groupe') : '', c.denies_count ? `${c.denies_count} refus` : ''].filter(Boolean).join(' · ') || `vu ${quand(c.last_seen_at)}`
</script>

<div class="perm-cols acc-cols" class:acc-ouvert={!!choisi}>
  <section class="acc-annuaire" aria-label="Comptes">
    <label class="acc-cherche">
      <Icon name="cherche" size={16} />
      <span class="perm-sr">Rechercher un compte</span>
      <input id="acc-q" type="search" bind:value={q} placeholder="E-mail, nom ou sub" autocomplete="off" />
    </label>
    {#if page.data === null}
      <Attente error={page.error} retry={page.reload} />
    {:else}
      {#if page.data.from_keycloak}<p class="v-note">{page.data.from_keycloak} compte(s) trouvé(s) dans Keycloak et ajoutés à l'annuaire.</p>{/if}
      <div class="perm-lignes">
        {#each page.data.accounts as c (c.sub)}
          <button type="button" class="acc-compte" aria-current={choisi === c.sub} onclick={() => (choisi = c.sub)}>
            <span class="acc-av acc-av--{teinte(c.sub)}" aria-hidden="true">{initiales(c)}</span>
            <span class="acc-compte-t">
              <b class="truncate">{nomCompte(c)}</b>
              <span class="acc-compte-r">{resume(c)}{#if !c.in_keycloak} · <span class="v-tag dgr">supprimé de Keycloak</span>{/if}</span>
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
    {#key choisi}<FicheCompte sub={choisi} {catalogue} onChange={page.reload} onBack={() => (choisi = null)} />{/key}
  {:else}
    <section class="v-card perm-carte perm-vide"><p class="v-note">Choisissez un compte pour voir et régler ses droits.</p></section>
  {/if}
</div>
