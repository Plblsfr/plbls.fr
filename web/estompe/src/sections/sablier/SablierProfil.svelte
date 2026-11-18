<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { jourLong, jourMoyen, majuscule, nomMois, valeur } from './formats'
  import { sablierApi, type Profil, type ValeurPublique } from './sablier'
  import SablierMedaille from './SablierMedaille.svelte'

  let { id, mien = false, onregler }: { id: string; mien?: boolean; onregler?: () => void } = $props()
  let profil = $state.raw<Profil | null>(null)
  let choix = $state<'mois' | 'total'>('mois')

  $effect(() => {
    const cible = id
    profil = null
    let actif = true
    sablierApi.profil(cible).then((p) => { if (actif) profil = p }, () => {})
    return () => { actif = false }
  })

  const aujourdhui = new Date()
  const moisCourant = majuscule(nomMois(`${aujourdhui.getFullYear()}-${String(aujourdhui.getMonth() + 1).padStart(2, '0')}-01`))
  const stats = $derived(!!profil && (profil.month.length > 0 || profil.total.length > 0 || !!profil.first_day))
  const visible = $derived(!!profil && (stats || profil.trophies.length > 0))
  const onglet = $derived<'mois' | 'total'>(profil && !profil.month.length ? 'total' : profil && !profil.total.length && !profil.first_day ? 'mois' : choix)
  const liste = $derived<ValeurPublique[]>(profil ? (onglet === 'mois' ? profil.month : profil.total) : [])
  const libelle = (v: ValeurPublique) => (v.domain === 'entracte' ? `${v.label} · Entracte` : v.domain === 'sillon' ? `${v.label} · Sillon` : v.label)
</script>

{#if profil && visible}
  <div class="sab-profil" data-bloc="stats-profil">
    {#if stats}
      <section class="sab-sec" aria-label="Stats publiques">
        <div class="sab-tete" style:align-items="center">
          <h3 class="sab-h3" style:display="flex" style:align-items="center" style:gap="8px"><Icon name="sablier" size={18} />Stats</h3>
          {#if profil.month.length && (profil.total.length || profil.first_day)}
            <div class="v-seg" role="group" aria-label="Période des stats">
              <button type="button" aria-pressed={onglet === 'mois'} onclick={() => (choix = 'mois')}>{moisCourant}</button>
              <button type="button" aria-pressed={onglet === 'total'} onclick={() => (choix = 'total')}>Total</button>
            </div>
          {:else}
            <span class="sab-note">{onglet === 'mois' ? moisCourant : 'Total'}</span>
          {/if}
        </div>
        {#if liste.length}
          <div class="sab-g sab-g2">
            {#each liste as v (v.key)}<div class="sab-card sab-card--creux"><div class="sab-tile"><span>{libelle(v)}</span><b>{valeur(v.value, v.unit)}</b></div></div>{/each}
          </div>
        {/if}
        {#if onglet === 'total' && profil.first_day}<p class="sab-note">Depuis le {jourLong(profil.first_day)}.</p>{/if}
      </section>
    {/if}
    {#if profil.trophies.length}
      <section class="sab-sec" aria-label="Trophées publics">
        <h3 class="sab-h3">Trophées</h3>
        <ul>
          {#each profil.trophies as t, n (n)}
            <li><SablierMedaille icone={t.icon} taille="m" /><div class="sab-col sab-grow" style:gap="2px"><p class="sab-h3">{t.name}</p><p class="sab-note">{t.tier >= 2 ? `Palier ${t.tier} · ` : ''}{jourMoyen(t.unlocked_at)}</p></div></li>
          {/each}
        </ul>
      </section>
    {/if}
    {#if mien && onregler}
      <div class="sab-ligne"><button class="plbls-button plbls-button--contour plbls-button--compact" type="button" onclick={onregler}>Régler ce que j’affiche</button></div>
    {/if}
  </div>
{/if}
