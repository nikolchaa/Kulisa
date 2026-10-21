// The games published under /play/ (built by scripts/play.mjs from the same folders).
import type { ImageMetadata } from 'astro';
import classicVn from '../../docs/media/classic-vn.webp';
import desktopOs from '../../docs/media/desktop-os.webp';
import interact from '../../docs/media/interact.webp';
import parallax from '../../docs/media/parallax.webp';
import pcUnwrap from '../../docs/media/pc-unwrap.webp';
import smoke from '../../docs/media/smoke.webp';
import textRpg from '../../docs/media/text-rpg.webp';

export interface Game {
	name: string;
	path: string;
	kind: 'template' | 'demo';
	what: string;
	image: ImageMetadata;
}

export const games: Game[] = [
	{
		name: 'classic-vn',
		path: 'templates/classic-vn',
		kind: 'template',
		what: "The Ren'Py-style visual novel: skip, auto, rollback, history, saves, preferences, touch and gamepad.",
		image: classicVn,
	},
	{
		name: 'text-rpg',
		path: 'templates/text-rpg',
		kind: 'template',
		what: 'An illustrated text adventure with stats, an inventory and a phone layout.',
		image: textRpg,
	},
	{
		name: 'desktop-os',
		path: 'templates/desktop-os',
		kind: 'template',
		what: 'A story told through a fake desktop: windows, chat, a browser and notifications.',
		image: desktopOs,
	},
	{
		name: 'parallax',
		path: 'demos/parallax',
		kind: 'demo',
		what: 'Parallax layers, terrain, GPU particles, post effects and a display lab for every screen shape.',
		image: parallax,
	},
	{
		name: 'pc-unwrap',
		path: 'demos/pc-unwrap',
		kind: 'demo',
		what: 'Unbox a PC and peel its films: meshes, cloth, custom shaders, sound and haptics.',
		image: pcUnwrap,
	},
	{
		name: 'interact',
		path: 'demos/interact',
		kind: 'demo',
		what: 'Cloth, drag and drop, hotspots, gestures, gamepads, text input and sensors.',
		image: interact,
	},
	{
		name: 'smoke',
		path: 'demos/smoke',
		kind: 'demo',
		what: 'A small tour of the API: parallax, shaders, typewriter text, a story with choices.',
		image: smoke,
	},
];

/** The URL of a game's page under /play/, with the site's base path. */
export function playUrl(name: string): string {
	const base = import.meta.env.BASE_URL.replace(/\/$/, '');
	return `${base}/play/${name}/`;
}

export function sourceUrl(game: Game): string {
	return `https://github.com/nikolchaa/Kulisa/tree/main/${game.path}`;
}
