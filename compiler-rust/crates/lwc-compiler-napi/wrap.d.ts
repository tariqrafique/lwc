// Untyped like index.d.ts (S1). `binding` is the loaded native module.
export declare function wrapBinding(binding: {
    transform(src: string, filename: string, options: unknown, instrumentation?: unknown): unknown;
}): { transform(src: string, filename: string, options?: unknown): unknown };
