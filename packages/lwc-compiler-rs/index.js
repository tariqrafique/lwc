// Re-export the Rust napi ESM loader. The native binary is resolved next to that
// loader (lwc-compiler.<triple>.node), not from unpublished optionalDependencies.
import { transform } from '../../../../lwc-compiler-rust/crates/lwc-compiler-napi/index.js';

export { transform };
// S3 will add a distinct transformSync + async transform on the loader. Until that
// lands, the binding's `transform` is already synchronous.
export const transformSync = transform;
