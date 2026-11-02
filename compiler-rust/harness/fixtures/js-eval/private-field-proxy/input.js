export class C {
  #x = 41;
  static #s = 1;
  get x() { return this.#x + C.#s; }
  set x(v) { this.#x = v; }
  has(o) { return #x in o; }
}
