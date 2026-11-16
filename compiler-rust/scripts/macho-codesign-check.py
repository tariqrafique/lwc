#!/usr/bin/env python3
"""Item 15(a): check a thin Mach-O's embedded code signature from Linux.

Parses LC_CODE_SIGNATURE -> SuperBlob -> CodeDirectory and re-hashes every code page
(the check the macOS kernel does at page-in for ad-hoc / linker-signed code).
usage: macho-codesign-check.py FILE   -> prints flags/pages, exit 0 iff every page hash matches.
"""
import hashlib, struct, sys

def main(path):
    data = open(path, 'rb').read()
    magic, = struct.unpack_from('<I', data, 0)
    if magic != 0xFEEDFACF:
        sys.exit(f'{path}: not a thin 64-bit little-endian Mach-O (magic {magic:#x})')
    ncmds, = struct.unpack_from('<I', data, 16)
    off, sig = 32, None
    for _ in range(ncmds):
        cmd, size = struct.unpack_from('<II', data, off)
        if cmd == 0x1D:  # LC_CODE_SIGNATURE
            sig = struct.unpack_from('<II', data, off + 8)
        off += size
    if not sig:
        sys.exit(f'{path}: no LC_CODE_SIGNATURE')
    dataoff, datasize = sig
    blob = data[dataoff:dataoff + datasize]
    smagic, _, count = struct.unpack_from('>III', blob, 0)
    if smagic != 0xFADE0CC0:
        sys.exit(f'{path}: bad SuperBlob magic {smagic:#x}')
    cd = None
    for i in range(count):
        btype, boff = struct.unpack_from('>II', blob, 12 + 8 * i)
        if btype == 0 and struct.unpack_from('>I', blob, boff)[0] == 0xFADE0C02:
            cd = boff
    if cd is None:
        sys.exit(f'{path}: no CodeDirectory')
    (_, _, version, flags, hash_off, ident_off, n_special, n_code, code_limit,
     hash_size, hash_type, _, page_log2) = struct.unpack_from('>IIIIIIIIIBBBB', blob, cd)
    algo = {1: 'sha1', 2: 'sha256'}.get(hash_type)
    if algo is None or hashlib.new(algo).digest_size != hash_size:
        sys.exit(f'{path}: unsupported hash type {hash_type}')
    if code_limit != dataoff:
        sys.exit(f'{path}: codeLimit {code_limit} != signature offset {dataoff}')
    page = 1 << page_log2
    bad = 0
    for i in range(n_code):
        want = blob[cd + hash_off + i * hash_size: cd + hash_off + (i + 1) * hash_size]
        got = hashlib.new(algo, data[i * page: min((i + 1) * page, code_limit)]).digest()
        bad += want != got
    expected_pages = (code_limit + page - 1) // page
    names = [n for b, n in ((0x2, 'ADHOC'), (0x20000, 'LINKER_SIGNED')) if flags & b]
    print(f'codesign: {algo} pages={n_code}/{expected_pages} bad={bad} flags={"|".join(names) or hex(flags)}')
    sys.exit(0 if bad == 0 and n_code == expected_pages and flags & 0x2 else 1)

if __name__ == '__main__':
    main(sys.argv[1])
