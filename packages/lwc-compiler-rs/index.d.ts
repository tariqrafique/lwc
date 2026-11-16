import type { CompilerDiagnostic } from '@lwc/errors';

/** Result of the Rust napi transform. Map typings stay loose until S2 lands on the package. */
export interface TransformResult {
    code: string;
    map?: object | null;
    warnings?: CompilerDiagnostic[];
    cssScopeTokens?: string[];
}

export function transform(
    src: string,
    filename: string,
    options?: object
): TransformResult;

export function transformSync(
    src: string,
    filename: string,
    options?: object
): TransformResult;
