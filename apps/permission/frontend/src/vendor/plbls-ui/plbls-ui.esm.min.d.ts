/*! @plbls/ui v2.0.1 — MIT */
export type PlblsTheme = 'light' | 'dark' | 'auto'
export type PlblsToastType = 'info' | 'succes' | 'erreur'

export const toast: {
  show(text: string, type?: PlblsToastType, durationMs?: number): number
  success(text: string): number
  error(text: string): number
  info(text: string): number
  remove(id: number): void
  clear(): void
}

export function passwordScore(value: string): number
export function getTheme(): PlblsTheme
export function setTheme(theme: PlblsTheme, options?: { persist?: boolean }): void
export function initTheme(): void
export function installBehaviors(): void

export class PlblsModal extends HTMLElement {
  open: boolean
}
export class PlblsDropdownMenu extends HTMLElement {}
export class PlblsTabs extends HTMLElement {}
export class PlblsToastContainer extends HTMLElement {
  show(text: string, type?: PlblsToastType, durationMs?: number): number
  dismiss(id: number): void
  clear(): void
}
export class PlblsOtpInput extends HTMLElement {
  readonly length: number
  readonly value: string
}
export class PlblsCopyField extends HTMLElement {
  copy(): Promise<void>
}
export class PlblsPasswordStrength extends HTMLElement {
  value: string
}
export class PlblsPagination extends HTMLElement {
  page: number
  total: number
}
export class PlblsDataTable extends HTMLElement {
  columns: unknown[]
  rows: unknown[]
}
export class PlblsDropzone extends HTMLElement {}
