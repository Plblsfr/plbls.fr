<script lang="ts">
  import { onMount } from 'svelte'
  import { api } from '../../lib/api'
  import { REPORT_CATEGORIES, REPORT_STATUS, label } from './report'
  import { openReport } from './report.svelte'
  import { notify } from '../../lib/ui'
  import type { Report } from '../../lib/types'
  import Icon from '../../components/Icon.svelte'

  let list = $state<Report[]>([])
  onMount(() => { api.myReports().then((l) => (list = l)).catch((e) => notify.error(e, 'Chargement des signalements impossible')) })
</script>

<section class="p-section">
  <div class="p-carte p-carte-pleine">
    <p class="v-note">Un problème ? Décrivez-le, joignez une capture : les administrateurs le verront. Raccourci : <kbd>Ctrl+Alt+B</kbd> (PC).</p>
    <button class="plbls-button rep-signaler" onclick={() => void openReport()}><Icon name="bug" size={16} />Signaler un bug</button>
  </div>
</section>
<section class="p-section">
  <h3 class="p-sec">Mes signalements</h3>
  <ul class="p-carte rep-liste">
    {#each list as r (r.id)}
      <li class="p-ligne">
        <strong class="p-grow">{r.title}</strong>
        <span class="v-tag">{REPORT_STATUS[r.status]}</span>
        <span class="v-note">{new Date(r.created_at).toLocaleDateString('fr-FR')}{r.category ? ` · ${label(REPORT_CATEGORIES, r.category)}` : ''}</span>
      </li>
    {:else}<li class="p-ligne v-note">Aucun signalement envoyé.</li>{/each}
  </ul>
</section>

<style>
  .rep-signaler { align-self: flex-start; }
  .rep-liste { margin: 0; list-style: none; }
</style>
