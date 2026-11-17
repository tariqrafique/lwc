// Run by gen.mjs through `$LWC_ROOT/node_modules/.bin/tsx` (.mts: ESM, for top-level await). Imports the upstream TS sources at the
// pinned checkout and prints (1) the exported tables and (2) the upstream predicate answers over the
// probe inputs gen.mjs passes in argv[2] (a JSON file), so gen.mjs can self-check its emitted tables.
import fs from 'node:fs';
import { pathToFileURL } from 'node:url';

const LWC = process.env.LWC_ROOT!;
const imp = (p: string) => import(pathToFileURL(`${LWC}/packages/@lwc/${p}`).href);
console.warn = () => {};

const shared = await imp('shared/src/index.ts');
const pc = await imp('template-compiler/src/parser/constants.ts');
const sc = await imp('template-compiler/src/shared/constants.ts');
const haem = await imp('template-compiler/src/parser/utils/html-element-attributes.ts');

const probes: { names: string[]; tags: string[] } = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const results = {
    isVoidHtml: probes.names.map((n) => shared.isVoidElement(n, shared.HTML_NAMESPACE)),
    isVoidSvg: probes.names.map((n) => shared.isVoidElement(n, shared.SVG_NAMESPACE)),
    isBoolean: probes.names.map((n) => probes.tags.map((t) => shared.isBooleanAttribute(n, t))),
    isGlobal: probes.names.map((n) => shared.isGlobalHtmlAttribute(n)),
    isAria: probes.names.map((n) => shared.isAriaAttribute(n)),
    propToAttr: probes.names.map((n) => shared.htmlPropertyToAttribute(n)),
};
const own = (o: object) => Object.entries(o);
process.stdout.write(
    JSON.stringify({
        exports: {
            HTML_NAMESPACE: shared.HTML_NAMESPACE,
            XML_NAMESPACE: shared.XML_NAMESPACE,
            SVG_NAMESPACE: shared.SVG_NAMESPACE,
            MATHML_NAMESPACE: shared.MATHML_NAMESPACE,
            XLINK_NAMESPACE: shared.XLINK_NAMESPACE,
            SPECIAL_PROPERTY_ATTRIBUTE_MAPPING: [...shared.SPECIAL_PROPERTY_ATTRIBUTE_MAPPING],
            REFLECTIVE_GLOBAL_PROPERTY_SET: [...shared.REFLECTIVE_GLOBAL_PROPERTY_SET],
            AMBIGUOUS_PROP_SET: [...shared.AMBIGUOUS_PROP_SET],
            DISALLOWED_PROP_SET: [...shared.DISALLOWED_PROP_SET],
            ID_REFERENCING_ATTRIBUTES_SET: [...shared.ID_REFERENCING_ATTRIBUTES_SET],
            AriaPropNameToAttrNameMap: own(shared.AriaPropNameToAttrNameMap),
            AriaAttrNameToPropNameMap: own(shared.AriaAttrNameToPropNameMap),
            KNOWN_HTML_AND_SVG_ELEMENTS: [...pc.KNOWN_HTML_AND_SVG_ELEMENTS],
            SUPPORTED_SVG_TAGS: [...pc.SUPPORTED_SVG_TAGS],
            DASHED_TAGNAME_ELEMENT_SET: [...sc.DASHED_TAGNAME_ELEMENT_SET],
            HTML_ATTRIBUTE_ELEMENT_MAP: own(haem.HTML_ATTRIBUTE_ELEMENT_MAP),
        },
        results,
    })
);
