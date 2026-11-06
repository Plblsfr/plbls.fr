// Installe la plateforme du build avant tout autre module (certains lisent `platform` à leur chargement) : doit rester le premier import de main.ts.
import { platform } from '@platform'
import { setPlatform } from './lib/platform'
import { parseHash, router } from './lib/route.svelte'

setPlatform(platform)
router.current = parseHash(location.hash) // le routeur a pu être chargé avant la plateforme : les espaces de la plateforme (#/bobyn) n'étaient pas connus
