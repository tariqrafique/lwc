/*
 * Copyright (c) 2018, salesforce.com, inc.
 * All rights reserved.
 * SPDX-License-Identifier: MIT
 * For full license text, see the LICENSE file in the repo root or https://opensource.org/licenses/MIT
 */
import { ArrayJoin, ArrayPush, isNull, StringToLowerCase } from '@lwc/shared';
// import { tagNameGetter } from '../env/element';

function getFormattedComponentStack(elm: Element): string {
    const componentStack: string[] = [];

    // TODO: Decouple DOM tree traversal for logging

    return ArrayJoin.call(componentStack, '\n');
}

export function logError(message: string, elm?: Element) {
    let msg = `[LWC error]: ${message}`;

    if (elm) {
        msg = `${msg}\n${getFormattedComponentStack(elm)}`;
    }

    if (process.env.NODE_ENV === 'test') {
        /* eslint-disable-next-line no-console */
        console.error(msg);
        return;
    }
    try {
        throw new Error(msg);
    } catch (e) {
        /* eslint-disable-next-line no-console */
        console.error(e);
    }
}
