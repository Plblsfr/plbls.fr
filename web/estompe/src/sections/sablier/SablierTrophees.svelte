<script lang="ts">
  import { pluriel } from './formats'
  import { enCours, enPeriode } from './periodes'
  import type { Periode, TropheeDebloque, Trophees } from './sablier'
  import SablierMedaille from './SablierMedaille.svelte'
  import { dateDeblocage, estExploit, pourcentage, texteAvancement, textePalier, vueTrophees } from './stats'

  let { trophees, periode, date, aujourdhui }: { trophees: Trophees; periode: Periode; date: string; aujourdhui: string } = $props()
  const APERCU = 4
  let tout = $state(false)
  let toutEnCours = $state(false)
  const vue = $derived(vueTrophees(trophees, periode, date))
  const partiel = $derived(periode !== 'all' && vue.tous.length > vue.recents.length)
  const liste = $derived(tout || periode === 'all' ? vue.tous : vue.recents)
  const quand = $derived(periode === 'week' ? (enCours(periode, date, aujourdhui) ? 'cette semaine' : enPeriode(periode, date)) : enPeriode(periode, date))
  const titre = $derived(tout || periode === 'all' ? 'Débloqués' : `Débloqués ${quand}`)
  const enCoursVisibles = $derived(toutEnCours ? vue.enCours : vue.enCours.slice(0, APERCU))
  const sansPoint = (s: string) => s.replace(/\.$/, '')
</script>

{#snippet debloque(t: TropheeDebloque)}
  <li class="sab-tro" data-trophee={t.key}>
    <SablierMedaille icone={t.icon} />
    <div class="sab-col">
      <p class="sab-h3">{t.name}</p>
      {#if t.hidden}<span class="sab-etiquette sab-etiquette--rose" style:align-self="flex-start">Caché, trouvé</span>
      {:else if estExploit(t)}<span class="sab-etiquette sab-etiquette--rose" style:align-self="flex-start">Exploit</span>
      {:else}<div class="sab-pal" role="img" aria-label={textePalier(t)}>{#each { length: t.tiers } as _, i (i)}<i class:on={i < t.tier}></i>{/each}</div>{/if}
      <p class="sab-note">{sansPoint(t.description)} · {dateDeblocage(t.unlocked_at, aujourdhui)}</p>
    </div>
  </li>
{/snippet}

{#if vue.debloques > 0 || vue.enCours.length || vue.cachesRestants > 0}
  <section class="sab-sec" aria-label="Trophées">
    <div class="sab-tete">
      <h2 class="sab-h2">Trophées</h2>
      {#if partiel}
        <button class="plbls-button plbls-button--contour plbls-button--compact" type="button" aria-pressed={tout} onclick={() => (tout = !tout)}>Les {vue.debloques} débloqués sur {vue.total}</button>
      {:else if vue.debloques > 0}
        <p class="sab-note">{vue.debloques} débloqués sur {vue.total}</p>
      {/if}
    </div>

    {#if liste.length}
      <h3 class="sab-h3">{titre}</h3>
      <ul class="sab-g sab-g3">{#each liste as t (t.id)}{@render debloque(t)}{/each}</ul>
    {/if}

    {#if vue.enCours.length}
      <h3 class="sab-h3">En cours</h3>
      <ul class="sab-g sab-g2">
        {#each enCoursVisibles as t (t.id)}
          <li class="sab-tro" data-trophee={t.key}>
            <SablierMedaille icone={t.icon} etat="cours" />
            <div class="sab-col sab-grow">
              <div class="sab-ligne" style:justify-content="space-between"><p class="sab-h3">{t.name}</p><p class="sab-note">{texteAvancement(t)}</p></div>
              <div class="sab-prog" role="progressbar" aria-valuenow={pourcentage(t.value, t.next_threshold)} aria-valuemin={0} aria-valuemax={100} aria-label="{t.name}{t.tiers > 1 ? `, palier ${t.tier + 1}` : ''}"><i style:width="{pourcentage(t.value, t.next_threshold)}%"></i></div>
              <p class="sab-note">{t.tiers > 1 ? `Palier ${t.tier + 1} sur ${t.tiers} · ` : ''}{sansPoint(t.description)}</p>
            </div>
          </li>
        {/each}
      </ul>
      {#if vue.enCours.length > APERCU}
        <div class="sab-ligne"><button class="plbls-button plbls-button--contour plbls-button--compact" type="button" aria-pressed={toutEnCours} onclick={() => (toutEnCours = !toutEnCours)}>{toutEnCours ? 'Les plus proches seulement' : `Les ${vue.enCours.length} trophées en cours`}</button></div>
      {/if}
    {/if}

    {#if vue.cachesTrouves.length || vue.cachesRestants > 0}
      <h3 class="sab-h3">Cachés</h3>
      <ul class="sab-g sab-g3">
        {#each vue.cachesTrouves as t (t.id)}{@render debloque(t)}{/each}
        {#if vue.cachesRestants > 0}
          <li class="sab-tro sab-tro--cache" data-trophee="caches">
            <SablierMedaille icone="interro" etat="cache" />
            <div class="sab-col"><p class="sab-h3">{pluriel(vue.cachesRestants, 'trophée caché', 'trophées cachés')}</p><p class="sab-note">{vue.cachesRestants >= 2 ? 'Ils se montrent seulement une fois débloqués.' : 'Il se montre seulement une fois débloqué.'}</p></div>
          </li>
        {/if}
      </ul>
    {/if}
  </section>
{/if}
