<script lang="ts">
  let { kind }: { kind: 'bot' | 'owner' } = $props()
</script>

<span class="role-badge role-badge--{kind}" title={kind === 'bot' ? 'Compte automatique' : 'Créateur du serveur'}>{kind === 'bot' ? 'Bot' : 'Créateur'}</span>
