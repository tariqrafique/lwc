enum NodeType {
    Text = 'text',
    Element = 'element',
    ShadowRoot = 'shadow-root',
}

interface BaseNode {
    parent: ElementNode | null;
}

interface ShadowRootNode {
    type: NodeType.ShadowRoot;
    children: [];
}

interface ElementNode extends BaseNode {
    type: NodeType.Element;
    tagName: string;
    namespaceURI: string;
    children: ElementNode[];
    shadowRoot: ShadowRootNode | null;
}

interface TextNode extends BaseNode {
    type: NodeType.Text;
    parent: ElementNode | null;
    text: string;
}

const HTML_NAMESPACE = 'http://www.w3.org/1999/xhtml';

export function createElement(tagName: string): HTMLElement {
    const node: ElementNode = {
        type: NodeType.Element,
        parent: null,
        tagName,
        namespaceURI: HTML_NAMESPACE,
        children: [],
        shadowRoot: null,
    };

    // HACK TYPING
    return (node as any) as HTMLElement;
}

export function createElementNS(namespaceURI: string, tagName: string): Element {
    const node: ElementNode = {
        type: NodeType.Element,
        parent: null,
        tagName,
        namespaceURI,
        children: [],
        shadowRoot: null,
    };

    // HACK TYPING
    return (node as any) as Element;
}

export function createTextNode(text: string): Text {
    const node: TextNode = {
        type: NodeType.Text,
        parent: null,
        text,
    };

    // HACK TYPING
    return (node as any) as Text;
}

export function attachShadow(element: HTMLElement) {
    const node: ShadowRootNode = {
        type: NodeType.ShadowRoot,
        children: [],
    };

    // HACK TYPING
    ((element as any) as ElementNode).shadowRoot = node;

    return node;
}
