<script lang="ts">
  import { pluriel } from './formats'
  import type { Trophees } from './sablier'
  import SablierMedaille from './SablierMedaille.svelte'
  import { collection, dateDeblocage, NOM_MATIERE, type Piece } from './stats'

  let { trophees, aujourdhui }: { trophees: Trophees; aujourdhui: string } = $props()
  const vue = $derived(collection(trophees))
  const anneau = (p: Piece) => ({ pct: p.pct ?? 100, vers: p.vers })
  const matiere = (p: Piece) => (p.cache ? 'Caché, trouvé' : p.palier ? `${NOM_MATIERE[p.matiere]} · ${p.palier.toLowerCase()}` : NOM_MATIERE[p.matiere])
  const note = (p: Piece) => (p.pct !== null ? p.avancement : p.debloque ? `Débloqué ${dateDeblocage(p.debloque, aujourdhui)}` : '')
</script>

{#if vue.proches.length}
  <section class="sab-sec" aria-label="Presque là">
    <div class="sab-tete">
      <h2 class="sab-h2">Presque là</h2>
      <p class="sab-note">{vue.proches.length >= 2 ? `Les ${vue.proches.length} trophées les plus proches` : 'Le trophée le plus proche'}</p>
    </div>
    <ul class="sab-g sab-g-proches">
      {#each vue.proches as p (p.id)}
        <li class="sab-tro" data-proche={p.key}>
          <SablierMedaille icone={p.icone} taille="g" matiere={p.matiere} anneau={anneau(p)} />
          <div class="sab-col">
            <p class="sab-surtitre sab-mat" data-matiere={p.vers}>{p.cible} · {p.pct} %</p>
            <p class="sab-h3">{p.nom}</p>
            <p class="sab-proche-num">{p.avancement}</p>
            <p class="sab-note">{p.description}</p>
          </div>
        </li>
      {/each}
    </ul>
  </section>
{/if}

{#if vue.pieces.length || vue.cachesRestants > 0}
  <section class="sab-sec" aria-label="Trophées">
    <div class="sab-tete">
      <div class="sab-col">
        <h2 class="sab-h2">Collection</h2>
        {#if vue.debloques > 0}<p class="sab-note">{vue.debloques} débloqués sur {vue.total}</p>{/if}
      </div>
      {#if vue.debloques > 0}
        <ul class="sab-ligne" aria-label="Médailles par matière">
          {#each vue.compte as c (c.matiere)}<li class="sab-etiquette"><i class="sab-point" data-matiere={c.matiere} aria-hidden="true"></i>{c.nom} · {c.n}</li>{/each}
        </ul>
      {/if}
    </div>
    <ul class="sab-g sab-g-pieces">
      {#each vue.pieces as p (p.id)}
        <li class="sab-piece" data-trophee={p.key}>
          <SablierMedaille icone={p.icone} taille="g" matiere={p.matiere} anneau={anneau(p)} nomAnneau={p.pct !== null ? `${p.nom}${p.palierVise > 1 ? `, palier ${p.palierVise}` : ''}` : ''} />
          <p class="sab-h3">{p.nom}</p>
          <p class="sab-mat" data-matiere={p.matiere}>{matiere(p)}</p>
          <p class="sab-note">{p.description}</p>
          {#if note(p)}<p class="sab-note sab-piece-note">{note(p)}</p>{/if}
        </li>
      {/each}
      {#if vue.cachesRestants > 0}
        <li class="sab-piece sab-piece--cache" data-trophee="caches">
          <SablierMedaille icone="interro" taille="g" etat="cache" />
          <p class="sab-h3">{pluriel(vue.cachesRestants, 'trophée caché', 'trophées cachés')}</p>
          <p class="sab-note">{vue.cachesRestants >= 2 ? 'Ils se montrent seulement une fois débloqués.' : 'Il se montre seulement une fois débloqué.'}</p>
        </li>
      {/if}
    </ul>
  </section>
{/if}
