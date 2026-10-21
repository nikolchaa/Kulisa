// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import luau from '@shikijs/langs/luau';
import kulisa from './src/kulisa.tmLanguage.json' with { type: 'json' };

// GitHub Pages serves the site under /Kulisa/; with a custom domain, set KULISA_SITE_BASE=/.
const site = process.env.KULISA_SITE_URL ?? 'https://nikolchaa.github.io';
const base = process.env.KULISA_SITE_BASE ?? '/Kulisa';

export default defineConfig({
	site,
	base,
	trailingSlash: 'always',
	vite: {
		// Pages import the screenshots in ../docs/media.
		server: { fs: { allow: ['..'] } },
	},
	integrations: [
		starlight({
			title: 'Kulisa',
			description: 'A fast, fully customizable visual novel and narrative game engine, written in Rust.',
			logo: { src: './src/assets/logo.svg', alt: 'Kulisa' },
			favicon: '/favicon.svg',
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/nikolchaa/Kulisa' }],
			editLink: { baseUrl: 'https://github.com/nikolchaa/Kulisa/edit/main/site/' },
			customCss: [
				'@fontsource/inter/400.css',
				'@fontsource/inter/500.css',
				'@fontsource/inter/600.css',
				'@fontsource/jetbrains-mono/400.css',
				'./src/styles/kulisa.css',
			],
			expressiveCode: {
				// kulisa embeds Luau, so Luau loads first.
				shiki: { langs: [...luau, /** @type {any} */ (kulisa)] },
				styleOverrides: { borderRadius: '8px', codeFontFamily: "'JetBrains Mono', ui-monospace, monospace" },
			},
			sidebar: [
				{
					label: 'Start here',
					items: [
						{ label: 'Installation', slug: 'start/install' },
						{ label: 'Your first game', slug: 'start/first-game' },
						{ label: 'Building and shipping', slug: 'start/building' },
						{ label: 'Play the demos', slug: 'play' },
					],
				},
				{
					label: 'Story language',
					items: [
						{ label: 'Overview', slug: 'story' },
						{ label: 'Dialogue and text', slug: 'story/dialogue' },
						{ label: 'Choices and flow', slug: 'story/flow' },
						{ label: 'Variables and Luau', slug: 'story/variables' },
						{ label: 'Stage and sound', slug: 'story/stage' },
						{ label: 'Saves and updates', slug: 'story/saves' },
						{ label: 'Errors', slug: 'story/errors' },
						{ label: 'Cheat sheet', slug: 'story/cheat-sheet' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'Luau API', link: 'https://github.com/nikolchaa/Kulisa/blob/main/docs/API.md', attrs: { target: '_blank' } },
						{ label: 'Design and decisions', link: 'https://github.com/nikolchaa/Kulisa/blob/main/docs/DESIGN.md', attrs: { target: '_blank' } },
					],
				},
			],
		}),
	],
});
