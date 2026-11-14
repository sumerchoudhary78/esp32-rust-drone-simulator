import { mkdir, copyFile } from 'node:fs/promises';
await mkdir('web/vendor', { recursive: true });
for (const file of ['three.module.js', 'three.core.js']) {
  await copyFile(`node_modules/three/build/${file}`, `web/vendor/${file}`);
}
await copyFile('node_modules/three/LICENSE', 'web/vendor/THREE-LICENSE.txt');
