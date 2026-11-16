// Untyped like index.d.ts (S1). `binding` is the loaded native module.
export declare function wrapBinding(binding: {
    transform(src: string, filename: string, options: unknown, callbacks?: { log: boolean; incrementCounter: boolean }): unknown;
}): { transform(src: string, filename: string, options?: unknown): unknown };
