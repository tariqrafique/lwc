// SD-2b loader. Not the CSR register: `lwc` stays the pinned @lwc/ssr-runtime.
import { register } from 'node:module';

register(new URL('./ssr-hooks.mjs', import.meta.url));
