// semdiff SD-2a: installs the exec loader hooks (see hooks.mjs). Used as `node --import <this> run.mjs ...`.
import { register } from 'node:module';
register(new URL('./hooks.mjs', import.meta.url));
