// D-0280(b): JS wrapper around scripts/tracked-files.sh (the single scripts helper).
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const helper = path.join(path.dirname(fileURLToPath(import.meta.url)), 'tracked-files.sh');

/** Repo-relative tracked paths under `root`, optionally restricted by git pathspecs. */
export function trackedFiles(root, pathspecs = []) {
  let buf;
  try {
    buf = execFileSync(helper, [root, ...pathspecs], { encoding: 'buffer' });
  } catch (e) {
    const err = new Error(`tracked-files: git ls-files failed under ${root}: ${e.message}`);
    err.cause = e;
    throw err;
  }
  return buf.toString('utf8').split('\0').filter(Boolean);
}

export default trackedFiles;
