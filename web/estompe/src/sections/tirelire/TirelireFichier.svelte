<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { ecart, euros, instant, periode, pluriel } from './formats'
  import { CONTROLES, messageRefus, raisonControle, type Issue } from './tirelire'

  let { issue, occupe = false, onretry }: { issue: Issue; occupe?: boolean; onretry?: () => void } = $props()
</script>

{#if issue.status === 'imported'}
  <section class="tir-carte tir-carte--serre" aria-label="Fichier importé : {issue.file}" data-issue="imported">
    <div class="tir-ligne tir-ligne--haut">
      <span class="tir-pas tir-pas--ok"><Icon name="ok" size={20} /></span>
      <div class="tir-col tir-col--serre tir-grandit"><span class="tir-h3">{issue.file}</span><span class="tir-note">{periode(issue.statement.from, issue.statement.to)} · {pluriel(issue.statement.operations, 'opération', 'opérations')}</span></div>
      <span class="tir-etq tir-etq--ok">Importé</span>
    </div>
    <div class="tir-ligne tir-ligne--plis"><span class="tir-etq">totaux vérifiés</span><span class="tir-etq">{pluriel(issue.statement.pages, 'page', 'pages')}</span></div>
  </section>
{:else if issue.status === 'already_imported'}
  <section class="tir-carte tir-carte--serre" aria-label="Fichier déjà importé : {issue.file}" data-issue="already_imported">
    <div class="tir-ligne tir-ligne--haut">
      <span class="tir-pas tir-pas--info"><Icon name="file" size={20} /></span>
      <div class="tir-col tir-col--serre tir-grandit"><span class="tir-h3">{issue.file}</span><span class="tir-note">Déjà importé le {instant(issue.statement.imported_at)} · aucune opération ajoutée</span></div>
      <span class="tir-etq tir-etq--info">Déjà importé</span>
    </div>
  </section>
{:else}
  {@const refus = issue.status === 'rejected' ? issue.error : null}
  {@const controle = refus?.code === 'checks_failed'}
  <section class="tir-carte tir-carte--alerte" aria-label="{refus ? 'Fichier refusé' : 'Fichier non envoyé'} : {issue.file}" data-issue={issue.status}>
    <div class="tir-ligne tir-ligne--haut">
      <span class="tir-pas tir-pas--danger"><Icon name="alerte" size={20} /></span>
      <div class="tir-col tir-col--serre tir-grandit"><span class="tir-h3">{issue.file}</span><span class="tir-note tir-note--clair">Rien n’a été importé de ce fichier.</span></div>
      <span class="tir-etq tir-etq--danger">{refus ? 'Refusé' : 'Non envoyé'}</span>
    </div>
    {#if issue.status === 'failed'}
      <p role="alert">{issue.message}</p>
    {:else if refus && controle}
      <p role="alert"><span class="tir-fort">Le contrôle a échoué :</span> {raisonControle(refus)}</p>
      {#if refus.gaps?.length}
        <table class="tir-ctl">
          <thead><tr><th scope="col">Contrôle</th><th scope="col">Lu</th><th scope="col">Relevé</th><th scope="col">Écart</th></tr></thead>
          <tbody>
            {#each refus.gaps as g (g.check)}
              <tr><th scope="row">{CONTROLES[g.check] ?? g.check}</th><td>{euros(g.read)}</td><td>{euros(g.expected)}</td><td class="tir-fort" class:tir-hausse={g.read !== g.expected} class:tir-egal={g.read === g.expected}>{ecart(g.read - g.expected)}</td></tr>
            {/each}
          </tbody>
        </table>
      {/if}
      {#if refus.missing?.length}
        <div class="tir-col">
          <span class="tir-h3">Introuvable sur le relevé</span>
          <ul class="tir-ligne tir-ligne--plis">{#each refus.missing as m (m)}<li class="tir-etq tir-etq--danger">{m}</li>{/each}</ul>
        </div>
      {/if}
      {#if refus.unrecognized_lines?.length}
        <div class="tir-col">
          <span class="tir-h3">{pluriel(refus.unrecognized_lines.length, 'ligne non reconnue', 'lignes non reconnues')}</span>
          <ol class="tir-mono tir-bloc tir-inconnues" aria-label="Lignes non reconnues">
            {#each refus.unrecognized_lines as l, i (i)}<li><span>page {l.page}</span>{l.text}</li>{/each}
          </ol>
        </div>
      {/if}
    {:else if refus}
      <p role="alert"><span class="tir-fort">{messageRefus(refus)}</span></p>
    {/if}
    {#if onretry}<div class="tir-ligne"><button class="tir-btn tir-btn--contour tir-btn--grandit" disabled={occupe} onclick={onretry}>Réessayer</button></div>{/if}
  </section>
{/if}
