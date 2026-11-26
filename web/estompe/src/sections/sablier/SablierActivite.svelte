<script lang="ts">
  import { jourMoyen, pluriel } from './formats'
  import type { JourDuree } from './sablier'
  import { grilleActivite } from './stats'

  let { activite, aujourdhui }: { activite: JourDuree[]; aujourdhui: string } = $props()
  const grille = $derived(grilleActivite(activite, aujourdhui))
  const auBout = (cadre: HTMLElement) => { cadre.scrollLeft = cadre.scrollWidth }
  const resume = $derived(pluriel(grille.actifs, 'jour actif', 'jours actifs'))
</script>

{#if grille.actifs > 0}
  <section class="sab-sec" aria-label="Activité des douze derniers mois">
    <div class="sab-tete">
      <h2 class="sab-h2">Douze derniers mois</h2>
      <p class="sab-note">{resume}</p>
    </div>
    <div class="sab-card">
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div class="sab-scroll" role="group" aria-label="Grille d’activité, défilable" tabindex="0" use:auBout>
        <div class="sab-chaleur">
          <div class="sab-chaleur-jours" aria-hidden="true"><span>Lun</span><span></span><span>Mer</span><span></span><span>Ven</span><span></span><span></span></div>
          <div>
            <div class="sab-chaleur-mois" aria-hidden="true">{#each grille.mois as m, i (i)}<span>{m}</span>{/each}</div>
            <div class="sab-chaleur-grille" role="img" aria-label="{resume} du {jourMoyen(grille.du)} au {jourMoyen(aujourdhui)}, une case par jour, une colonne par semaine">
              {#each grille.cases as c (c.jour)}<i data-n={c.niveau} data-futur={c.futur ? '' : undefined} data-auj={c.aujourdhui ? '' : undefined} title={c.titre}></i>{/each}
            </div>
          </div>
        </div>
      </div>
      <div class="sab-tete sab-chaleur-pied">
        <p class="sab-note">Plus la case est rose, plus la journée a compté de temps actif.</p>
        <div class="sab-chaleur-legende" aria-hidden="true"><span>Moins</span><i data-n="0"></i><i data-n="1"></i><i data-n="2"></i><i data-n="3"></i><i data-n="4"></i><span>Plus</span></div>
      </div>
    </div>
  </section>
{/if}
