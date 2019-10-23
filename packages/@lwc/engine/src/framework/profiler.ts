import { runtimeFlags } from '@lwc/features';
import { VM, UninitializedVM } from './vm';
import { tagNameGetter } from '../env/element';

const perf = performance;
runtimeFlags.PROFILER_ENABLED = true;
const profilerEnabled = runtimeFlags.PROFILER_ENABLED;

const stringMap : Record<string, number> = Object.create(null);
let stringMapCounter = 1;
if (profilerEnabled) {
    initProfiler();
}

const buffer : Float64Array = new Float64Array(50000);
let i = 0;

enum OperationId {
    CreateStart = 0,
    CreateStop = 1,
    RenderStart = 2,
    RenderStop = 3,
    PatchStart = 4,
    PatchStop = 5,
    ConnectedCallbackStart = 6,
    ConnectedCallbackStop = 7,
    RenderedCallbackStart = 8,
    RenderedCallbackStop = 9,
    HydrateStart = 10,
    HydrateStop = 11,
    DisconnectedCallbackStart = 12,
    DisconnectedCallbackStop = 13
}

function initProfiler() {
    // create buffer
}

function logOperation(id: OperationId, vm: VM | UninitializedVM | undefined) {
    const elmName = vm && tagNameGetter.call(vm.elm);
    buffer[i] = perf.now();
    buffer[i + 1] = id;
    buffer[i + 2] = transformStringToNumber(elmName);
    i = i + 3;
}

function transformStringToNumber(cmpId) {
    const id = stringMap[cmpId];
    if(id) {
        return id;
    } else {
        stringMap[cmpId] = stringMapCounter;
        stringMapCounter++;
        return stringMapCounter;
    }
}

export { logOperation, OperationId, stringMap, buffer }