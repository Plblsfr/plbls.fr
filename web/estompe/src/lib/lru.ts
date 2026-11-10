/** Cache borné : au-delà de `max` entrées, la moins récemment lue ou écrite est oubliée. */
export class Lru<K, V> {
  private readonly items = new Map<K, V>()
  constructor(private readonly max: number) {}

  get size() { return this.items.size }

  get(key: K): V | undefined {
    if (!this.items.has(key)) return undefined
    const v = this.items.get(key) as V
    this.items.delete(key)
    this.items.set(key, v)
    return v
  }

  set(key: K, value: V) {
    this.items.delete(key)
    this.items.set(key, value)
    while (this.items.size > this.max) this.items.delete(this.items.keys().next().value as K)
  }

  getOrSet(key: K, make: () => V): V {
    if (this.items.has(key)) return this.get(key) as V
    const v = make()
    this.set(key, v)
    return v
  }
}
