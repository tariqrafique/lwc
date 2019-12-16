/*
 * Copyright (c) 2018, salesforce.com, inc.
 * All rights reserved.
 * SPDX-License-Identifier: MIT
 * For full license text, see the LICENSE file in the repo root or https://opensource.org/licenses/MIT
 */
import { defineProperties } from '@lwc/shared';
import {
    windowRemoveEventListener as nativeWindowRemoveEventListener,
    windowAddEventListener as nativeWindowAddEventListener,
} from '../../env/window';
import {
    removeEventListener as nativeRemoveEventListener,
    addEventListener as nativeAddEventListener,
} from '../../env/element';
import { eventTargetGetter } from '../../env/dom';
import { patchEvent } from '../../faux-shadow/events';
import { isNodeDeepShadowed } from '../../faux-shadow/node';

function doesEventNeedsPatch(e: Event): boolean {
    const originalTarget = eventTargetGetter.call(e);
    return originalTarget instanceof Node && isNodeDeepShadowed(originalTarget);
}

function isValidEventListener(listener: EventListenerOrEventListenerObject): boolean {
    if (handlerType !== 'function' && handlerType !== 'object') {
        return false;
    }

    if (
        handlerType === 'object' &&
        (!listener.handleEvent || typeof listener.handleEvent !== 'function')
    ) {
        return false;
    }

    return true;
}

function getEventListenerWrapper(fnOrObj: EventListenerOrEventListenerObject): EventListener {
    let wrapperFn: EventListener | null = null;
    try {
        wrapperFn = fnOrObj.$$lwcEventWrapper$$;
        if (!wrapperFn) {
            const isHandlerFunction = typeof fnOrObj === 'function';
            wrapperFn = fnOrObj.$$lwcEventWrapper$$ = function(this: EventTarget, e: Event) {
                // we don't want to patch every event, only when the original target is coming
                // from inside a synthetic shadow
                if (doesEventNeedsPatch(e)) {
                    patchEvent(e);
                }
                return isHandlerFunction
                    ? fnOrObj.call(this, e)
                    : fnOrObj.handleEvent && fnOrObj.handleEvent(e);
            };
        }
    } catch (e) {
        /** ignore */
    }
    return wrapperFn;
}

function windowAddEventListener(
    this: EventTarget,
    type: string,
    listener: EventListenerOrEventListenerObject,
    optionsOrCapture?: boolean | EventListenerOptions
) {
    if (!isValidEventListener(listener)) {
        return;
    }

    const wrapperFn = getEventListenerWrapper(listener);
    nativeWindowAddEventListener.call(this, type, wrapperFn, optionsOrCapture);
}

function windowRemoveEventListener(
    this: EventTarget,
    type: string,
    listener: EventListenerOrEventListenerObject,
    optionsOrCapture?: boolean | EventListenerOptions
) {
    const wrapperFn = getEventListenerWrapper(listener);
    nativeWindowRemoveEventListener.call(this, type, wrapperFn || listener, optionsOrCapture);
}

function addEventListener(
    this: EventTarget,
    type: string,
    listener: EventListenerOrEventListenerObject,
    optionsOrCapture?: boolean | EventListenerOptions
) {
    if (!isValidEventListener(listener)) {
        return;
    }

    const wrapperFn = getEventListenerWrapper(listener);
    nativeAddEventListener.call(this, type, wrapperFn, optionsOrCapture);
}

function removeEventListener(
    this: EventTarget,
    type: string,
    listener: EventListenerOrEventListenerObject,
    optionsOrCapture?: boolean | EventListenerOptions
) {
    const wrapperFn = getEventListenerWrapper(listener);
    nativeRemoveEventListener.call(this, type, wrapperFn || listener, optionsOrCapture);
}

// TODO [#1305]: these patches should be on EventTarget.prototype instead of win and node prototypes
//       but IE doesn't support that.
window.addEventListener = windowAddEventListener;
window.removeEventListener = windowRemoveEventListener;

// IE11 doesn't have EventTarget, so we have to patch it conditionally:
const protoToBePatched =
    typeof EventTarget !== 'undefined' ? EventTarget.prototype : Node.prototype;

defineProperties(protoToBePatched, {
    addEventListener: {
        value: addEventListener,
        enumerable: true,
        writable: true,
        configurable: true,
    },
    removeEventListener: {
        value: removeEventListener,
        enumerable: true,
        writable: true,
        configurable: true,
    },
});
