// `node --experimental-transform-types --import ./ts-register.mjs <script.ts>`
import { register } from 'node:module';

register(new URL('./ts-hooks.mjs', import.meta.url));
