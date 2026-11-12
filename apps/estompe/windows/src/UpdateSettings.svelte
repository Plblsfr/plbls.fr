<script lang="ts">
  import UpdaterPanel from '../../../../web/estompe/src/components/UpdaterPanel.svelte'
  import { prefs } from '../../../../web/estompe/src/lib/prefs.svelte'
  import { notify } from '../../../../web/estompe/src/lib/ui'

  // 7 clics sur la version : débloque (ou verrouille) les fonctions alpha, sans autre indice dans l'interface
  let clicks = 0
  function versionClick() {
    if (++clicks < 7) return
    clicks = 0
    const on = !prefs.alpha
    prefs.alpha = on
    prefs.save()
    notify.info(on ? 'Fonctions alpha débloquées.' : 'Fonctions alpha verrouillées.')
  }
</script>

<UpdaterPanel onversion={versionClick} />
