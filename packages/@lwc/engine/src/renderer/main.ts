import * as domRenderer from './dom';
import * as ssrRenderer from './ssr';

// HACK TYPING
interface Renderer {
    createElement(tagName: string): HTMLElement;
    createElementNS(namespaceURI: string, tagName: string): Element;
    createTextNode(text: string): Text;
    attachShadow(element: HTMLElement): ShadowRoot;
}

export default ('document' in globalThis ? domRenderer : ssrRenderer) as Renderer;
