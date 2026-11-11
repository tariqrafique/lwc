// Resolve extensionless relative imports from TypeScript so
// `node --experimental-transform-types --import ./ts-register.mjs` can load harness sources.
export async function resolve(spec, ctx, next) {
    const parent = ctx.parentURL ?? '';
    if (spec.startsWith('.') && /\.tsx?$/.test(parent) && !/\.[cm]?[jt]sx?$/.test(spec)) {
        return next(`${spec}.ts`, ctx);
    }
    return next(spec, ctx);
}
