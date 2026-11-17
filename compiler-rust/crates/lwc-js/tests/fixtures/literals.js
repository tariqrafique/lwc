import { LightningElement, api } from 'lwc';
// line comment at top
/*! legal bang comment */
/** @license MIT */
export default class Literals extends LightningElement {
    /** JSDoc on field */
    @api dec = 10000;
    hex = 0xFF;
    hexLower = 0xff;
    oct = 0o17;
    bin = 0b1010;
    sep = 1_000_000;
    exp = 1e3;
    expBig = 1E21;
    trailing = 1.0;
    trailing2 = 0.50;
    lead = .5;
    small = 0.0001;
    large = 123456789012345680000;
    maxSafe = 9007199254740991;
    u32 = 4294967295;
    neg = -0;
    big = 0xFFn;
    big2 = 1_000n;
    sq = 'single';
    esc = '\u00e9\x41\n\t';
    emoji = '\u{1F600}';
    dq = "it's";
    nul = '\0';
    ls = 'a\u2028b';
    re = /a\/b[/]+/giu;
    tpl = `x ${1000} \u0041 \`y\``;
    obj = {
        // comment inside object
        a: 1, /* trailing block */
        b: [1, /* in array */ 2],
    };
    fn = () => /*#__PURE__*/ foo(/* arg comment */ 1);
    /**
     * Multi-line
     *     indented code example
     *       deeper
     */
    method() {
        const x = 1; // trailing line comment

        // after blank line
        /* plain block
              keep this indent
           ok */
        return x;
    }
}
function foo() {}
