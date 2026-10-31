// Fails the build when a page of the built site links to a path on this site that
// does not exist, or to a ".md" file (a link the sync step did not rewrite).
// Run after `astro build`. External links are not checked.

import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const dist = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'dist');

function* htmlFiles(dir) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) yield* htmlFiles(path);
    else if (name.endsWith('.html')) yield path;
  }
}

function exists(route) {
  const path = join(dist, decodeURIComponent(route));
  if (existsSync(join(path, 'index.html'))) return true;
  return existsSync(path) && statSync(path).isFile();
}

const problems = [];
let count = 0;
for (const file of htmlFiles(dist)) {
  const html = readFileSync(file, 'utf8');
  for (const [, href] of html.matchAll(/\shref="([^"]+)"/g)) {
    if (!href.startsWith('/') || href.startsWith('//')) continue;
    count++;
    const [path] = href.split(/[?#]/);
    if (path.endsWith('.md') || !exists(path)) problems.push(`${file.slice(dist.length)}: ${href}`);
  }
}

if (problems.length) {
  console.error(`check-links: ${problems.length} broken link(s):\n  ${[...new Set(problems)].join('\n  ')}`);
  process.exit(1);
}
console.log(`check-links: ${count} internal links ok`);
