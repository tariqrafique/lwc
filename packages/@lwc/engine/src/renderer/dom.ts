const isBrowser = 'document' in globalThis;

function invalidMethod(name: string): () => {} {
    return () => {
        throw new TypeError(`"${name} is not available on this platform!"`);
    };
}

export const createElement = isBrowser
    ? document.createElement
    : invalidMethod('document.createElement');
export const createElementNS = isBrowser
    ? document.createElementNS
    : invalidMethod('document.createElementNS');
export const createTextNode = isBrowser
    ? document.createTextNode
    : invalidMethod('document.createElementNS');
export const attachShadow = isBrowser
    ? HTMLElement.prototype.attachShadow
    : invalidMethod('HTMLElement.prototype.attachShadow');
