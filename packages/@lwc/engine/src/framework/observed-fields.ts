/*
 * Copyright (c) 2018, salesforce.com, inc.
 * All rights reserved.
 * SPDX-License-Identifier: MIT
 * For full license text, see the LICENSE file in the repo root or https://opensource.org/licenses/MIT
 */
import { assert, ArrayReduce, isFalse, getOwnPropertyNames } from '@lwc/shared';
import { ComponentInterface } from './component';
import { getComponentVM } from './vm';
import { valueMutated, valueObserved } from '../libs/mutation-tracker';
import { FieldDef } from './decorators/register';

export function createObservedFieldsDescriptorMap(fields: FieldDef): PropertyDescriptorMap {
    return ArrayReduce.call(
        getOwnPropertyNames(fields),
        (acc: PropertyDescriptorMap, field) => {
            acc[field] = createObservedFieldPropertyDescriptor(field);

            return acc;
        },
        {}
    ) as PropertyDescriptorMap;
}

function createObservedFieldPropertyDescriptor(key: PropertyKey): PropertyDescriptor {
    return {
        get(this: ComponentInterface): any {
            const vm = getComponentVM(this);
            if (process.env.NODE_ENV !== 'production') {
                assert.isTrue(vm && 'cmpRoot' in vm, `${vm} is not a valid vm.`);
            }
            valueObserved(this, key);
            return vm.cmpTrack[key];
        },
        set(this: ComponentInterface, newValue: any) {
            const vm = getComponentVM(this);
            if (process.env.NODE_ENV !== 'production') {
                assert.isTrue(vm && 'cmpRoot' in vm, `${vm} is not a valid vm.`);
            }

            if (newValue !== vm.cmpTrack[key]) {
                vm.cmpTrack[key] = newValue;
                if (isFalse(vm.isDirty)) {
                    valueMutated(this, key);
                }
            }
        },
        enumerable: true,
        configurable: true,
    };
}
