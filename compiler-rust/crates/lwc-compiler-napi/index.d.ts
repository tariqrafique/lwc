// S1: untyped. S2 generates option types from the AB-0008 schema and adds hand-written result/error types.
// Exports mirror the native binding as-is; the API rename (transform -> transformSync) is S3.
export declare function transform(src: string, filename: string, options?: unknown): unknown;
