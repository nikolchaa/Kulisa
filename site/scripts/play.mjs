// Builds the playable web versions of the demos and templates into public/play/<name>/.
// Every game gets its own page and game.kpak; they share one player (kulisa.js + wasm) in
// public/play/player/, so the site carries the 13 MB wasm once.
//
// Usage: node scripts/play.mjs [--debug]   (needs what `kulisa build web` needs)
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const GAMES = [
	'demos/parallax',
	'demos/pc-unwrap',
	'demos/interact',
	'demos/smoke',
	'templates/classic-vn',
	'templates/text-rpg',
	'templates/desktop-os',
];
const PLAYER = ['kulisa.js', 'kulisa_bg.wasm'];

const site = join(dirname(fileURLToPath(import.meta.url)), '..');
const engine = join(site, '..');
const out = join(site, 'public', 'play');
const staging = join(engine, 'target', 'site-play');
const release = !process.argv.includes('--debug');

const hash = (file) => createHash('sha256').update(readFileSync(file)).digest('hex');

rmSync(staging, { recursive: true, force: true });
rmSync(out, { recursive: true, force: true });
mkdirSync(join(out, 'player'), { recursive: true });

let playerHash = null;
for (const game of GAMES) {
	const name = game.split('/').pop();
	const dir = join(staging, name);
	const args = ['run', '-q', '-p', 'kulisa-cli', '--', 'build', 'web', game, '--out', dir];
	if (release) args.push('--release');
	console.log(`play: building ${game}`);
	execFileSync('cargo', args, { cwd: engine, stdio: 'inherit' });

	// One player for all: they must be the same build.
	const h = PLAYER.map((f) => hash(join(dir, f))).join();
	if (playerHash === null) {
		for (const f of PLAYER) cpSync(join(dir, f), join(out, 'player', f));
		playerHash = h;
	} else if (h !== playerHash) {
		throw new Error(`${game} got a different player build than the first game`);
	}

	const page = readFileSync(join(dir, 'index.html'), 'utf8');
	const shared = page.replace('import("./kulisa.js")', 'import("../player/kulisa.js")');
	if (shared === page) throw new Error(`${game}: the page no longer imports ./kulisa.js; update scripts/play.mjs`);
	mkdirSync(join(out, name));
	writeFileSync(join(out, name, 'index.html'), shared);
	for (const f of readdirSync(dir)) {
		if (f !== 'index.html' && !PLAYER.includes(f)) cpSync(join(dir, f), join(out, name, f));
	}
}
console.log(`play: ${GAMES.length} games in ${out}`);
if (!existsSync(join(out, 'player', 'kulisa_bg.wasm'))) process.exit(1);
