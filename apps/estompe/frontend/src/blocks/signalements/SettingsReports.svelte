<script lang="ts">
  import { onMount } from 'svelte'
  import { api } from '../../lib/api'
  import { REPORT_CATEGORIES, REPORT_STATUS, label } from './report'
  import { openReport } from './report.svelte'
  import { notify } from '../../lib/ui'
  import type { Report } from '../../lib/types'

  let list = $state<Report[]>([])
  onMount(() => { api.myReports().then((l) => (list = l)).catch((e) => notify.error(e, 'Chargement des signalements impossible')) })
</script>

<p class="muted small">Un problème ? Décrivez-le, joignez une capture : les administrateurs le verront. Raccourci : Ctrl+Alt+B (PC).</p>
<button class="plbls-button" onclick={() => void openReport()}>Signaler un bug</button>
<h3>Mes signalements</h3>
<ul class="stack">
  {#each list as r (r.id)}
    <li><strong>{r.title}</strong> <span class="plbls-badge">{REPORT_STATUS[r.status]}</span>
      <span class="muted small">{new Date(r.created_at).toLocaleDateString('fr-FR')}{r.category ? ` · ${label(REPORT_CATEGORIES, r.category)}` : ''}</span></li>
  {:else}<li class="muted">Aucun signalement envoyé.</li>{/each}
</ul>
