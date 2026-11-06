import '@testing-library/jest-dom/vitest'
import { afterEach } from 'vitest'
import { cleanup } from '@testing-library/svelte'
import { platform } from '@platform'
import { setPlatform } from './lib/platform'

setPlatform(platform)
afterEach(() => cleanup())
