<script lang="ts">
  import Icon from '../../components/Icon.svelte'
  import { hrefs } from '../../lib/route.svelte'
</script>

<section class="tir-depot tir-depot--grand" aria-label="Aucun relevé importé">
  <svg width="124" height="124" viewBox="0 0 124 124" aria-hidden="true">
    <circle cx="62" cy="62" r="50" fill="none" stroke="var(--trait)" stroke-width="12" />
    <circle cx="62" cy="62" r="50" fill="none" stroke="var(--rose-fonce)" stroke-width="12" stroke-dasharray="2 14" stroke-linecap="round" />
    <path d="M62 78V48M50 59l12-12 12 12" fill="none" stroke="var(--encre)" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" />
  </svg>
  <h2 class="tir-h2 tir-h2--grand">Aucun relevé pour l’instant</h2>
  <p class="tir-note tir-note--texte">Dépose tes relevés de compte en PDF : Tirelire te montre où part l’argent, mois par mois.</p>
  <a class="tir-btn tir-btn--rose" href={hrefs.tirelire('import')}><Icon name="envoie" size={16} />Importer un relevé PDF</a>
</section>

<section class="tir-carte tir-liste" aria-label="Comment ça marche">
  <div class="tir-li"><span class="tir-pas tir-pas--rose">1</span><div class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">Importe</span><span class="tir-note">Un ou plusieurs PDF ; les totaux sont vérifiés.</span></div></div>
  <div class="tir-li"><span class="tir-pas">2</span><div class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">Classe l’inconnu</span><span class="tir-note">Un choix crée une règle pour tout le commerçant.</span></div></div>
  <div class="tir-li"><span class="tir-pas">3</span><div class="tir-col tir-col--colle tir-grandit"><span class="tir-h3">Vois ce qui dépasse</span><span class="tir-note">Plafonds, écarts, évolution et leviers d’économie.</span></div><span class="tir-etq">à venir</span></div>
</section>

<section class="tir-g2" aria-label="Chiffres clés, vides">
  <div class="tir-carte tir-carte--serre tir-carte--creux"><span class="tir-note">Dépenses du mois</span><span class="tir-val tir-val--vide">— €</span></div>
  <div class="tir-carte tir-carte--serre tir-carte--creux"><span class="tir-note">Reste à vivre</span><span class="tir-val tir-val--vide">— €</span></div>
</section>
